package tech.rws8.hitop.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.rws8.hitop.core.HistoryEntry
import tech.rws8.hitop.core.formatDuration
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.ui.theme.MethodTag
import tech.rws8.hitop.ui.theme.statusColor
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale

/** "Today", "Yesterday" or the date, for history groups. */
fun dayLabel(ms: Long, now: Long = System.currentTimeMillis()): String {
    val a = Calendar.getInstance().apply { timeInMillis = ms }
    val b = Calendar.getInstance().apply { timeInMillis = now }
    val days = (b.get(Calendar.YEAR) - a.get(Calendar.YEAR)) * 366 + b.get(Calendar.DAY_OF_YEAR) - a.get(Calendar.DAY_OF_YEAR)
    return when (days) {
        0 -> "Today"
        1 -> "Yesterday"
        else -> SimpleDateFormat("EEEE, d MMMM", Locale.getDefault()).format(Date(ms))
    }
}

@Composable
fun HistoryScreen(vm: AppViewModel) {
    val s by vm.library.state.collectAsState()
    var confirm by remember { mutableStateOf(false) }
    val groups: Map<String, List<HistoryEntry>> = s.history.groupBy { dayLabel(it.timestamp) }
    LazyColumn(Modifier.fillMaxSize().testTag("history"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("History", Modifier.weight(1f).semantics { heading() }, style = HitopType.screenTitle, color = Hitop.text)
                if (s.history.isNotEmpty()) TextButton(onClick = { confirm = true }) { Text("Clear", color = Hitop.accent) }
            }
            Text("Every request sent from this phone, newest first. Tap one to open it as a new request.", style = HitopType.caption, color = Hitop.textMuted)
        }
        if (s.history.isEmpty()) item { Text("Nothing sent yet.", style = HitopType.bodySmall, color = Hitop.textMuted) }
        groups.forEach { (day, entries) ->
            item { SectionLabel(day) }
            item {
                Card {
                    entries.forEach { h -> HistoryRow(h) { vm.openHistory(h.id) } }
                }
            }
        }
    }
    if (confirm) ConfirmDialog("Clear the history?", "Every entry is deleted from this phone.", "Clear", { confirm = false }) { vm.library.clearHistory(); confirm = false }
}

@Composable
private fun HistoryRow(h: HistoryEntry, open: () -> Unit) {
    val locale = androidx.compose.ui.platform.LocalConfiguration.current.locales[0]
    val time = SimpleDateFormat("HH:mm", locale).format(Date(h.timestamp))
    Row(
        Modifier.fillMaxWidth().clickable(onClickLabel = "Open as a new request") { open() }.heightIn(min = 56.dp).padding(horizontal = 16.dp).semantics(mergeDescendants = true) {}.testTag("history-${h.request.name}"),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        MethodTag(h.request.tag)
        Column(Modifier.weight(1f)) {
            Text(h.request.name, style = HitopType.body, color = Hitop.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("${h.collectionName} · $time${h.response?.let { " · " + formatDuration(it.durationMs) } ?: ""}", style = HitopType.caption, color = Hitop.textMuted, maxLines = 1)
        }
        val st = h.response?.status
        Text(st?.toString() ?: "failed", style = HitopType.monoSmall, color = if (st == null) Hitop.error else statusColor(st))
    }
}

