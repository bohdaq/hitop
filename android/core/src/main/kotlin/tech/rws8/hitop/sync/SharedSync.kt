package tech.rws8.hitop.sync

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.cloud.CloudAccount
import tech.rws8.hitop.cloud.CloudException
import tech.rws8.hitop.cloud.CloudResponse
import tech.rws8.hitop.cloud.HitopCloud
import tech.rws8.hitop.cloud.StoreText
import tech.rws8.hitop.core.EmptyObject
import tech.rws8.hitop.core.Environment
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.Ids
import tech.rws8.hitop.core.Library
import tech.rws8.hitop.core.bool
import tech.rws8.hitop.core.obj
import tech.rws8.hitop.core.str
import tech.rws8.hitop.core.strOrNull
import tech.rws8.hitop.core.stringList
import tech.rws8.hitop.core.stringMap
import java.io.File

/*
 * Shared collections: hitop sync workspaces, end-to-end encrypted, as the web app's sync.js
 * SyncClient, the CLI's `hitop sync` and the iOS app's SharedSync keep them, item for item. A shared
 * collection is a collection folder (hitop.yaml, requests/, environments/, globals.yaml,
 * comments/) whose files are blobs on the server sealed under the workspace key. Every decision
 * (plan, merge, which wrapped keys may be used) is the Rust every hitop client runs ([SyncCore]);
 * this file is the transport and the bookkeeping.
 *
 * On this phone (hitop-library/): shared-sync.json (pins, key memory per workspace, links, plan
 * state, shared-value versions) and shared/<ws>.json (the workspace's files as last synced or
 * edited here). Workspace keys are never stored: unwrapped again on every sync.
 */

data class SharedLink(val workspace: String, val role: String, val org: String? = null, val orgName: String? = null, val orgPersonal: Boolean = false, val gone: Boolean = false) {
    fun toJson() = buildJsonObject {
        put("workspace", workspace); put("role", role); org?.let { put("org", it) }; orgName?.let { put("orgName", it) }; put("orgPersonal", orgPersonal); put("gone", gone)
    }

    companion object {
        fun of(o: JsonObject) = SharedLink(o.str("workspace"), o.str("role", "viewer"), o.strOrNull("org"), o.strOrNull("orgName"), o.bool("orgPersonal", false), o.bool("gone", false))
    }
}

data class SharedState(
    val account: String = "",
    val pins: Map<String, String> = emptyMap(),
    val memory: Map<String, JsonElement> = emptyMap(),
    val links: Map<String, SharedLink> = emptyMap(),
    val states: Map<String, JsonElement> = emptyMap(),
    val shareVersions: Map<String, Map<String, Long>> = emptyMap(),
) {
    fun toJson() = buildJsonObject {
        put("account", account)
        put("pins", JsonObject(pins.mapValues { JsonPrimitive(it.value) }))
        put("memory", JsonObject(memory))
        put("links", JsonObject(links.mapValues { it.value.toJson() }))
        put("states", JsonObject(states))
        put("shareVersions", JsonObject(shareVersions.mapValues { (_, m) -> JsonObject(m.mapValues { JsonPrimitive(it.value) }) }))
    }

    companion object {
        fun of(o: JsonObject?): SharedState = if (o == null) SharedState() else SharedState(
            o.str("account"),
            o.obj("pins")?.mapValues { (it.value as JsonPrimitive).content } ?: emptyMap(),
            o.obj("memory") ?: emptyMap(),
            o.obj("links")?.mapNotNull { (k, v) -> (v as? JsonObject)?.let { k to SharedLink.of(it) } }?.toMap() ?: emptyMap(),
            o.obj("states") ?: emptyMap(),
            o.obj("shareVersions")?.mapValues { (_, m) -> (m as? JsonObject)?.mapValues { (it.value as JsonPrimitive).content.toLong() } ?: emptyMap() } ?: emptyMap(),
        )
    }
}

interface SharedSyncStore {
    fun load(): SharedState
    fun save(s: SharedState)
    fun files(ws: String): Map<String, String>
    fun saveFiles(ws: String, files: Map<String, String>)
    fun dropFiles(ws: String)

    /** Team environments as last synced, and edits of them still to send. */
    fun loadTeam(): JsonObject
    fun saveTeam(o: JsonObject)
}

class FileSharedSyncStore(private val root: File) : SharedSyncStore {
    private val stateFile get() = File(root, "shared-sync.json")
    private fun filesFile(ws: String) = File(File(root, "shared"), "$ws.json")
    private fun write(f: File, text: String) {
        tech.rws8.hitop.core.SafeFiles.write(f, text)
    }

    override fun load() = SharedState.of(runCatching { HitopJson.parseToJsonElement(stateFile.readText()).jsonObject }.getOrNull())
    override fun save(s: SharedState) = write(stateFile, s.toJson().toString())
    override fun files(ws: String) = runCatching { HitopJson.parseToJsonElement(filesFile(ws).readText()).jsonObject.mapValues { (it.value as JsonPrimitive).content } }.getOrDefault(emptyMap())
    override fun saveFiles(ws: String, files: Map<String, String>) = write(filesFile(ws), JsonObject(files.mapValues { JsonPrimitive(it.value) }).toString())
    override fun dropFiles(ws: String) { filesFile(ws).delete() }
    override fun loadTeam() = runCatching { HitopJson.parseToJsonElement(File(root, "team-environments.json").readText()).jsonObject }.getOrDefault(EmptyObject)
    override fun saveTeam(o: JsonObject) = write(File(root, "team-environments.json"), o.toString())
}

class MemorySharedSyncStore : SharedSyncStore {
    private var state = SharedState()
    private val all = HashMap<String, Map<String, String>>()
    @Synchronized override fun load() = state
    @Synchronized override fun save(s: SharedState) { state = s }
    @Synchronized override fun files(ws: String) = all[ws] ?: emptyMap()
    @Synchronized override fun saveFiles(ws: String, files: Map<String, String>) { all[ws] = files }
    @Synchronized override fun dropFiles(ws: String) { all.remove(ws) }
    private var team = EmptyObject
    override fun loadTeam() = team
    override fun saveTeam(o: JsonObject) { team = o }
}

data class SharedWorkspace(val id: String, val role: String, val epoch: Int, val org: String?, val orgName: String?, val orgPersonal: Boolean)

data class MemberKey(val id: String, val name: String, val publicKey: String, val hasKey: Boolean, val legacy: Boolean, val fingerprint: String, val trust: Trust) {
    enum class Trust { TRUSTED, CHANGED, NEW }
}

data class WorkspaceMember(val userId: String, val email: String, val role: String, val runner: Boolean, val keys: List<MemberKey>)
data class WorkspaceInvite(val id: String, val email: String, val role: String, val acceptedBy: WorkspaceMember?)
data class WorkspaceInfo(val workspace: String, val epoch: Int, val role: String, val members: List<WorkspaceMember>, val invites: List<WorkspaceInvite>)

/** A file changed here and by someone else. */
data class SharedConflict(val path: String, val name: String, val base: String?, val mine: String?, val theirs: String?) {
    /** Keep both: only a request file both sides still have. */
    val canKeepBoth: Boolean get() = path.startsWith("requests/") && mine != null && theirs != null
}

data class SharedResult(val pulled: List<String> = emptyList(), val removed: List<String> = emptyList(), val pushed: List<String> = emptyList(), val merged: List<String> = emptyList(), val conflicts: List<String> = emptyList(), val collectionId: String)

class SharedException(message: String) : Exception(message)

