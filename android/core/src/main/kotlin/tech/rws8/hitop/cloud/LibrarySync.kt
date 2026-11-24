package tech.rws8.hitop.cloud

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.core.AutoName
import tech.rws8.hitop.core.Collection
import tech.rws8.hitop.core.EmptyObject
import tech.rws8.hitop.core.Environment
import tech.rws8.hitop.core.Globals
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.Ids
import tech.rws8.hitop.core.Library
import tech.rws8.hitop.core.Seed
import tech.rws8.hitop.core.bool
import tech.rws8.hitop.core.obj
import tech.rws8.hitop.core.str
import tech.rws8.hitop.core.strOrNull
import tech.rws8.hitop.core.stringList
import tech.rws8.hitop.core.stringMap
import tech.rws8.hitop.core.toJsonArray
import tech.rws8.hitop.core.toJsonObject
import java.io.File

/**
 * The personal library (HITOP Cloud C8, `GET /v1/library`, `PUT`/`DELETE /v1/library/:kind/:id`),
 * the web app's `library.js` and the iOS app's LibrarySync item for item:
 *
 * - items: `collection/<id>` (the web app's JSON), `environments/all` (`{environments,
 *   activeEnvironmentId}`, secret values empty) and `globals/all` (`{variables, secrets}`); secret
 *   values travel apart in `secrets` (the server seals them under its host key or doesn't keep
 *   them). On the phone they stay in the keystore-backed secret store only.
 * - every write names the version it last saw (`baseVersion`); a 409 keeps the server's item and
 *   this phone's collection as a copy "… (this phone)".
 * - deletes leave tombstones; a pull removes a collection only if this phone had it from the library.
 * - offline first: what differs from what the library last took (`known`, library-sync.json) is the
 *   queue; a transport error, 5xx or 429 stops the round and retries; a refused item stays here.
 * - the first link uploads this phone's collections except an untouched sample or an empty My
 *   requests; linking another account later uploads none (they stay on this phone).
 * - shared collections (A3) are never the library's.
 */
