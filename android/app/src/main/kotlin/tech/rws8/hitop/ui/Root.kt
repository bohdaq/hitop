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
import tech.rws8.hitop.ui.theme.Dot
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType

@Composable
fun HitopRoot(vm: AppViewModel) {
    BackHandler(enabled = vm.stack.isNotEmpty()) { vm.pop() }
    when (val top = vm.top) {
        is Screen.Request -> RequestScreen(vm, top.collectionId, top.requestId)
        is Screen.EnvironmentDetail -> EnvironmentDetailScreen(vm, top.envId)
        Screen.Globals -> GlobalsScreen(vm)
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
                Tab.RUNS -> RunsPlaceholder()
            }
        }
    }
}

@Composable
private fun RunsPlaceholder() {
    Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text("Runs", style = HitopType.screenTitle, color = Hitop.text)
        Text("Run every request of a collection in order and see a report. Coming in the next version of this app.", style = HitopType.bodySmall, color = Hitop.textMuted)
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
