package tech.rws8.hitop.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.focus.focusRequester
import kotlinx.coroutines.launch
import tech.rws8.hitop.core.Collection
import tech.rws8.hitop.core.Request
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopChip
import tech.rws8.hitop.ui.theme.HitopIcons
import tech.rws8.hitop.ui.theme.HitopType
import tech.rws8.hitop.ui.theme.LogoMark
import tech.rws8.hitop.ui.theme.MethodTag
import tech.rws8.hitop.ui.theme.statusColor

private enum class Filter(val label: String) { ALL("All"), PINNED("Pinned"), RECENT("Recent") }

@Composable
fun CollectionsScreen(vm: AppViewModel) {
    val inPane = LocalInListPane.current
    val s by vm.library.state.collectAsState()
    val cloud by vm.cloud.cloud.settings.collectAsState()
    val badges by vm.cloud.sync.badges.collectAsState()
    val syncStatus by vm.cloud.sync.status.collectAsState()
    val notices by vm.cloud.sync.notices.collectAsState()
    val sharedConflicts by vm.shared.conflicts.collectAsState()
    val sharedNotices by vm.shared.notices.collectAsState()
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    var query by rememberSaveable { mutableStateOf("") }
    var filter by rememberSaveable { mutableStateOf(Filter.ALL) }
    var dialog by remember { mutableStateOf<(@Composable () -> Unit)?>(null) }
    val collapsed = remember { mutableStateListOf<String>() }
    val last = s.lastStatus

    // The organization picked on the Account screen: a collection's `org`, none = personal (web app `orgOf`).
    val current = vm.cloud.cloud.currentOrg
    val visible = s.collections.filter { c -> if (cloud.account == null || current == null || current.personal) c.org == null || c.org == current?.id else c.org == current.id }
    androidx.compose.material3.pulltorefresh.PullToRefreshBox(
        isRefreshing = syncStatus.pulling,
        onRefresh = { scope.launch { vm.cloud.sync.sync() } },
        modifier = Modifier.fillMaxSize(),
    ) {
        LazyColumn(Modifier.fillMaxSize().testTag("collections"), contentPadding = PaddingValues(bottom = 96.dp)) {
            item {
                Row(Modifier.padding(start = 16.dp, end = 16.dp, top = 20.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    LogoMark(40, Modifier.clip(CircleShape).clickable(onClickLabel = "Account") { vm.push(Screen.Account) }.semantics { contentDescription = "Account" }.testTag("account-button"))
                    Column(Modifier.weight(1f)) {
                        Text("Collections", style = HitopType.screenTitle, color = Hitop.text, maxLines = 1, softWrap = false, modifier = Modifier.semantics { heading() })
                        val where = when {
                            cloud.account == null -> "On this phone"
                            else -> (if (current == null || current.personal) "Personal" else current.name) + when {
                                syncStatus.saving -> " · saving…"
                                syncStatus.error != null -> " · waiting to sync"
                                else -> " · synced"
                            }
                        }
                        Text(where, style = HitopType.caption, color = Hitop.textMuted, modifier = Modifier.testTag("collections-subtitle"))
                    }
                    IconButton(onClick = { dialog = { FromTextDialog(vm) { dialog = null } } }, modifier = Modifier.testTag("from-text-button")) {
                        Icon(HitopIcons.Sparkle, "New request from text or curl", tint = Hitop.accent)
                    }
                    if (!inPane) EnvironmentPill(vm)
                }
                if (s.damaged.isNotEmpty()) Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp).testTag("damaged"), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("${if (s.damaged.size == 1) "A file" else "${s.damaged.size} files"} couldn't be read and was kept aside: ${s.damaged.joinToString()}. Anything in HITOP Cloud comes back with the next sync.", Modifier.weight(1f), style = HitopType.caption, color = Hitop.warning)
                    androidx.compose.material3.TextButton(onClick = { vm.library.dismissDamaged() }) { Text("OK", color = Hitop.accent) }
                }
            }
            item {
                Row(
                    Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth().clip(RoundedCornerShape(26.dp)).background(Hitop.surfaceHigh).height(52.dp).padding(horizontal = 18.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Icon(HitopIcons.Search, null, tint = Hitop.textMuted, modifier = Modifier.size(20.dp))
                    Box(Modifier.weight(1f)) {
                        if (query.isEmpty()) Text("Search requests, collections, history", style = HitopType.body, color = Hitop.textMuted, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        val focus = remember { androidx.compose.ui.focus.FocusRequester() }
                        androidx.compose.runtime.LaunchedEffect(vm.searchFocus) { if (vm.searchFocus > 0) runCatching { focus.requestFocus() } }
                        BasicTextField(query, { query = it }, singleLine = true, textStyle = HitopType.body.copy(color = Hitop.text), cursorBrush = SolidColor(Hitop.accent),
                            modifier = Modifier.fillMaxWidth().focusRequester(focus).semantics { contentDescription = "Search" }.testTag("search"))
                    }
                }
            }
            item {
                Row(Modifier.padding(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Filter.entries.forEach { f -> HitopChip(f.label, filter == f, Modifier.clickable { filter = f }.semantics { role = Role.Tab }.testTag("filter-${f.label}")) }
                }
            }
            notices.forEach { n ->
                item {
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.insight).padding(14.dp).testTag("notice"), verticalAlignment = Alignment.CenterVertically) {
                        Text(n, Modifier.weight(1f), style = HitopType.bodySmall, color = Hitop.onInsight)
                        IconButton(onClick = { vm.cloud.sync.dismissNotice(n) }) { Icon(HitopIcons.Close, "Dismiss", tint = Hitop.textMuted, modifier = Modifier.size(16.dp)) }
                    }
                }
            }
            sharedNotices.forEach { n ->
                item {
                    Row(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(Hitop.insight).padding(14.dp).testTag("notice"), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(n.text, style = HitopType.bodySmall, color = Hitop.onInsight)
                            n.detail?.let { Text(it, style = HitopType.monoSmall, color = Hitop.textMuted) }
                        }
                        IconButton(onClick = { vm.shared.dismissNotice(n) }) { Icon(HitopIcons.Close, "Dismiss", tint = Hitop.textMuted, modifier = Modifier.size(16.dp)) }
                    }
                }
            }
            val q = query.trim().lowercase()
            fun matches(c: Collection, r: Request) = q.isEmpty() || r.name.lowercase().contains(q) || r.url.lowercase().contains(q) || c.name.lowercase().contains(q)
            when (filter) {
                Filter.ALL -> items(visible, key = { "c-" + it.id }) { c ->
                    val rows = c.requests.filter { matches(c, it) }
                    if (q.isNotEmpty() && rows.isEmpty()) return@items
                    val badge = when {
                        cloud.account == null -> "on this phone"
                        else -> when (badges[c.id]) {
                            tech.rws8.hitop.cloud.LibrarySync.Badge.SYNCED -> syncStatus.lastSync?.let { "synced " + tech.rws8.hitop.cloud.LibrarySync.ago(it) } ?: "synced"
                            tech.rws8.hitop.cloud.LibrarySync.Badge.WAITING -> if (syncStatus.saving) "saving…" else "waiting to sync"
                            tech.rws8.hitop.cloud.LibrarySync.Badge.REFUSED -> "not in HITOP Cloud"
                            else -> "on this phone"
                        }
                    }
                    val sharedBadge = vm.shared.badge(c.id)
                    CollectionCard(vm, c, rows, last, c.id in collapsed && q.isEmpty(), { if (c.id in collapsed) collapsed.remove(c.id) else collapsed.add(c.id) }, sharedBadge?.let { b -> (if (s.sharedRoles[c.id] == "viewer") "$b · View only" else b) + (sharedConflicts[c.id]?.size?.takeIf { it > 0 }?.let { " · $it conflict" } ?: "") } ?: badge, cloud.account != null && vm.cloud.sync.isLocalOnly(c.id)) { dialog = it }
                }
                Filter.PINNED -> {
                    val pinned = s.pins.mapNotNull { s.find(it) }.filter { (c, r) -> matches(c, r) }
                    item { FlatCard(vm, pinned, last, "Pin a request from its menu to keep it here.") }
                }
                Filter.RECENT -> {
                    val recent = s.history.map { it.request.id }.distinct().mapNotNull { s.find(it) }.filter { (c, r) -> matches(c, r) }.take(20)
                    item { FlatCard(vm, recent, last, "Requests you send show here.") }
                }
            }
            item {
                TextButton(onClick = { dialog = { TextDialog("New collection", "", "Create", { dialog = null }) { n -> vm.library.addCollection(n); dialog = null } } }, modifier = Modifier.padding(horizontal = 8.dp).testTag("new-collection")) {
                    Icon(HitopIcons.Plus, null, tint = Hitop.accent, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.size(6.dp))
                    Text("New collection", style = HitopType.label, color = Hitop.accent)
                }
            }
        }
        if (!inPane) ExtendedFloatingActionButton(
            onClick = { vm.newRequest() },
            containerColor = Hitop.accent, contentColor = Hitop.onAccent,
            shape = RoundedCornerShape(16.dp),
            icon = { Icon(HitopIcons.Plus, null) },
            text = { Text("New request", style = HitopType.button.copy(fontSize = HitopType.body.fontSize)) },
            modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp).testTag("new-request"),
        )
        dialog?.invoke()
    }
}