/** An invite code: `<token>.<fingerprint of the inviter's device key>` (hitop 1.2), or a bare token. */
data class InviteCode(val token: String, val fingerprint: String?) {
    companion object {
        fun parse(text: String): InviteCode {
            val t = text.trim()
            val code = t.split('#').last()
            val parts = code.split('.', limit = 2)
            val token = parts.firstOrNull()?.takeIf { it.isNotEmpty() } ?: throw SharedException("Paste the invite code you were sent")
            if (parts.size < 2) return InviteCode(token, null)
            val fp = SyncCore.normalize(parts[1])
            if (fp.length != 32) throw SharedException("This invite code is damaged (the part after the “.” must be a key fingerprint)")
            return InviteCode(token, fp)
        }
    }
}

data class SharedNotice(val text: String, val detail: String? = null)

class SharedSync(private val cloud: HitopCloud, private val library: Library, private val store: SharedSyncStore, private val scope: CoroutineScope) {
    data class Status(val syncing: Boolean = false, val error: String? = null, val lastSync: Long? = null)

    private val _statuses = MutableStateFlow<Map<String, Status>>(emptyMap())
    val statuses: StateFlow<Map<String, Status>> = _statuses.asStateFlow()
    private val _conflicts = MutableStateFlow<Map<String, List<SharedConflict>>>(emptyMap())
    val conflicts: StateFlow<Map<String, List<SharedConflict>>> = _conflicts.asStateFlow()
    private val _workspaces = MutableStateFlow<List<SharedWorkspace>>(emptyList())
    val workspaces: StateFlow<List<SharedWorkspace>> = _workspaces.asStateFlow()
    private val _notices = MutableStateFlow<List<SharedNotice>>(emptyList())
    val notices: StateFlow<List<SharedNotice>> = _notices.asStateFlow()
    /** A workspace key came from a device this one hasn't trusted: the Account screen asks to compare its fingerprint. */
    val pendingTrust = MutableStateFlow<SyncCore.KeyRefusal?>(null)

    var pushDelayMs = 1_000L
    var state: SharedState = store.load()
        private set
    private val pending = HashMap<String, Job>()
    private val running = HashSet<String>()
    private val again = HashSet<String>()

    /** A team environment as the server has it: its organization, version and data. */
    private data class TeamKnown(val org: String, val version: Long, val data: JsonObject)
    private val teamKnown = LinkedHashMap<String, TeamKnown>()
    private val teamLock = kotlinx.coroutines.sync.Mutex()
    private var teamJob: Job? = null
    /** The account [teamKnown] belongs to: another account's environments are never sent or deleted. */
    private var teamAccount = ""

    private fun checkTeamAccount() {
        if (teamAccount == cloud.accountKey) return
        teamAccount = cloud.accountKey
        teamKnown.clear()
        library.applyTeamEnvironments(emptyList(), null)
        saveTeamState()
    }

    init {
        loadTeamState()
        library.onTeamChange = { saveTeamState(); scheduleTeam() }
        library.onSharedChange = { schedule(it) }
        library.onCollectionDeleted = { forget(it) }
        cloud.onAccountDeleted = { accountDeleted(it) }
        applyRoles()
        state.links.forEach { (id, l) -> setConflicts(id, conflictList(id, l)) }
    }

    private fun save() = store.save(state)
    private fun status(id: String, f: (Status) -> Status) = _statuses.update { it + (id to f(it[id] ?: Status())) }
    private fun setConflicts(id: String, list: List<SharedConflict>?) = _conflicts.update { if (list == null) it - id else it + (id to list) }
    private fun notice(n: SharedNotice) = _notices.update { if (n in it) it else listOf(n) + it }
    fun dismissNotice(n: SharedNotice) = _notices.update { it - n }

    // ---- links and roles

    fun link(id: String): SharedLink? = state.links[id]
    private fun live(id: String): SharedLink = link(id)?.takeIf { !it.gone } ?: throw SharedException("This collection isn't shared.")
    val linkedWorkspaces: Set<String> get() = state.links.values.map { it.workspace }.toSet()
    /** Workspaces this account is in that aren't open on this phone ("Shared with you"). */
    val joinable: List<SharedWorkspace> get() = _workspaces.value.filter { it.id !in linkedWorkspaces }

    /** "Shared with Acme", "Shared", "No longer shared". */
    fun badge(id: String): String? {
        val l = link(id) ?: return null
        if (l.gone) return "No longer shared"
        if (!l.orgName.isNullOrEmpty() && !l.orgPersonal) return "Shared with ${l.orgName}"
        return "Shared"
    }

    fun applyRoles() {
        val teams = cloud.settings.value.orgs.filter { !it.personal }.map { it.id }.toSet()
        library.setShared(
            state.links.mapValues { (_, l) -> if (l.gone) "removed" else l.role },
            state.links.mapValues { (_, l) -> l.org?.takeIf { it in teams } },
        )
    }

    private fun updateLinks(f: (MutableMap<String, SharedLink>) -> Unit) {
        val m = state.links.toMutableMap()
        f(m)
        state = state.copy(links = m)
    }

    /** The signed-in account's links; another account's are kept, not synced (`gone`). This device's own key is pinned. */
    fun checkAccount(): Boolean {
        val a = cloud.account ?: return false
        val secret = cloud.deviceSecret() ?: return false
        val key = cloud.accountKey
        if (state.account != key) {
            if (state.account.isNotEmpty()) updateLinks { m -> m.replaceAll { _, l -> l.copy(gone = true) } }
            state = state.copy(account = key)
        }
        if (state.pins[a.keyId] == null) runCatching { SyncCore.publicKey(secret) }.getOrNull()?.let { state = state.copy(pins = state.pins + (a.keyId to it)) }
        save()
        applyRoles()
        return true
    }

    /** The account was deleted: its shared collections stay as last synced, never synced again. */
    fun accountDeleted(a: CloudAccount) {
        updateLinks { m -> m.replaceAll { _, l -> l.copy(gone = true) } }
        state = state.copy(pins = state.pins - a.keyId, account = "")
        save()
        applyRoles()
    }

    private fun me(): Pair<CloudAccount, String> {
        if (!checkAccount()) throw SharedException("Sign in to HITOP Cloud first.")
        val a = cloud.account ?: throw SharedException("Sign in to HITOP Cloud first.")
        return a to (cloud.deviceSecret() ?: throw SharedException("Sign in to HITOP Cloud first."))
    }

    /** This device's key fingerprint ("62bd a406 …"), for the person to read out to the others. */
    val fingerprint: String?
        get() = cloud.deviceSecret()?.let { s -> runCatching { SyncCore.fingerprint(SyncCore.publicKey(s)) }.getOrNull() }

    private suspend fun api(method: String, path: String, body: JsonElement? = null, allow: Set<Int> = emptySet()): CloudResponse = cloud.call(method, path, body, allow)
    private suspend fun json(method: String, path: String, body: JsonElement? = null): JsonObject = api(method, path, body).json ?: EmptyObject

    // ---- trust on first use

    fun trustOf(keyId: String, publicKey: String): MemberKey.Trust {
        val p = state.pins[keyId] ?: return MemberKey.Trust.NEW
        return if (p == publicKey) MemberKey.Trust.TRUSTED else MemberKey.Trust.CHANGED
    }

    /** Trusts device keys after the person compared their fingerprints; a changed key never replaces a trusted one. */
    fun trust(keys: List<MemberKey>) {
        state = state.copy(pins = state.pins + keys.filter { it.id !in state.pins }.associate { it.id to it.publicKey })
        save()
    }

