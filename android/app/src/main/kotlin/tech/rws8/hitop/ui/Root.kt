package tech.rws8.hitop.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.focusable
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import tech.rws8.hitop.ui.theme.Dot
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType

@Composable
fun HitopRoot(vm: AppViewModel, single: Boolean = false) {
    if (!single) {
        BackHandler(enabled = vm.stack.isNotEmpty()) { vm.pop() }
        TrustPrompt(vm)
        CrashPrompt()
        if (vm.importing) ImportSheet(vm, vm.importSource) { vm.importing = false; vm.importSource = null }
        vm.sharedText?.let { t -> FromTextDialog(vm, t) { vm.sharedText = null } }
        // Focusable at the root so a hardware keyboard's shortcuts arrive before anything has focus.
        val rootFocus = androidx.compose.runtime.remember { androidx.compose.ui.focus.FocusRequester() }
        androidx.compose.runtime.LaunchedEffect(Unit) { runCatching { rootFocus.requestFocus() } }
        androidx.compose.foundation.layout.BoxWithConstraints(Modifier.fillMaxSize().onPreviewKeyEvent { shortcut(vm, it) }.focusRequester(rootFocus).focusable().testTag("root")) {
            val panes = Panes.of(maxWidth)
            if (panes == Panes.ONE) Screens(vm) else WideLayout(vm, panes)
            vm.undo?.let { u -> UndoBar(u, Modifier.align(Alignment.BottomCenter)) { vm.undo = null } }
        }
        return
    }
    Screens(vm)
}

/** After a crash: look at the report, share it, or drop it. */
@Composable
private fun CrashPrompt() {
    val context = androidx.compose.ui.platform.LocalContext.current
    var report by remember { mutableStateOf(tech.rws8.hitop.platform.CrashReport.pending(context)) }
    val text = report ?: return
    fun done() {
        tech.rws8.hitop.platform.CrashReport.dismiss(context)
        report = null
    }
    InputSheet("HITOP closed unexpectedly", { done() }, actions = {
        androidx.compose.material3.TextButton(onClick = { done() }, modifier = Modifier.testTag("crash-dismiss")) { Text("Dismiss", color = Hitop.textSecondary) }
        androidx.compose.material3.TextButton(onClick = {
            runCatching { tech.rws8.hitop.platform.Incoming.share(context, "hitop-crash-report.txt", "text/plain", text.toByteArray(), "Share the crash report") }
            done()
        }, modifier = Modifier.testTag("crash-share")) { Text("Share report", color = Hitop.accent) }
    }) {
        Text("Nothing was sent. The report below has no request, address, variable or secret in it; share it only if you want to.", style = HitopType.caption, color = Hitop.textSecondary)
        Text(text, Modifier.fillMaxWidth().heightIn(max = 240.dp).verticalScroll(androidx.compose.foundation.rememberScrollState()).testTag("crash-text"), style = HitopType.monoSmall, color = Hitop.text)
    }
}

