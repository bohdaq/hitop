package tech.rws8.hitop.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import tech.rws8.hitop.sync.InviteCode
import tech.rws8.hitop.sync.MemberKey
import tech.rws8.hitop.sync.SharedSync
import tech.rws8.hitop.sync.SyncCore
import tech.rws8.hitop.sync.WorkspaceInfo
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.R
import tech.rws8.hitop.platform.tr

/** The Share sheet of one collection: share it, people and their device fingerprints, invite, add, conflicts. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ShareSheet(vm: AppViewModel, collectionId: String, close: () -> Unit) {
    val shared = vm.shared
    val scope = rememberCoroutineScope()
    val clipboard = LocalClipboardManager.current
    val conflicts by shared.conflicts.collectAsState()
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var info by remember { mutableStateOf<WorkspaceInfo?>(null) }
    var untrusted by remember { mutableStateOf<List<MemberKey>>(emptyList()) }
    var email by remember { mutableStateOf("") }
    var role by remember { mutableStateOf("editor") }
    var code by remember { mutableStateOf<String?>(null) }
    var tick by remember { mutableStateOf(0) }
    var confirm by remember { mutableStateOf<Triple<String, String, () -> Unit>?>(null) }
    confirm?.let { (title, text, go) -> ConfirmDialog(title, text, title.substringBefore(' '), { confirm = null }) { confirm = null; go() } }
    val link = shared.link(collectionId)
    fun act(block: suspend () -> Unit) = scope.launch {
        busy = true
        error = null
        try { block() } catch (e: Exception) { error = e.message ?: e.toString() } finally { busy = false; tick++ }
    }
    LaunchedEffect(collectionId, tick, link?.workspace) {
        runCatching {
            if (link == null) untrusted = shared.ownUntrustedDevices() else info = shared.members(collectionId)
        }.onFailure { error = it.message }
    }
    ModalBottomSheet(onDismissRequest = close, containerColor = Hitop.surface, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.padding(horizontal = 16.dp).padding(bottom = 16.dp).navigationBarsPadding().verticalScroll(rememberScrollState()).testTag("share-sheet"), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(if (link == null) tr(R.string.s_share_this_collection) else tr(R.string.s_people_2), style = HitopType.title, color = Hitop.text)
            error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText, modifier = Modifier.testTag("share-error")) }
            if (busy) CircularProgressIndicator(color = Hitop.accent)
            if (link == null) {
                Text(tr(R.string.s_shared_collections_are_end_to_end_encrypted), style = HitopType.bodySmall, color = Hitop.textSecondary)
                if (untrusted.isNotEmpty()) {
                    Text(tr(R.string.s_first_compare_and_trust_your_other_devices), style = HitopType.caption, color = Hitop.textMuted)
                    untrusted.forEach { k -> KeyRow(k) }
                    OutlinedButton(onClick = { shared.trust(untrusted); tick++ }) { Text(tr(R.string.s_they_match_trust_them), color = Hitop.text) }
                }
                Button(onClick = { act { shared.share(collectionId) } }, enabled = !busy && untrusted.isEmpty(), colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent), modifier = Modifier.fillMaxWidth().testTag("share-now")) {
                    Text(tr(R.string.s_share_3), style = HitopType.button)
                }
                return@Column
            }
            Text(tr(R.string.s_your_device_s_fingerprint, shared.fingerprint?.let(SyncCore::grouped) ?: "—"), style = HitopType.monoSmall, color = Hitop.textSecondary, modifier = Modifier.testTag("my-fingerprint"))
            conflicts[collectionId].orEmpty().forEach { k ->
                Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.errorContainer).padding(12.dp).testTag("conflict"), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(tr(R.string.s_changed_here_and_by_someone_else, k.name), style = HitopType.bodySmall, color = Hitop.text)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(onClick = { act { shared.resolve(collectionId, k.path, SharedSync.Choice.MINE) } }) { Text(tr(R.string.s_keep_mine), color = Hitop.accent) }
                        TextButton(onClick = { act { shared.resolve(collectionId, k.path, SharedSync.Choice.THEIRS) } }) { Text(tr(R.string.s_take_theirs), color = Hitop.accent) }
                        if (k.canKeepBoth) TextButton(onClick = { act { shared.resolve(collectionId, k.path, SharedSync.Choice.BOTH) } }) { Text(tr(R.string.s_keep_both), color = Hitop.accent) }
                    }
                }
            }
            info?.let { i ->
                SectionLabel(tr(R.string.s_members))
                i.members.forEach { m ->
                    Column(Modifier.fillMaxWidth().testTag("member-${m.email}")) {
                        Text("${m.email} · ${m.role}", style = HitopType.body, color = Hitop.text)
                        m.keys.forEach { KeyRow(it) }
                        if (i.role == "owner" && !m.runner && m.email != vm.cloud.cloud.account?.email) Row(verticalAlignment = Alignment.CenterVertically) {
                            listOf("viewer", "editor", "owner").forEach { r ->
                                TextButton(onClick = { if (r == "owner") confirm = Triple("Make ${m.email} an owner?", "Owners invite and remove people, and can delete the shared collection.") { act { shared.setRole(collectionId, m.userId, r) } } else act { shared.setRole(collectionId, m.userId, r) } }, enabled = !busy && m.role != r, modifier = Modifier.testTag("set-$r-${m.email}")) { Text(if (m.role == r) "✓ ${r}" else r, color = if (m.role == r) Hitop.text else Hitop.accent) }
                            }
                            TextButton(onClick = { confirm = Triple("Remove ${m.email}?", "The collection gets a new key and is sealed again for everyone else. What they already have stays with them.") { act { shared.removeMember(collectionId, m.userId) } } }, enabled = !busy, modifier = Modifier.testTag("remove-${m.email}")) { Text(tr(R.string.s_remove_2), color = Hitop.error) }
                        }
                    }
                }
                i.invites.forEach { inv ->
                    Column(Modifier.fillMaxWidth()) {
                        Text(tr(R.string.s_invited, inv.email.ifEmpty { tr(R.string.s_by_code) }, inv.role, if (inv.acceptedBy != null) tr(R.string.s_accepted) else ""), style = HitopType.bodySmall, color = Hitop.textSecondary)
                        inv.acceptedBy?.let { acc ->
                            acc.keys.forEach { KeyRow(it) }
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                if (acc.keys.any { it.trust != MemberKey.Trust.TRUSTED }) TextButton(onClick = { shared.trust(acc.keys); tick++ }) { Text(tr(R.string.s_they_match_trust), color = Hitop.accent) }
                                else TextButton(onClick = { act { shared.addMember(collectionId, acc.email, inv.role, acc.keys) } }) { Text(tr(R.string.s_add), color = Hitop.accent) }
                            }
                        }
                    }
                }
                if (i.role == "owner") {
                    SectionLabel(tr(R.string.s_invite))
                    OutlinedTextField(email, { email = it }, singleLine = true, label = { Text(tr(R.string.s_email_optional)) }, colors = fieldColors(), modifier = Modifier.fillMaxWidth().testTag("invite-email"))
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        listOf("viewer", "editor").forEach { r -> TextButton(onClick = { role = r }, modifier = Modifier.testTag("role-$r")) { Text(if (role == r) tr(R.string.s_as, r) else tr(R.string.s_as_2, r), color = if (role == r) Hitop.accent else Hitop.textMuted) } }
                    }
                    Button(onClick = { act { code = shared.invite(collectionId, email, role).first } }, colors = ButtonDefaults.buttonColors(containerColor = Hitop.accent, contentColor = Hitop.onAccent), modifier = Modifier.testTag("invite")) {
                        Text(tr(R.string.s_invite), style = HitopType.button)
                    }
                    code?.let { c ->
                        Text(tr(R.string.s_send_this_code_to_them_it_names), style = HitopType.caption, color = Hitop.textMuted)
                        Text(c, style = HitopType.monoSmall, color = Hitop.text, modifier = Modifier.testTag("invite-code"))
                        TextButton(onClick = { clipboard.setText(AnnotatedString(c)) }) { Text(tr(R.string.s_copy), color = Hitop.accent) }
                    }
                    TextButton(onClick = { confirm = Triple("Delete the shared collection?", "It is deleted from the server for everyone it is shared with. It stays on this phone, no longer shared; other people keep the copy they have.") { act { shared.deleteWorkspace(collectionId); close() } } }, enabled = !busy, modifier = Modifier.testTag("delete-workspace")) { Text(tr(R.string.s_delete_for_everyone), color = Hitop.error) }
                }
            }
        }
    }
}

@Composable
private fun KeyRow(k: MemberKey) {
    Row(Modifier.fillMaxWidth().heightIn(min = 40.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(k.name.ifEmpty { tr(R.string.s_a_device) }, style = HitopType.bodySmall, color = Hitop.text)
            Text(SyncCore.grouped(k.fingerprint), style = HitopType.monoSmall, color = Hitop.textMuted)
        }
        Text(
            when (k.trust) { MemberKey.Trust.TRUSTED -> tr(R.string.s_trusted); MemberKey.Trust.CHANGED -> tr(R.string.s_key_changed); MemberKey.Trust.NEW -> tr(R.string.s_not_checked) },
            style = HitopType.caption,
            color = when (k.trust) { MemberKey.Trust.TRUSTED -> Hitop.success; MemberKey.Trust.CHANGED -> Hitop.error; MemberKey.Trust.NEW -> Hitop.warning })
    }
}

/** Join with an invite code: compare the code's fingerprint, accept, wait to be added, open. */
@Composable
fun JoinDialog(vm: AppViewModel, close: () -> Unit) {
    val shared = vm.shared
    val scope = rememberCoroutineScope()
    var text by remember { mutableStateOf("") }
    var parsed by remember { mutableStateOf<InviteCode?>(null) }
    var status by remember { mutableStateOf<String?>(null) }
    InputSheet(tr(R.string.s_join_with_invite_code_2), close, actions = {
        TextButton(onClick = close) { Text(tr(R.string.s_cancel), color = Hitop.textSecondary) }
        TextButton(onClick = {
            val code = parsed ?: return@TextButton
            scope.launch {
                try {
                    val (ws, _) = shared.accept(code)
                    status = "Accepted. Tell the owner this device's fingerprint: ${shared.fingerprint?.let(SyncCore::grouped)}. Waiting to be added…"
                    repeat(120) {
                        shared.refreshWorkspaces()
                        if (shared.workspaces.value.any { it.id == ws }) {
                            val id = runCatching { shared.join(ws) }.getOrNull()
                            if (id != null) { close(); vm.tab = Tab.COLLECTIONS; return@launch }
                        }
                        delay(1000)
                    }
                } catch (e: Exception) {
                    status = e.message
                }
            }
        }, enabled = parsed != null, modifier = Modifier.testTag("join-accept")) { Text(if (parsed?.fingerprint != null) tr(R.string.s_they_match_accept) else tr(R.string.s_accept), color = Hitop.accent) }
    }) {
        SmallField(text, { text = it; parsed = runCatching { InviteCode.parse(it) }.getOrNull() }, tr(R.string.s_invite_code), Modifier.fillMaxWidth().testTag("join-code"))
        parsed?.let { p ->
            Text(
                p.fingerprint?.let { fp -> tr(R.string.s_ask_who_invited_you_to_read_out, SyncCore.grouped(fp)) } ?: tr(R.string.s_this_code_names_no_device_the_key),
                style = HitopType.caption, color = Hitop.textSecondary)
        }
        status?.let { Text(it, style = HitopType.caption, color = Hitop.textMuted, modifier = Modifier.testTag("join-status")) }
    }
}

