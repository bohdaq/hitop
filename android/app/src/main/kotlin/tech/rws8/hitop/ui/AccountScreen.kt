package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.RadioButtonDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import tech.rws8.hitop.cloud.AuthMethods
import tech.rws8.hitop.cloud.CloudSettings
import tech.rws8.hitop.cloud.LibrarySync
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.ui.theme.LogoMark
import tech.rws8.hitop.R
import tech.rws8.hitop.platform.tr

@Composable
fun AccountScreen(vm: AppViewModel) {
    val m = vm.cloud
    val settings by m.cloud.settings.collectAsState()
    val status by m.sync.status.collectAsState()
    val notices by m.sync.notices.collectAsState()
    var dialog by remember { mutableStateOf<(@Composable () -> Unit)?>(null) }
    var joining by remember { mutableStateOf(false) }
    val joinScope = androidx.compose.runtime.rememberCoroutineScope()
    val account = settings.account
    LaunchedEffect(account?.userId) { if (account != null) m.refresh() else m.loadMethods() }

    Column(Modifier.fillMaxSize().background(Hitop.ground).statusBarsPadding()) {
        Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { vm.pop() }, modifier = Modifier.testTag("back")) { Icon(HitopIcons.Back, tr(R.string.s_back), tint = Hitop.text) }
            Text(tr(R.string.s_account), Modifier.weight(1f).semantics { heading() }, style = HitopType.title, color = Hitop.text)
            if (m.busy) CircularProgressIndicator(Modifier.size(20.dp).padding(end = 8.dp), color = Hitop.accent, strokeWidth = 2.dp)
        }
        LazyColumn(Modifier.testTag("account"), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            m.error?.let { e -> item { Notice(e, error = true) } }
            if (account == null) {
                item {
                    Card {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            LogoMark(48)
                            Text(tr(R.string.s_hitop_cloud), style = HitopType.cardTitle, color = Hitop.text)
                            Text(tr(R.string.s_sign_in_to_keep_your_collections_environments), style = HitopType.bodySmall, color = Hitop.textSecondary)
                            Button(onClick = { m.showSignIn = true }, colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent), modifier = Modifier.fillMaxWidth().height(48.dp).testTag("sign-in")) {
                                Text(tr(R.string.s_sign_in), style = HitopType.button)
                            }
                        }
                    }
                }
            } else {
                item {
                    Card {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(account.email, style = HitopType.cardTitle, color = Hitop.text, modifier = Modifier.testTag("account-email"))
                            Text(if (account.server == CloudSettings.DEFAULT_SERVER) tr(R.string.s_hitop_cloud) else account.server, style = HitopType.caption, color = Hitop.textMuted)
                            val sync = when {
                                status.saving -> "Saving…"
                                status.pulling -> "Checking for changes…"
                                status.error != null -> status.error!!
                                status.lastSync != null -> "Synced ${LibrarySync.ago(status.lastSync!!)}"
                                else -> "Not synced yet"
                            }
                            Text(sync, style = HitopType.bodySmall, color = if (status.error != null) Hitop.warning else Hitop.textSecondary, modifier = Modifier.testTag("sync-status"))
                            if (status.secretsStored == false) Text(tr(R.string.s_this_server_doesn_t_keep_secret_values), style = HitopType.caption, color = Hitop.textMuted)
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                OutlinedButton(onClick = { m.syncNow() }, modifier = Modifier.testTag("sync-now")) { Text(tr(R.string.s_sync_now), color = Hitop.text) }
                            }
                        }
                    }
                }
                notices.forEach { n -> item { Notice(n, error = false) { m.sync.dismissNotice(n) } } }
                item { SectionLabel(tr(R.string.s_shared_collections)) }
                item {
                    Card(Modifier.testTag("shared-card")) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(tr(R.string.s_this_device_s_fingerprint), style = HitopType.caption, color = Hitop.textMuted)
                            Text(vm.shared.fingerprint?.let(tech.rws8.hitop.sync.SyncCore::grouped) ?: "—", style = HitopType.monoSmall, color = Hitop.text, modifier = Modifier.testTag("fingerprint"))
                            Text(tr(R.string.s_others_compare_it_before_they_share_a), style = HitopType.caption, color = Hitop.textMuted)
                            val joinable by vm.shared.workspaces.collectAsState()
                            vm.shared.joinable.forEach { w ->
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text(tr(R.string.s_shared_with_you, w.orgName?.takeIf { !w.orgPersonal }?.let { " · ${it}" } ?: ""), Modifier.weight(1f), style = HitopType.bodySmall, color = Hitop.text)
                                    TextButton(onClick = { joinScope.launch { runCatching { vm.shared.join(w.id) }.onFailure { m.error = it.message } } }) { Text(tr(R.string.s_open_here), color = Hitop.accent) }
                                }
                            }
                            if (joinable.isEmpty()) Unit
                            OutlinedButton(onClick = { joining = true }, modifier = Modifier.testTag("join")) { Text(tr(R.string.s_join_with_invite_code), color = Hitop.text) }
                        }
                    }
                }
                item { SectionLabel(tr(R.string.s_devices)) }
                item {
                    Card(Modifier.testTag("devices")) {
                        m.devices.forEach { d ->
                            Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                                Column(Modifier.weight(1f)) {
                                    Text(d.name + if (d.thisDevice) tr(R.string.s_this_phone) else "", style = HitopType.body, color = Hitop.text)
                                    Text(listOfNotNull(d.lastUsedAt?.let { tr(R.string.s_used, LibrarySync.ago(it * 1000)) }, if (d.workspaces > 0) tr(R.string.s_keys_to_shared, d.workspaces) else null).joinToString(" · ").ifEmpty { " " }, style = HitopType.caption, color = Hitop.textMuted)
                                }
                                if (d.thisDevice) {
                                    TextButton(onClick = { dialog = { TextDialog(tr(R.string.s_rename_this_device), d.name, tr(R.string.s_rename), { dialog = null }) { n -> m.renameDevice(d.id, n); dialog = null } } }) { Text(tr(R.string.s_rename), color = Hitop.accent) }
                                } else {
                                    TextButton(onClick = {
                                        dialog = { ConfirmDialog(tr(R.string.s_remove, d.name), if (d.workspaces > 0) tr(R.string.s_it_holds_keys_to_shared_collections_removing) else tr(R.string.s_it_is_signed_out_and_can_t), tr(R.string.s_remove_2), { dialog = null }) { m.removeDevice(d.id, d.workspaces > 0); dialog = null } }
                                    }) { Text(tr(R.string.s_remove_2), color = Hitop.error) }
                                }
                            }
                        }
                        if (m.devices.isEmpty()) Text(tr(R.string.s_loading), Modifier.padding(16.dp), style = HitopType.caption, color = Hitop.textMuted)
                    }
                }
                if (settings.orgs.size > 1) {
                    item { SectionLabel(tr(R.string.s_organization)) }
                    item {
                        Card(Modifier.testTag("orgs")) {
                            settings.orgs.forEach { o ->
                                val selected = (m.cloud.currentOrg?.id == o.id)
                                Row(Modifier.fillMaxWidth().selectable(selected, role = Role.RadioButton) { m.selectOrg(o.id) }.heightIn(min = 52.dp).padding(horizontal = 8.dp).testTag("org-${o.name}"), verticalAlignment = Alignment.CenterVertically) {
                                    RadioButton(selected, null, colors = RadioButtonDefaults.colors(selectedColor = Hitop.accent), modifier = Modifier.padding(8.dp))
                                    Text(if (o.personal) tr(R.string.s_personal) else o.name, style = HitopType.body, color = Hitop.text)
                                }
                            }
                        }
                    }
                }
                m.usage?.let { u ->
                    item { SectionLabel(tr(R.string.s_plan_usage)) }
                    item {
                        Card(Modifier.testTag("usage")) {
                            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                                Text(if (u.ended) tr(R.string.s_this_organization_s_plan_has_ended) else tr(R.string.s_plan, u.planName), style = HitopType.cardTitle, color = Hitop.text)
                                u.meters.forEach { meter ->
                                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                        Row { Text(meter.label, Modifier.weight(1f), style = HitopType.bodySmall, color = Hitop.textSecondary); Text(meter.text, style = HitopType.monoSmall, color = Hitop.text) }
                                        if (meter.max != null) LinearProgressIndicator({ meter.fraction }, Modifier.fillMaxWidth().clip(RoundedCornerShape(3.dp)), color = if (meter.level == "full") Hitop.error else if (meter.level == "warn") Hitop.warning else Hitop.accent, trackColor = Hitop.divider)
                                        if (meter.note.isNotEmpty()) Text(meter.note, style = HitopType.caption, color = Hitop.textMuted)
                                    }
                                }
                            }
                        }
                    }
                }
                item {
                    OutlinedButton(onClick = { m.signOut() }, modifier = Modifier.fillMaxWidth().height(48.dp).testTag("sign-out")) { Text(tr(R.string.s_sign_out), color = Hitop.text) }
                }
                item {
                    TextButton(onClick = { dialog = { DeleteAccountDialog(account.email, { dialog = null }) { dialog = null; m.deleteAccount { } } } }, modifier = Modifier.fillMaxWidth().testTag("delete-account")) {
                        Text(tr(R.string.s_delete_account), color = Hitop.error)
                    }
                }
            }
            item {
                Card(Modifier.clickable { vm.push(Screen.Settings) }.testTag("settings")) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(tr(R.string.s_settings), style = HitopType.cardTitle, color = Hitop.text)
                        Text(tr(R.string.s_time_limit_cookies_network_and_sync), style = HitopType.bodySmall, color = Hitop.textSecondary)
                    }
                }
            }
            item { AboutCard() }
        }
    }
    dialog?.invoke()
    if (m.showSignIn) SignInSheet(vm)
    if (joining) JoinDialog(vm) { joining = false }
    m.deviceLimit?.let { DeviceLimitDialog(vm) }
}

