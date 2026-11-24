package tech.rws8.hitop.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import tech.rws8.hitop.core.Collection
import tech.rws8.hitop.core.ImportPlan
import tech.rws8.hitop.core.Importing
import tech.rws8.hitop.core.Request
import tech.rws8.hitop.platform.Incoming
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.R
import tech.rws8.hitop.platform.tr

/** The document at [url], fetched as any request (kept out of the history). */
private suspend fun fetch(vm: AppViewModel, url: String): String {
    val r = vm.library.send("", Request.new(url = url.trim()), history = false)
    r.error?.let { throw IllegalStateException(it.headline) }
    if (r.status !in 200..299) throw IllegalStateException("The address answered ${r.status}.")
    return r.body
}

/** Import: a file (Postman, OpenAPI, HITOP, a collection folder's zip) or an OpenAPI address; what it holds first, then added. */
@Composable
fun ImportSheet(vm: AppViewModel, start: ImportSource?, close: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var url by remember { mutableStateOf("") }
    var plan by remember { mutableStateOf<ImportPlan?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    fun load(make: suspend () -> ImportPlan) {
        busy = true
        error = null
        scope.launch {
            try {
                plan = make()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "This can't be imported."
            } finally {
                busy = false
            }
        }
    }
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) load { withContext(Dispatchers.IO) { Importing.read(Incoming.read(context, uri), Incoming.name(context, uri)) } }
    }
    LaunchedEffect(start) {
        when (start) {
            is ImportSource.File -> load { withContext(Dispatchers.IO) { Importing.read(Incoming.read(context, start.uri), Incoming.name(context, start.uri)) } }
            is ImportSource.Text -> load { withContext(Dispatchers.IO) { Importing.read(start.text) } }
            null -> {}
        }
    }
    val p = plan
    InputSheet(tr(R.string.s_import), close, actions = {
        TextButton(onClick = close) { Text(tr(R.string.s_cancel), color = Hitop.textSecondary) }
        if (p != null) TextButton(onClick = { vm.library.applyImport(p); close() }, enabled = !p.isEmpty, modifier = Modifier.testTag("import-apply")) { Text(tr(R.string.s_import), color = Hitop.accent) }
        else TextButton(onClick = { load { val t = fetch(vm, url); withContext(Dispatchers.IO) { Importing.read(t, url.trim()) } } }, enabled = url.isNotBlank() && !busy, modifier = Modifier.testTag("import-load")) { Text(tr(R.string.s_load), color = Hitop.accent) }
    }) {
        if (p == null) {
            Text(tr(R.string.s_a_postman_collection_or_environment_an_openapi), style = HitopType.caption, color = Hitop.textSecondary)
            OutlinedButton(onClick = { pick.launch(arrayOf("*/*")) }, enabled = !busy, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("import-file")) { Text(tr(R.string.s_choose_a_file), color = Hitop.text) }
            SmallField(url, { url = it }, tr(R.string.s_or_the_address_of_an_openapi_document), Modifier.fillMaxWidth().testTag("import-url"))
            if (busy) Text(tr(R.string.s_reading), style = HitopType.caption, color = Hitop.textMuted)
        } else {
            Column(Modifier.heightIn(max = 360.dp).verticalScroll(rememberScrollState()).testTag("import-plan"), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(p.title, style = HitopType.label, color = Hitop.accent)
                p.collections.forEach { c -> Text("${c.name} · ${c.requests.size} ${if (c.requests.size == 1) tr(R.string.s_request) else tr(R.string.s_requests)}", style = HitopType.body, color = Hitop.text) }
                p.environments.forEach { e -> Text(tr(R.string.s_environment_variables, e.environment.name, e.environment.variables.size + e.environment.secrets.count { it !in e.environment.variables }), style = HitopType.body, color = Hitop.text) }
                p.globals?.let { g -> Text(tr(R.string.s_globals_variables, g.environment.variables.size + g.environment.secrets.count { it !in g.environment.variables }), style = HitopType.body, color = Hitop.text) }
                if (p.isEmpty) Text(tr(R.string.s_there_is_nothing_to_import_in_it), style = HitopType.caption, color = Hitop.warning)
                p.issues.forEach { Text(it, style = HitopType.caption, color = Hitop.warning) }
            }
            Text(tr(R.string.s_it_is_added_nothing_here_is_replaced), style = HitopType.caption, color = Hitop.textMuted)
        }
        error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText, modifier = Modifier.testTag("import-error")) }
    }
}

