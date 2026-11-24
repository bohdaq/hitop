package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.rws8.hitop.core.GraphQLSchema
import tech.rws8.hitop.core.GrpcFields
import tech.rws8.hitop.core.LogLine
import tech.rws8.hitop.core.Request
import tech.rws8.hitop.core.WebSocketSession
import tech.rws8.hitop.core.WsMessage
import tech.rws8.hitop.core.grpcFields
import tech.rws8.hitop.core.subprotocols
import tech.rws8.hitop.core.withGrpc
import tech.rws8.hitop.core.withWebSocket
import tech.rws8.hitop.core.wsMessages
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopType
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import tech.rws8.hitop.R
import tech.rws8.hitop.platform.tr

/** A WebSocket request's Messages tab: subprotocols and saved messages. */
@Composable
fun WebSocketTab(r: Request, edit: ((Request) -> Request) -> Unit) {
    SectionLabel(tr(R.string.s_subprotocols))
    KeyValueRows(r.subprotocols.map { Triple(it, "", true) }, { list -> edit { it.withWebSocket(subprotocols = list.map { t -> t.first }.filter { s -> s.isNotEmpty() }) } }, tr(R.string.s_subprotocol), "", tr(R.string.s_add_subprotocol))
    SectionLabel(tr(R.string.s_saved_messages))
    val extras = r.wsMessages
    KeyValueRows(r.wsMessages.map { Triple(it.name, it.content, true) }, { list ->
        edit { it.withWebSocket(messages = list.mapIndexed { i, t -> WsMessage(t.first, t.second, extras.getOrNull(i)?.extra ?: tech.rws8.hitop.core.EmptyObject) }) }
    }, tr(R.string.s_name), tr(R.string.s_message), tr(R.string.s_add_message))
    Text(tr(R.string.s_messages_are_sent_with_variables_filled_in), style = HitopType.caption, color = Hitop.textMuted)
}

/** A gRPC request's Service tab: service, method, message (with a template from reflection), TLS. */
@Composable
fun GrpcTab(vm: AppViewModel, collectionId: String, r: Request, edit: ((Request) -> Request) -> Unit) {
    val g = r.grpcFields
    val session = vm.grpc(r.id)
    val services by session.services.collectAsState()
    val methods by session.methods.collectAsState()
    val error by session.schemaError.collectAsState()
    val source by session.source.collectAsState()
    val scope = rememberCoroutineScope()
    SmallField(g.service, { v -> edit { it.withGrpc(g.copy(service = v)) } }, tr(R.string.s_service), Modifier.fillMaxWidth())
    SmallField(g.method, { v -> edit { it.withGrpc(g.copy(method = v)) } }, tr(R.string.s_method), Modifier.fillMaxWidth())
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(tr(R.string.s_plaintext_no_tls), Modifier.weight(1f), style = HitopType.body, color = Hitop.text)
        Switch(g.plaintext, { v -> edit { it.withGrpc(g.copy(plaintext = v)) } }, colors = SwitchDefaults.colors(checkedTrackColor = Hitop.accent, checkedThumbColor = Hitop.onAccent))
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(onClick = { scope.launch { session.load(collectionId, r) } }, modifier = Modifier.testTag("grpc-load")) { Text(tr(R.string.s_load_services), color = Hitop.text) }
        if (g.service.isNotEmpty() && g.method.isNotEmpty()) OutlinedButton(onClick = {
            scope.launch { session.template(collectionId, r, "${g.service}.${g.method}")?.let { t -> edit { it.withGrpc(g.copy(message = t)) } } }
        }) { Text(tr(R.string.s_template), color = Hitop.text) }
    }
    source?.let { Text(tr(R.string.s_from, it), style = HitopType.caption, color = Hitop.textMuted) }
    error?.let { Text(it, style = HitopType.caption, color = Hitop.warning, modifier = Modifier.testTag("grpc-error")) }
    if (services.isNotEmpty()) {
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            services.forEach { s -> methods[s].orEmpty().forEach { m ->
                tech.rws8.hitop.ui.theme.HitopChip("${s.substringAfterLast('.')}/$m", s == g.service && m == g.method, Modifier.clickable { edit { it.withGrpc(g.copy(service = s, method = m)) } })
            } }
        }
    }
    SectionLabel(tr(R.string.s_message_json))
    CodeEditor(g.message, { v -> edit { it.withGrpc(g.copy(message = v)) } }, json = true, label = tr(R.string.s_grpc_message), placeholder = "{ }", minHeight = 160)
    Text(tr(R.string.s_headers_are_the_call_s_metadata_auth), style = HitopType.caption, color = Hitop.textMuted)
}

