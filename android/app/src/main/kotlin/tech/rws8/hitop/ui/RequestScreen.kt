package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRowDefaults
import androidx.compose.material3.TabRowDefaults.tabIndicatorOffset
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.rws8.hitop.core.Auth
import tech.rws8.hitop.core.BodyKind
import tech.rws8.hitop.core.FormRow
import tech.rws8.hitop.core.Header
import tech.rws8.hitop.core.JsonOutline
import tech.rws8.hitop.core.ParamRow
import tech.rws8.hitop.core.Request
import tech.rws8.hitop.core.Variables
import tech.rws8.hitop.ui.theme.Dot
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.ui.theme.MethodColors
import tech.rws8.hitop.ui.theme.MethodTag

private val EDITOR_TABS = listOf("Params", "Headers", "Body", "Auth", "Scripts", "Tests")
private val WS_TABS = listOf("Params", "Headers", "Messages", "Auth")
private val GRPC_TABS = listOf("Service", "Metadata", "Auth")

@Composable
fun RequestScreen(vm: AppViewModel, collectionId: String, requestId: String) {
    @Suppress("LocalVariableName") val LocalResponsePaneValue = LocalResponsePane.current
    val s by vm.library.state.collectAsState()
    val c = s.collection(collectionId)
    val r = vm.draft(requestId)
    if (c == null || r == null) {
        Text("This request was deleted.", Modifier.statusBarsPadding().padding(16.dp), color = Hitop.textMuted)
        return
    }
    val tabs = when { r.isWebSocket -> WS_TABS; r.isGrpc -> GRPC_TABS; else -> EDITOR_TABS }
    var tab by rememberSaveable(requestId) { mutableStateOf(if (r.isGrpc) "Service" else if (r.bodyKind != BodyKind.NONE && !r.isWebSocket) "Body" else "Params") }
    if (tab !in tabs) tab = tabs.first()
    var menu by remember { mutableStateOf(false) }
    var dialog by remember { mutableStateOf<(@Composable () -> Unit)?>(null) }
    var peek by remember { mutableStateOf<String?>(null) }
    var showCode by remember { mutableStateOf(false) }
    val dirty = vm.isDirty(requestId)
    val viewOnly = vm.library.isReadOnly(collectionId)
    val edit = { f: (Request) -> Request -> vm.edit(requestId, f) }
    val title = if (requestId in s.autoNamed) tech.rws8.hitop.core.AutoName.of(r) else r.name

    Column(Modifier.fillMaxSize().background(Hitop.ground).statusBarsPadding().imePadding()) {
        Row(Modifier.padding(start = 8.dp, end = 8.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            if (!LocalResponsePane.current) IconButton(onClick = { vm.pop() }, modifier = Modifier.testTag("back")) { Icon(HitopIcons.Back, "Back", tint = Hitop.text) }
            Column(Modifier.weight(1f)) {
                Text(title, style = HitopType.title, color = Hitop.text, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.semantics { heading() }.testTag("request-title"))
                Text(listOf(c.name, r.folder).filter { it.isNotEmpty() }.joinToString(" › ") + if (viewOnly) " · View only" else if (dirty) " · unsaved" else "", style = HitopType.caption, color = Hitop.textMuted, maxLines = 1)
            }
            if (!viewOnly) IconButton(onClick = { vm.save(collectionId, requestId) }, enabled = dirty || requestId in s.autoNamed, modifier = Modifier.testTag("save")) {
                Icon(HitopIcons.Save, if (dirty) "Save changes" else "Saved", tint = if (dirty) Hitop.accent else Hitop.textMuted)
            }
            Box {
                IconButton(onClick = { menu = true }, modifier = Modifier.testTag("request-more")) { Icon(HitopIcons.More, "More", tint = Hitop.text) }
                DropdownMenu(menu, { menu = false }, containerColor = Hitop.surfaceHigh) {
                    DropdownMenuItem(text = { Text("Rename", color = Hitop.text) }, onClick = {
                        menu = false
                        dialog = { TextDialog("Rename request", r.name, "Rename", { dialog = null }) { n -> vm.save(collectionId, requestId); vm.library.renameRequest(requestId, n); dialog = null } }
                    })
                    DropdownMenuItem(text = { Text(if (requestId in s.pins) "Unpin" else "Pin", color = Hitop.text) }, onClick = { menu = false; vm.library.togglePin(requestId) })
                    DropdownMenuItem(text = { Text("Duplicate", color = Hitop.text) }, onClick = { menu = false; vm.library.duplicateRequest(requestId)?.let { vm.openRequest(collectionId, it.id) } })
                    if (dirty) DropdownMenuItem(text = { Text("Discard changes", color = Hitop.text) }, onClick = { menu = false; vm.discard(requestId) })
                    DropdownMenuItem(text = { Text("Delete", color = Hitop.error) }, onClick = {
                        menu = false
                        dialog = { ConfirmDialog("Delete ${r.name}?", "It is deleted from ${c.name} on this phone.", "Delete", { dialog = null }) { dialog = null; vm.pop(); vm.library.deleteRequest(requestId); vm.discard(requestId) } }
                    })
                }
            }
        }

        Column(Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                MethodMenu(tech.rws8.hitop.core.Modes.of(r)) { m -> edit { tech.rws8.hitop.core.Modes.set(it, m) } }
                EnvironmentPill(vm)
            }
            CodeEditor(r.url, { u -> edit { it.withUrl(u.replace("\n", "")) } }, label = "URL", placeholder = "https://api.example.com/path", minHeight = 0, ground = Hitop.surface)
            val vars = Variables.names(r.url + "\n" + r.body + "\n" + r.headers.joinToString { it.value })
            if (vars.isNotEmpty()) {
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    vars.forEach { v ->
                        Text("\${$v}", style = HitopType.monoSmall, color = Hitop.accent,
                            modifier = Modifier.clip(RoundedCornerShape(8.dp)).background(Hitop.accentContainer).clickable(onClickLabel = "Show its value") { peek = if (peek == v) null else v }.heightIn(min = 32.dp).padding(horizontal = 10.dp, vertical = 8.dp).testTag("var-$v"))
                    }
                }
            }
            peek?.let { name -> PeekCard(vm, collectionId, name) { peek = null } }
        }

        PrimaryScrollableTabRow(
            selectedTabIndex = tabs.indexOf(tab),
            containerColor = Hitop.ground, contentColor = Hitop.text, edgePadding = 8.dp,
            indicator = { TabRowDefaults.PrimaryIndicator(Modifier.tabIndicatorOffset(tabs.indexOf(tab), matchContentSize = false), color = Hitop.accent, width = 48.dp) },
            divider = { Box(Modifier.fillMaxWidth().height(1.dp).background(Hitop.divider)) },
        ) {
            tabs.forEach { t ->
                val count = when (t) {
                    "Headers", "Metadata" -> r.headers.count { it.enabled }.takeIf { it > 0 }
                    "Params" -> queryOf(r.url).size.takeIf { it > 0 }
                    else -> null
                }
                Tab(selected = tab == t, onClick = { tab = t }, modifier = Modifier.testTag("editor-tab-$t"),
                    text = { Text(if (count != null) "$t $count" else t, style = HitopType.bodySmall, color = if (tab == t) Hitop.text else Hitop.textMuted) })
            }
        }

        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            when (tab) {
                "Params" -> ParamsTab(r, edit)
                "Headers", "Metadata" -> HeadersTab(r, c.auth, edit)
                "Body" -> if (r.bodyKind == BodyKind.GRAPHQL) { BodyTab(r, edit, graphql = { GraphQLEditor(vm, collectionId, r, edit) }) } else BodyTab(r, edit)
                "Messages" -> WebSocketTab(r, edit)
                "Service" -> GrpcTab(vm, collectionId, r, edit)
                "Auth" -> AuthTab(r, c.auth, edit)
                "Scripts" -> {
                    Text("Runs before the request is sent: set variables with pm.environment.set, change the request.", style = HitopType.caption, color = Hitop.textMuted)
                    CodeEditor(r.preRequestScript, { v -> edit { it.withPre(v) } }, label = "Pre-request script", placeholder = "pm.environment.set(\"ts\", Date.now())", minHeight = 220)
                }
                "Tests" -> {
                    Text("Runs after the response: pm.test with pm.expect, as in Postman.", style = HitopType.caption, color = Hitop.textMuted)
                    CodeEditor(r.postRequestScript, { v -> edit { it.withPost(v) } }, label = "Tests script", placeholder = "pm.test(\"Status is 200\", () => pm.response.to.have.status(200));", minHeight = 220)
                }
            }
        }

        Row(Modifier.navigationBarsPadding().padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedButton(onClick = { showCode = true }, modifier = Modifier.height(56.dp).testTag("code"), shape = RoundedCornerShape(28.dp), border = androidx.compose.foundation.BorderStroke(1.dp, Hitop.border)) {
                Text("Code", style = HitopType.body, color = Hitop.text)
            }
            val busy = vm.sending[requestId] == true
            val live = r.isWebSocket || r.isGrpc || r.tag == "SSE"
            Button(
                onClick = { if (live) vm.startLive(collectionId, requestId) else vm.send(collectionId, requestId) },
                enabled = !busy,
                modifier = Modifier.weight(1f).height(56.dp).testTag("send"),
                shape = RoundedCornerShape(28.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent, disabledContainerColor = Hitop.accentContainer, disabledContentColor = Hitop.textMuted),
            ) {
                if (busy) CircularProgressIndicator(Modifier.size(20.dp), color = Hitop.onAccent, strokeWidth = 2.dp) else Icon(HitopIcons.Send, null, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(10.dp))
                Text(if (busy) "Sending…" else if (r.isWebSocket) "Connect" else if (r.isGrpc) "Call" else "Send", style = HitopType.button)
            }
        }
    }
    dialog?.invoke()
    if (showCode) CodeSheet(vm, collectionId, r) { showCode = false }
    if (vm.sheetFor == requestId && !LocalResponsePane.current) vm.responses[requestId]?.let { ResponseSheet(vm, collectionId, requestId, it) { vm.sheetFor = null } }
    if (vm.explainFor == requestId) vm.responses[requestId]?.let { res ->
        ExplainSheet(vm, collectionId, requestId, res) { vm.explainFor = null; if (!LocalResponsePaneValue) vm.sheetFor = requestId }
    }
    if (vm.liveFor == requestId) {
        val close = { vm.liveFor = null }
        when {
            r.isWebSocket -> WebSocketSheet(vm, r, close)
            r.isGrpc -> GrpcSheet(vm, r, close)
            else -> SseSheet(vm, r, close)
        }
    }
}

