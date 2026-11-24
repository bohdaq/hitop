package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import tech.rws8.hitop.core.ComparedResponse
import tech.rws8.hitop.core.ResponseDiff
import tech.rws8.hitop.core.SendResult
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType

/** The current response against an example or an earlier one. */
@Composable
fun CompareSheet(vm: AppViewModel, requestId: String, res: SendResult, close: () -> Unit) {
    val sources = remember(requestId, res) { vm.compareSources(requestId) }
    var with by remember { mutableStateOf(sources.firstOrNull()) }
    InputSheet("Compare response", close, actions = { TextButton(onClick = close) { Text("Done", color = Hitop.accent) } }) {
        ChipRow(sources.map { it.label }.distinct(), with?.label.orEmpty(), { l -> with = sources.first { it.label == l } })
        val w = with ?: return@InputSheet
        val d = remember(w, res) { ResponseDiff.compare(w, ComparedResponse.of("now", res)) }
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 420.dp).testTag("diff"), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            if (d.same) item { Text("The responses are the same.", style = HitopType.body, color = Hitop.text) }
            d.status?.let { s -> item { SectionLabel("Status") }; item { ChangeRow(s) } }
            if (d.headers.isNotEmpty()) { item { SectionLabel("Headers") }; items(d.headers) { ChangeRow(it) } }
            if (d.body.any { it.kind != ResponseDiff.Kind.SAME }) { item { SectionLabel(if (d.json) "Body, by path" else "Body, by line") }; items(d.body) { ChangeRow(it) } }
        }
    }
}

@Composable
private fun ChangeRow(c: ResponseDiff.Change) {
    val (sign, color, word) = when (c.kind) {
        ResponseDiff.Kind.ADDED -> Triple("+", Hitop.success, "added")
        ResponseDiff.Kind.REMOVED -> Triple("−", Hitop.error, "removed")
        ResponseDiff.Kind.CHANGED -> Triple("~", Hitop.warning, "changed")
        ResponseDiff.Kind.SAME -> Triple(" ", Hitop.textMuted, "same")
    }
    fun cut(s: String?) = s.orEmpty().let { if (it.length > 300) it.take(300) + "…" else it }
    val text = when {
        c.path.isEmpty() -> cut(c.after ?: c.before)
        c.kind == ResponseDiff.Kind.CHANGED -> "${c.path}: ${cut(c.before)} → ${cut(c.after)}"
        else -> "${c.path}: ${cut(c.after ?: c.before)}"
    }
    Text("$sign $text", Modifier.fillMaxWidth().background(Hitop.codeDeep).padding(horizontal = 8.dp, vertical = 2.dp).semantics { contentDescription = "$word $text" }, style = HitopType.monoSmall, color = color)
}

/** A request's saved examples: look, rename, delete. */
@Composable
fun ExamplesSheet(vm: AppViewModel, collectionId: String, requestId: String, close: () -> Unit) {
    val r = vm.draft(requestId)
    val readOnly = vm.library.isReadOnly(collectionId)
    var open by remember { mutableStateOf<Int?>(null) }
    fun change(f: (List<JsonObject>) -> List<JsonObject>) = vm.edit(requestId) { q -> f(q.examples.filterIsInstance<JsonObject>()).let { q.set("examples", if (it.isEmpty()) null else JsonArray(it)) } }
    InputSheet("Examples", close, actions = { TextButton(onClick = close) { Text("Done", color = Hitop.accent) } }) {
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = 420.dp).testTag("examples-list")) {
            val list = r?.examples.orEmpty().filterIsInstance<JsonObject>()
            items(list.size) { i ->
                val e = list[i]
                val x = ComparedResponse.ofExample("", e)
                val name = (e["name"] as? JsonPrimitive)?.content ?: "Example ${i + 1}"
                Column {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        TextButton(onClick = { open = if (open == i) null else i }, modifier = Modifier.weight(1f).testTag("example-$i")) { Text("$name · ${x.status ?: "no status"}", Modifier.fillMaxWidth(), color = Hitop.text) }
                        if (!readOnly) IconButton(onClick = { open = null; change { l -> l.filterIndexed { n, _ -> n != i } } }, modifier = Modifier.testTag("delete-example-$i")) { Icon(HitopIcons.Trash, "Delete example $name", tint = Hitop.textMuted) }
                    }
                    if (open == i) {
                        if (!readOnly) SmallField(name, { v -> change { l -> l.mapIndexed { n, o -> if (n == i) JsonObject(o + ("name" to JsonPrimitive(v))) else o } } }, "Name", Modifier.fillMaxWidth())
                        Text(x.body.take(20_000).ifEmpty { "(empty body)" }, Modifier.fillMaxWidth().background(Hitop.codeDeep).padding(8.dp), style = HitopType.monoSmall, color = Hitop.text)
                    }
                }
            }
            if (list.isEmpty()) item { Text("No examples. Save one from a response.", style = HitopType.caption, color = Hitop.textMuted) }
        }
        if (!readOnly) Text("Changes are kept when you save the request.", style = HitopType.caption, color = Hitop.textMuted)
    }
}
