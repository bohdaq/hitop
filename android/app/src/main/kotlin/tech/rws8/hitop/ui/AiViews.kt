package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.rws8.hitop.core.CurlImport
import tech.rws8.hitop.core.ModelInput
import tech.rws8.hitop.core.Prompts
import tech.rws8.hitop.core.RequestDraft
import tech.rws8.hitop.core.SendResult
import tech.rws8.hitop.core.TestScript
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.R
import tech.rws8.hitop.platform.tr

/** Explain a response with the on-device model; suggest tests that pass on it and add them to the request. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ExplainSheet(vm: AppViewModel, collectionId: String, requestId: String, res: SendResult, close: () -> Unit) {
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    var text by remember { mutableStateOf("") }
    var done by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var script by remember { mutableStateOf<String?>(null) }
    var suggesting by remember { mutableStateOf(false) }
    val r = vm.draft(requestId)
    val input = remember(res) { r?.let { ModelInput.exchange(it, vm.library.state.value.collection(collectionId)?.auth, res, vm.secretValues()) }.orEmpty() }
    LaunchedEffect(res) {
        try {
            vm.assistant.stream(Prompts.explain(input)).collect { text += it }
        } catch (e: Exception) {
            error = e.message ?: "The on-device model couldn't answer."
        }
        done = true
    }
    ModalBottomSheet(onDismissRequest = close, containerColor = Hitop.surface, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), modifier = Modifier.testTag("explain-sheet")) {
        Column(Modifier.padding(horizontal = 16.dp).padding(bottom = 16.dp).navigationBarsPadding().verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(HitopIcons.Sparkle, null, tint = Hitop.accent, modifier = Modifier.size(20.dp))
                Text(tr(R.string.s_explain_response), style = HitopType.title, color = Hitop.text)
            }
            Text(text.ifEmpty { if (done) "" else tr(R.string.s_thinking_on_this_phone) }, style = HitopType.body, color = Hitop.onInsight,
                modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.insight).padding(16.dp).testTag("explanation"))
            error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText) }
            Text(tr(R.string.s_on_this_phone_the_model_sees_the), style = HitopType.caption, color = Hitop.textMuted)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { clipboard.setText(AnnotatedString(text)) }, enabled = text.isNotEmpty()) { Text(tr(R.string.s_copy), color = Hitop.text) }
                Button(onClick = {
                    suggesting = true
                    scope.launch {
                        try {
                            val checks = TestScript.validate(TestScript.parse(vm.assistant.answer(Prompts.suggestTests(input))), res)
                            script = if (checks.isEmpty()) "" else TestScript.render(checks)
                        } catch (e: Exception) {
                            error = e.message
                        }
                        suggesting = false
                    }
                }, enabled = done && !suggesting, colors = ButtonDefaults.buttonColors(containerColor = Hitop.accentContainer, contentColor = Hitop.accent), modifier = Modifier.testTag("suggest-test")) {
                    if (suggesting) CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp) else Text(tr(R.string.s_suggest_a_test))
                }
            }
            script?.let { s ->
                if (s.isEmpty()) Text(tr(R.string.s_no_check_the_model_suggested_holds_for), style = HitopType.caption, color = Hitop.textMuted)
                else {
                    Text(s, style = HitopType.monoSmall, color = Hitop.text, modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Hitop.codeDeep).padding(12.dp).testTag("suggested-script"))
                    if (!vm.library.isReadOnly(collectionId)) Button(onClick = {
                        vm.edit(requestId) { it.withPost((it.postRequestScript.trimEnd() + "\n" + s).trim() + "\n") }
                        close()
                    }, colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent), modifier = Modifier.testTag("add-test")) { Text(tr(R.string.s_add_test_to_request)) }
                }
            }
        }
    }
}

/** A new request from a curl command (no model needed) or from a description (on-device model). */
@Composable
fun FromTextDialog(vm: AppViewModel, initial: String = "", close: () -> Unit) {
    val scope = rememberCoroutineScope()
    var text by remember { mutableStateOf(initial) }
    var busy by remember { mutableStateOf(false) }
    var note by remember { mutableStateOf<String?>(null) }
    InputSheet(tr(R.string.s_new_request_from_text), close, actions = {
        TextButton(onClick = close) { Text(tr(R.string.s_cancel), color = Hitop.textSecondary) }
        TextButton(onClick = {
            scope.launch {
                busy = true
                val made = if (CurlImport.looksLikeCurl(text)) CurlImport.parse(text)?.also { if (it.ignored.isNotEmpty()) note = "Left out: ${it.ignored.joinToString()}" }?.request
                else if (vm.aiReady) runCatching { RequestDraft.parse(vm.assistant.answer(Prompts.requestFromText(text))) }.getOrNull()
                else null
                busy = false
                if (made == null) note = "Couldn't make a request from that." else {
                    val (c, r) = vm.library.newRequest(null, "", made)
                    close()
                    vm.openRequest(c.id, r.id)
                }
            }
        }, enabled = text.isNotBlank() && !busy, modifier = Modifier.testTag("from-text-make")) { Text(tr(R.string.s_make_request), color = Hitop.accent) }
    }) {
        Text(if (vm.aiReady) tr(R.string.s_paste_a_curl_command_or_describe_the) else tr(R.string.s_paste_a_curl_command_describing_a_request), style = HitopType.caption, color = Hitop.textSecondary)
        CodeEditor(text, { text = it }, label = "from-text", placeholder = tr(R.string.s_curl_https_or_a_description), minHeight = 120, ground = Hitop.surface, style = HitopType.monoSmall)
        note?.let { Text(it, style = HitopType.caption, color = Hitop.warning, modifier = Modifier.testTag("from-text-note")) }
    }
}