@Composable
private fun MethodMenu(method: String, onPick: (String) -> Unit) {
    val tag = when (method) { "WebSocket" -> "WS"; "gRPC" -> "GRPC"; else -> method }
    var open by remember { mutableStateOf(false) }
    Box {
        Row(
            Modifier.clip(RoundedCornerShape(12.dp)).background(MethodColors.container(tag)).clickable { open = true }.heightIn(min = 40.dp).padding(horizontal = 12.dp)
                .semantics { contentDescription = "Method $method" }.testTag("method"),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(tag, style = HitopType.method.copy(fontSize = HitopType.label.fontSize), color = MethodColors.of(tag))
            Icon(HitopIcons.ChevronDown, null, tint = MethodColors.of(tag), modifier = Modifier.size(14.dp))
        }
        DropdownMenu(open, { open = false }, containerColor = Hitop.surfaceHigh) {
            tech.rws8.hitop.core.Modes.ALL.forEach { m ->
                DropdownMenuItem(text = { Text(m, style = HitopType.method.copy(fontSize = HitopType.label.fontSize), color = MethodColors.of(when (m) { "WebSocket" -> "WS"; "gRPC" -> "GRPC"; else -> m })) }, onClick = { onPick(m); open = false }, modifier = Modifier.testTag("method-$m"))
            }
        }
    }
}

