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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType

/** Colours `${name}` in the accent and, with [json], keys, strings and numbers as the drafts do. */
fun highlight(text: String, json: Boolean): AnnotatedString = buildAnnotatedString {
    append(text)
    if (json) {
        Regex("\"(?:[^\"\\\\]|\\\\.)*\"(\\s*:)?").findAll(text).forEach { m ->
            val isKey = m.groupValues[1].isNotEmpty()
            val end = if (isKey) m.range.first + m.value.trimEnd(':', ' ').length else m.range.last + 1
            addStyle(SpanStyle(color = if (isKey) Hitop.jsonKey else Hitop.jsonString), m.range.first, end)
        }
        Regex("(?<![\\w\"$.{])-?\\d+(\\.\\d+)?([eE][+-]?\\d+)?\\b").findAll(text).forEach { m ->
            addStyle(SpanStyle(color = Hitop.jsonNumber), m.range.first, m.range.last + 1)
        }
    }
    Regex("\\$\\{[^}]+\\}").findAll(text).forEach { m -> addStyle(SpanStyle(color = Hitop.accent), m.range.first, m.range.last + 1) }
}

private class Highlighter(val json: Boolean) : VisualTransformation {
    override fun filter(text: AnnotatedString) = TransformedText(highlight(text.text, json), OffsetMapping.Identity)
}

/** A monospace editor (URL, body, scripts) on the code ground. */
@Composable
fun CodeEditor(
    value: String,
    onChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    json: Boolean = false,
    placeholder: String = "",
    readOnly: Boolean = false,
    singleLine: Boolean = false,
    label: String,
    minHeight: Int = 160,
    ground: Color = Hitop.code,
    style: TextStyle = HitopType.mono,
) {
    val transformation = remember(json) { Highlighter(json) }
    Box(
        modifier.clip(RoundedCornerShape(16.dp)).background(ground).heightIn(min = minHeight.dp).padding(16.dp),
    ) {
        if (value.isEmpty() && placeholder.isNotEmpty()) Text(placeholder, style = style, color = Hitop.textMuted)
        BasicTextField(
            value = value,
            onValueChange = onChange,
            readOnly = readOnly,
            singleLine = singleLine,
            textStyle = style.copy(color = Hitop.text),
            cursorBrush = SolidColor(Hitop.accent),
            visualTransformation = transformation,
            keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, capitalization = KeyboardCapitalization.None, keyboardType = if (singleLine) KeyboardType.Uri else KeyboardType.Ascii),
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = label }.testTag(label),
        )
    }
}

@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(text.uppercase(), modifier.padding(top = 8.dp, bottom = 4.dp), style = HitopType.navLabel, color = Hitop.textMuted)
}

@Composable
fun fieldColors() = OutlinedTextFieldDefaults.colors(
    focusedBorderColor = Hitop.accent,
    unfocusedBorderColor = Hitop.border,
    focusedContainerColor = Hitop.surface,
    unfocusedContainerColor = Hitop.surface,
    cursorColor = Hitop.accent,
    focusedLabelColor = Hitop.accent,
    unfocusedLabelColor = Hitop.textMuted,
)

