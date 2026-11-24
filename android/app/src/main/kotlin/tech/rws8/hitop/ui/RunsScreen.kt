package tech.rws8.hitop.ui

import android.content.Intent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import kotlinx.coroutines.launch
import tech.rws8.hitop.cloud.LibrarySync
import tech.rws8.hitop.core.RunProgress
import tech.rws8.hitop.core.RunRecord
import tech.rws8.hitop.core.RunReport
import tech.rws8.hitop.core.formatDuration
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.ui.theme.MethodTag
import java.io.File

/** The Runs tab: the run going on, recent runs, run a collection. */
@Composable
fun RunsTab(vm: AppViewModel) {
    val s by vm.library.state.collectAsState()
    val history by vm.runs.history.collectAsState()
    val current by vm.runs.current.collectAsState()
    var confirm by remember { mutableStateOf(false) }
    LazyColumn(Modifier.fillMaxSize().testTag("runs"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Runs", Modifier.weight(1f).semantics { heading() }, style = HitopType.screenTitle, color = Hitop.text)
                if (history.isNotEmpty()) TextButton(onClick = { confirm = true }) { Text("Clear", color = Hitop.accent) }
            }
            Text("Every request of a collection in order, as hitop run does, with a report.", style = HitopType.caption, color = Hitop.textMuted)
        }
        current?.takeIf { it.finished == null }?.let { p ->
            item {
                Card(Modifier.clickable { vm.push(Screen.Run(p.collectionId)) }) {
                    Text("Running ${p.collectionName} · ${p.done} of ${p.total}", Modifier.padding(16.dp), style = HitopType.body, color = Hitop.text)
                }
            }
        }
        item { SectionLabel("Run a collection") }
        item {
            Card {
                s.collections.filter { c -> c.requests.any { !it.isWebSocket && !it.isGrpc } }.forEach { c ->
                    Row(Modifier.fillMaxWidth().clickable { vm.push(Screen.Run(c.id)) }.heightIn(min = 52.dp).padding(horizontal = 16.dp).testTag("run-${c.name}"), verticalAlignment = Alignment.CenterVertically) {
                        Text(c.name, Modifier.weight(1f), style = HitopType.body, color = Hitop.text)
                        Icon(HitopIcons.Run, "Run ${c.name}", tint = Hitop.accent, modifier = Modifier.size(18.dp))
                    }
                }
            }
        }
        if (history.isNotEmpty()) item { SectionLabel("Recent runs") }
        items(history, key = { it.id }) { r ->
            Card(Modifier.clickable { vm.push(Screen.Report(r.id)) }.testTag("history-run-${r.collectionName}")) {
                Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(r.collectionName, style = HitopType.body, color = Hitop.text)
                        Text(listOfNotNull(r.environment, LibrarySync.ago(r.startedAt), formatDuration(r.durationMs), if (r.cancelled) "stopped" else null).joinToString(" · "), style = HitopType.caption, color = Hitop.textMuted)
                    }
                    Text("${r.passed}/${r.total}", style = HitopType.mono, color = if (r.ok) Hitop.success else Hitop.warning)
                }
            }
        }
    }
    if (confirm) ConfirmDialog("Clear the run history?", "Every run's report is deleted from this phone.", "Clear", { confirm = false }) { vm.runs.clear(); confirm = false }
}