    /** Trusts the device that handed this one a workspace key, after the person compared its fingerprint. */
    suspend fun trustSender(r: SyncCore.KeyRefusal) {
        val sender = r.senderKeyId
        val fp = r.fingerprint
        if (sender == null || fp == null) throw SharedException("That device isn't listed by the server: not trusting it")
        val info = json("GET", "/v1/workspaces/${r.workspace}")
        val k = (info["members"] as? JsonArray).orEmpty().flatMap { (it.jsonObject["keys"] as? JsonArray).orEmpty() }.map { it.jsonObject }.firstOrNull { it.str("id") == sender }
        val pk = k?.strOrNull("publicKey")
        if (pk == null || SyncCore.normalize(SyncCore.fingerprint(pk)) != SyncCore.normalize(fp)) throw SharedException("That device's key isn't the one you compared any more: not trusting it")
        if (trustOf(sender, pk) == MemberKey.Trust.CHANGED) throw SharedException("That device's key changed since you trusted it: not trusting the new one")
        state = state.copy(pins = state.pins + (sender to pk))
        save()
        if (pendingTrust.value == r) pendingTrust.value = null
    }

    private fun checkTrusted(keys: List<MemberKey>, who: String) {
        val bad = keys.filter { it.trust != MemberKey.Trust.TRUSTED }
        if (bad.any { it.trust == MemberKey.Trust.CHANGED }) throw SharedException("A device key of $who changed since you trusted it: not sharing the collection's key with it")
        if (bad.isNotEmpty()) throw SharedException("Check and trust the device keys of $who first (compare the fingerprints with them)")
    }

    private fun memberKeys(list: JsonElement?): List<MemberKey> = (list as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }.map { k ->
        val pk = k.str("publicKey")
        MemberKey(k.str("id"), k.str("name"), pk, k.bool("hasKey", true), !(k["legacyEpochs"] as? JsonArray).isNullOrEmpty(),
            runCatching { SyncCore.fingerprint(pk) }.getOrDefault(""), trustOf(k.str("id"), pk))
    }

    private fun member(m: JsonObject) = WorkspaceMember(m.str("userId"), m.str("email"), m.str("role", "viewer"), m.bool("runner", false), memberKeys(m["keys"]))

    // ---- keys

    data class Keys(val epoch: Int, val keys: Map<Int, String>, val legacy: Boolean)

    /** The keys this phone may use for [ws] (hitop-sync-crypto `accept_wraps`), or a [SyncCore.KeyRefusal]. */
    suspend fun workspaceKeys(ws: String): Keys {
        val (a, secret) = me()
        val r = json("GET", "/v1/workspaces/$ws/wraps?keyId=${a.keyId}")
        val wraps = (r["wraps"] as? JsonArray).orEmpty().mapNotNull { (it as? JsonObject)?.let { w -> w.strOrNull("epoch")?.toIntOrNull()?.let { e -> e to w.str("wrapped") } } }
        val headers = wraps.map { runCatching { SyncCore.wrapHeader(it.second) }.getOrDefault(SyncCore.WrapHeader(true, null, null)) }
        val listed = HashMap<String, String>()
        val names = HashMap<String, String>()
        if (headers.any { !it.legacy && state.pins[it.sender.orEmpty()] == null }) {
            val info = json("GET", "/v1/workspaces/$ws")
            (info["members"] as? JsonArray).orEmpty().map { it.jsonObject }.forEach { m ->
                (m["keys"] as? JsonArray).orEmpty().map { it.jsonObject }.forEach { k ->
                    listed[k.str("id")] = k.str("publicKey")
                    names[k.str("id")] = k.str("name").ifEmpty { "a device of ${m.str("email")}" }
                }
            }
        }
        try {
            val ring = SyncCore.acceptWraps(secret, a.keyId, ws, r.str("epoch", "0").toInt(), wraps, state.pins, listed, state.memory[ws] ?: EmptyObject)
            state = state.copy(memory = state.memory + (ws to ring.memory), pins = state.pins + ring.newPins)
            save()
            ring.details.forEach { notice(describe(it, names)) }
            return Keys(ring.current, ring.keys, headers.any { it.legacy })
        } catch (e: SyncCore.KeyRefusal) {
            if (e.kind == "untrustedSender" && e.fingerprint != null) pendingTrust.value = e
            throw e
        }
    }

    private fun wraps(keys: List<MemberKey>, ws: String, epoch: Int, key: String, head: Int? = null): JsonArray {
        val (a, secret) = me()
        return buildJsonArray {
            keys.forEach { k -> add(buildJsonObject { put("keyId", k.id); put("wrapped", SyncCore.sealWrap(secret, a.keyId, k.id, k.publicKey, ws, epoch, head ?: epoch, key)) }) }
        }
    }

    /** As an owner: replaces unauthenticated (hitop 1.1) wraps with authenticated ones for every trusted device. */
    private suspend fun upgradeWraps(ws: String, k: Keys) {
        val info = json("GET", "/v1/workspaces/$ws")
        if (info.str("role") != "owner") return
        val byEpoch = sortedMapOf<Int, MutableList<MemberKey>>()
        (info["members"] as? JsonArray).orEmpty().map { it.jsonObject }.filter { !it.bool("runner", false) }.forEach { m ->
            (m["keys"] as? JsonArray).orEmpty().map { it.jsonObject }.forEach { raw ->
                val epochs = (raw["legacyEpochs"] as? JsonArray).orEmpty().mapNotNull { (it as? JsonPrimitive)?.content?.toIntOrNull() }.filter { it in k.keys }
                val key = memberKeys(JsonArray(listOf(raw))).firstOrNull()
                if (epochs.isEmpty() || key == null || key.trust != MemberKey.Trust.TRUSTED) return@forEach
                epochs.forEach { e -> byEpoch.getOrPut(e) { ArrayList() }.add(key) }
            }
        }
        byEpoch.forEach { (e, list) -> api("POST", "/v1/workspaces/$ws/wraps", buildJsonObject { put("epoch", e); put("wraps", wraps(list, ws, e, k.keys.getValue(e), k.epoch)) }) }
    }

    // ---- files

    companion object {
        val EMPTY_STATE: JsonObject = buildJsonObject { put("seq", 0); put("entries", EmptyObject); put("conflicts", EmptyObject); put("losers", EmptyObject) }
        private fun isCollectionFile(p: String) = p == "hitop.yaml" || p.startsWith("requests/")

        /** A key notice in words for people: the device's name (never its key id), the fingerprint as secondary text. */
        fun describe(n: SyncCore.KeyNotice, names: Map<String, String>): SharedNotice {
            val device = n.senderKeyId?.let { names[it] }?.takeIf { it.isNotEmpty() } ?: "a device"
            val cap = device.replaceFirstChar { it.uppercase() }
            val fp = n.fingerprint?.let { "Fingerprint $it" }
            return when (n.kind) {
                "senderPinned" -> SharedNotice("Trusted $device — the device your invite code named", fp)
                "senderKeyChanged" -> SharedNotice("Security warning: $device has a different key on the server now. Keys from it are refused; if its owner replaced it, they add the new device.", fp)
                "knownKeyFromUntrusted" -> SharedNotice("$cap sent a key you already had. Compare its fingerprint and trust it, or its next key will be refused.", fp)
                "untrustedIgnored" -> SharedNotice("An older key from $device, which you haven't trusted, wasn't used.", fp)
                "legacyAccepted" -> SharedNotice("This collection's key came from an older HITOP that didn't sign it. It's in use; the owner's next sync replaces it with a signed one.")
                "legacyIgnored" -> SharedNotice("An older key that wasn't signed wasn't used: files still under it can't be read until the owner syncs.")
                else -> SharedNotice(n.senderKeyId?.takeIf { it.isNotEmpty() }?.let { n.message.replace("device $it", device) } ?: n.message, fp)
            }
        }
    }

