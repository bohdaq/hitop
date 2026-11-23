package tech.rws8.hitop.ui

import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.unit.dp
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.core.JsonOutline
import tech.rws8.hitop.core.SendResult
import tech.rws8.hitop.core.formatDuration
import tech.rws8.hitop.core.formatSize
import tech.rws8.hitop.core.reason
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.ui.theme.StatusPill

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ResponseSheet(vm: AppViewModel, collectionId: String, requestId: String, res: SendResult, close: () -> Unit) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(onDismissRequest = close, sheetState = state, containerColor = Hitop.surface, modifier = Modifier.testTag("response-sheet")) {
        ResponseContent(vm, collectionId, requestId, res, close, Modifier.fillMaxHeight(0.92f))
    }
}

@Composable
fun ResponseContent(vm: AppViewModel, collectionId: String, requestId: String, res: SendResult, close: (() -> Unit)?, modifier: Modifier = Modifier) {
    var tab by rememberSaveable(res) { mutableStateOf("Body") }

    val clipboard = LocalClipboardManager.current
    val context = LocalContext.current
    Column(modifier.fillMaxWidth()) {
        Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            val status = res.status
            if (status != null) {
                StatusPill("$status ${reason(status)}".trim(), status in 1..399, Modifier.testTag("status"))
                Text("${formatDuration(res.durationMs)} · ${formatSize(res.size)}", style = HitopType.mono.copy(fontSize = HitopType.bodySmall.fontSize), color = Hitop.textSecondary, maxLines = 1, modifier = Modifier.weight(1f))
            } else {
                StatusPill("Failed", false, Modifier.testTag("status"))
                Spacer(Modifier.weight(1f))
            }
            if (vm.aiReady && res.status != null) IconButton(onClick = { vm.explainFor = requestId; vm.sheetFor = null }, modifier = Modifier.size(44.dp).clip(androidx.compose.foundation.shape.CircleShape).background(Hitop.accentContainer).testTag("explain")) {
                Icon(HitopIcons.Sparkle, "Explain response", tint = Hitop.accent, modifier = Modifier.size(20.dp))
            }
            close?.let { IconButton(onClick = it, modifier = Modifier.testTag("close-response")) { Icon(HitopIcons.Close, "Close", tint = Hitop.text) } }
        }
        res.timings?.takeIf { it.totalMs > 0 }?.let { TimingBar(it) }
        val error = res.error
        if (error != null) {
            ErrorCard(error)
            return@Column
        }
        val tabs = listOf("Body", "Headers ${res.headers.size}", if (res.tests.isEmpty()) "Tests" else "Tests ${res.testsPassed}/${res.tests.size}", "Cookies ${res.cookies.size}")
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            tabs.forEach { t ->
                val key = t.substringBefore(' ')
                val on = tab == key
                val color = when {
                    on -> Hitop.onSelected
                    key == "Tests" && res.tests.isNotEmpty() -> if (res.testsPassed == res.tests.size) Hitop.success else Hitop.error
                    else -> Hitop.textSecondary
                }
                Box(
                    Modifier.clip(RoundedCornerShape(18.dp)).then(if (on) Modifier.background(Hitop.selected) else Modifier.background(Color.Transparent))
                        .then(if (on) Modifier else Modifier.clip(RoundedCornerShape(18.dp)))
                        .clickable { tab = key }.heightIn(min = 40.dp).padding(horizontal = 14.dp).testTag("response-tab-$key"),
                    contentAlignment = Alignment.Center,
                ) { Text(t, style = HitopType.label, color = color) }
            }
        }
        Box(Modifier.weight(1f, fill = false).padding(horizontal = 16.dp, vertical = 8.dp)) {
            when (tab) {
                "Body" -> BodyView(res)
                "Headers" -> Rows(res.headers)
                "Tests" -> TestsView(res)
                "Cookies" -> if (res.cookies.isEmpty()) Text("No cookies for this host.", style = HitopType.bodySmall, color = Hitop.textMuted)
                    else Rows(res.cookies.map { it.name to "${it.value}  ·  ${it.domain}${it.path}${if (it.secure) " · secure" else ""}" })
            }
        }
        Row(Modifier.navigationBarsPadding().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            SheetButton("Copy", Modifier.weight(1f)) { clipboard.setText(AnnotatedString(res.body)) }
            SheetButton("Share", Modifier.weight(1f)) {
                val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, res.body)
                context.startActivity(Intent.createChooser(send, "Share the response"))
            }
            SheetButton("Save example", Modifier.weight(1f), accent = true) {
                vm.edit(requestId) { r ->
                    val ex = buildJsonObject {
                        put("name", "${res.status} ${reason(res.status ?: 0)}".trim())
                        put("status", res.status ?: 0)
                        put("headers", JsonArray(res.headers.map { (k, v) -> buildJsonObject { put("name", k); put("value", v) } }))
                        put("body", res.body)
                    }
                    r.set("examples", JsonArray(r.examples + ex))
                }
                vm.save(collectionId, requestId)
            }
        }
    }
}

@Composable
private fun SheetButton(label: String, modifier: Modifier, accent: Boolean = false, onClick: () -> Unit) {
    OutlinedButton(
        onClick = onClick, modifier = modifier.height(48.dp).testTag("response-$label"), shape = RoundedCornerShape(24.dp),
        border = if (accent) null else androidx.compose.foundation.BorderStroke(1.dp, Hitop.border),
        colors = androidx.compose.material3.ButtonDefaults.outlinedButtonColors(containerColor = if (accent) Hitop.accentContainer else Color.Transparent),
    ) { Text(label, style = HitopType.bodySmall, color = if (accent) Hitop.accent else Hitop.text, maxLines = 1) }
}