/** A collection's run (drafts' "Collection run"): options, live progress, the result. */
@Composable
fun RunScreen(vm: AppViewModel, collectionId: String) {
    val s by vm.library.state.collectAsState()
    val current by vm.runs.current.collectAsState()
    val c = s.collection(collectionId) ?: run { vm.pop(); return }
    var bail by remember { mutableStateOf(false) }
    var iterations by remember { mutableStateOf(1) }
    val context = LocalContext.current
    // The run's progress notification needs the permission (Android 13+); the run goes either way.
    val ask = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.RequestPermission()) { }
    var data by remember { mutableStateOf<tech.rws8.hitop.core.RunData?>(null) }
    var dataError by remember { mutableStateOf<String?>(null) }
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    val schedules by vm.runs.schedules.collectAsState()
    val pickData = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) scope.launch {
            dataError = null
            try {
                data = kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { tech.rws8.hitop.core.RunData.read(tech.rws8.hitop.platform.Incoming.name(context, uri), String(tech.rws8.hitop.platform.Incoming.read(context, uri), Charsets.UTF_8)) }
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (e: Exception) {
                dataError = e.message ?: "This file can't be read."
            }
        }
    }
    fun askPermission() {
        if (android.os.Build.VERSION.SDK_INT >= 33 && androidx.core.content.ContextCompat.checkSelfPermission(context, android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED && !tech.rws8.hitop.platform.AppGraph.runsInProcess) {
            ask.launch(android.Manifest.permission.POST_NOTIFICATIONS)
        }
    }
    fun start() {
        askPermission()
        vm.startRun(context, collectionId, iterations, bail, data)
    }
    val p = current?.takeIf { it.collectionId == collectionId }
    Column(Modifier.fillMaxSize().background(Hitop.ground).statusBarsPadding()) {
        Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { vm.pop() }, modifier = Modifier.testTag("back")) { Icon(HitopIcons.Close, "Close", tint = Hitop.text) }
            Column(Modifier.weight(1f)) {
                Text("Run ${c.name}", style = HitopType.title, color = Hitop.text, modifier = Modifier.semantics { heading() })
                Text("${s.activeEnvironment?.name ?: "No environment"} · $iterations ${if (iterations == 1) "iteration" else "iterations"}", style = HitopType.caption, color = Hitop.textMuted)
            }
            if (p?.finished != null) IconButton(onClick = { start() }, modifier = Modifier.testTag("run-again")) { Icon(HitopIcons.Refresh, "Run again", tint = Hitop.text) }
        }
        if (p == null) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Card {
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("Stop at the first failure", Modifier.weight(1f), style = HitopType.body, color = Hitop.text)
                        Switch(bail, { bail = it }, colors = SwitchDefaults.colors(checkedTrackColor = Hitop.accent, checkedThumbColor = Hitop.onAccent), modifier = Modifier.testTag("bail"))
                    }
                    if (data == null) Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("Iterations", Modifier.weight(1f), style = HitopType.body, color = Hitop.text)
                        TextButton(onClick = { if (iterations > 1) iterations-- }) { Text("−", color = Hitop.accent, style = HitopType.title) }
                        Text("$iterations", style = HitopType.mono, color = Hitop.text)
                        TextButton(onClick = { if (iterations < 20) iterations++ }) { Text("+", color = Hitop.accent, style = HitopType.title) }
                    }
                }
                Card {
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(data?.name ?: "Data file", style = HitopType.body, color = Hitop.text)
                            Text(data?.let { "${it.rows} rows: ${it.columns.joinToString()}" } ?: "CSV or JSON: one iteration per row, its values as variables.", style = HitopType.caption, color = Hitop.textMuted)
                            dataError?.let { Text(it, style = HitopType.caption, color = Hitop.errorText) }
                        }
                        TextButton(onClick = { if (data == null) pickData.launch(arrayOf("*/*")) else data = null }, modifier = Modifier.testTag("run-data")) { Text(if (data == null) "Choose" else "Remove", color = Hitop.accent) }
                    }
                }
                Card {
                    val mine = schedules.firstOrNull { it.collectionId == collectionId }
                    Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("Run on a schedule", style = HitopType.body, color = Hitop.text)
                        val names = linkedMapOf(0L to "Off", 15L to "15 min", 60L to "Hour", 360L to "6 hours", 1440L to "Day")
                        ChipRow(names.values.toList(), names[mine?.everyMinutes ?: 0L] ?: "${mine?.everyMinutes} min", { l ->
                            val every = names.entries.first { it.value == l }.key
                            if (every > 0) askPermission()
                            vm.runs.setSchedule(collectionId, if (every == 0L) null else tech.rws8.hitop.core.RunSchedule(collectionId, s.activeEnvironmentId, every))
                        })
                        Text(if (mine == null) "Runs in the background at about these intervals; Android decides when. You are told when a run fails." else "With ${s.environments.firstOrNull { it.id == mine.environmentId }?.name ?: "no environment"}, about every ${names[mine.everyMinutes]?.lowercase() ?: "${mine.everyMinutes} min"}. A run over about 10 minutes is stopped.", style = HitopType.caption, color = Hitop.textMuted, modifier = Modifier.testTag("schedule-text"))
                    }
                }
                Text("Every request in order with ${s.activeEnvironment?.name ?: "no environment"}; WebSocket and gRPC requests are left out. It runs on with the app closed, its progress in a notification.", style = HitopType.caption, color = Hitop.textMuted)
                Button(onClick = { start() }, colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent), modifier = Modifier.fillMaxWidth().height(56.dp).testTag("start-run"), shape = RoundedCornerShape(28.dp)) {
                    Icon(HitopIcons.Run, null, Modifier.size(18.dp)); Spacer(Modifier.width(8.dp)); Text("Run", style = HitopType.button)
                }
            }
            return@Column
        }
        RunResult(vm, p, Modifier.weight(1f))
        val f = p.finished
        Row(Modifier.navigationBarsPadding().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            if (f == null) {
                OutlinedButton(onClick = { vm.runs.cancel() }, modifier = Modifier.weight(1f).height(56.dp).testTag("stop-run"), shape = RoundedCornerShape(28.dp)) { Text("Stop", color = Hitop.text) }
            } else {
                OutlinedButton(onClick = { shareReport(context, f) }, modifier = Modifier.weight(1f).height(56.dp).testTag("share-report"), shape = RoundedCornerShape(28.dp)) { Text("Share report", color = Hitop.text) }
                Button(onClick = { vm.push(Screen.Report(f.id)) }, colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent), modifier = Modifier.weight(1f).height(56.dp).testTag("open-report"), shape = RoundedCornerShape(28.dp)) { Text("Open report", style = HitopType.button) }
            }
        }
    }
}