@Composable
private fun PeekCard(vm: AppViewModel, collectionId: String, name: String, close: () -> Unit) {
    val p = vm.library.peek(collectionId, name)
    var show by remember(name) { mutableStateOf(false) }
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.surface).padding(horizontal = 14.dp, vertical = 10.dp).testTag("peek"),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Icon(HitopIcons.Sparkle, null, tint = Hitop.accent, modifier = Modifier.size(18.dp))
        val value = when {
            p.value == null -> "no value"
            p.secret && !show -> "••••••"
            else -> p.value
        }
        Text("$name = $value · ${if (p.scope == null) p.where else "from ${p.where}"}", Modifier.weight(1f), style = HitopType.caption, color = Hitop.textSecondary)
        if (p.secret && p.value != null) TextButton(onClick = { show = !show }) { Text(if (show) "Hide" else "Show", color = Hitop.accent) }
        IconButton(onClick = close) { Icon(HitopIcons.Close, "Close", tint = Hitop.textMuted, modifier = Modifier.size(16.dp)) }
    }
}

/** The URL's query as rows (`?a=1&b=2`), in order. */
fun queryOf(url: String): List<Pair<String, String>> {
    val q = url.substringAfter('?', "").substringBefore('#')
    if (q.isEmpty()) return emptyList()
    return q.split('&').filter { it.isNotEmpty() }.map { p -> p.substringBefore('=') to p.substringAfter('=', "") }
}