@Composable
private fun Notice(text: String, error: Boolean, dismiss: (() -> Unit)? = null) {
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(if (error) Hitop.errorContainer else Hitop.insight).padding(14.dp).testTag("notice"), verticalAlignment = Alignment.CenterVertically) {
        Text(text, Modifier.weight(1f), style = HitopType.bodySmall, color = if (error) Hitop.errorText else Hitop.onInsight)
        dismiss?.let { IconButton(onClick = it) { Icon(HitopIcons.Close, tr(R.string.s_dismiss), tint = Hitop.textMuted, modifier = Modifier.size(16.dp)) } }
    }
}

@Composable
private fun AboutCard() {
    var open by remember { mutableStateOf(false) }
    Card(Modifier.clickable { open = !open }.testTag("about")) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(tr(R.string.s_about_hitop), style = HitopType.cardTitle, color = Hitop.text)
            Text(tr(R.string.s_the_api_client_your_terminal_your_ci, tech.rws8.hitop.BuildConfig.VERSION_NAME), style = HitopType.bodySmall, color = Hitop.textSecondary)
            if (open) {
                Text(tr(R.string.s_acknowledgements_geist_and_geist_mono_sil_open), style = HitopType.caption, color = Hitop.textMuted)
            }
        }
    }
}

@Composable
private fun DeleteAccountDialog(email: String, dismiss: () -> Unit, confirm: () -> Unit) {
    androidx.compose.material3.AlertDialog(
        onDismissRequest = dismiss,
        containerColor = Hitop.surfaceHigh,
        title = { Text(tr(R.string.s_delete_your_hitop_cloud_account), style = HitopType.title, color = Hitop.text) },
        text = {
            Text(
                tr(R.string.s_this_deletes_for_good_the_synced_library, email),
                style = HitopType.bodySmall, color = Hitop.textSecondary)
        },
        confirmButton = { TextButton(onClick = confirm, modifier = Modifier.testTag("confirm-delete-account")) { Text(tr(R.string.s_delete_account), color = Hitop.error) } },
        dismissButton = { TextButton(onClick = dismiss) { Text(tr(R.string.s_cancel), color = Hitop.textSecondary) } })
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SignInSheet(vm: AppViewModel) {
    val m = vm.cloud
    val context = LocalContext.current
    val settings by m.cloud.settings.collectAsState()
    var server by remember { mutableStateOf(false) }
    var paste by remember { mutableStateOf("") }
    LaunchedEffect(Unit) { m.loadMethods() }
    ModalBottomSheet(onDismissRequest = { m.showSignIn = false }, containerColor = Hitop.surface, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.padding(horizontal = 16.dp).padding(bottom = 16.dp).navigationBarsPadding().testTag("sign-in-sheet"), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(tr(R.string.s_sign_in_to_hitop_cloud), style = HitopType.title, color = Hitop.text)
            val methods = m.methods
            if (methods == null) {
                if (m.busy) CircularProgressIndicator(color = Hitop.accent)
            } else if (methods.providers.isEmpty()) {
                Text(tr(R.string.s_this_server_offers_no_sign_in_this), style = HitopType.bodySmall, color = Hitop.textMuted)
            }
            methods?.providers?.forEach { kind ->
                OutlinedButton(onClick = { m.signIn(context, kind) }, enabled = !m.busy, modifier = Modifier.fillMaxWidth().height(52.dp).testTag("continue-$kind"), shape = RoundedCornerShape(26.dp)) {
                    Text(tr(R.string.s_continue_with, AuthMethods.name(kind)), style = HitopType.body, color = Hitop.text)
                }
            }
            var sso by remember { mutableStateOf(false) }
            var work by remember { mutableStateOf("") }
            if (!sso) TextButton(onClick = { sso = true }, modifier = Modifier.testTag("company-sso")) { Text(tr(R.string.s_company_sso), color = Hitop.accent) }
            else {
                OutlinedTextField(work, { work = it }, singleLine = true, label = { Text(tr(R.string.s_work_email)) }, colors = fieldColors(), modifier = Modifier.fillMaxWidth().testTag("sso-email"))
                OutlinedButton(onClick = { m.signInSso(context, work) }, enabled = !m.busy && '@' in work, modifier = Modifier.fillMaxWidth().height(52.dp).testTag("continue-sso"), shape = RoundedCornerShape(26.dp)) { Text(tr(R.string.s_continue_with_company_sso), style = HitopType.body, color = Hitop.text) }
            }
            m.pasteFor?.let { kind ->
                Text(tr(R.string.s_this_server_shows_a_code_after_you, AuthMethods.name(kind)), style = HitopType.caption, color = Hitop.textMuted)
                OutlinedTextField(paste, { paste = it }, singleLine = true, label = { Text(tr(R.string.s_sign_in_code)) }, colors = fieldColors(), modifier = Modifier.fillMaxWidth())
                Button(onClick = { m.handleReturn(paste) }, enabled = paste.isNotBlank(), colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent)) { Text(tr(R.string.s_sign_in)) }
            }
            m.error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText, modifier = Modifier.testTag("sign-in-error")) }
            Text(tr(R.string.s_by_signing_in_you_agree_to_hitop), style = HitopType.caption, color = Hitop.textMuted)
            Text(if (server) tr(R.string.s_hide) else tr(R.string.s_your_own_server), Modifier.clickable { server = !server }.padding(vertical = 8.dp), style = HitopType.label, color = Hitop.accent)
            if (server) {
                var text by remember { mutableStateOf(settings.server) }
                OutlinedTextField(text, { text = it }, singleLine = true, label = { Text(tr(R.string.s_server)) }, colors = fieldColors(), modifier = Modifier.fillMaxWidth().testTag("server-field"))
                TextButton(onClick = { m.setServer(text) }) { Text(tr(R.string.s_use_this_server), color = Hitop.accent) }
            }
        }
    }
}