@Composable
private fun RunResult(vm: AppViewModel, p: RunProgress, modifier: Modifier) {
    val total = p.total.coerceAtLeast(1)
    val ok = p.failed == 0
    val ring = if (p.finished == null) Hitop.accent else if (ok) Hitop.success else Hitop.warning
    val checks = p.steps.flatMap { it.tests }
    val durations = p.steps.map { it.durationMs }.sorted()
    val p95 = durations.getOrNull(((durations.size * 0.95).toInt()).coerceAtMost(durations.size - 1).coerceAtLeast(0))
    LazyColumn(modifier.testTag("run-result"), contentPadding = PaddingValues(horizontal = 16.dp)) {
        item {
            Row(Modifier.padding(vertical = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                Box(Modifier.size(120.dp).semantics { contentDescription = "${p.passed} of ${p.done} passed" }, contentAlignment = Alignment.Center) {
                    Canvas(Modifier.size(120.dp)) {
                        val w = 12.dp.toPx()
                        drawArc(Hitop.divider, 0f, 360f, false, topLeft = Offset(w / 2, w / 2), size = Size(size.width - w, size.height - w), style = Stroke(w))
                        drawArc(ring, -90f, 360f * p.done / total, false, topLeft = Offset(w / 2, w / 2), size = Size(size.width - w, size.height - w), style = Stroke(w, cap = StrokeCap.Round))
                    }
                    Text("${p.passed}/${if (p.finished == null) p.total else p.done}", style = HitopType.screenTitle, color = Hitop.text, modifier = Modifier.testTag("run-count"))
                }
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Stat("Checks", "${checks.count { it.pass }}/${checks.size}")
                    Stat("p95", p95?.let { "$it ms" } ?: "—")
                    Stat("Total", formatDuration(p.finished?.durationMs ?: (System.currentTimeMillis() - p.startedAt)))
                }
            }
        }
        p.steps.filter { !it.passed }.take(3).forEach { f ->
            item {
                Row(Modifier.fillMaxWidth().padding(bottom = 12.dp).clip(RoundedCornerShape(16.dp)).background(Hitop.errorContainer).padding(14.dp).testTag("failure-card"), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    MethodTag(f.method)
                    Column(Modifier.weight(1f)) {
                        Text(f.name, style = HitopType.cardTitle, color = Hitop.text)
                        Text((f.status?.let { "HTTP $it" } ?: f.error ?: "failed") + (f.tests.firstOrNull { !it.pass }?.let { " · ${it.name}" } ?: ""), style = HitopType.caption, color = Hitop.errorText, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
        item {
            Card {
                p.steps.forEach { st ->
                    Row(Modifier.fillMaxWidth().heightIn(min = 50.dp).padding(horizontal = 16.dp).semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(if (st.passed) "✓" else "✕", style = HitopType.cardTitle, color = if (st.passed) Hitop.success else Hitop.error, modifier = Modifier.width(20.dp))
                        MethodTag(st.method)
                        Text(st.name, Modifier.weight(1f), style = HitopType.body, color = Hitop.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(if (st.passed || st.status == null) "${st.durationMs} ms" else "${st.status}", style = HitopType.monoSmall, color = Hitop.textMuted)
                    }
                }
            }
        }
    }
}

@Composable
private fun Stat(label: String, value: String) {
    Row { Text(label, Modifier.weight(1f), style = HitopType.bodySmall, color = Hitop.textMuted); Text(value, style = HitopType.mono, color = Hitop.text) }
}

/** report.json as a file to share (FileProvider, cache/reports). */
fun shareReport(context: android.content.Context, r: RunRecord) {
    val dir = File(context.cacheDir, "reports").apply { mkdirs() }
    val f = File(dir, "report-${r.collectionName.replace(Regex("[^A-Za-z0-9-]"), "-")}.json").apply { writeText(r.reportJson) }
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", f)
    val send = Intent(Intent.ACTION_SEND).setType("application/json").putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    context.startActivity(Intent.createChooser(send, "Share the report"))
}

/** A run's report: per iteration, each request with status and time; failures open with their checks. */
@Composable
fun ReportScreen(vm: AppViewModel, id: Long) {
    val history by vm.runs.history.collectAsState()
    val r = history.firstOrNull { it.id == id } ?: run { vm.pop(); return }
    val report = remember(r.reportJson) { RunReport.parse(r.reportJson) }
    Column(Modifier.fillMaxSize().background(Hitop.ground).statusBarsPadding()) {
        Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { vm.pop() }, modifier = Modifier.testTag("back")) { Icon(HitopIcons.Back, "Back", tint = Hitop.text) }
            Column(Modifier.weight(1f)) {
                Text("Report: ${r.collectionName}", style = HitopType.title, color = Hitop.text, modifier = Modifier.semantics { heading() })
                Text("${r.passed}/${r.total} requests passed${r.environment?.let { " · $it" } ?: ""}${if (r.bailed) " · stopped at the first failure" else ""}", style = HitopType.caption, color = Hitop.textMuted)
            }
        }
        LazyColumn(Modifier.testTag("report"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            report?.iterations?.forEachIndexed { i, reqs ->
                item { SectionLabel("Iteration ${i + 1}") }
                item {
                    Card {
                        reqs.forEach { q ->
                            var open by remember { mutableStateOf(!q.passed) }
                            Column(Modifier.fillMaxWidth().clickable { open = !open }.padding(horizontal = 16.dp, vertical = 8.dp)) {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                    Text(if (q.passed) "✓" else "✕", style = HitopType.cardTitle, color = if (q.passed) Hitop.success else Hitop.error)
                                    MethodTag(q.method)
                                    Text(q.name, Modifier.weight(1f), style = HitopType.body, color = Hitop.text)
                                    Text(q.status?.let { "HTTP $it" } ?: "—", style = HitopType.monoSmall, color = Hitop.textMuted)
                                }
                                if (open) {
                                    Text(q.url, style = HitopType.monoSmall, color = Hitop.textSecondary)
                                    q.error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText) }
                                    q.assertions.forEach { a -> Text((if (a.pass) "✓ " else "✕ ") + a.name + if (!a.pass && a.detail.isNotEmpty()) ": ${a.detail}" else "", style = HitopType.caption, color = if (a.pass) Hitop.textSecondary else Hitop.errorText) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