/** A GraphQL query editor that knows its cursor: Load schema, then the fields of the type at the cursor as chips. */
@Composable
fun GraphQLEditor(vm: AppViewModel, collectionId: String, r: Request, edit: ((Request) -> Request) -> Unit) {
    var value by remember(r.id) { mutableStateOf(TextFieldValue(r.body)) }
    if (value.text != r.body) value = value.copy(text = r.body, selection = TextRange(r.body.length.coerceAtMost(value.selection.end)))
    val schema = vm.schemas[r.id]
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        OutlinedButton(onClick = { scope.launch { error = vm.loadSchema(collectionId, r) } }, modifier = Modifier.testTag("load-schema")) { Text(if (schema == null) tr(R.string.s_load_schema) else tr(R.string.s_reload_schema), color = Hitop.text) }
        schema?.let { Text(tr(R.string.s_types, it.types.size), style = HitopType.caption, color = Hitop.textMuted, modifier = Modifier.testTag("schema-loaded")) }
    }
    error?.let { Text(it, style = HitopType.caption, color = Hitop.warning) }
    val suggestions = schema?.suggestions(value.text, value.selection.end).orEmpty()
    if (suggestions.isNotEmpty()) {
        Row(Modifier.horizontalScroll(rememberScrollState()).testTag("suggestions"), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            suggestions.take(12).forEach { f ->
                Text(f.name, style = HitopType.monoSmall, color = Hitop.onSelected, modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(Hitop.selected)
                    .clickable {
                        val at = value.selection.end
                        val prefix = Regex("[_A-Za-z][_0-9A-Za-z]*$").find(value.text.substring(0, at))?.value.orEmpty()
                        val text = value.text.substring(0, at - prefix.length) + f.name + value.text.substring(at)
                        value = TextFieldValue(text, TextRange(at - prefix.length + f.name.length))
                        edit { it.withBody(text) }
                    }.heightIn(min = 36.dp).padding(horizontal = 10.dp, vertical = 9.dp).semantics { contentDescription = tr(R.string.s_insert, f.name, f.type) }.testTag("suggest-${f.name}"))
            }
        }
    }
    androidx.compose.foundation.layout.Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.code).heightIn(min = 200.dp).padding(16.dp)) {
        BasicTextField(value, { v -> value = v; if (v.text != r.body) edit { it.withBody(v.text) } }, textStyle = HitopType.mono.copy(color = Hitop.text), cursorBrush = SolidColor(Hitop.accent),
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = tr(R.string.s_graphql_query) }.testTag("GraphQL query"))
    }
    schema?.let { s -> SchemaBrowser(s) }
}

@Composable
private fun SchemaBrowser(s: GraphQLSchema) {
    var open by remember { mutableStateOf<String?>(null) }
    Text(if (open == null) tr(R.string.s_browse_the_schema) else tr(R.string.s_schema, open), Modifier.clickable { open = if (open == null) s.query else null }.padding(vertical = 8.dp).testTag("browse-schema"), style = HitopType.label, color = Hitop.accent)
    val t = open?.let { s.types[it] } ?: return
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Hitop.surface).padding(12.dp)) {
        t.fields.forEach { f ->
            Text("${f.name}${if (f.args.isNotEmpty()) "(${f.args.joinToString()})" else ""}: ${f.type}", Modifier.fillMaxWidth().clickable { if (s.types[f.named]?.fields?.isNotEmpty() == true) open = f.named }.padding(vertical = 6.dp),
                style = HitopType.monoSmall, color = Hitop.text)
        }
    }
}

private fun time(ms: Long) = SimpleDateFormat("HH:mm:ss", Locale.ROOT).format(Date(ms))