    /**
     * The workspace's files with this phone's edits: the stored folder, with hitop.yaml and
     * requests/ as the collection now is here, rewritten only where they differ from what the
     * stored files read back as (another client's hand-written or `.yml` files keep their bytes).
     * environments/, globals.yaml and comments/ stay as they came.
     */
    fun localFiles(id: String): Map<String, String> {
        val l = live(id)
        val files = store.files(l.workspace).toMutableMap()
        val c = library.state.value.collection(id)
        if (l.role == "viewer" || c == null) return files
        val own = SyncCore.toFiles(c).filterKeys(::isCollectionFile)
        val base = if (files["hitop.yaml"] == null) emptyMap() else SyncCore.toFiles(SyncCore.readFiles(files, c.name)).filterKeys(::isCollectionFile)
        fun stored(p: String): String {
            if (!p.endsWith(".yaml")) return p
            val yml = p.dropLast(5) + ".yml"
            return if (files[p] == null && files[yml] != null) yml else p
        }
        own.forEach { (p, text) -> if (base[p] != text) files[stored(p)] = text }
        base.keys.filter { it !in own }.forEach { files.remove(stored(it)) }
        return files
    }

    /** Writes the workspace's files here and shows the collection they hold; its id (new when hitop.yaml's id changed). */
    fun applyFiles(id: String, files: Map<String, String>): String {
        val l = live(id)
        store.saveFiles(l.workspace, files)
        val c = SyncCore.readFiles(files, library.state.value.collection(id)?.name ?: "Shared collection")
        if (c.id != id) {
            updateLinks { it[c.id] = l; it.remove(id) }
            setConflicts(id, null)
            _statuses.update { s -> (s - id) + (c.id to (s[id] ?: Status())) }
            save()
            applyRoles()
            library.applyRemoteRemoval(id)
        }
        library.applyRemote(c)
        return c.id
    }

    // ---- transport

    private suspend fun changes(ws: String, since: Long, keys: Map<Int, String>): Pair<List<JsonObject>, Long> {
        val out = ArrayList<JsonObject>()
        var cursor = since
        var seq = since
        while (true) {
            val r = json("GET", "/v1/workspaces/$ws/changes?since=$cursor")
            seq = r.strOrNull("seq")?.toLongOrNull() ?: seq
            val list = (r["changes"] as? JsonArray).orEmpty().map { it.jsonObject }
            for (c in list) {
                val id = c.strOrNull("id") ?: continue
                cursor = maxOf(cursor, c.strOrNull("seq")?.toLongOrNull() ?: cursor)
                if (c.bool("deleted", false)) {
                    out.add(buildJsonObject { put("blob", id); put("version", c["version"] ?: JsonPrimitive(0)); put("vv", c["vv"] ?: EmptyObject); put("deleted", true) })
                    continue
                }
                val b = json("GET", "/v1/workspaces/$ws/blobs/$id")
                val envelope = b.str("content")
                val epoch = SyncCore.blobEpoch(envelope)
                val key = keys[epoch] ?: throw SharedException("A file is under key epoch $epoch, which this device doesn't have")
                val version = b.str("version", "0").toLong()
                val (path, text) = SyncCore.open(key, ws, id, version, envelope)
                out.add(buildJsonObject { put("blob", id); put("version", version); put("vv", b["vv"] ?: EmptyObject); put("deleted", false); put("path", path); put("text", text) })
            }
            if (!r.bool("more", false) || list.isEmpty()) break
        }
        return out to seq
    }

    /** One write; the refusal's code (conflict, stale_epoch) or null. */
    private suspend fun put(ws: String, push: JsonObject, epoch: Int, key: String): String? {
        val blob = push.strOrNull("blob") ?: return "conflict"
        val version = push.strOrNull("version")?.toLongOrNull() ?: return "conflict"
        val body = buildJsonObject {
            put("version", version); put("vv", push["vv"] ?: EmptyObject); put("epoch", epoch)
            val text = push["text"]?.takeIf { it !is JsonNull }?.let { (it as JsonPrimitive).content }
            if (text != null) put("content", SyncCore.seal(key, ws, blob, version, epoch, push.str("path"), text)) else put("deleted", true)
        }
        val r = api("PUT", "/v1/workspaces/$ws/blobs/$blob", body, allow = setOf(409))
        return if (r.status == 409) r.json?.str("error")?.ifEmpty { "conflict" } ?: "conflict" else null
    }

    private fun JsonElement.with(key: String, value: JsonElement): JsonObject = JsonObject(LinkedHashMap(this as? JsonObject ?: EmptyObject).apply { put(key, value) })

    // ---- sync

    /** Pulls and pushes one shared collection (a viewer only pulls). */
    suspend fun syncNow(id: String): SharedResult {
        val l = live(id)
        val ws = l.workspace
        val (a, _) = me()
        var cid = id
        status(cid) { it.copy(syncing = true) }
        try {
            var k = workspaceKeys(ws)
            if (k.legacy) runCatching { upgradeWraps(ws, k) }
            // The folder's files are gone or unreadable here: start over from the server's, deleting nothing.
            if (store.files(ws).isEmpty() && (state.states[ws] ?: EMPTY_STATE) != EMPTY_STATE) {
                state = state.copy(states = state.states - ws)
                save()
            }
            repeat(5) {
                val st = state.states[ws] ?: EMPTY_STATE
                val (remote, seq) = changes(ws, (st as? JsonObject)?.strOrNull("seq")?.toLongOrNull() ?: 0, k.keys)
                val local = localFiles(cid)
                val plan = SyncCore.call("plan", buildJsonObject {
                    put("local", JsonObject(local.mapValues { JsonPrimitive(it.value) })); put("state", st); put("remote", JsonArray(remote)); put("me", a.keyId)
                }).jsonObject
                var next: JsonElement = (plan["state"] ?: EMPTY_STATE).with("seq", JsonPrimitive(seq))
                val write = plan.obj("write")?.mapValues { (it.value as JsonPrimitive).content } ?: emptyMap()
                val remove = plan.stringList("remove")
                if (write.isNotEmpty() || remove.isNotEmpty()) {
                    val files = (local + write).toMutableMap()
                    remove.forEach { files.remove(it) }
                    val before = cid
                    cid = applyFiles(cid, files)
                    if (before != cid) status(cid) { it.copy(syncing = true) }
                } else if (library.state.value.collection(cid) == null && local.isNotEmpty()) {
                    // The collection's own file was lost (a deleted collection drops its link).
                    cid = applyFiles(cid, local)
                } else {
                    store.saveFiles(ws, local)
                }
                state = state.copy(states = state.states + (ws to next))
                save()
                val pushed = ArrayList<String>()
                setConflicts(cid, conflictList(cid, live(cid)))
                val base = SharedResult(write.keys.sorted(), remove, pushed, plan.stringList("merged"), plan.stringList("conflicts"), cid)
                if (l.role == "viewer") {
                    synced(cid)
                    return base
                }
                var retry = false
                for (p in (plan["push"] as? JsonArray).orEmpty().map { it.jsonObject }) {
                    val refused = put(ws, p, k.epoch, k.keys[k.epoch].orEmpty())
                    if (refused != null) {
                        if (refused == "stale_epoch") k = workspaceKeys(ws)
                        retry = true
                        break
                    }
                    next = SyncCore.call("pushed", buildJsonObject { put("state", next); put("push", p) })
                    state = state.copy(states = state.states + (ws to next))
                    save()
                    p.strOrNull("path")?.takeIf { it.isNotEmpty() }?.let { pushed.add(it) }
                }
                if (!retry) {
                    synced(cid)
                    runCatching { refreshShares(ws, cid) }
                    return base.copy(pushed = pushed.toList())
                }
            }
            throw SharedException("The shared collection kept changing while syncing; try again")
        } catch (e: Exception) {
            if ((e as? CloudException)?.status == 401) cloud.sessionEnded()
            status(cid) { it.copy(error = StoreText.withoutPlanLink(e.message ?: e.toString())) }
            throw e
        } finally {
            status(cid) { it.copy(syncing = false) }
        }
    }