@Composable
private fun CollectionCard(vm: AppViewModel, c: Collection, rows: List<Request>, last: Map<String, Int>, folded: Boolean, toggle: () -> Unit, badge: String, localOnly: Boolean, showDialog: ((@Composable () -> Unit)?) -> Unit) {
    var menu by remember { mutableStateOf(false) }
    var share by remember { mutableStateOf(false) }
    if (share) ShareSheet(vm, c.id) { share = false }
    Card(Modifier.padding(horizontal = 16.dp, vertical = 6.dp)) {
        Row(
            Modifier.fillMaxWidth().clickable(onClickLabel = if (folded) "Show requests" else "Hide requests") { toggle() }.padding(horizontal = 16.dp, vertical = 10.dp).testTag("collection-${c.name}"),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Box(Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).background(Hitop.accentContainer), contentAlignment = Alignment.Center) {
                Icon(HitopIcons.Folder, null, tint = Hitop.accent, modifier = Modifier.size(20.dp))
            }
            Column(Modifier.weight(1f)) {
                Text(c.name, style = HitopType.cardTitle, color = Hitop.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val n = c.requests.size
                Text("$n ${if (n == 1) "request" else "requests"} · $badge", style = HitopType.caption, color = Hitop.textMuted, modifier = Modifier.testTag("badge-${c.name}"))
            }
            if (c.requests.any { !it.isWebSocket && !it.isGrpc }) {
                IconButton(onClick = { vm.push(Screen.Run(c.id)) }, modifier = Modifier.size(44.dp).clip(CircleShape).background(Hitop.accentContainer).testTag("run-button-${c.name}")) {
                    Icon(HitopIcons.Run, "Run ${c.name}", tint = Hitop.accent, modifier = Modifier.size(18.dp))
                }
            }
            Box {
                IconButton(onClick = { menu = true }, modifier = Modifier.testTag("collection-menu-${c.name}")) { Icon(HitopIcons.More, "More for ${c.name}", tint = Hitop.textSecondary) }
                DropdownMenu(menu, { menu = false }, containerColor = Hitop.surfaceHigh) {
                    DropdownMenuItem(text = { Text("New request", color = Hitop.text) }, onClick = { menu = false; vm.newRequest(c.id) })
                    DropdownMenuItem(text = { Text("New folder", color = Hitop.text) }, onClick = {
                        menu = false
                        showDialog { TextDialog("New folder", "", "Create", { showDialog(null) }) { n -> vm.library.addFolder(c.id, n); showDialog(null) } }
                    })
                    if (localOnly) DropdownMenuItem(text = { Text("Save to HITOP Cloud", color = Hitop.text) }, onClick = { menu = false; vm.cloud.upload(c.id) })
                    if (vm.cloud.cloud.signedIn) DropdownMenuItem(text = { Text(if (vm.shared.link(c.id) != null) "People…" else "Share…", color = Hitop.text) }, onClick = { menu = false; share = true }, modifier = Modifier.testTag("share-${c.name}"))
                    DropdownMenuItem(text = { Text("Rename", color = Hitop.text) }, onClick = {
                        menu = false
                        showDialog { TextDialog("Rename collection", c.name, "Rename", { showDialog(null) }) { n -> vm.library.renameCollection(c.id, n); showDialog(null) } }
                    })
                    DropdownMenuItem(text = { Text("Delete", color = Hitop.error) }, onClick = {
                        menu = false
                        showDialog { ConfirmDialog("Delete ${c.name}?", "Its ${c.requests.size} requests are deleted from this phone.", "Delete", { showDialog(null) }) { vm.library.deleteCollection(c.id); showDialog(null) } }
                    })
                }
            }
        }
        if (!folded) {
            var folder: String? = null
            // Top-level requests first, then each folder in the collection's order (stable inside).
            val order = c.allFolders
            val grouped = rows.sortedBy { if (it.folder.isEmpty()) -1 else order.indexOf(it.folder) }
            grouped.forEach { r ->
                if (r.folder != folder && r.folder.isNotEmpty()) {
                    Text(r.folder, Modifier.padding(start = 16.dp, top = 8.dp, bottom = 2.dp), style = HitopType.navLabel, color = Hitop.textMuted)
                }
                folder = r.folder
                RequestRow(vm, c, r, last[r.id], showDialog)
            }
            if (rows.isEmpty()) Text("No requests yet", Modifier.padding(16.dp), style = HitopType.caption, color = Hitop.textMuted)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun RequestRow(vm: AppViewModel, c: Collection, r: Request, status: Int?, showDialog: ((@Composable () -> Unit)?) -> Unit, subtitle: String? = null) {
    var menu by remember { mutableStateOf(false) }
    val state by vm.library.state.collectAsState()
    val pinned = r.id in state.pins
    Box {
        Row(
            Modifier.fillMaxWidth()
                .combinedClickable(onClickLabel = "Open", onLongClickLabel = "More", onLongClick = { menu = true }) { vm.openRequest(c.id, r.id) }
                .heightIn(min = 52.dp).padding(horizontal = 16.dp)
                .semantics(mergeDescendants = true) {}
                .testTag("request-${r.name}"),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            MethodTag(r.tag)
            Column(Modifier.weight(1f)) {
                Text(r.name, style = HitopType.body, color = Hitop.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                subtitle?.let { Text(it, style = HitopType.caption, color = Hitop.textMuted, maxLines = 1) }
            }
            if (pinned) Icon(HitopIcons.Pin, "Pinned", tint = Hitop.textMuted, modifier = Modifier.size(14.dp))
            status?.let { Text("$it", style = HitopType.monoSmall, color = statusColor(it)) }
        }
        DropdownMenu(menu, { menu = false }, containerColor = Hitop.surfaceHigh) {
            DropdownMenuItem(text = { Text(if (pinned) "Unpin" else "Pin", color = Hitop.text) }, onClick = { menu = false; vm.library.togglePin(r.id) })
            DropdownMenuItem(text = { Text("Rename", color = Hitop.text) }, onClick = {
                menu = false
                showDialog { TextDialog("Rename request", r.name, "Rename", { showDialog(null) }) { n -> vm.library.renameRequest(r.id, n); showDialog(null) } }
            })
            DropdownMenuItem(text = { Text("Duplicate", color = Hitop.text) }, onClick = { menu = false; vm.library.duplicateRequest(r.id) })
            DropdownMenuItem(text = { Text("Move…", color = Hitop.text) }, onClick = { menu = false; showDialog { MoveDialog(vm, r) { showDialog(null) } } })
            DropdownMenuItem(text = { Text("Delete", color = Hitop.error) }, onClick = {
                menu = false
                showDialog { ConfirmDialog("Delete ${r.name}?", "It is deleted from ${c.name} on this phone.", "Delete", { showDialog(null) }) { vm.library.deleteRequest(r.id); showDialog(null) } }
            })
        }
    }
}

@Composable
private fun MoveDialog(vm: AppViewModel, r: Request, done: () -> Unit) {
    val s by vm.library.state.collectAsState()
    androidx.compose.material3.AlertDialog(
        onDismissRequest = done,
        containerColor = Hitop.surfaceHigh,
        title = { Text("Move ${r.name}", style = HitopType.title, color = Hitop.text) },
        text = {
            LazyColumn {
                s.collections.forEach { c ->
                    item {
                        Text(c.name, Modifier.fillMaxWidth().clickable { vm.library.moveRequest(r.id, c.id, ""); done() }.heightIn(min = 48.dp).padding(vertical = 12.dp), style = HitopType.body, color = Hitop.text)
                    }
                    items(c.allFolders) { f ->
                        Text("  $f", Modifier.fillMaxWidth().clickable { vm.library.moveRequest(r.id, c.id, f); done() }.heightIn(min = 48.dp).padding(start = 16.dp, top = 12.dp, bottom = 12.dp), style = HitopType.bodySmall, color = Hitop.textSecondary)
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = done) { Text("Cancel", color = Hitop.textSecondary) } },
    )
}

@Composable
private fun FlatCard(vm: AppViewModel, rows: List<Pair<Collection, Request>>, last: Map<String, Int>, empty: String) {
    var dialog by remember { mutableStateOf<(@Composable () -> Unit)?>(null) }
    Card(Modifier.padding(horizontal = 16.dp, vertical = 6.dp)) {
        if (rows.isEmpty()) Text(empty, Modifier.padding(16.dp), style = HitopType.bodySmall, color = Hitop.textMuted)
        rows.forEach { (c, r) -> RequestRow(vm, c, r, last[r.id], { dialog = it }, subtitle = c.name) }
    }
    dialog?.invoke()
}