fun withQuery(url: String, rows: List<Pair<String, String>>): String {
    val base = url.substringBefore('?')
    val frag = url.substringAfter('#', "").let { if (it.isEmpty()) "" else "#$it" }
    val q = rows.filter { it.first.isNotEmpty() || it.second.isNotEmpty() }.joinToString("&") { (k, v) -> if (v.isEmpty() && k.isNotEmpty()) k else "$k=$v" }
    return base + (if (q.isEmpty()) "" else "?$q") + frag
}

@Composable
private fun ParamsTab(r: Request, edit: ((Request) -> Request) -> Unit) {
    val rows = queryOf(r.url).map { Triple(it.first, it.second, true) } + r.disabledParams.map { Triple(it.key, it.value, false) }
    KeyValueRows(rows, { list ->
        edit { it.withUrl(withQuery(it.url, list.filter { t -> t.third }.map { t -> t.first to t.second })).withDisabledParams(list.filter { t -> !t.third }.map { t -> ParamRow(t.first, t.second) }) }
    }, "Parameter", "Value", "Add parameter")
}

@Composable
private fun HeadersTab(r: Request, collectionAuth: Auth?, edit: ((Request) -> Request) -> Unit) {
    val extras = r.headers.map { it.extra }
    KeyValueRows(r.headers.map { Triple(it.name, it.value, it.enabled) }, { list ->
        edit { it.withHeaders(list.mapIndexed { i, t -> Header(t.first, t.second, t.third, extras.getOrNull(i) ?: tech.rws8.hitop.core.EmptyObject) }) }
    }, "Header", "Value", "Add header")
    val auth = r.auth ?: collectionAuth
    if (auth != null && auth.type != "none") {
        Text("Auth adds its own header (${auth.type}${if (r.auth == null) ", from the collection" else ""}).", style = HitopType.caption, color = Hitop.textMuted)
    }
}