/** The open WebSocket: status, saved messages, a composer and the timestamped log. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WebSocketSheet(vm: AppViewModel, r: Request, close: () -> Unit) {
    val s = vm.ws(r.id)
    val status by s.status.collectAsState()
    val log by s.log.collectAsState()
    val scope = rememberCoroutineScope()
    var draft by remember { mutableStateOf("") }
    ModalBottomSheet(onDismissRequest = close, containerColor = Hitop.surface, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), modifier = Modifier.testTag("ws-sheet")) {
        Column(Modifier.fillMaxHeight(0.9f).padding(horizontal = 16.dp).navigationBarsPadding(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(when (status) { WebSocketSession.Status.OPEN -> tr(R.string.s_connected) + (s.protocol?.let { " · ${it}" } ?: ""); WebSocketSession.Status.CONNECTING -> tr(R.string.s_connecting); else -> tr(R.string.s_not_connected) },
                    Modifier.weight(1f).testTag("ws-status"), style = HitopType.cardTitle, color = if (status == WebSocketSession.Status.OPEN) Hitop.success else Hitop.textSecondary)
                if (status == WebSocketSession.Status.OPEN) TextButton(onClick = { s.close() }, modifier = Modifier.testTag("ws-disconnect")) { Text(tr(R.string.s_disconnect), color = Hitop.error) }
                else TextButton(onClick = { s.connect(vm.collectionOf(r.id) ?: return@TextButton, vm.draft(r.id) ?: r) }) { Text(tr(R.string.s_connect), color = Hitop.accent) }
            }
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                r.wsMessages.forEach { m -> OutlinedButton(onClick = { scope.launch { s.sendSaved(m.name) } }, enabled = status == WebSocketSession.Status.OPEN, modifier = Modifier.testTag("ws-saved-${m.name}")) { Text(m.name, color = Hitop.text) } }
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SmallField(draft, { draft = it }, tr(R.string.s_message), Modifier.weight(1f))
                Button(onClick = { val t = draft; draft = ""; scope.launch { s.send(t) } }, enabled = status == WebSocketSession.Status.OPEN && draft.isNotEmpty(),
                    colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent), modifier = Modifier.testTag("ws-send")) { Text(tr(R.string.s_send_2)) }
            }
            LazyColumn(Modifier.weight(1f).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.codeDeep).padding(12.dp).testTag("ws-log"), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                items(log) { l ->
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.semantics(mergeDescendants = true) {}) {
                        Text(time(l.at), style = HitopType.monoSmall, color = Hitop.textMuted)
                        Text(when (l.kind) { LogLine.Kind.SENT -> "↑"; LogLine.Kind.RECEIVED -> "↓"; else -> "·" }, style = HitopType.monoSmall, color = Hitop.accent)
                        Text(l.text, style = HitopType.monoSmall, color = if (l.kind == LogLine.Kind.ERROR) Hitop.errorText else Hitop.text)
                    }
                }
            }
        }
    }
}

/** A request read as Server-Sent Events: live count, Stop, each event with time, name, id, data. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SseSheet(vm: AppViewModel, r: Request, close: () -> Unit) {
    val s = vm.sse(r.id)
    val st by s.state.collectAsState()
    ModalBottomSheet(onDismissRequest = { s.stop(); close() }, containerColor = Hitop.surface, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), modifier = Modifier.testTag("sse-sheet")) {
        Column(Modifier.fillMaxHeight(0.9f).padding(horizontal = 16.dp).navigationBarsPadding(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text((if (st.running) tr(R.string.s_live) else if (st.stopped) tr(R.string.s_stopped_2) else tr(R.string.s_ended)) + tr(R.string.s_events, st.events.size), Modifier.weight(1f).testTag("sse-count"), style = HitopType.cardTitle, color = Hitop.text)
                st.status?.let { Text("${it}", style = HitopType.mono, color = if (it < 400) Hitop.success else Hitop.error) }
                if (st.running) TextButton(onClick = { s.stop() }, modifier = Modifier.testTag("sse-stop")) { Text(tr(R.string.s_stop), color = Hitop.error) }
            }
            st.error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText) }
            LazyColumn(Modifier.weight(1f).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.codeDeep).padding(12.dp).testTag("sse-events"), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                items(st.events) { (at, e) ->
                    Column(Modifier.semantics(mergeDescendants = true) {}) {
                        Text("${time(at)} · ${e.event}${e.id?.let { " · #${it}" } ?: ""}", style = HitopType.monoSmall, color = Hitop.jsonKey)
                        Text(e.data, style = HitopType.monoSmall, color = Hitop.text)
                    }
                }
            }
        }
    }
}

/** A gRPC call: messages as they come, the status. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GrpcSheet(vm: AppViewModel, r: Request, close: () -> Unit) {
    val s = vm.grpc(r.id)
    val c by s.call.collectAsState()
    ModalBottomSheet(onDismissRequest = close, containerColor = Hitop.surface, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true), modifier = Modifier.testTag("grpc-sheet")) {
        Column(Modifier.fillMaxHeight(0.9f).padding(horizontal = 16.dp).navigationBarsPadding(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(c.status?.let { "${it}${c.statusMessage?.takeIf { m -> m.isNotEmpty() }?.let { m -> " · ${m}" } ?: ""}" } ?: if (c.running) tr(R.string.s_calling) else "—", Modifier.weight(1f).testTag("grpc-status"),
                    style = HitopType.cardTitle, color = if (c.status == "OK") Hitop.success else if (c.status != null || c.error != null) Hitop.error else Hitop.textSecondary)
                if (c.running) TextButton(onClick = { s.cancel() }) { Text(tr(R.string.s_cancel), color = Hitop.error) }
            }
            c.error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText, modifier = Modifier.testTag("grpc-call-error")) }
            LazyColumn(Modifier.weight(1f).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.codeDeep).padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                items(c.messages) { m -> Text(m, style = HitopType.monoSmall, color = Hitop.text) }
            }
        }
    }
}

@Suppress("unused")
private val keepFields = GrpcFields("", "", "", false)

@Composable
fun LaunchOnce(key: Any, block: suspend () -> Unit) = LaunchedEffect(key) { block() }