@Composable
private fun UndoBar(u: AppViewModel.Undo, modifier: Modifier, done: () -> Unit) {
    val context = androidx.compose.ui.platform.LocalContext.current
    androidx.compose.runtime.LaunchedEffect(u) {
        // As long as the person's accessibility settings ask for (a screen reader needs more time).
        val ms = context.getSystemService(android.view.accessibility.AccessibilityManager::class.java)
            ?.getRecommendedTimeoutMillis(8_000, android.view.accessibility.AccessibilityManager.FLAG_CONTENT_CONTROLS or android.view.accessibility.AccessibilityManager.FLAG_CONTENT_TEXT) ?: 8_000
        kotlinx.coroutines.delay(ms.toLong())
        done()
    }
    androidx.compose.foundation.layout.Row(
        modifier.navigationBarsPadding().padding(start = 16.dp, end = 16.dp, bottom = 96.dp).clip(androidx.compose.foundation.shape.RoundedCornerShape(16.dp)).background(Hitop.surfaceHigh).padding(start = 16.dp).testTag("undo-bar"),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(u.label, Modifier.weight(1f, fill = false), style = HitopType.bodySmall, color = Hitop.text, maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
        androidx.compose.material3.TextButton(onClick = { u.undo(); done() }, modifier = Modifier.testTag("undo")) { Text("Undo", color = Hitop.accent) }
    }
}

/** Keyboard shortcuts (tablets, ChromeOS, desktop mode): Ctrl+Enter send, Ctrl+S save, Ctrl+K search, Ctrl+N new request. */
private fun shortcut(vm: AppViewModel, e: androidx.compose.ui.input.key.KeyEvent): Boolean {
    if (e.type != androidx.compose.ui.input.key.KeyEventType.KeyDown || !(e.isCtrlPressed || e.isMetaPressed)) return false
    val r = vm.top as? Screen.Request
    return when (e.key) {
        androidx.compose.ui.input.key.Key.Enter -> { r?.let { vm.sendCurrent() }; r != null }
        androidx.compose.ui.input.key.Key.S -> { r?.let { vm.save(it.collectionId, it.requestId) }; r != null }
        androidx.compose.ui.input.key.Key.K -> { vm.showSearch(); true }
        androidx.compose.ui.input.key.Key.N -> { vm.newRequest(); true }
        else -> false
    }
}

@Composable
private fun Screens(vm: AppViewModel) {
    when (val top = vm.top) {
        is Screen.Request -> RequestScreen(vm, top.collectionId, top.requestId)
        is Screen.EnvironmentDetail -> EnvironmentDetailScreen(vm, top.envId)
        Screen.Globals -> GlobalsScreen(vm)
        Screen.Account -> AccountScreen(vm)
        Screen.Settings -> SettingsScreen(vm)
        is Screen.Run -> RunScreen(vm, top.collectionId)
        is Screen.Report -> ReportScreen(vm, top.id)
        null -> Tabs(vm)
    }
}

private val tabIcons = mapOf(Tab.COLLECTIONS to HitopIcons.Folder, Tab.HISTORY to HitopIcons.History, Tab.ENVIRONMENTS to HitopIcons.Environments, Tab.RUNS to HitopIcons.Run)

@Composable
private fun Tabs(vm: AppViewModel) {
    Scaffold(
        containerColor = Hitop.ground,
        bottomBar = {
            NavigationBar(containerColor = Hitop.surface, tonalElevation = 0.dp) {
                Tab.entries.forEach { t ->
                    NavigationBarItem(
                        selected = vm.tab == t,
                        onClick = { vm.tab = t },
                        icon = { Icon(tabIcons.getValue(t), null) },
                        label = { Text(t.label, style = HitopType.navLabel, maxLines = 1) },
                        colors = NavigationBarItemDefaults.colors(
                            selectedIconColor = Hitop.text, selectedTextColor = Hitop.text, indicatorColor = Hitop.selected,
                            unselectedIconColor = Hitop.textMuted, unselectedTextColor = Hitop.textMuted,
                        ),
                        modifier = Modifier.testTag("tab-${t.label}"),
                    )
                }
            }
        },
    ) { padding ->
        Box(Modifier.padding(padding).fillMaxSize()) {
            when (vm.tab) {
                Tab.COLLECTIONS -> CollectionsScreen(vm)
                Tab.HISTORY -> HistoryScreen(vm)
                Tab.ENVIRONMENTS -> EnvironmentsScreen(vm)
                Tab.RUNS -> RunsTab(vm)
            }
        }
    }
}

/** The environment pill: the active environment's dot and name; tap to pick another. */
@Composable
fun EnvironmentPill(vm: AppViewModel, modifier: Modifier = Modifier) {
    val s by vm.library.state.collectAsState()
    var open by remember { mutableStateOf(false) }
    val active = s.activeEnvironment
    Box(modifier) {
        Row(
            Modifier.clip(RoundedCornerShape(18.dp)).background(Hitop.surface)
                .clickable { open = true }
                .semantics { role = Role.Button; contentDescription = "Environment: ${active?.name ?: "none"}" }
                .heightIn(min = 40.dp).padding(horizontal = 14.dp)
                .testTag("environment-pill"),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Dot(if (active == null) Hitop.textMuted else Hitop.accent)
            Text(active?.name ?: "No environment", style = HitopType.label, color = Hitop.text)
            Icon(HitopIcons.ChevronDown, null, tint = Hitop.textMuted, modifier = Modifier.size(14.dp))
        }
        DropdownMenu(open, { open = false }, containerColor = Hitop.surfaceHigh) {
            DropdownMenuItem(text = { Text("No environment", color = Hitop.text) }, onClick = { vm.library.setActiveEnvironment(null); open = false })
            s.environments.forEach { e ->
                DropdownMenuItem(
                    text = { Text(e.name, color = Hitop.text) },
                    leadingIcon = { if (e.id == s.activeEnvironmentId) Icon(HitopIcons.Check, null, tint = Hitop.accent) },
                    onClick = { vm.library.setActiveEnvironment(e.id); open = false },
                    modifier = Modifier.testTag("env-option-${e.name}"),
                )
            }
        }
    }
}