@Composable
private fun BodyTab(r: Request, edit: ((Request) -> Request) -> Unit, graphql: (@Composable () -> Unit)? = null) {
    val kinds = listOf(BodyKind.JSON, BodyKind.TEXT, BodyKind.FORM, BodyKind.MULTIPART, BodyKind.GRAPHQL, BodyKind.NONE)
    val kind = r.bodyKind
    ChipRow(kinds.map { it.label }, kind.label, { l -> edit { it.withBodyKind(kinds.first { k -> k.label == l }) } })
    when (kind) {
        BodyKind.JSON, BodyKind.TEXT -> {
            CodeEditor(r.body, { v -> edit { it.withBody(v) } }, json = kind == BodyKind.JSON, label = "Body", placeholder = if (kind == BodyKind.JSON) "{ \"key\": \"value\" }" else "Text to send", minHeight = 240)
            if (kind == BodyKind.JSON && r.body.isNotBlank()) {
                val valid = remember(r.body) { JsonOutline.parse(r.body.replace(Regex("\\$\\{[^}]+\\}"), "0")) != null || runCatching { tech.rws8.hitop.core.HitopJson.parseToJsonElement(r.body.replace(Regex("\\$\\{[^}]+\\}"), "0")) }.isSuccess }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(if (valid) "Valid JSON" else "Not valid JSON yet", style = HitopType.caption, color = if (valid) Hitop.success else Hitop.warning)
                    Spacer(Modifier.weight(1f))
                    if (valid) TextButton(onClick = { edit { it.withBody(formatKeepingVariables(it.body)) } }) { Text("Format", color = Hitop.accent) }
                }
            }
        }
        BodyKind.FORM, BodyKind.MULTIPART -> {
            val extras = r.formFields
            KeyValueRows(r.formFields.map { Triple(it.key, it.value, it.enabled) }, { list ->
                edit { it.withFormFields(list.mapIndexed { i, t -> FormRow(t.first, t.second, extras.getOrNull(i)?.type ?: "text", t.third, extras.getOrNull(i)?.extra ?: tech.rws8.hitop.core.EmptyObject) }) }
            }, "Field", "Value", "Add field")
            Text(if (kind == BodyKind.FORM) "Sent as application/x-www-form-urlencoded." else "Sent as multipart/form-data (text fields; files come with a later version).", style = HitopType.caption, color = Hitop.textMuted)
        }
        BodyKind.GRAPHQL -> {
            if (graphql != null) graphql() else CodeEditor(r.body, { v -> edit { it.withBody(v) } }, label = "GraphQL query", placeholder = "query { … }", minHeight = 200)
            SectionLabel("Variables (JSON)")
            CodeEditor(r.graphqlVariables, { v -> edit { it.withGraphql(v, it.graphqlOperationName) } }, json = true, label = "GraphQL variables", placeholder = "{ }", minHeight = 80)
            SectionLabel("Operation name")
            SmallField(r.graphqlOperationName, { v -> edit { it.withGraphql(it.graphqlVariables, v) } }, "Operation name", Modifier.fillMaxWidth())
        }
        BodyKind.NONE -> Text("This request has no body.", style = HitopType.bodySmall, color = Hitop.textMuted)
    }
}

/** Pretty JSON with `${var}` placeholders kept as written (they are not valid JSON on their own). */
fun formatKeepingVariables(body: String): String {
    val vars = ArrayList<String>()
    val stand = Regex("\\$\\{[^}]+\\}").replace(body) { m -> vars.add(m.value); "\"__hitop_var_${vars.size - 1}__\"" }
    val pretty = runCatching { tech.rws8.hitop.core.PrettyJson.encodeToString(kotlinx.serialization.json.JsonElement.serializer(), tech.rws8.hitop.core.HitopJson.parseToJsonElement(stand)) }.getOrNull() ?: return body
    return Regex("\"__hitop_var_(\\d+)__\"").replace(pretty) { m -> vars[m.groupValues[1].toInt()] }
}