    private fun synced(id: String) = status(id) { it.copy(error = null, lastSync = System.currentTimeMillis()) }

    /** A change made here: synced a moment later (one round at a time per collection). */
    fun schedule(id: String) {
        val l = link(id) ?: return
        if (!cloud.signedIn || l.gone || l.role == "viewer") return
        pending[id]?.cancel()
        pending[id] = scope.launch {
            delay(pushDelayMs)
            run(id)
        }
    }

    private suspend fun run(id: String) {
        if (id in running) {
            again.add(id)
            return
        }
        running.add(id)
        runCatching { syncNow(id) }
        running.remove(id)
        if (again.remove(id)) run(id)
    }

    /** Lists the account's workspaces, syncs every shared collection, hands keys to members' new devices. */
    suspend fun syncAll() {
        if (!checkAccount()) return
        try {
            refreshWorkspaces()
        } catch (_: Exception) {
            return
        }
        state.links.entries.sortedBy { it.key }.filter { !it.value.gone }.forEach { (id, l) ->
            if (runCatching { syncNow(id) }.isSuccess && l.role != "viewer") runCatching { shareKeysWithNewDevices(id) }
        }
        loadTeamEnvironments()
    }

    /** Waits for scheduled pushes (tests, Sync now). */
    suspend fun flush() {
        pending.keys.toList().forEach { id ->
            pending.remove(id)?.cancel()
            run(id)
        }
    }