/** Name/value rows with a switch to send or keep each one (headers, params, form fields). */
@Composable
fun KeyValueRows(
    rows: List<Triple<String, String, Boolean>>,
    onChange: (List<Triple<String, String, Boolean>>) -> Unit,
    keyLabel: String,
    valueLabel: String,
    addLabel: String,
    readOnly: Boolean = false,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        rows.forEachIndexed { i, (k, v, on) ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                Checkbox(
                    checked = on,
                    onCheckedChange = { c -> onChange(rows.toMutableList().also { it[i] = Triple(k, v, c) }) },
                    enabled = !readOnly,
                    colors = CheckboxDefaults.colors(checkedColor = Hitop.accent, checkmarkColor = Hitop.onAccent, uncheckedColor = Hitop.textMuted),
                    modifier = Modifier.semantics { contentDescription = "Send $keyLabel ${k.ifEmpty { i + 1 }}" },
                )
                SmallField(k, { nk -> onChange(rows.toMutableList().also { it[i] = Triple(nk, v, on) }) }, keyLabel, Modifier.weight(1f), readOnly)
                SmallField(v, { nv -> onChange(rows.toMutableList().also { it[i] = Triple(k, nv, on) }) }, valueLabel, Modifier.weight(1.4f), readOnly)
                if (!readOnly) {
                    IconButton(onClick = { onChange(rows.toMutableList().also { it.removeAt(i) }) }) {
                        Icon(HitopIcons.Close, "Remove ${k.ifEmpty { keyLabel }}", tint = Hitop.textMuted, modifier = Modifier.size(18.dp))
                    }
                }
            }
        }
        if (!readOnly) {
            TextButton(onClick = { onChange(rows + Triple("", "", true)) }) {
                Icon(HitopIcons.Plus, null, tint = Hitop.accent, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(6.dp))
                Text(addLabel, style = HitopType.label, color = Hitop.accent)
            }
        }
    }
}

@Composable
fun SmallField(value: String, onChange: (String) -> Unit, label: String, modifier: Modifier = Modifier, readOnly: Boolean = false) {
    Box(modifier.clip(RoundedCornerShape(12.dp)).background(Hitop.surface).border(1.dp, Hitop.border, RoundedCornerShape(12.dp)).heightIn(min = 48.dp).padding(horizontal = 12.dp), contentAlignment = Alignment.CenterStart) {
        if (value.isEmpty()) Text(label, style = HitopType.monoSmall, color = Hitop.textMuted)
        BasicTextField(
            value, onChange, readOnly = readOnly, singleLine = true,
            textStyle = HitopType.mono.copy(color = Hitop.text),
            cursorBrush = SolidColor(Hitop.accent),
            visualTransformation = remember { Highlighter(false) },
            keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, capitalization = KeyboardCapitalization.None),
            modifier = Modifier.fillMaxWidth().semantics { contentDescription = label },
        )
    }
}

/** A row of choice chips that scrolls when it doesn't fit (large text). */
@Composable
fun ChipRow(options: List<String>, selected: String, onSelect: (String) -> Unit, modifier: Modifier = Modifier, colorOf: (String) -> Color? = { null }) {
    Row(modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        options.forEach { o ->
            tech.rws8.hitop.ui.theme.HitopChip(o, o == selected, Modifier.clickable { onSelect(o) }.testTag("chip-$o"), colorOf(o))
        }
    }
}

/** A one-field dialog (rename, new collection, new folder). */
@Composable
fun TextDialog(title: String, initial: String, confirm: String, onDismiss: () -> Unit, onConfirm: (String) -> Unit) {
    var text by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = Hitop.surfaceHigh,
        title = { Text(title, style = HitopType.title, color = Hitop.text) },
        text = {
            OutlinedTextField(text, { text = it }, singleLine = true, colors = fieldColors(), textStyle = HitopType.body.copy(color = Hitop.text), modifier = Modifier.fillMaxWidth().testTag("dialog-field"))
        },
        confirmButton = { TextButton(onClick = { onConfirm(text.trim()) }, enabled = text.isNotBlank()) { Text(confirm, color = Hitop.accent) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel", color = Hitop.textSecondary) } },
    )
}

@Composable
fun ConfirmDialog(title: String, text: String, confirm: String, onDismiss: () -> Unit, onConfirm: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = Hitop.surfaceHigh,
        title = { Text(title, style = HitopType.title, color = Hitop.text) },
        text = { Text(text, style = HitopType.bodySmall, color = Hitop.textSecondary) },
        confirmButton = { TextButton(onClick = onConfirm) { Text(confirm, color = Hitop.error) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel", color = Hitop.textSecondary) } },
    )
}

/** The drafts' rounded section card. */
@Composable
fun Card(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth().clip(RoundedCornerShape(24.dp)).background(Hitop.surface).padding(vertical = 8.dp)) { content() }
}