@Composable
private fun AuthTab(r: Request, collectionAuth: Auth?, edit: ((Request) -> Request) -> Unit) {
    val labels = linkedMapOf("inherit" to "From collection", "none" to "None", "bearer" to "Bearer token", "basic" to "Basic", "apikey" to "API key")
    val auth = r.auth
    val current = auth?.type ?: "inherit"
    if (auth != null && !auth.editable) {
        Text("${auth.type} auth: kept as it is; edit it in the web or desktop app.", style = HitopType.bodySmall, color = Hitop.textSecondary)
        TextButton(onClick = { edit { it.withAuth(null) } }) { Text("Use the collection's auth instead", color = Hitop.accent) }
        return
    }
    ChipRow(labels.values.toList(), labels[current] ?: current, { l ->
        val type = labels.entries.first { it.value == l }.key
        edit { it.withAuth(if (type == "inherit") null else if (type == it.auth?.type) it.auth else Auth.of(type)) }
    })
    when (current) {
        "inherit" -> Text(collectionAuth?.let { "The collection's auth: ${labels[it.type] ?: it.type}." } ?: "The collection has no auth.", style = HitopType.bodySmall, color = Hitop.textMuted)
        "none" -> Text("No auth is sent, even when the collection has one.", style = HitopType.bodySmall, color = Hitop.textMuted)
        "bearer" -> SmallField(auth!!.field("token"), { v -> edit { it.withAuth(auth.with("token", v)) } }, "Token", Modifier.fillMaxWidth())
        "basic" -> {
            SmallField(auth!!.field("username"), { v -> edit { it.withAuth(auth.with("username", v)) } }, "Username", Modifier.fillMaxWidth())
            SmallField(auth.field("password"), { v -> edit { it.withAuth(auth.with("password", v)) } }, "Password", Modifier.fillMaxWidth())
        }
        "apikey" -> {
            SmallField(auth!!.field("key"), { v -> edit { it.withAuth(auth.with("key", v)) } }, "Key name", Modifier.fillMaxWidth())
            SmallField(auth.field("value"), { v -> edit { it.withAuth(auth.with("value", v)) } }, "Value", Modifier.fillMaxWidth())
            ChipRow(listOf("Header", "Query"), if (auth.field("in") == "query") "Query" else "Header", { l -> edit { it.withAuth(auth.with("in", l.lowercase())) } })
        }
    }
    Text("Write \${name} to use a variable; secrets stay in this phone's keystore.", style = HitopType.caption, color = Hitop.textMuted)
}

@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
private fun CodeSheet(vm: AppViewModel, collectionId: String, r: Request, close: () -> Unit) {
    val langs = listOf("curl", "fetch", "python", "go", "java", "rust")
    var lang by rememberSaveable { mutableStateOf("curl") }
    var code by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    androidx.compose.runtime.LaunchedEffect(lang, r) { code = vm.library.snippet(collectionId, r, lang) }
    androidx.compose.material3.ModalBottomSheet(onDismissRequest = close, containerColor = Hitop.surface) {
        Column(Modifier.padding(16.dp).navigationBarsPadding(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Code", style = HitopType.title, color = Hitop.text)
            ChipRow(langs, lang, { lang = it })
            Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.codeDeep).border(1.dp, Hitop.divider, RoundedCornerShape(16.dp)).heightIn(min = 120.dp, max = 420.dp).verticalScroll(rememberScrollState()).padding(16.dp)) {
                Text(code, style = HitopType.monoSmall, color = Hitop.text, modifier = Modifier.testTag("snippet"))
            }
            Text("Secrets and variables without a value stay as {{name}}.", style = HitopType.caption, color = Hitop.textMuted)
            Button(onClick = { clipboard.setText(AnnotatedString(code)); scope.launch { close() } }, colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent), modifier = Modifier.fillMaxWidth().height(48.dp)) {
                Text("Copy", style = HitopType.button)
            }
        }
    }
}
