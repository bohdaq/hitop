package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.R
import tech.rws8.hitop.platform.tr

@Composable
fun EnvironmentsScreen(vm: AppViewModel) {
    val s by vm.library.state.collectAsState()
    var dialog by remember { mutableStateOf<(@Composable () -> Unit)?>(null) }
    LazyColumn(Modifier.fillMaxSize().testTag("environments"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Text(tr(R.string.s_environments), style = HitopType.screenTitle, color = Hitop.text, modifier = Modifier.semantics { heading() }.padding(top = 4.dp))
            Text(tr(R.string.s_the_active_one_fills_variables_in_every), style = HitopType.caption, color = Hitop.textMuted)
        }
        item {
            Card {
                val options = listOf(null) + s.environments
                options.forEach { e ->
                    Row(
                        Modifier.fillMaxWidth().selectable(selected = s.activeEnvironmentId == e?.id, role = Role.RadioButton) { vm.library.setActiveEnvironment(e?.id) }
                            .heightIn(min = 56.dp).padding(horizontal = 8.dp).testTag("env-row-${e?.name ?: "none"}"),
                        verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(selected = s.activeEnvironmentId == e?.id, onClick = null, colors = RadioButtonDefaults.colors(selectedColor = Hitop.accent, unselectedColor = Hitop.textMuted), modifier = Modifier.padding(8.dp))
                        Column(Modifier.weight(1f)) {
                            Text(e?.name ?: tr(R.string.s_no_environment), style = HitopType.body, color = Hitop.text)
                            e?.let { Text(tr(R.string.s_variables, if (it.team != null) tr(R.string.s_team) else "", it.variables.size, if (it.secrets.isNotEmpty()) tr(R.string.s_secret, it.secrets.size) else ""), style = HitopType.caption, color = Hitop.textMuted) }
                        }
                        if (e != null) IconButton(onClick = { vm.push(Screen.EnvironmentDetail(e.id)) }, modifier = Modifier.testTag("env-open-${e.name}")) {
                            Icon(HitopIcons.ChevronRight, tr(R.string.s_edit, e.name), tint = Hitop.textSecondary)
                        }
                    }
                }
            }
        }
        item {
            TextButton(onClick = { dialog = { TextDialog(tr(R.string.s_new_environment), "", tr(R.string.s_create), { dialog = null }) { n -> val e = vm.library.addEnvironment(n); dialog = null; vm.push(Screen.EnvironmentDetail(e.id)) } } }) {
                Icon(HitopIcons.Plus, null, tint = Hitop.accent, modifier = Modifier.size(18.dp)); Spacer(Modifier.size(6.dp)); Text(tr(R.string.s_new_environment), color = Hitop.accent, style = HitopType.label)
            }
        }
        item {
            Card(Modifier.clickable { vm.push(Screen.Globals) }.testTag("globals")) {
                Row(Modifier.padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(tr(R.string.s_globals), style = HitopType.cardTitle, color = Hitop.text)
                        Text(tr(R.string.s_below_every_collection_and_environment_variables, s.globals.variables.size), style = HitopType.caption, color = Hitop.textMuted)
                    }
                    Icon(HitopIcons.ChevronRight, null, tint = Hitop.textSecondary)
                }
            }
        }
    }
    dialog?.invoke()
}

/** Variable rows: name, value (masked for secrets, with Show), the Secret switch, remove. */
@Composable
private fun VariableEditor(
    variables: Map<String, String>,
    secrets: List<String>,
    secretValue: (String) -> String?,
    set: (String, String, Boolean) -> Unit,
    remove: (String) -> Unit,
) {
    var adding by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        variables.keys.sorted().forEach { name ->
            val secret = name in secrets
            var shown by remember(name) { mutableStateOf(false) }
            val value = if (secret) secretValue(name).orEmpty() else variables[name].orEmpty()
            Card {
                Column(Modifier.padding(horizontal = 16.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(name, Modifier.weight(1f), style = HitopType.mono, color = Hitop.jsonKey, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(tr(R.string.s_secret_2), style = HitopType.caption, color = Hitop.textMuted)
                        Switch(secret, { on -> set(name, value, on) }, colors = SwitchDefaults.colors(checkedTrackColor = Hitop.accent, checkedThumbColor = Hitop.onAccent), modifier = Modifier.padding(start = 8.dp).testTag("secret-$name"))
                        IconButton(onClick = { remove(name) }) { Icon(HitopIcons.Trash, tr(R.string.s_remove_3, name), tint = Hitop.textMuted, modifier = Modifier.size(18.dp)) }
                    }
                    if (secret && !shown) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(if (value.isEmpty()) tr(R.string.s_no_value) else "••••••", Modifier.weight(1f).testTag("value-$name"), style = HitopType.mono, color = Hitop.textSecondary)
                            TextButton(onClick = { shown = true }, modifier = Modifier.testTag("show-$name")) { Text(tr(R.string.s_show), color = Hitop.accent) }
                        }
                    } else {
                        SmallField(value, { v -> set(name, v, secret) }, tr(R.string.s_value_of, name), Modifier.fillMaxWidth())
                    }
                }
            }
        }
        TextButton(onClick = { adding = true }, modifier = Modifier.testTag("add-variable")) {
            Icon(HitopIcons.Plus, null, tint = Hitop.accent, modifier = Modifier.size(18.dp)); Spacer(Modifier.size(6.dp)); Text(tr(R.string.s_add_variable), color = Hitop.accent, style = HitopType.label)
        }
    }
    if (adding) TextDialog(tr(R.string.s_new_variable), "", tr(R.string.s_add), { adding = false }) { n -> set(n, "", false); adding = false }
}