class LibrarySync(
    private val cloud: HitopCloud,
    private val library: Library,
    private val stateFile: File?,
    private val scope: CoroutineScope,
    val deviceLabel: String = "this phone",
) {
    data class Known(val version: Long, val hash: String)

    data class State(
        val account: String = "",
        val cursor: Long? = null,
        val known: Map<String, Known> = emptyMap(),
        val localOnly: List<String> = emptyList(),
        val refused: Map<String, String> = emptyMap(),
        val lastSync: Long? = null,
    ) {
        fun toJson(): JsonObject = buildJsonObject {
            put("account", account)
            cursor?.let { put("cursor", it) }
            put("known", JsonObject(known.mapValues { (_, k) -> buildJsonObject { put("version", k.version); put("hash", k.hash) } }))
            put("localOnly", localOnly.toJsonArray())
            put("refused", refused.toJsonObject())
            lastSync?.let { put("lastSync", it) }
        }

        companion object {
            fun of(o: JsonObject?): State = if (o == null) State() else State(
                o.str("account"), o.strOrNull("cursor")?.toLongOrNull(),
                o.obj("known")?.mapNotNull { (k, v) -> (v as? JsonObject)?.let { k to Known(it.str("version", "0").toLong(), it.str("hash")) } }?.toMap() ?: emptyMap(),
                o.stringList("localOnly"), o.stringMap("refused"), o.strOrNull("lastSync")?.toLongOrNull(),
            )
        }
    }

    data class Status(val on: Boolean = false, val saving: Boolean = false, val pulling: Boolean = false, val error: String? = null, val secretsStored: Boolean? = null, val lastSync: Long? = null)

    enum class Badge { LOCAL, WAITING, SYNCED, REFUSED }

    private val _status = MutableStateFlow(Status())
    val status: StateFlow<Status> = _status.asStateFlow()
    private val _badges = MutableStateFlow<Map<String, Badge>>(emptyMap())
    val badges: StateFlow<Map<String, Badge>> = _badges.asStateFlow()
    private val _notices = MutableStateFlow<List<String>>(emptyList())
    val notices: StateFlow<List<String>> = _notices.asStateFlow()

    var pushDelayMs = 1_000L
    var retryDelayMs = 15_000L
    var state: State = State.of(stateFile?.takeIf { it.exists() }?.let { runCatching { HitopJson.parseToJsonElement(it.readText()).jsonObject }.getOrNull() })
        private set
    private var pushJob: Job? = null
    private val roundLock = Mutex()
    private var pushAgain = false

    init {
        _status.update { it.copy(lastSync = state.lastSync) }
        library.onLibraryChange = { changed() }
        refreshBadges()
    }

    private fun save() {
        val f = stateFile ?: return
        tech.rws8.hitop.core.SafeFiles.write(f, state.toJson().toString())
    }

    /**
     * Files on this phone couldn't be read (LibraryStore.damaged): what they held isn't deleted.
     * The library forgets it had them, so the next pull brings everything again and no push
     * removes or empties them. Call before the first sync, then set the files aside.
     */
    fun forgetDamaged(store: tech.rws8.hitop.core.LibraryStore) {
        val lost = state.known.keys.filter { k ->
            when {
                k.startsWith("collection/") -> store.isCollectionDamaged(k.removePrefix("collection/"))
                k == "environments/all" -> store.isDamaged("environments.json")
                k == "globals/all" -> store.isDamaged("globals.json")
                else -> false
            }
        }
        if (lost.isEmpty()) return
        state = state.copy(known = state.known - lost.toSet(), cursor = null)
        save()
        refreshBadges()
    }

    // ---- account

    /** Off in Settings: nothing is pulled or pushed, and what waits stays here. */
    @Volatile var enabled: () -> Boolean = { true }

    fun checkAccount(): Boolean {
        val key = cloud.accountKey
        if (key.isEmpty() || !enabled()) {
            _status.update { it.copy(on = false) }
            pushJob?.cancel()
            refreshBadges()
            return false
        }
        if (state.account != key) {
            state = firstLink(key, state, library.state.value.collections)
            save()
        }
        _status.update { it.copy(on = true) }
        return true
    }

    // ---- items

    data class Item(val kind: String, val id: String, val data: JsonElement, val secrets: JsonObject) {
        val key: String get() = "$kind/$id"
        /** A short, stable fingerprint of the item as pushed (canonical JSON, FNV-1a 64). */
        val hash: String
            get() {
                val bytes = canonical(buildJsonObject { put("data", data); put("secrets", secrets) }).toByteArray(Charsets.UTF_8)
                var h = -0x340d631b7bdddcdbL // 0xcbf29ce484222325
                for (b in bytes) {
                    h = h xor (b.toLong() and 0xff)
                    h *= 0x100000001b3L
                }
                return java.lang.Long.toUnsignedString(h, 16) + "-" + bytes.size
            }
        val isEmptySettings: Boolean
            get() = kind != "collection" && ((data as? JsonObject)?.get("environments") as? JsonArray).isNullOrEmpty() && ((data as? JsonObject)?.get("variables") as? JsonObject).isNullOrEmpty()
    }

    private fun isTeam(e: Environment) = e.team != null

    fun collectionItem(c: Collection) = Item("collection", c.id, c.json, EmptyObject)

    val environmentsItem: Item
        get() {
            val s = library.state.value
            val envs = s.environments.filterNot(::isTeam)
            val secrets = LinkedHashMap<String, JsonElement>()
            val list = envs.map { e ->
                val vars = LinkedHashMap(e.json.obj("variables") ?: EmptyObject)
                val values = LinkedHashMap<String, JsonElement>()
                e.secrets.forEach { n ->
                    vars[n] = JsonPrimitive("")
                    library.secretValue(e.id, n)?.takeIf { it.isNotEmpty() }?.let { values[n] = JsonPrimitive(it) }
                }
                if (values.isNotEmpty()) secrets[e.id] = JsonObject(values)
                JsonObject(LinkedHashMap(e.json).apply { put("variables", JsonObject(vars)) })
            }
            val active = envs.firstOrNull { it.id == s.activeEnvironmentId }?.let { e -> e.json["id"] } ?: JsonNull
            return Item("environments", "all", buildJsonObject { put("environments", JsonArray(list)); put("activeEnvironmentId", active) }, JsonObject(secrets))
        }

    val globalsItem: Item
        get() {
            val g = library.state.value.globals
            val vars = LinkedHashMap(g.variables)
            val secrets = LinkedHashMap<String, JsonElement>()
            g.secrets.forEach { n ->
                vars[n] = ""
                library.globalSecretValue(n)?.takeIf { it.isNotEmpty() }?.let { secrets[n] = JsonPrimitive(it) }
            }
            return Item("globals", "all", buildJsonObject { put("variables", vars.toJsonObject()); put("secrets", g.secrets.toJsonArray()) }, JsonObject(secrets))
        }

    /** What the library should hold now, by key. */
    fun desired(): Map<String, Item> {
        val out = LinkedHashMap<String, Item>()
        val localOnly = state.localOnly.toSet()
        library.state.value.collections.filter { it.id !in localOnly && !library.isShared(it.id) }.forEach { c -> collectionItem(c).let { out[it.key] = it } }
        listOf(environmentsItem, globalsItem).forEach { out[it.key] = it }
        return out
    }

    // ---- applying the server's items

    fun apply(items: List<JsonObject>) {
        val known = state.known.toMutableMap()
        val localOnly = state.localOnly.toMutableList()
        val refused = state.refused.toMutableMap()
        for (it in items) {
            val kind = it.strOrNull("kind") ?: continue
            val id = it.strOrNull("id") ?: continue
            val version = it.strOrNull("version")?.toLongOrNull() ?: continue
            val key = "$kind/$id"
            val k = known[key]
            if (k != null && k.version >= version) continue
            val deleted = it.bool("deleted", false)
            when (kind) {
                "collection" -> {
                    if (deleted) {
                        if (k != null && library.state.value.collection(id) != null && !library.isShared(id)) library.applyRemoteRemoval(id)
                        known.remove(key)
                        continue
                    }
                    val data = it.obj("data") ?: continue
                    if (library.isShared(id)) continue
                    val c = Collection(data.with("id", it["data"]?.let { d -> (d as? JsonObject)?.get("id") } ?: JsonPrimitive(id)))
                    library.applyRemote(c)
                    localOnly.remove(id)
                    refused.remove(key)
                    known[key] = Known(version, collectionItem(c).hash)
                }
                "environments" -> if (!deleted) {
                    applyEnvironments(it)
                    known[key] = Known(version, environmentsItem.hash)
                }
                "globals" -> if (!deleted) {
                    applyGlobals(it)
                    known[key] = Known(version, globalsItem.hash)
                }
            }
        }
        state = state.copy(known = known, localOnly = localOnly, refused = refused)
        save()
    }

    private fun JsonObject.with(k: String, v: JsonElement?) = JsonObject(LinkedHashMap(this).apply { if (v == null) remove(k) else put(k, v) })

    private fun applyEnvironments(it: JsonObject) {
        val data = it.obj("data") ?: EmptyObject
        val secrets = it.obj("secrets") ?: EmptyObject
        val list = (data["environments"] as? JsonArray)?.mapNotNull { j ->
            var e = Environment(j as? JsonObject ?: return@mapNotNull null)
            val vars = LinkedHashMap(e.json.obj("variables") ?: EmptyObject)
            e.secrets.forEach { n ->
                val v = secrets.obj(e.id)?.strOrNull(n) ?: library.secretValue(e.id, n) ?: e.variables[n] ?: ""
                library.setSecretValue(e.id, n, v)
                // Listed with an empty value in the file, the value in the secret store.
                vars[n] = JsonPrimitive("")
            }
            e = e.set("variables", JsonObject(vars))
            e
        } ?: emptyList()
        val kept = library.state.value.environments.filter(::isTeam)
        val activeId = (data["activeEnvironmentId"] as? JsonPrimitive)?.takeIf { p -> p !is JsonNull }?.content
        val active = if (list.any { e -> e.id == activeId }) activeId else library.state.value.activeEnvironmentId
        library.applyRemoteEnvironments(list + kept, active)
    }

    private fun applyGlobals(it: JsonObject) {
        val data = it.obj("data") ?: EmptyObject
        val names = data.stringList("secrets")
        val vars = LinkedHashMap(data.stringMap("variables"))
        names.forEach { n ->
            val v = it.obj("secrets")?.strOrNull(n) ?: library.globalSecretValue(n) ?: vars[n] ?: ""
            library.setGlobalSecretValue(n, v)
            vars[n] = ""
        }
        library.applyRemoteGlobals(Globals(vars, names, library.state.value.globals.extra))
    }

    // ---- pull

    /** Other devices' changes (launch, coming to the foreground, pull to refresh). */
    suspend fun pull() {
        if (!checkAccount()) return
        roundLock.withLock { pullNow() }
    }

    private suspend fun pullNow() {
        _status.update { it.copy(pulling = true) }
        try {
            val first = state.cursor == null
            val path = state.cursor?.let { "/v1/library?since=$it" } ?: "/v1/library"
            val r = cloud.call("GET", path, allow = (400..599).toSet())
            if (r.status == 401) return cloud.sessionEnded()
            val j = r.json
            if (r.status != 200 || j == null) {
                _status.update { it.copy(error = "Couldn't read HITOP Cloud: ${HitopCloud.error(r).message}") }
                return
            }
            apply((j["items"] as? JsonArray)?.mapNotNull { it as? JsonObject } ?: emptyList())
            if (first) dropEmptyMyRequests()
            state = state.copy(cursor = j.strOrNull("cursor")?.toLongOrNull() ?: state.cursor)
            (j["secretsStored"] as? JsonPrimitive)?.content?.toBooleanStrictOrNull()?.let { v -> _status.update { it.copy(secretsStored = v) } }
            synced()
        } catch (e: CloudException) {
            _status.update { it.copy(error = "Working on this $deviceWord until HITOP Cloud answers: ${e.message}") }
        } finally {
            _status.update { it.copy(pulling = false) }
            refreshBadges()
        }
    }

    private fun dropEmptyMyRequests() {
        val cols = library.state.value.collections
        if (cols.none { state.known["collection/${it.id}"] != null && AutoName.isMyRequests(it) }) return
        val localOnly = state.localOnly.toMutableList()
        cols.filter { it.id in localOnly && AutoName.isEmptyMyRequests(it) }.forEach {
            library.applyRemoteRemoval(it.id)
            localOnly.remove(it.id)
        }
        state = state.copy(localOnly = localOnly)
        save()
    }

    private fun synced() {
        val now = System.currentTimeMillis()
        state = state.copy(lastSync = now)
        _status.update { it.copy(lastSync = now, error = null) }
        save()
    }

    // ---- push

    private fun changed() {
        refreshBadges()
        schedulePush()
    }

    fun schedulePush(delayMs: Long = pushDelayMs) {
        if (!cloud.signedIn || !enabled()) return
        pushJob?.cancel()
        pushJob = scope.launch {
            delay(delayMs)
            push()
        }
    }

    /** Pull, then push what waits: the whole round. */
    suspend fun sync() {
        if (!checkAccount()) return
        pull()
        push()
    }

    /** Pushes what waits until nothing does (Sync now, tests). */
    suspend fun flush() {
        pushJob?.cancel()
        repeat(3) {
            push()
            if (!dirty) return
        }
    }

    val dirty: Boolean
        get() {
            if (!_status.value.on) return false
            val want = desired()
            for ((key, item) in want) {
                if (state.known[key]?.hash != item.hash && state.refused[key] != item.hash && (state.known[key] != null || !item.isEmptySettings)) return true
            }
            return state.known.keys.any { it.startsWith("collection/") && want[it] == null }
        }

    private class Stop(val signedOut: Boolean, message: String) : Exception(message)

    suspend fun push() {
        if (!checkAccount()) return
        if (roundLock.isLocked) {
            pushAgain = true
            return
        }
        roundLock.withLock { pushNow() }
        if (pushAgain) {
            pushAgain = false
            schedulePush(0)
        }
    }

    private suspend fun pushNow() {
        _status.update { it.copy(saving = true) }
        val itemErrors = ArrayList<String>()
        try {
            val want = desired()
            val keys = want.keys.sortedWith(compareBy({ if (it.startsWith("collection/")) 1 else 0 }, { it }))
            for (key in keys) {
                val item = want[key] ?: continue
                val h = item.hash
                val known = state.known[key]
                if (known?.hash == h || state.refused[key] == h) continue
                if (known == null && item.isEmptySettings) continue
                val body = buildJsonObject { put("data", item.data); put("secrets", item.secrets); put("baseVersion", known?.version ?: 0) }
                val r = send("PUT", "/v1/library/${item.kind}/${item.id}", body)
                when (r.status) {
                    200 -> {
                        val v = r.json?.strOrNull("version")?.toLongOrNull() ?: ((known?.version ?: 0) + 1)
                        state = state.copy(known = state.known + (key to Known(v, h)), refused = state.refused - key)
                        (r.json?.get("secretsStored") as? JsonPrimitive)?.content?.toBooleanStrictOrNull()?.let { s -> _status.update { it.copy(secretsStored = s) } }
                    }
                    409 -> conflict(item, r.json?.obj("current"))
                    else -> {
                        state = state.copy(refused = state.refused + (key to h))
                        val name = if (item.kind == "collection") "“${(item.data as? JsonObject)?.str("name") ?: "A collection"}”" else if (item.kind == "globals") "The globals" else "The environments"
                        val why = when (r.status) { 402 -> "HITOP Cloud storage is full"; 413 -> "it is over 5 MB"; else -> HitopCloud.error(r).message }
                        itemErrors.add("$name wasn't saved to HITOP Cloud ($why). It stays on this $deviceWord.")
                    }
                }
                save()
            }
            for (key in state.known.keys.sorted().filter { it.startsWith("collection/") && want[it] == null }) {
                val k = state.known[key] ?: continue
                val id = key.removePrefix("collection/")
                val r = send("DELETE", "/v1/library/collection/$id?baseVersion=${k.version}", null)
                if (r.status == 409) conflict(Item("collection", id, JsonNull, EmptyObject), r.json?.obj("current"))
                else state = state.copy(known = state.known - key)
                save()
            }
            synced()
            _status.update { it.copy(error = itemErrors.firstOrNull()) }
            _notices.update { n -> itemErrors.filter { it !in n } + n }
        } catch (s: Stop) {
            if (s.signedOut) cloud.sessionEnded()
            else {
                _status.update { it.copy(error = s.message) }
                schedulePush(retryDelayMs)
            }
        } catch (e: CloudException) {
            _status.update { it.copy(error = e.message) }
        } finally {
            _status.update { it.copy(saving = false) }
            refreshBadges()
        }
    }

    private val deviceWord: String get() = deviceLabel.removePrefix("this ")

    private suspend fun send(method: String, path: String, body: JsonElement?): CloudResponse {
        val r = try {
            cloud.call(method, path, body, allow = (400..599).toSet())
        } catch (e: CloudException) {
            if (e.code == "not_signed_in") throw Stop(true, e.message)
            throw Stop(false, "Saved on this $deviceWord; HITOP Cloud can't be reached, trying again.")
        }
        if (r.status == 401) throw Stop(true, "signed out")
        if (r.status >= 500 || r.status == 429) throw Stop(false, "Saved on this $deviceWord; HITOP Cloud answered ${r.status}, trying again.")
        return r
    }

    /** Another device changed `item` first: the server's stays, this phone's collection is kept as "… (this phone)". */
    fun conflict(item: Item, current: JsonObject?) {
        val key = item.key
        if (current == null) {
            state = state.copy(known = state.known - key)
            pushAgain = true
            return
        }
        val version = current.strOrNull("version")?.toLongOrNull() ?: 0
        val theirsDeleted = current.bool("deleted", false)
        if (item.kind != "collection") {
            state = state.copy(known = state.known - key)
            apply(listOf(current))
            _notices.update { listOf(if (item.kind == "globals") "Globals changed on another device: showing theirs." else "Environments changed on another device: showing theirs.") + it }
            return
        }
        val cid = item.id
        if (library.isShared(cid)) {
            state = state.copy(known = state.known - key)
            save()
            return
        }
        val mine = if (item.data is JsonNull) null else library.state.value.collection(cid)
        library.applyRemoteRemoval(cid)
        val data = current.obj("data")
        if (!theirsDeleted && data != null) {
            val theirs = Collection(data.with("id", data["id"] ?: JsonPrimitive(cid)))
            library.applyRemote(theirs)
            state = state.copy(known = state.known + (key to Known(version, collectionItem(theirs).hash)))
        } else {
            state = state.copy(known = state.known - key)
        }
        if (mine != null) {
            val copy = mine.set("id", JsonPrimitive(Ids.next())).withName("${mine.name} ($deviceLabel)")
            library.addCollectionCopy(copy)
            _notices.update { listOf("“${mine.name}” changed on another device. Kept theirs and saved yours as “${copy.name}”.") + it }
        }
        save()
    }

    // ---- local only

    fun isLocalOnly(id: String) = id in state.localOnly

    /** A collection kept on this phone only goes to HITOP Cloud now. */
    suspend fun upload(id: String) {
        state = state.copy(localOnly = state.localOnly - id)
        save()
        refreshBadges()
        flush()
    }

    fun dismissNotice(text: String) = _notices.update { n -> n - text }

    fun refreshBadges() {
        val on = cloud.signedIn && enabled() && state.account == cloud.accountKey
        val out = HashMap<String, Badge>()
        library.state.value.collections.filter { !library.isShared(it.id) }.forEach { c ->
            val key = "collection/${c.id}"
            out[c.id] = when {
                !on || c.id in state.localOnly -> Badge.LOCAL
                state.known[key]?.hash == collectionItem(c).hash -> Badge.SYNCED
                state.refused[key] == collectionItem(c).hash -> Badge.REFUSED
                else -> Badge.WAITING
            }
        }
        _badges.value = out
    }

    companion object {
        /** A new account's state: the first account uploads what is here, except what nobody filled in. */
        fun firstLink(account: String, previous: State, collections: List<Collection>): State {
            val anotherBefore = previous.account.isNotEmpty()
            return State(account = account, localOnly = collections.filter { anotherBefore || untouched(it) }.map { it.id })
        }

        fun isPristineSample(c: Collection): Boolean {
            if (c.name != AutoName.SAMPLE_NAME) return false
            val sample = Seed.sample().requests.associate { it.name to listOf(it.method, it.url, it.body, it.preRequestScript, it.postRequestScript) }
            return c.requests.all { r -> sample[r.name] == listOf(r.method, r.url, r.body, r.preRequestScript, r.postRequestScript) }
        }

        fun untouched(c: Collection) = AutoName.isEmptyMyRequests(c) || isPristineSample(c)

        /** "just now", "2 min ago", "3 h ago", "4 days ago". */
        fun ago(ms: Long, now: Long = System.currentTimeMillis()): String {
            val s = (now - ms) / 1000
            return when {
                s < 60 -> "just now"
                s < 3600 -> "${s / 60} min ago"
                s < 86400 -> "${s / 3600} h ago"
                s / 86400 == 1L -> "yesterday"
                else -> "${s / 86400} days ago"
            }
        }

        /** Canonical JSON (keys sorted), for fingerprints that don't depend on key order. */
        fun canonical(e: JsonElement): String = when (e) {
            is JsonObject -> e.entries.sortedBy { it.key }.joinToString(",", "{", "}") { (k, v) -> JsonPrimitive(k).toString() + ":" + canonical(v) }
            is JsonArray -> e.joinToString(",", "[", "]") { canonical(it) }
            else -> e.toString()
        }
    }
}