/** A key from a device this phone hasn't trusted: compare its fingerprint, then trust it. */
@Composable
fun TrustPrompt(vm: AppViewModel) {
    val shared = vm.shared
    val r by shared.pendingTrust.collectAsState()
    val refusal = r ?: return
    val scope = rememberCoroutineScope()
    androidx.compose.material3.AlertDialog(
        onDismissRequest = { shared.pendingTrust.value = null },
        containerColor = Hitop.surfaceHigh,
        title = { Text(tr(R.string.s_check_this_device), style = HitopType.title, color = Hitop.text) },
        text = { Text(tr(R.string.s_a_shared_collection_s_key_came_from, refusal.fingerprint?.let(SyncCore::grouped)), style = HitopType.bodySmall, color = Hitop.textSecondary) },
        confirmButton = { TextButton(onClick = { scope.launch { runCatching { shared.trustSender(refusal) }; shared.syncAll() } }) { Text(tr(R.string.s_they_match_trust), color = Hitop.accent) } },
        dismissButton = { TextButton(onClick = { shared.pendingTrust.value = null }) { Text(tr(R.string.s_not_now), color = Hitop.textSecondary) } })
}

/** Comments on a request of a shared collection: end-to-end encrypted files that sync with it. */
@Composable
fun CommentsSheet(vm: AppViewModel, collectionId: String, requestId: String, close: () -> Unit) {
    val scope = rememberCoroutineScope()
    var list by remember { mutableStateOf(vm.shared.comments(collectionId, requestId)) }
    var text by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun act(block: suspend () -> Unit) = scope.launch {
        busy = true
        error = null
        try {
            block()
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            error = e.message ?: e.toString()
        } finally {
            busy = false
            list = vm.shared.comments(collectionId, requestId)
        }
    }
    LaunchedEffect(Unit) { act { vm.shared.syncNow(collectionId) } }
    val viewer = vm.library.isReadOnly(collectionId)
    InputSheet(tr(R.string.s_comments), close, actions = {
        TextButton(onClick = close) { Text(tr(R.string.s_done), color = Hitop.textSecondary) }
        if (!viewer) TextButton(onClick = { act { vm.shared.addComment(collectionId, requestId, text); text = "" } }, enabled = !busy && text.isNotBlank(), modifier = Modifier.testTag("post-comment")) { Text(tr(R.string.s_comment), color = Hitop.accent) }
    }) {
        Text(tr(R.string.s_seen_by_the_people_this_collection_is), style = HitopType.caption, color = Hitop.textSecondary)
        Column(Modifier.heightIn(max = 320.dp).verticalScroll(rememberScrollState()).testTag("comments"), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            if (list.isEmpty()) Text(tr(R.string.s_no_comments_yet), style = HitopType.caption, color = Hitop.textMuted)
            list.forEach { c ->
                Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(Hitop.surface).padding(12.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("${c.author} · ${c.at.take(16).replace('T', ' ')}", Modifier.weight(1f), style = HitopType.caption, color = Hitop.textMuted)
                        if (c.mine && !viewer) TextButton(onClick = { act { vm.shared.deleteComment(collectionId, c) } }, enabled = !busy) { Text(tr(R.string.s_delete), color = Hitop.error) }
                    }
                    Text(c.text, style = HitopType.bodySmall, color = Hitop.text)
                }
            }
        }
        if (!viewer) OutlinedTextField(text, { text = it }, label = { Text(tr(R.string.s_add_a_comment)) }, colors = fieldColors(), maxLines = 4, modifier = Modifier.fillMaxWidth().testTag("comment-text"))
        error?.let { Text(it, style = HitopType.caption, color = Hitop.errorText) }
    }
}
