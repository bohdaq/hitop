package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.NavigationRailItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.ui.theme.LogoMark
import tech.rws8.hitop.R
import tech.rws8.hitop.platform.tr

/**
 * How wide the window is: phone (one pane), foldable and small tablet (list + request, the response
 * as a sheet), large tablet (list + request + response). Three panes start at Material's "large"
 * width (1200 dp): below it the request pane got too narrow to use (≈170 dp at 960 dp).
 */
enum class Panes { ONE, TWO, THREE;
    companion object {
        fun of(width: Dp) = when { width >= 1200.dp -> THREE; width >= 600.dp -> TWO; else -> ONE }
    }
}

/** A tab's list drawn as the wide layout's list pane: the rail has New request, the request pane the environment. */
val LocalInListPane = compositionLocalOf { false }

/** The response shows in its own pane (tablet), not as a sheet. */
val LocalResponsePane = compositionLocalOf { false }

private val railIcons = mapOf(Tab.COLLECTIONS to HitopIcons.Folder, Tab.HISTORY to HitopIcons.History, Tab.ENVIRONMENTS to HitopIcons.Environments, Tab.RUNS to HitopIcons.Run)

/** Tablet and unfolded foldable (the drafts' "Tablet / foldable"): rail, the tab's list, the request, its response. */
@Composable
fun WideLayout(vm: AppViewModel, panes: Panes) {
    val top = vm.top
    val request = top as? Screen.Request
    Row(Modifier.fillMaxSize().background(Hitop.ground).testTag("wide-$panes")) {
        NavigationRail(containerColor = Hitop.surface, modifier = Modifier.width(88.dp).statusBarsPadding(), header = {
            LogoMark(44)
            FloatingActionButton(onClick = { vm.newRequest() }, containerColor = Hitop.accent, contentColor = Hitop.onAccent, shape = RoundedCornerShape(16.dp), modifier = Modifier.padding(top = 12.dp).testTag("rail-new-request")) {
                Icon(HitopIcons.Plus, tr(R.string.s_new_request))
            }
        }) {
            Tab.entries.forEach { t ->
                NavigationRailItem(
                    selected = vm.tab == t && top !is Screen.Account,
                    onClick = { vm.tab = t; if (top != null && top !is Screen.Request) vm.pop() },
                    icon = { Icon(railIcons.getValue(t), null) },
                    label = { Text(if (t == Tab.ENVIRONMENTS) tr(R.string.s_envs) else t.label, style = HitopType.navLabel) },
                    colors = NavigationRailItemDefaults.colors(selectedIconColor = Hitop.text, selectedTextColor = Hitop.text, indicatorColor = Hitop.selected, unselectedIconColor = Hitop.textMuted, unselectedTextColor = Hitop.textMuted),
                    modifier = Modifier.testTag("rail-${t.label}"))
            }
        }
        // A full screen of its own (account, environment, run, report) takes the panes' place.
        if (top != null && top !is Screen.Request) {
            Box(Modifier.weight(1f)) { HitopRoot(vm, single = true) }
            return@Row
        }
        Box(Modifier.width(320.dp).fillMaxHeight().statusBarsPadding().testTag("pane-list")) {
            androidx.compose.runtime.CompositionLocalProvider(LocalInListPane provides true) {
                when (vm.tab) {
                    Tab.COLLECTIONS -> CollectionsScreen(vm)
                    Tab.HISTORY -> HistoryScreen(vm)
                    Tab.ENVIRONMENTS -> EnvironmentsScreen(vm)
                    Tab.RUNS -> RunsTab(vm)
                }
            }
        }
        Box(Modifier.width(1.dp).fillMaxHeight().background(Hitop.dividerSoft))
        Box(Modifier.weight(1f).fillMaxHeight().testTag("pane-request")) {
            if (request == null) EmptyPane("Pick a request, or make a new one (Ctrl+N).")
            else androidx.compose.runtime.CompositionLocalProvider(LocalResponsePane provides (panes == Panes.THREE)) { RequestScreen(vm, request.collectionId, request.requestId) }
        }
        if (panes == Panes.THREE) {
            Box(Modifier.width(1.dp).fillMaxHeight().background(Hitop.dividerSoft))
            Box(Modifier.width(380.dp).fillMaxHeight().background(Hitop.pane).statusBarsPadding().testTag("pane-response")) {
                val res = request?.let { vm.responses[it.requestId] }
                if (request == null || res == null) EmptyPane("The response shows here (Ctrl+Enter sends).")
                else ResponseContent(vm, request.collectionId, request.requestId, res, close = null, modifier = Modifier.fillMaxSize().padding(top = 8.dp))
            }
        }
    }
}

@Composable
private fun EmptyPane(text: String) {
    Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
        Icon(HitopIcons.Send, null, tint = Hitop.textMuted, modifier = Modifier.size(28.dp))
        Text(text, style = HitopType.bodySmall, color = Hitop.textMuted, modifier = Modifier.padding(top = 12.dp))
    }
}