    suspend fun refreshWorkspaces() {
        val r = json("GET", "/v1/workspaces")
        _workspaces.value = (r["workspaces"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }.mapNotNull { w ->
            val id = w.strOrNull("id") ?: return@mapNotNull null
            val o = w.obj("org")
            SharedWorkspace(id, w.str("role", "viewer"), w.str("epoch", "1").toInt(), o?.strOrNull("id"), o?.strOrNull("name"), o?.bool("personal", false) ?: false)
        }
        val byId = _workspaces.value.associateBy { it.id }
        updateLinks { m ->
            m.entries.toList().forEach { (id, l) ->
                val w = byId[l.workspace]
                if (w != null) {
                    m[id] = l.copy(gone = false, role = w.role, org = w.org, orgName = w.orgName, orgPersonal = w.orgPersonal)
                } else if (!l.gone) {
                    m[id] = l.copy(gone = true)
                    val name = library.state.value.collection(id)?.name ?: "A shared collection"
                    notice(SharedNotice("You're no longer a member of “$name”: its copy stays on this device and no longer syncs."))
                }
            }
        }
        save()
        applyRoles()
    }

    // ---- conflicts

    private fun conflictList(id: String, l: SharedLink): List<SharedConflict> {
        val st = state.states[l.workspace] as? JsonObject ?: EMPTY_STATE
        val open = st.obj("conflicts") ?: EmptyObject
        if (open.isEmpty()) return emptyList()
        val local = runCatching { localFiles(id) }.getOrElse { store.files(l.workspace) }
        return open.keys.sorted().map { path ->
            val c = open[path] as? JsonObject ?: EmptyObject
            val mine = local[path]
            val text = mine ?: c.strOrNull("theirs") ?: ""
            val name = text.lines().firstOrNull { it.startsWith("name:") }?.removePrefix("name:")?.trim(' ', '\'', '"')
            SharedConflict(path, name ?: path, c.strOrNull("base"), mine, c.strOrNull("theirs"))
        }
    }

    enum class Choice { MINE, THEIRS, BOTH }

    /** Resolves a conflict: keep mine (pushed next), take theirs, or keep both (theirs, and mine as "… (mine)"), then syncs. */
    suspend fun resolve(id: String, path: String, choice: Choice) {
        val l = live(id)
        val ws = l.workspace
        val local = localFiles(id)
        val mineText = local[path]
        var copy: tech.rws8.hitop.core.Request? = null
        if (choice == Choice.BOTH && mineText != null && path.startsWith("requests/")) {
            val probe = mapOf(path to mineText, "hitop.yaml" to (local["hitop.yaml"] ?: "format: 1\nname: x\n"))
            SyncCore.readFiles(probe, "x").requests.firstOrNull()?.let { r ->
                val stem = path.removePrefix("requests/").removeSuffix(".yaml").removeSuffix(".yml")
                copy = r.set("id", JsonPrimitive(Ids.next())).withName("${r.name} (mine)").withFile("$stem-mine")
            }
        }
        val r = SyncCore.call("resolve", buildJsonObject { put("state", state.states[ws] ?: EMPTY_STATE); put("path", path); put("keepMine", choice == Choice.MINE) }).jsonObject
        state = state.copy(states = state.states + (ws to (r["state"] ?: EMPTY_STATE)))
        save()
        var cid = id
        val resolution = r.obj("resolution") ?: EmptyObject
        val action = resolution.str("action", "keepMine")
        if (action != "keepMine") {
            val files = local.toMutableMap()
            if (action == "write") files[path] = resolution.str("text") else files.remove(path)
            cid = applyFiles(id, files)
        }
        copy?.let { library.addRequestCopy(it, cid) }
        setConflicts(cid, conflictList(cid, live(cid)))
        syncNow(cid)
    }

    // ---- workspaces

    /** Shares a collection: a new workspace in its organization, epoch 1's key wrapped for every device of this account (trusted here first). */
    suspend fun share(id: String): SharedResult {
        me()
        val c = library.state.value.collection(id)
        if (c == null || link(id) != null) throw SharedException("This collection is shared already")
        val keys = memberKeys(json("GET", "/v1/me")["keys"])
        checkTrusted(keys, "your other devices")
        val ws = SyncCore.randomId()
        val key = SyncCore.newKey()
        val org = c.org ?: cloud.orgForNew
        api("POST", "/v1/workspaces", buildJsonObject { put("id", ws); put("wraps", wraps(keys, ws, 1, key)); org?.let { put("org", it) } })
        val o = cloud.settings.value.orgs.firstOrNull { it.id == org }
        state = state.copy(memory = state.memory + (ws to buildJsonObject { put("strict", true) }), states = state.states + (ws to EMPTY_STATE))
        updateLinks { it[id] = SharedLink(ws, "owner", org, o?.name, o?.personal ?: (org == null)) }
        save()
        store.saveFiles(ws, SyncCore.toFiles(c))
        applyRoles()
        val r = syncNow(id)
        runCatching { refreshWorkspaces() }
        return r
    }

    /** The device keys of this account that aren't trusted here yet (Share needs them all). */
    suspend fun ownUntrustedDevices(): List<MemberKey> = memberKeys(json("GET", "/v1/me")["keys"]).filter { it.trust != MemberKey.Trust.TRUSTED }

    /** Opens a workspace this account is a member of as a collection here. */
    suspend fun join(ws: String): String {
        val (a, _) = me()
        val k = workspaceKeys(ws)
        val (remote, seq) = changes(ws, 0, k.keys)
        val plan = SyncCore.call("plan", buildJsonObject { put("local", EmptyObject); put("state", EMPTY_STATE); put("remote", JsonArray(remote)); put("me", a.keyId) }).jsonObject
        val files = plan.obj("write")?.mapValues { (it.value as JsonPrimitive).content } ?: emptyMap()
        if (files["hitop.yaml"] == null) throw SharedException("This shared collection has nothing in it yet")
        val c = SyncCore.readFiles(files, "Shared collection")
        val w = _workspaces.value.firstOrNull { it.id == ws }
        updateLinks { it[c.id] = SharedLink(ws, w?.role ?: "viewer", w?.org, w?.orgName, w?.orgPersonal ?: true) }
        state = state.copy(states = state.states + (ws to (plan["state"] ?: EMPTY_STATE).with("seq", JsonPrimitive(seq))))
        save()
        store.saveFiles(ws, files)
        // The link first: the collection is never the personal library's, not even for a moment.
        applyRoles()
        library.applyRemote(c)
        synced(c.id)
        setConflicts(c.id, emptyList())
        return c.id
    }

    /** Stops syncing a collection deleted here (the workspace and its files on the server stay). */
    fun forget(id: String) {
        val l = state.links[id] ?: return
        updateLinks { it.remove(id) }
        if (state.links.values.none { it.workspace == l.workspace }) {
            state = state.copy(states = state.states - l.workspace)
            store.dropFiles(l.workspace)
        }
        pending.remove(id)?.cancel()
        _statuses.update { it - id }
        setConflicts(id, null)
        save()
        applyRoles()
    }

    // ---- members

    suspend fun members(id: String): WorkspaceInfo {
        val ws = live(id).workspace
        val info = json("GET", "/v1/workspaces/$ws")
        val role = info.str("role", "viewer")
        val invites = if (role == "owner") (json("GET", "/v1/workspaces/$ws/invites")["invites"] as? JsonArray).orEmpty().map { it.jsonObject }.map { i ->
            val acc = i["accepted"] as? JsonObject
            WorkspaceInvite(i.str("id"), i.str("email"), i.str("role", "editor"), acc?.let { WorkspaceMember(it.str("userId"), it.str("email"), i.str("role", "editor"), false, memberKeys(it["keys"])) })
        } else emptyList()
        return WorkspaceInfo(ws, info.str("epoch", "1").toInt(), role, (info["members"] as? JsonArray).orEmpty().map { member(it.jsonObject) }, invites)
    }

    /** An invite (mailed when [email] is given): the code `<token>.<this device's fingerprint>`. */
    suspend fun invite(id: String, email: String, role: String): Pair<String, Boolean> {
        val ws = live(id).workspace
        val r = json("POST", "/v1/workspaces/$ws/invites", buildJsonObject { put("role", role); email.trim().takeIf { it.isNotEmpty() }?.let { put("email", it) } })
        val token = r.strOrNull("token")
        val fp = fingerprint
        if (token == null || fp == null) throw SharedException("The server's invite has no code")
        return "$token.${SyncCore.normalize(fp)}" to r.bool("mailed", false)
    }

    /** Accepts an invite code; from now on only authenticated keys, and the device the code names is trusted. */
    suspend fun accept(code: InviteCode): Pair<String, String> {
        me()
        val r = json("POST", "/v1/invites/accept", buildJsonObject { put("token", code.token) })
        val ws = r.strOrNull("workspaceId") ?: throw SharedException("The server's answer has no workspace")
        val m = LinkedHashMap((state.memory[ws] as? JsonObject) ?: EmptyObject)
        val expected = (m["expectedSenders"] as? JsonArray).orEmpty().mapNotNull { (it as? JsonPrimitive)?.content }.toMutableList()
        code.fingerprint?.let { if (it !in expected) expected.add(it) }
        m["strict"] = JsonPrimitive(true)
        m["expectedSenders"] = JsonArray(expected.map { JsonPrimitive(it) })
        state = state.copy(memory = state.memory + (ws to JsonObject(m)))
        save()
        return ws to r.str("role", "viewer")
    }

    /** Adds an account that accepted an invite, after its device keys were trusted here. */
    suspend fun addMember(id: String, email: String, role: String, keys: List<MemberKey>) {
        val ws = live(id).workspace
        checkTrusted(keys, email)
        val k = workspaceKeys(ws)
        val current = k.keys[k.epoch] ?: throw SharedException("This device has no key for the collection")
        api("POST", "/v1/workspaces/$ws/members", buildJsonObject { put("email", email); put("role", role); put("wraps", wraps(keys, ws, k.epoch, current)) })
        k.keys.toSortedMap().filterKeys { it != k.epoch }.forEach { (e, key) ->
            api("POST", "/v1/workspaces/$ws/wraps", buildJsonObject { put("epoch", e); put("wraps", wraps(keys, ws, e, key, k.epoch)) })
        }
        try {
            resealShares(ws)
        } catch (e: Exception) {
            notice(SharedNotice("Shared environment values not handed to them: ${e.message}"))
        }
    }

    /** Hands the workspace key to members' new devices whose keys are trusted here. */
    suspend fun shareKeysWithNewDevices(id: String): Int {
        val info = members(id)
        val waiting = info.members.flatMap { m -> m.keys.filter { !it.hasKey && it.trust == MemberKey.Trust.TRUSTED } }
        if (waiting.isEmpty() || info.role == "viewer") return 0
        val k = workspaceKeys(info.workspace)
        k.keys.toSortedMap().forEach { (e, key) -> api("POST", "/v1/workspaces/${info.workspace}/wraps", buildJsonObject { put("epoch", e); put("wraps", wraps(waiting, info.workspace, e, key, k.epoch)) }) }
        runCatching { resealShares(info.workspace) }
        return waiting.size
    }

    /** Removes a member: the next epoch's key for every remaining device, every file sealed again, shared values re-keyed. */
    suspend fun removeMember(id: String, userId: String): Int {
        val info = members(id)
        val ws = info.workspace
        val remaining = info.members.filter { it.userId != userId }.flatMap { it.keys }
        checkTrusted(remaining, "the remaining members")
        val (a, _) = me()
        val next = info.epoch + 1
        val key = SyncCore.newKey()
        api("DELETE", "/v1/workspaces/$ws/members/$userId", buildJsonObject { put("epoch", next); put("wraps", wraps(remaining, ws, next, key)) })
        // Their devices aren't trusted here any more.
        state = state.copy(pins = state.pins - (info.members.firstOrNull { it.userId == userId }?.keys?.map { it.id }?.toSet() ?: emptySet()))
        save()
        val k = workspaceKeys(ws)
        val (remote, _) = changes(ws, 0, k.keys)
        remote.filter { !it.bool("deleted", false) }.forEach { b ->
            val vv = LinkedHashMap((b["vv"] as? JsonObject) ?: EmptyObject)
            vv[a.keyId] = JsonPrimitive(((vv[a.keyId] as? JsonPrimitive)?.content?.toLongOrNull() ?: 0) + 1)
            val push = buildJsonObject {
                put("path", b["path"] ?: JsonPrimitive("")); put("blob", b["blob"] ?: JsonPrimitive("")); put("version", b.str("version", "0").toLong() + 1)
                put("vv", JsonObject(vv)); put("text", b["text"] ?: JsonNull)
            }
            put(ws, push, next, k.keys[next] ?: key)
        }
        try {
            rekeyShares(ws)
        } catch (e: Exception) {
            notice(SharedNotice("Shared environment values not re-keyed: ${e.message}"))
        }
        runCatching { syncNow(id) }
        return next
    }

    suspend fun setRole(id: String, userId: String, role: String) {
        api("PUT", "/v1/workspaces/${live(id).workspace}/members/$userId", buildJsonObject { put("role", role) })
        runCatching { refreshWorkspaces() }
    }

    // ---- shared environment values (HITOP Cloud C4)

    data class OpenedShare(val id: String, val version: Long, val content: JsonElement, val key: String, val wrappedFor: List<String>)

    private suspend fun openShares(ws: String): Pair<List<OpenedShare>, Int> {
        val r = api("GET", "/v1/workspaces/$ws/env-shares", allow = setOf(404))
        val j = r.json
        if (r.status != 200 || j == null) return emptyList<OpenedShare>() to 0
        val (a, secret) = me()
        val seen = state.shareVersions[ws] ?: emptyMap()
        val opened = ArrayList<OpenedShare>()
        var closed = 0
        for (sh in (j["shares"] as? JsonArray).orEmpty().map { it.jsonObject }) {
            val sid = sh.strOrNull("id") ?: continue
            val version = sh.strOrNull("version")?.toLongOrNull() ?: continue
            val wraps = (sh["wraps"] as? JsonArray).orEmpty().map { it.jsonObject }
            val mine = wraps.firstOrNull { it.str("keyId") == a.keyId }?.strOrNull("wrapped")
            if (mine == null) { closed++; continue }
            val sender = SyncCore.string("shareWrapSender", buildJsonObject { put("wrapped", mine) })
            val senderKey = state.pins[sender]
            if (senderKey == null) { closed++; continue }
            if (version < (seen[sid] ?: 0)) throw SharedException("The server hands out version $version of a shared environment this device saw at version ${seen[sid]}: refusing an older version")
            val o = SyncCore.call("shareOpen", buildJsonObject {
                put("secret", secret); put("keyId", a.keyId); put("senderPublicKey", senderKey); put("workspace", ws)
                put("shareId", sid); put("version", version); put("wrapped", mine); put("sealed", sh["sealed"] ?: JsonPrimitive(""))
            }).jsonObject
            val content = runCatching { HitopJson.parseToJsonElement(o.str("content", "{}")) }.getOrDefault(EmptyObject)
            opened.add(OpenedShare(sid, version, content, o.str("key"), wraps.map { it.str("keyId") }))
        }
        state = state.copy(shareVersions = state.shareVersions + (ws to (seen + opened.associate { it.id to it.version })))
        save()
        return opened to closed
    }

    private fun shareRecipients(info: JsonObject, c: JsonElement): List<MemberKey> {
        val co = c as? JsonObject ?: EmptyObject
        return (info["members"] as? JsonArray).orEmpty().map { it.jsonObject }.filter { m ->
            val role = m.str("role")
            if (m.bool("runner", false)) co.bool("allRunners", false) || (co["runners"] as? JsonArray).orEmpty().any { (it as? JsonPrimitive)?.content == m.str("userId") }
            else role == "owner" || role == "editor" || (role == "viewer" && co.bool("viewers", false))
        }.flatMap { memberKeys(it["keys"]) }
    }

    private fun shareWraps(keys: List<MemberKey>, ws: String, id: String, version: Long, key: String): JsonArray {
        val (a, secret) = me()
        return buildJsonArray {
            keys.forEach { k ->
                add(buildJsonObject {
                    put("keyId", k.id)
                    put("wrapped", SyncCore.string("shareWrap", buildJsonObject {
                        put("secret", secret); put("senderKeyId", a.keyId); put("recipientKeyId", k.id); put("recipientPublicKey", k.publicKey)
                        put("workspace", ws); put("shareId", id); put("version", version); put("key", key)
                    }))
                })
            }
        }
    }

    private suspend fun resealShares(ws: String): Int {
        val (opened, _) = openShares(ws)
        if (opened.isEmpty()) return 0
        val info = json("GET", "/v1/workspaces/$ws")
        if (info.str("role") == "viewer") return 0
        val myId = me().first.keyId
        var n = 0
        for (o in opened) {
            val missing = shareRecipients(info, o.content).filter { it.id !in o.wrappedFor && (it.id == myId || it.trust == MemberKey.Trust.TRUSTED) }
            if (missing.isEmpty()) continue
            val r = json("POST", "/v1/workspaces/$ws/env-shares/${o.id}/wraps", buildJsonObject { put("version", o.version); put("wraps", shareWraps(missing, ws, o.id, o.version, o.key)) })
            n += r.str("stored", "0").toIntOrNull() ?: 0
        }
        return n
    }

    private suspend fun rekeyShares(ws: String) {
        val (opened, closed) = openShares(ws)
        if (opened.isEmpty()) {
            if (closed > 0) notice(SharedNotice("$closed shared environment value set(s) couldn't be re-keyed from this device: ask whoever shared them to open the collection."))
            return
        }
        val info = json("GET", "/v1/workspaces/$ws")
        val myId = me().first.keyId
        for (o in opened) {
            val to = shareRecipients(info, o.content).filter { it.id == myId || it.trust == MemberKey.Trust.TRUSTED }
            val version = o.version + 1
            val key = SyncCore.newKey()
            val sealed = SyncCore.string("shareSeal", buildJsonObject { put("key", key); put("workspace", ws); put("shareId", o.id); put("version", version); put("content", tech.rws8.hitop.cloud.LibrarySync.canonical(o.content)) })
            api("PUT", "/v1/workspaces/$ws/env-shares/${o.id}", buildJsonObject { put("version", version); put("sealed", sealed); put("wraps", shareWraps(to, ws, o.id, version, key)) })
        }
    }

    private suspend fun refreshShares(ws: String, collectionId: String) {
        val (opened, _) = openShares(ws)
        library.setSharedValues(collectionId, opened.mapNotNull { o ->
            val c = o.content as? JsonObject ?: return@mapNotNull null
            c.strOrNull("environment")?.let { it to c.stringMap("values") }
        }.toMap())
        if (opened.isNotEmpty()) resealShares(ws)
    }

    // ---- comments (shared collections): `comments/<request file>/<id>.yaml`, one file each, as the web app

    data class Comment(val path: String, val author: String, val at: String, val text: String, val mine: Boolean)

    /** The request's path in the folder (`orders/create-order`), as hitop-core names its file. */
    private fun requestPath(id: String, requestId: String): String? {
        val c = library.state.value.collection(id) ?: return null
        c.request(requestId)?.file?.let { return it }
        // Its file is the one that holds a mark put in its URL (names don't depend on the URL).
        val mark = "https://mark.invalid/${SyncCore.randomId()}"
        val marked = c.withRequests(c.requests.map { if (it.id == requestId) it.withUrl(mark) else it })
        return SyncCore.toFiles(marked).entries.firstOrNull { it.key.startsWith("requests/") && it.value.contains(mark) }?.key?.removePrefix("requests/")?.removeSuffix(".yaml")
    }

    fun comments(id: String, requestId: String): List<Comment> {
        val l = link(id) ?: return emptyList()
        val file = runCatching { requestPath(id, requestId) }.getOrNull() ?: return emptyList()
        val prefix = "comments/$file/"
        val me = cloud.account?.email
        return store.files(l.workspace).filterKeys { it.startsWith(prefix) && '/' !in it.removePrefix(prefix) }.map { (path, text) ->
            // Written by hand: shown as text.
            val o = runCatching { HitopJson.parseToJsonElement(text).jsonObject }.getOrNull() ?: JsonObject(mapOf("text" to JsonPrimitive(text)))
            Comment(path, o.str("author", "?"), o.str("at"), o.str("text"), me != null && o.strOrNull("author") == me)
        }.sortedWith(compareBy({ it.at }, { it.path }))
    }

    suspend fun addComment(id: String, requestId: String, text: String) {
        val l = live(id)
        if (l.role == "viewer") throw SharedException("Viewers can read comments but not write them")
        val file = requestPath(id, requestId) ?: throw SharedException("Save the request first: comments belong to a saved request")
        val body = text.trim()
        if (body.isEmpty()) return
        if (body.length > 10_000) throw SharedException("A comment is at most 10,000 characters")
        val (a, _) = me()
        val comment = buildJsonObject {
            put("author", a.email); put("at", java.time.Instant.ofEpochMilli(System.currentTimeMillis()).toString())
            put("request", library.state.value.collection(id)?.request(requestId)?.name.orEmpty()); put("text", body)
        }
        // YAML is a superset of JSON: the file is JSON, readable as YAML by every hitop.
        val path = "comments/$file/${System.currentTimeMillis()}-${SyncCore.randomId().take(8)}.yaml"
        store.saveFiles(l.workspace, localFiles(id) + (path to tech.rws8.hitop.core.PrettyJson.encodeToString(JsonObject.serializer(), comment) + "\n"))
        syncNow(id)
    }

    suspend fun deleteComment(id: String, c: Comment) {
        val l = live(id)
        if (!c.mine) throw SharedException("Only your own comments can be deleted")
        store.saveFiles(l.workspace, localFiles(id) - c.path)
        syncNow(id)
    }

    /** Deletes the shared collection for everyone; it stays on this phone as an unshared one. */
    suspend fun deleteWorkspace(id: String) {
        val l = live(id)
        api("DELETE", "/v1/workspaces/${l.workspace}")
        forget(id)
        library.onLibraryChange?.invoke()
        runCatching { refreshWorkspaces() }
    }

    // ---- team environments (sync-server teamenvs.rs)

    /** What the server keeps of [e]: secret variables empty (each member keeps their own values). */
    private fun teamData(e: Environment): JsonObject = buildJsonObject {
        put("name", e.name)
        put("variables", JsonObject(e.variables.mapValues { (n, v) -> JsonPrimitive(if (n in e.secrets) "" else v) }))
        put("secrets", JsonArray(e.secrets.map(::JsonPrimitive)))
    }

    private fun sameData(a: JsonObject, b: JsonObject) = tech.rws8.hitop.cloud.LibrarySync.canonical(a) == tech.rws8.hitop.cloud.LibrarySync.canonical(b)

    private fun teamEnvironment(id: String, org: String, data: JsonObject, updatedBy: String?): Environment {
        var env = Environment(data)
        val vars = LinkedHashMap(data.obj("variables") ?: EmptyObject)
        env.secrets.forEach { vars[it] = JsonPrimitive("") }
        env = env.set("id", JsonPrimitive(id)).set("variables", JsonObject(vars)).set("team", JsonPrimitive(org)).set("org", JsonPrimitive(org))
        return updatedBy?.takeIf { it.isNotEmpty() }?.let { env.set("updatedBy", JsonPrimitive(it)) } ?: env
    }

    private fun team() = library.state.value.environments.filter { it.team != null }

    private fun saveTeamState() = store.saveTeam(buildJsonObject {
        put("account", cloud.accountKey)
        put("environments", JsonArray(team().map { it.json }))
        put("known", JsonObject(teamKnown.mapValues { (_, k) -> buildJsonObject { put("org", k.org); put("version", k.version); put("data", k.data) } }))
    })

    private fun loadTeamState() {
        val o = store.loadTeam()
        teamAccount = cloud.accountKey
        if (!cloud.signedIn || o.str("account") != cloud.accountKey) return
        o.obj("known")?.forEach { (id, v) -> (v as? JsonObject)?.let { teamKnown[id] = TeamKnown(it.str("org"), it.str("version", "0").toLongOrNull() ?: 0, it.obj("data") ?: EmptyObject) } }
        val list = (o["environments"] as? JsonArray).orEmpty().mapNotNull { (it as? JsonObject)?.let(::Environment) }
        if (list.isNotEmpty()) library.applyTeamEnvironments(list, null)
    }

    private fun scheduleTeam(delayMs: Long = pushDelayMs) {
        if (!cloud.signedIn) return
        teamJob?.cancel()
        teamJob = scope.launch {
            delay(delayMs)
            runCatching { teamLock.withLock { pushTeam() } }
                .onFailure { if (it is kotlinx.coroutines.CancellationException) throw it else notice(SharedNotice("Team environments aren't saved yet: ${it.message}. They go with the next sync.")) }
        }
    }

    /** Sends what changed here: one PUT or DELETE per environment, each naming the version it saw. */
    private suspend fun pushTeam() {
        for (e in team()) {
            val org = e.team ?: continue
            val data = teamData(e)
            val k = teamKnown[e.id]?.takeIf { it.org == org }
            if (k != null && sameData(k.data, data)) continue
            val r = api("PUT", "/v1/orgs/$org/environments/${e.id}", buildJsonObject { put("data", data); put("baseVersion", k?.version ?: 0) }, allow = setOf(409))
            if (r.status == 409) {
                val cur = r.json?.obj("current")
                val theirs = cur?.obj("data")
                if (cur != null && theirs != null) {
                    teamKnown[e.id] = TeamKnown(org, cur.str("version", "0").toLong(), theirs)
                    val by = cur.strOrNull("updatedBy")
                    replaceTeam(e.id, teamEnvironment(e.id, org, theirs, by))
                    notice(SharedNotice("${e.name} was changed by ${by?.takeIf { it.isNotEmpty() } ?: "someone in the organization"}: showing theirs."))
                } else if (k != null) {
                    // Deleted by someone else meanwhile.
                    teamKnown.remove(e.id)
                    replaceTeam(e.id, null)
                    notice(SharedNotice("${e.name} was deleted by someone in the organization."))
                }
            } else {
                teamKnown[e.id] = TeamKnown(org, r.json?.str("version", "0")?.toLongOrNull() ?: ((k?.version ?: 0) + 1), data)
            }
            saveTeamState()
        }
        val here = team().associate { it.id to it.team }
        for ((id, k) in teamKnown.entries.map { it.key to it.value }) {
            if (here[id] == k.org) continue
            api("DELETE", "/v1/orgs/${k.org}/environments/$id?baseVersion=${k.version}", allow = setOf(403, 404, 409))
            teamKnown.remove(id)
            saveTeamState()
        }
    }

    private fun replaceTeam(id: String, e: Environment?) = library.applyTeamEnvironments(team().mapNotNull { if (it.id == id) e else it }, null)

    /** The current organization's team environments: edits made here first, then the server's. */
    suspend fun loadTeamEnvironments() = teamLock.withLock {
        checkTeamAccount()
        val org = cloud.currentOrg?.takeIf { cloud.signedIn && !it.personal }
        if (cloud.signedIn) {
            try {
                pushTeam()
            } catch (e: kotlinx.coroutines.CancellationException) {
                throw e
            } catch (_: Exception) {
                // Offline: what is here stays.
                return@withLock
            }
        }
        if (org == null) {
            library.applyTeamEnvironments(emptyList(), null)
            teamKnown.clear()
            saveTeamState()
            return@withLock
        }
        val before = team()
        val r = runCatching { api("GET", "/v1/orgs/${org.id}/environments", allow = setOf(400, 403, 404)) }.getOrNull()
        val j = r?.json
        if (r?.status != 200 || j == null) return@withLock
        // Edited while the answer was on its way: those go first, with the next round.
        if (team() != before) return@withLock scheduleTeam()
        teamKnown.clear()
        val list = (j["environments"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }.mapNotNull { e ->
            val id = e.strOrNull("id") ?: return@mapNotNull null
            val data = e.obj("data") ?: EmptyObject
            teamKnown[id] = TeamKnown(org.id, e.str("version", "0").toLongOrNull() ?: 0, data)
            teamEnvironment(id, org.id, data, e.strOrNull("updatedBy"))
        }
        library.applyTeamEnvironments(list, org.id)
        saveTeamState()
    }
}
