package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import tech.rws8.hitop.core.AppSettings
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.R
import tech.rws8.hitop.platform.tr

private fun label(ms: Long) = when {
    ms == 0L -> "30 s"
    ms < 120_000 -> "${ms / 1000} s"
    else -> "${ms / 60_000} min"
}

@Composable
fun SettingsScreen(vm: AppViewModel) {
    val s by vm.settings.state.collectAsState()
    var cookies by remember { mutableStateOf(vm.library.cookies()) }
    val certificates by produceState<Int?>(null, s.trustUserCas) { value = if (s.trustUserCas) withContext(Dispatchers.IO) { vm.network.userRoots().size } else null }
    Column(Modifier.fillMaxSize().background(Hitop.ground).statusBarsPadding()) {
        Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { vm.pop() }, modifier = Modifier.testTag("back")) { Icon(HitopIcons.Back, tr(R.string.s_back), tint = Hitop.text) }
            Text(tr(R.string.s_settings), Modifier.weight(1f).semantics { heading() }, style = HitopType.title, color = Hitop.text)
        }
        LazyColumn(Modifier.testTag("settings-list"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { SectionLabel(tr(R.string.s_requests_2)) }
            item {
                Card {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(tr(R.string.s_time_limit_of_one_request), style = HitopType.body, color = Hitop.text)
                        ChipRow(AppSettings.TIMEOUTS.map(::label), label(s.timeoutMs), { l -> vm.settings.update { it.copy(timeoutMs = AppSettings.TIMEOUTS.first { t -> label(t) == l }) } })
                    }
                }
            }
            item { SectionLabel(tr(R.string.s_network)) }
            item {
                Card {
                    Toggle(tr(R.string.s_trust_this_phone_s_certificates), tr(R.string.s_certificate_authorities_you_or_your_organization_installed) + (certificates?.let { tr(R.string.s_installed, it) } ?: ""), s.trustUserCas, "trust-user-cas") { on -> vm.settings.update { it.copy(trustUserCas = on) } }
                    Toggle(tr(R.string.s_use_this_phone_s_proxy), tr(R.string.s_the_proxy_of_the_current_network_for), s.systemProxy, "system-proxy") { on -> vm.settings.update { it.copy(systemProxy = on) } }
                }
            }
            item { SectionLabel(tr(R.string.s_hitop_cloud)) }
            item {
                Card {
                    Toggle(tr(R.string.s_keep_the_library_in_sync), tr(R.string.s_off_collections_environments_and_globals_stay_as), s.librarySync, "library-sync") { on -> vm.settings.update { it.copy(librarySync = on) } }
                    Toggle(tr(R.string.s_sync_in_the_background), tr(R.string.s_about_every_6_hours_while_hitop_is), s.backgroundSync, "background-sync") { on -> vm.settings.update { it.copy(backgroundSync = on) } }
                }
            }
            item {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) { SectionLabel(tr(R.string.s_cookies)) }
                    if (cookies.isNotEmpty()) TextButton(onClick = { vm.library.clearCookies(); cookies = vm.library.cookies() }, modifier = Modifier.testTag("clear-cookies")) { Text(tr(R.string.s_delete_all), color = Hitop.error) }
                }
            }
            item {
                Card(Modifier.testTag("cookies")) {
                    if (cookies.isEmpty()) Text(tr(R.string.s_no_cookies_are_stored), Modifier.padding(16.dp), style = HitopType.bodySmall, color = Hitop.textMuted)
                    cookies.forEach { c ->
                        Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(start = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text(c.name, style = HitopType.mono, color = Hitop.text)
                                Text(c.domain + c.path, style = HitopType.caption, color = Hitop.textMuted)
                            }
                            IconButton(onClick = { vm.library.deleteCookie(c); cookies = vm.library.cookies() }) { Icon(HitopIcons.Trash, tr(R.string.s_delete_cookie, c.name), tint = Hitop.textMuted) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Toggle(title: String, text: String, on: Boolean, tag: String, change: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().toggleable(on, role = Role.Switch, onValueChange = change).padding(16.dp).testTag(tag), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(Modifier.weight(1f)) {
            Text(title, style = HitopType.body, color = Hitop.text)
            Text(text, style = HitopType.caption, color = Hitop.textSecondary)
        }
        Switch(on, null, colors = SwitchDefaults.colors(checkedTrackColor = Hitop.accent, checkedThumbColor = Hitop.onAccent))
    }
}