@Composable
private fun DeviceLimitDialog(vm: AppViewModel) {
    val m = vm.cloud
    val l = m.deviceLimit ?: return
    androidx.compose.material3.AlertDialog(
        onDismissRequest = { m.deviceLimit = null },
        containerColor = Hitop.surfaceHigh,
        title = { Text(tr(R.string.s_remove_a_device_to_sign_in_here), style = HitopType.title, color = Hitop.text) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.testTag("device-limit")) {
                Text(tr(R.string.s_this_account_allows_devices, l.max), style = HitopType.bodySmall, color = Hitop.textSecondary)
                l.devices.forEach { d ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(d.name, Modifier.weight(1f), style = HitopType.body, color = Hitop.text)
                        TextButton(onClick = { m.removeAtLimit(d.id) }) { Text(tr(R.string.s_remove_2), color = Hitop.error) }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = { m.retryAtLimit() }, enabled = l.devices.size < l.max, modifier = Modifier.testTag("limit-sign-in")) { Text(tr(R.string.s_sign_in), color = Hitop.accent) } },
        dismissButton = { TextButton(onClick = { m.deviceLimit = null }) { Text(tr(R.string.s_cancel), color = Hitop.textSecondary) } })
}

@Suppress("unused")
private val keepBox: @Composable () -> Unit = { Box {} }