@Composable
private fun DetailHeader(vm: AppViewModel, title: String, subtitle: String, actions: @Composable () -> Unit = {}) {
    Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = { vm.pop() }) { Icon(HitopIcons.Back, tr(R.string.s_back), tint = Hitop.text) }
        Column(Modifier.weight(1f)) {
            Text(title, style = HitopType.title, color = Hitop.text, modifier = Modifier.semantics { heading() })
            Text(subtitle, style = HitopType.caption, color = Hitop.textMuted)
        }
        actions()
    }
}

@Composable
fun EnvironmentDetailScreen(vm: AppViewModel, envId: String) {
    val s by vm.library.state.collectAsState()
    val e = s.environments.firstOrNull { it.id == envId } ?: run { vm.pop(); return }
    var dialog by remember { mutableStateOf<(@Composable () -> Unit)?>(null) }
    Column(Modifier.fillMaxSize().background(Hitop.ground).statusBarsPadding()) {
        DetailHeader(vm, e.name, if (s.activeEnvironmentId == envId) tr(R.string.s_active) else tr(R.string.s_not_active)) {
            TextButton(onClick = { dialog = { TextDialog(tr(R.string.s_rename_environment), e.name, tr(R.string.s_rename), { dialog = null }) { n -> vm.library.renameEnvironment(envId, n); dialog = null } } }) { Text(tr(R.string.s_rename), color = Hitop.accent) }
            IconButton(onClick = { dialog = { ConfirmDialog(tr(R.string.s_delete_2, e.name), tr(R.string.s_its_variables_and_secret_values_are_deleted), tr(R.string.s_delete), { dialog = null }) { dialog = null; vm.library.deleteEnvironment(envId) } } }) {
                Icon(HitopIcons.Trash, tr(R.string.s_delete_3, e.name), tint = Hitop.error)
            }
        }
        LazyColumn(contentPadding = PaddingValues(16.dp)) {
            val org = vm.cloud.cloud.currentOrg?.takeIf { vm.cloud.cloud.signedIn && !it.personal }
            if (e.team != null || org != null) item {
                Card(Modifier.padding(bottom = 12.dp)) {
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(tr(R.string.s_shared_with, vm.cloud.cloud.settings.value.orgs.firstOrNull { it.id == (e.team ?: org?.id) }?.name ?: tr(R.string.s_the_organization)), style = HitopType.body, color = Hitop.text)
                            Text(tr(R.string.s_its_members_see_and_edit_the_variables) + (e.json["updatedBy"] as? kotlinx.serialization.json.JsonPrimitive)?.let { tr(R.string.s_last_changed_by, it.content) }.orEmpty(), style = HitopType.caption, color = Hitop.textMuted)
                        }
                        Switch(e.team != null, { on -> vm.library.setTeam(envId, if (on) org?.id else null) }, colors = SwitchDefaults.colors(checkedTrackColor = Hitop.accent, checkedThumbColor = Hitop.onAccent), modifier = Modifier.testTag("team-switch"))
                    }
                }
            }
            item {
                VariableEditor(e.variables, e.secrets, { vm.library.secretValue(envId, it) }, { n, v, sec -> vm.library.setVariable(envId, n, v, sec) }, { vm.library.removeVariable(envId, it) })
            }
        }
    }
    dialog?.invoke()
}

@Composable
fun GlobalsScreen(vm: AppViewModel) {
    val s by vm.library.state.collectAsState()
    Column(Modifier.fillMaxSize().background(Hitop.ground).statusBarsPadding()) {
        DetailHeader(vm, tr(R.string.s_globals), tr(R.string.s_below_every_collection_and_environment))
        LazyColumn(contentPadding = PaddingValues(16.dp)) {
            item {
                VariableEditor(s.globals.variables, s.globals.secrets, { vm.library.globalSecretValue(it) }, { n, v, sec -> vm.library.setGlobal(n, v, sec) }, { vm.library.removeGlobal(it) })
            }
        }
    }
}