sealed interface ImportSource {
    data class File(val uri: android.net.Uri) : ImportSource
    data class Text(val text: String) : ImportSource
}

/** A collection as a file for another app. */
@Composable
fun ExportSheet(c: Collection, close: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    fun share(suffix: String, type: String, make: () -> ByteArray) {
        scope.launch {
            try {
                val bytes = withContext(Dispatchers.IO) { make() }
                Incoming.share(context, c.name.ifBlank { "collection" } + suffix, type, bytes, tr(R.string.s_share_2, c.name))
                close()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "The collection can't be exported."
            }
        }
    }
    InputSheet(tr(R.string.s_export_2, c.name), close, actions = { TextButton(onClick = close) { Text(tr(R.string.s_cancel), color = Hitop.textSecondary) } }) {
        Text(tr(R.string.s_environments_and_secret_values_aren_t_included), style = HitopType.caption, color = Hitop.textSecondary)
        OutlinedButton(onClick = { share(".zip", "application/zip") { Importing.exportZip(c) } }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("export-zip")) { Text(tr(R.string.s_collection_folder_zip), color = Hitop.text) }
        OutlinedButton(onClick = { share(".hitop.json", "application/json") { Importing.exportJson(c, "hitop").toByteArray() } }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("export-hitop")) { Text(tr(R.string.s_hitop_json), color = Hitop.text) }
        OutlinedButton(onClick = { share(".postman_collection.json", "application/json") { Importing.exportJson(c, "postman").toByteArray() } }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).testTag("export-postman")) { Text(tr(R.string.s_postman_v2_1), color = Hitop.text) }
        error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText) }
    }
}

/** Sync with OpenAPI: the changes first, then applied; edits made here stay. */
@Composable
fun SpecSyncSheet(vm: AppViewModel, c: Collection, close: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var url by remember { mutableStateOf(Importing.specSource(c).orEmpty()) }
    var text by remember { mutableStateOf<String?>(null) }
    var prune by remember { mutableStateOf(false) }
    var sync by remember { mutableStateOf<Importing.SpecSync?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    fun preview(make: suspend () -> String) {
        scope.launch {
            error = null
            try {
                val t = make()
                text = t
                sync = withContext(Dispatchers.IO) { Importing.syncSpec(c, t, prune) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                error = e.message ?: "The document can't be read."
            }
        }
    }
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) preview { withContext(Dispatchers.IO) { String(Incoming.read(context, uri), Charsets.UTF_8) } }
    }
    val s = sync
    InputSheet(tr(R.string.s_sync_with_openapi_2, c.name), close, actions = {
        TextButton(onClick = close) { Text(tr(R.string.s_cancel), color = Hitop.textSecondary) }
        if (s == null) TextButton(onClick = { preview { fetch(vm, url) } }, enabled = url.isNotBlank(), modifier = Modifier.testTag("spec-preview")) { Text(tr(R.string.s_preview), color = Hitop.accent) }
        else TextButton(onClick = { vm.library.replaceCollection(s.collection); close() }, enabled = !s.empty, modifier = Modifier.testTag("spec-apply")) { Text(tr(R.string.s_apply), color = Hitop.accent) }
    }) {
        if (s == null) {
            SmallField(url, { url = it }, tr(R.string.s_address_of_the_openapi_document), Modifier.fillMaxWidth().testTag("spec-url"))
            OutlinedButton(onClick = { pick.launch(arrayOf("*/*")) }, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(tr(R.string.s_choose_a_file), color = Hitop.text) }
        } else {
            Column(Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState()).testTag("spec-diff"), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                if (s.empty) Text(tr(R.string.s_nothing_to_change), style = HitopType.body, color = Hitop.text)
                s.added.forEach { Text("+ ${it}", style = HitopType.monoSmall, color = Hitop.success) }
                s.changed.forEach { Text("~ ${it}", style = HitopType.monoSmall, color = Hitop.text) }
                s.removed.forEach { Text("− ${it}" + if (prune) "" else tr(R.string.s_stays_here), style = HitopType.monoSmall, color = Hitop.warning) }
                Text(tr(R.string.s_unchanged, s.unchanged), style = HitopType.caption, color = Hitop.textMuted)
            }
            if (s.removed.isNotEmpty()) Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(prune, { prune = it; text?.let { t -> preview { t } } }, modifier = Modifier.testTag("spec-prune"))
                Text(tr(R.string.s_delete_requests_no_longer_in_the_document), style = HitopType.bodySmall, color = Hitop.text)
            }
        }
        error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText) }
    }
}