@Composable
private fun TimingBar(t: tech.rws8.hitop.core.Timings) {
    val parts = listOf(Triple("DNS", t.dnsMs, Hitop.timingDns), Triple("Connect", t.connectMs, Hitop.timingTls), Triple("Waiting", t.waitMs, Hitop.timingWait), Triple("Download", t.downloadMs, Hitop.timingDownload)).filter { it.second > 0 }
    val total = parts.sumOf { it.second }.coerceAtLeast(1)
    Column(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).semantics(mergeDescendants = true) {
        contentDescription = "Timing: " + parts.joinToString { "${it.first} ${it.second} ms" } + if (t.reused) ", reused connection" else ""
    }) {
        Row(Modifier.fillMaxWidth().height(8.dp).clip(RoundedCornerShape(4.dp)), horizontalArrangement = Arrangement.spacedBy(2.dp)) {
            if (parts.isEmpty()) Box(Modifier.weight(1f).fillMaxHeight().background(Hitop.timingWait))
            parts.forEach { (_, ms, color) -> Box(Modifier.weight(ms.toFloat() / total).fillMaxHeight().background(color)) }
        }
        Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.SpaceBetween) {
            parts.forEachIndexed { i, (label, ms, _) -> Text("$label $ms${if (i == parts.lastIndex) " ms" else ""}", style = HitopType.navLabel, color = Hitop.textMuted) }
            if (t.reused) Text("reused", style = HitopType.navLabel, color = Hitop.textMuted)
        }
    }
}

@Composable
private fun ErrorCard(e: tech.rws8.hitop.core.ExplainedError) {
    var details by remember { mutableStateOf(false) }
    Column(Modifier.padding(16.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.errorContainer).padding(16.dp).testTag("error-card"), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(e.headline, style = HitopType.cardTitle, color = Hitop.text)
        Text(e.next, style = HitopType.bodySmall, color = Hitop.errorText)
        Text(if (details) "Hide details" else "Details", Modifier.clickable { details = !details }.padding(vertical = 8.dp), style = HitopType.label, color = Hitop.accent)
        if (details) Text(e.raw, style = HitopType.monoSmall, color = Hitop.textSecondary)
    }
}

@Composable
private fun BodyView(res: SendResult) {
    val root = remember(res.body) { JsonOutline.parse(res.body) }
    if (root == null) {
        Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.codeDeep).padding(16.dp)) {
            Text(res.body.ifEmpty { "(empty body)" }, style = HitopType.mono, color = if (res.body.isEmpty()) Hitop.textMuted else Hitop.text, modifier = Modifier.testTag("body-text"))
        }
        return
    }
    var folded by remember(res.body) { mutableStateOf(JsonOutline.defaultFolded(root)) }
    val lines = remember(root, folded) { JsonOutline.lines(root, folded) }
    LazyColumn(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.codeDeep).padding(vertical = 12.dp).testTag("body-json")) {
        items(lines) { l ->
            val foldable = l.path != null && (l.kind == JsonOutline.Kind.OPEN || l.kind == JsonOutline.Kind.SUMMARY)
            Row(
                Modifier.fillMaxWidth()
                    .then(if (foldable) Modifier.clickable(onClickLabel = if (l.folded) "Unfold" else "Fold") { folded = if (l.folded) folded - l.path!! else folded + l.path!! } else Modifier)
                    .padding(start = 8.dp, end = 12.dp),
            ) {
                Text(if (foldable) (if (l.folded) "▸" else "▾") else " ", Modifier.width(16.dp), style = HitopType.mono, color = Hitop.textMuted)
                Text(buildAnnotatedString {
                    append("  ".repeat(l.depth))
                    l.key?.let { withStyle(SpanStyle(color = Hitop.jsonKey)) { append("\"$it\"") }; append(": ") }
                    val c = when (l.kind) {
                        JsonOutline.Kind.STRING -> Hitop.jsonString
                        JsonOutline.Kind.NUMBER, JsonOutline.Kind.BOOL -> Hitop.jsonNumber
                        JsonOutline.Kind.NULL, JsonOutline.Kind.SUMMARY -> Hitop.jsonMuted
                        else -> Hitop.text
                    }
                    withStyle(SpanStyle(color = c)) { append(l.text) }
                    if (l.comma) append(",")
                }, style = HitopType.mono, color = Hitop.text)
            }
        }
    }
}

@Composable
private fun Rows(rows: List<Pair<String, String>>) {
    LazyColumn(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.codeDeep).padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        items(rows) { (k, v) ->
            Column(Modifier.semantics(mergeDescendants = true) {}) {
                Text(k, style = HitopType.monoSmall, color = Hitop.jsonKey)
                Text(v, style = HitopType.mono, color = Hitop.text)
            }
        }
    }
}

@Composable
private fun TestsView(res: SendResult) {
    if (res.tests.isEmpty()) {
        Text("No tests ran. Write them in the request's Tests tab.", style = HitopType.bodySmall, color = Hitop.textMuted)
        return
    }
    LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.testTag("tests")) {
        items(res.tests) { t ->
            Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Hitop.codeDeep).padding(12.dp).semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(if (t.pass) "✓" else "✕", style = HitopType.cardTitle, color = if (t.pass) Hitop.success else Hitop.error)
                Column {
                    Text(t.name, style = HitopType.bodySmall, color = Hitop.text)
                    if (!t.pass && t.detail.isNotEmpty()) Text(t.detail, style = HitopType.caption, color = Hitop.errorText)
                }
            }
        }
    }
}


