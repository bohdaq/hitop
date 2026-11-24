package tech.rws8.hitop.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.ffi.Ffi

data class LibraryState(
    val collections: List<Collection> = emptyList(),
    val environments: List<Environment> = emptyList(),
    val activeEnvironmentId: String? = null,
    val globals: Globals = Globals(),
    val history: List<HistoryEntry> = emptyList(),
    /** Pinned request ids. */
    val pins: List<String> = emptyList(),
    /** New requests still named from their URL (until saved under a name or renamed). */
    val autoNamed: Set<String> = emptySet(),
    val loaded: Boolean = false,
    /** Files that couldn't be read at the start (kept aside; the person is told once). */
    val damaged: List<String> = emptyList(),
    /** Shared collections: collection id → role (owner, editor, viewer; "removed" = no longer shared). */
    val sharedRoles: Map<String, String> = emptyMap(),
    /** Shared collections' team organization (none: personal). */
    val sharedOrgs: Map<String, String?> = emptyMap(),
) {
    val activeEnvironment: Environment? get() = environments.firstOrNull { it.id == activeEnvironmentId }
    fun collection(id: String) = collections.firstOrNull { it.id == id }
    fun find(requestId: String): Pair<Collection, Request>? {
        for (c in collections) c.request(requestId)?.let { return c to it }
        return null
    }

    /** The last status each request answered with (newest history entry). */
    val lastStatus: Map<String, Int>
        get() {
            val out = HashMap<String, Int>()
            for (h in history) {
                val s = h.response?.status ?: continue
                out.putIfAbsent(h.request.id, s)
            }
            return out
        }
}

/** What a send gives back, already masked by the engine. */
data class SendResult(
    val status: Int?,
    val headers: List<Pair<String, String>>,
    val body: String,
    val durationMs: Long,
    val timings: Timings?,
    val redirects: List<Redirect>,
    val tests: List<TestResult>,
    val error: ExplainedError?,
    val url: String,
    val method: String,
    val sentHeaders: List<Pair<String, String>>,
    val sentBody: String,
    val cookies: List<CookieRow>,
) {
    val size: Long get() = body.toByteArray(Charsets.UTF_8).size.toLong()
    val ok: Boolean get() = error == null && (status ?: 0) in 1..399
    val contentType: String get() = headers.firstOrNull { it.first.equals("content-type", true) }?.second.orEmpty()
    val testsPassed: Int get() = tests.count { it.pass }
}

data class Timings(val dnsMs: Long, val connectMs: Long, val waitMs: Long, val downloadMs: Long, val totalMs: Long, val reused: Boolean)
data class Redirect(val status: Int, val url: String, val location: String)
data class CookieRow(val name: String, val value: String, val domain: String, val path: String, val expires: Long?, val secure: Boolean)

/**
 * The library: collections, environments, globals, history and pins, kept in [store] (secret values
 * in [secrets]), and sending through hitop-ffi. Every change persists before it returns.
 */
class Library(private val store: LibraryStore, private val secrets: SecretStore) {
    private val _state = MutableStateFlow(LibraryState())
    val state: StateFlow<LibraryState> = _state.asStateFlow()

    /** Called after every change made here (not after `applyRemote*`): HITOP Cloud pushes. */
    var onLibraryChange: (() -> Unit)? = null
    /** Shared (end-to-end encrypted) collections, A3: never in the personal library. */
    var isShared: (String) -> Boolean = { id -> id in _state.value.sharedRoles }
    /** A shared collection changed here (SharedSync pushes it). */
    var onSharedChange: ((String) -> Unit)? = null
    /** A collection was deleted here (SharedSync forgets its link). */
    var onCollectionDeleted: ((String) -> Unit)? = null
    /** The organization a new collection is filed under (null: personal). */
    var orgForNew: () -> String? = { null }

    private fun changed(collectionId: String? = null) {
        if (collectionId != null && isShared(collectionId)) onSharedChange?.invoke(collectionId)
        onLibraryChange?.invoke()
    }

    /** A viewer's shared collection (or one no longer shared): nothing in it changes here. */
    fun isReadOnly(collectionId: String): Boolean = _state.value.sharedRoles[collectionId].let { it == "viewer" || it == "removed" }

    /** Shared collections' roles and organizations (SharedSync keeps them current). */
    fun setShared(roles: Map<String, String>, orgs: Map<String, String?>) = _state.update { it.copy(sharedRoles = roles, sharedOrgs = orgs) }

    /** A request copy added to a collection (a conflict kept both ways). */
    fun addRequestCopy(r: Request, collectionId: String) {
        val c = _state.value.collection(collectionId) ?: return
        putCollection(c.withRequests(c.requests + r))
    }

    /** Team environments (`team: <org>`) of [org] replace those of any organization. */
    fun applyTeamEnvironments(list: List<Environment>, org: String?) {
        val s = _state.value
        val mine = s.environments.filter { it.team == null }
        val next = mine + list
        val active = s.activeEnvironmentId.takeIf { a -> next.any { it.id == a } }
        store.saveEnvironments(next.filter { it.team == null }, active)
        _state.update { it.copy(environments = next, activeEnvironmentId = active) }
    }

    /** What scripts set, per collection, for this session (as the web app keeps `scriptVariables`). */
    private val scriptVariables = HashMap<String, MutableMap<String, String?>>()
    private val scriptGlobals = HashMap<String, String?>()
    private val contexts = HashMap<String, JsonObject>()

    fun load() {
        val (envs, active) = store.loadEnvironments()
        val st = store.loadState()
        _state.value = LibraryState(
            collections = store.loadCollections(),
            environments = envs,
            activeEnvironmentId = active,
            globals = store.loadGlobals(),
            history = store.loadHistory(),
            pins = st.stringList("pins"),
            loaded = true,
            damaged = store.damaged,
        )
    }

    fun dismissDamaged() = _state.update { it.copy(damaged = emptyList()) }

    /** Unsaved edits kept from before, of requests that are still here and differ from their saved form. */
    fun loadDrafts(): Map<String, Request> = store.loadDrafts().filter { (id, d) -> _state.value.find(id)?.second.let { it != null && it != d } }
    fun saveDrafts(d: Map<String, Request>) = store.saveDrafts(d)

    // ---- seeding

    /** A first visit: the sample and an empty My requests, as the web app starts (FV1). */
    fun seedFirstVisit() {
        if (_state.value.collections.isNotEmpty()) return
        val sample = Seed.sample()
        val mine = Collection.new(AutoName.MY_REQUESTS)
        listOf(sample, mine).forEach { store.saveCollection(it) }
        _state.update { it.copy(collections = listOf(sample, mine)) }
    }

    /** UI tests and screenshots: the shop-api sample of the drafts, its environments and history. */
    fun seedShopApi(wsBase: String? = null) {
        val cols = Seed.collections() + listOfNotNull(wsBase?.let { Seed.realtime(it) })
        cols.forEach { store.saveCollection(it) }
        val envs = Seed.environments()
        store.saveEnvironments(envs, "env-staging")
        Seed.secrets.forEach { (env, name, value) -> secrets.set(SecretStore.envKey(env, name), value) }
        val history = Seed.history(cols)
        store.saveHistory(history)
        store.saveState(buildJsonObject { put("pins", listOf("1104", "1103", "1101").toJsonArray()) })
        load()
    }

    // ---- importing, files

    /** Adds what [plan] holds (never replaces a collection); the ids of the new collections. */
    fun applyImport(plan: ImportPlan): List<String> {
        val added = plan.collections.map { c0 ->
            val s = _state.value
            var c = Importing.fresh(c0, s.collections.mapTo(HashSet()) { it.id }, s.collections.flatMapTo(HashSet()) { x -> x.requests.map { it.id } })
            c = c.set("org", orgForNew()?.let { JsonPrimitive(it) })
            putCollection(c)
            c.id
        }
        plan.environments.forEach { i ->
            val clash = i.environment.id.isEmpty() || _state.value.environments.any { it.id == i.environment.id }
            val e = (if (clash) i.environment.set("id", JsonPrimitive("env-${Ids.next()}")) else i.environment)
                .let { it.withVariables(it.variables + it.secrets.associateWith { "" }) }.set("team", null)
            i.secretValues.forEach { (n, v) -> secrets.set(SecretStore.envKey(e.id, n), v) }
            putEnvironments(_state.value.environments + e, _state.value.activeEnvironmentId ?: e.id)
        }
        plan.globals?.let { g ->
            g.environment.variables.filterKeys { it !in g.environment.secrets }.forEach { (n, v) -> setGlobal(n, v, secret = false) }
            g.environment.secrets.forEach { n -> setGlobal(n, g.secretValues[n] ?: globalSecretValue(n).orEmpty(), secret = true) }
        }
        return added
    }

    fun replaceCollection(c: Collection) {
        if (_state.value.collection(c.id) != null) putCollection(c)
    }

    private fun uploads(): Map<String, JsonObject> = (store.readJson("uploads.json") as? JsonObject)?.mapNotNull { (k, v) -> (v as? JsonObject)?.let { k to it } }?.toMap() ?: emptyMap()
    private fun uploadFile(token: String) = store.file("uploads/$token")

    /** Keeps a picked file for a multipart row of [requestId]; the row's keys that name it. */
    @Synchronized
    fun addUpload(requestId: String, name: String, type: String, bytes: java.io.InputStream, max: Long = 50L * 1024 * 1024): JsonObject {
        val token = java.util.UUID.randomUUID().toString()
        val f = uploadFile(token)
        f.parentFile?.mkdirs()
        try {
            var total = 0L
            f.outputStream().use { out ->
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = bytes.read(buf)
                    if (n < 0) break
                    total += n
                    require(total <= max) { "The file is larger than ${max / 1024 / 1024} MB." }
                    out.write(buf, 0, n)
                }
            }
        } catch (e: Exception) {
            f.delete()
            throw e
        }
        // A header value can't hold line breaks; hitop-core quotes the rest of the name.
        val mime = type.takeIf { Regex("[A-Za-z0-9.+-]+/[A-Za-z0-9.+-]+").matches(it) } ?: "application/octet-stream"
        val entry = buildJsonObject { put("request", requestId); put("name", name); put("type", mime) }
        store.write("uploads.json", JsonObject(uploads() + (token to entry)))
        return buildJsonObject { put("fileName", name); put("fileToken", token) }
    }

    /** The file of a row is on this phone (picked for this request). */
    fun hasUpload(requestId: String, token: String): Boolean = uploads()[token]?.str("request") == requestId && uploadFile(token).exists()

    /** Drops files no saved request or draft names any more (start of the app). */
    @Synchronized
    fun pruneUploads() {
        val all = uploads()
        if (all.isEmpty()) return
        val used = (_state.value.collections.flatMap { it.requests } + store.loadDrafts().values).flatMapTo(HashSet()) { r -> r.formFields.mapNotNull { it.extra.strOrNull("fileToken") } }
        val gone = all.keys - used
        if (gone.isEmpty()) return
        gone.forEach { uploadFile(it).delete() }
        store.write("uploads.json", JsonObject(all - gone))
    }

    /** The files [requests] name, for hitop-ffi (which reads only these, each only for its request). */
    private fun filesArg(requests: List<Request>): JsonObject? {
        val rows = requests.filter { it.bodyKind == BodyKind.MULTIPART }.flatMap { r -> r.formFields.mapNotNull { f -> f.extra.strOrNull("fileToken")?.let { r.id to it } } }
        if (rows.isEmpty()) return null
        val all = uploads()
        return JsonObject(rows.mapNotNull { (id, token) ->
            val e = all[token]?.takeIf { it.str("request") == id && uploadFile(token).exists() } ?: return@mapNotNull null
            token to buildJsonObject { put("path", uploadFile(token).absolutePath); put("name", e.str("name")); put("type", e.str("type")); put("request", id) }
        }.toMap())
    }

    // ---- collections and requests

    private fun putCollection(c: Collection, notify: Boolean = true) {
        // A viewer's shared collection changes only by syncing.
        if (notify && isReadOnly(c.id)) return
        store.saveCollection(c)
        _state.update { s -> s.copy(collections = if (s.collections.any { it.id == c.id }) s.collections.map { if (it.id == c.id) c else it } else s.collections + c) }
        if (notify) changed(c.id)
    }

    fun addCollection(name: String): Collection {
        var c = Collection.new(name.ifBlank { "New collection" })
        orgForNew()?.let { c = c.set("org", JsonPrimitive(it)) }
        putCollection(c)
        return c
    }

    // ---- from HITOP Cloud (no change notification: they came from there)

    fun applyRemote(c: Collection) = putCollection(c, notify = false)

    fun applyRemoteRemoval(id: String) {
        store.deleteCollection(id)
        _state.update { s -> s.copy(collections = s.collections.filter { it.id != id }) }
    }

    fun applyRemoteEnvironments(list: List<Environment>, active: String?) {
        store.saveEnvironments(list, active)
        _state.update { it.copy(environments = list, activeEnvironmentId = active) }
    }

    fun applyRemoteGlobals(g: Globals) {
        store.saveGlobals(g)
        _state.update { it.copy(globals = g) }
    }

    /** A conflict copy ("… (this phone)"): a collection of its own, synced like any other. */
    fun addCollectionCopy(c: Collection) = putCollection(c)

    fun setSecretValue(envId: String, name: String, value: String?) = secrets.set(SecretStore.envKey(envId, name), value)
    fun setGlobalSecretValue(name: String, value: String?) = secrets.set(SecretStore.globalKey(name), value)

    fun renameCollection(id: String, name: String) {
        val c = _state.value.collection(id) ?: return
        putCollection(c.withName(name))
    }

    fun deleteCollection(id: String) {
        if (_state.value.sharedRoles[id] == "viewer") {
            // A viewer may drop the copy on this phone; the workspace stays.
        }
        applyRemoteRemoval(id)
        onCollectionDeleted?.invoke(id)
        changed()
    }

    /** Where a request goes when no collection is meant: My requests (made when missing). */
    fun defaultCollection(): Collection = _state.value.collections.firstOrNull { it.name == AutoName.MY_REQUESTS }
        ?: _state.value.collections.firstOrNull { AutoName.isMyRequests(it) }
        ?: addCollection(AutoName.MY_REQUESTS)

    /** A new "Untitled" request, added to the collection at once and named from its URL until saved. */
    fun newRequest(collectionId: String? = null, folder: String = "", template: Request? = null): Pair<Collection, Request> {
        val c = collectionId?.takeIf { !isReadOnly(it) }?.let { _state.value.collection(it) } ?: defaultCollection()
        var r = template ?: Request.new()
        if (folder.isNotEmpty()) r = r.withFile("$folder/untitled")
        val updated = c.withRequests(c.requests + r)
        putCollection(updated)
        if (template == null) _state.update { it.copy(autoNamed = it.autoNamed + r.id) }
        return updated to r
    }

    /** Saves the editor's request; an auto-named one takes its name from its URL now. */
    fun saveRequest(collectionId: String, request: Request): Request {
        val c = _state.value.collection(collectionId) ?: return request
        var r = request
        if (r.id in _state.value.autoNamed) r = r.withName(AutoName.of(r))
        val old = c.request(r.id)
        // A request's file follows its name only when the name changes.
        if (old != null && old.name != r.name && r.file != null) r = r.withFile(fileFor(r.folder, r.name))
        putCollection(if (old == null) c.withRequests(c.requests + r) else c.replacing(r))
        _state.update { it.copy(autoNamed = it.autoNamed - r.id) }
        return r
    }

    private fun fileFor(folder: String, name: String): String {
        val slug = name.lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-').ifEmpty { "request" }
        return if (folder.isEmpty()) slug else "$folder/$slug"
    }

    fun renameRequest(requestId: String, name: String) {
        val (c, r) = _state.value.find(requestId) ?: return
        val renamed = r.withName(name).let { if (it.file != null) it.withFile(fileFor(r.folder, name)) else it }
        putCollection(c.replacing(renamed))
        _state.update { it.copy(autoNamed = it.autoNamed - requestId) }
    }

    fun deleteRequest(requestId: String) {
        val (c, _) = _state.value.find(requestId) ?: return
        putCollection(c.withRequests(c.requests.filter { it.id != requestId }))
        _state.update { it.copy(pins = it.pins - requestId) }
        persistState()
    }

    fun duplicateRequest(requestId: String): Request? {
        val (c, r) = _state.value.find(requestId) ?: return null
        val copy = r.set("id", JsonPrimitive(Ids.next())).withName("${r.name} copy").let { if (it.file != null) it.withFile(fileFor(r.folder, "${r.name} copy")) else it }
        val list = c.requests.toMutableList()
        list.add(list.indexOfFirst { it.id == requestId } + 1, copy)
        putCollection(c.withRequests(list))
        return copy
    }

    fun moveRequest(requestId: String, toCollectionId: String, folder: String) {
        val (from, r) = _state.value.find(requestId) ?: return
        val moved = r.withFile(fileFor(folder, r.name).takeIf { folder.isNotEmpty() || r.file != null })
        if (from.id == toCollectionId) {
            putCollection(from.replacing(moved))
            return
        }
        val to = _state.value.collection(toCollectionId) ?: return
        putCollection(from.withRequests(from.requests.filter { it.id != requestId }))
        putCollection(to.withRequests(to.requests + moved))
    }

    /**
     * Runs [change] and returns what takes it back, or null when no collection lost or moved
     * anything. Requests come back where they were (also when the collection changed since); a
     * deleted collection comes back under a new id, since HITOP Cloud may have its deletion.
     */
    fun undoable(change: () -> Unit): (() -> Unit)? {
        val before = _state.value.collections
        val pins = _state.value.pins
        change()
        val after = _state.value.collections
        if (before == after) return null
        return {
            before.forEach { old ->
                val then = after.firstOrNull { it.id == old.id }
                val now = _state.value.collection(old.id)
                if (then == null) {
                    if (now == null && !isShared(old.id)) putCollection(old.set("id", JsonPrimitive(Ids.next())))
                } else if (then != old && now != null) {
                    val changed = old.requests.filter { o -> then.request(o.id) != o }.associateBy { it.id }
                    val added = then.requests.filter { old.request(it.id) == null }.map { it.id }.toSet()
                    val list = now.requests.filter { it.id !in added }.map { changed[it.id] ?: it }.toMutableList()
                    old.requests.forEachIndexed { i, r ->
                        if (r.id in changed && list.none { it.id == r.id }) {
                            val prev = old.requests.take(i).lastOrNull { p -> list.any { it.id == p.id } }
                            list.add(if (prev == null) 0 else list.indexOfFirst { it.id == prev.id } + 1, r)
                        }
                    }
                    val folders = old.folders.filter { it !in now.folders && it !in then.folders }
                    val order = if (now.folders == then.folders) old.folders else now.folders + folders
                    putCollection(now.withRequests(list).withFolders(order))
                }
            }
            val lost = pins.filter { p -> p !in _state.value.pins && _state.value.find(p) != null }
            if (lost.isNotEmpty()) {
                _state.update { it.copy(pins = it.pins + lost) }
                persistState()
            }
        }
    }

    fun togglePin(requestId: String) {
        _state.update { s -> s.copy(pins = if (requestId in s.pins) s.pins - requestId else s.pins + requestId) }
        persistState()
    }

    fun addFolder(collectionId: String, path: String) {
        val c = _state.value.collection(collectionId) ?: return
        if (path.isBlank() || path in c.allFolders) return
        putCollection(c.withFolders(c.folders + path.trim('/')))
    }

    fun renameFolder(collectionId: String, from: String, to: String) {
        val c = _state.value.collection(collectionId) ?: return
        fun moved(p: String) = if (p == from) to else if (p.startsWith("$from/")) to + p.removePrefix(from) else p
        val reqs = c.requests.map { r -> r.file?.let { f -> if (r.folder == from || r.folder.startsWith("$from/")) r.withFile(moved(r.folder) + "/" + f.substringAfterLast('/')) else r } ?: r }
        putCollection(c.withRequests(reqs).withFolders(c.folders.map(::moved)))
    }

    fun deleteFolder(collectionId: String, path: String) {
        val c = _state.value.collection(collectionId) ?: return
        val inside = { r: Request -> r.folder == path || r.folder.startsWith("$path/") }
        putCollection(c.withRequests(c.requests.filterNot(inside)).withFolders(c.folders.filterNot { it == path || it.startsWith("$path/") }))
    }

    private fun persistState() {
        store.saveState(store.loadState().with("pins", _state.value.pins.toJsonArray()))
    }

    // ---- environments, globals, secrets

    private fun putEnvironments(list: List<Environment>, active: String?) {
        applyRemoteEnvironments(list, active)
        changed()
    }

    fun setActiveEnvironment(id: String?) = putEnvironments(_state.value.environments, id)

    fun addEnvironment(name: String): Environment {
        val e = Environment.new(name.ifBlank { "New environment" })
        putEnvironments(_state.value.environments + e, _state.value.activeEnvironmentId ?: e.id)
        return e
    }

    fun renameEnvironment(id: String, name: String) = putEnvironments(_state.value.environments.map { if (it.id == id) it.withName(name) else it }, _state.value.activeEnvironmentId)

    fun deleteEnvironment(id: String) {
        val e = _state.value.environments.firstOrNull { it.id == id } ?: return
        e.secrets.forEach { secrets.set(SecretStore.envKey(id, it), null) }
        val active = _state.value.activeEnvironmentId.takeIf { it != id }
        putEnvironments(_state.value.environments.filter { it.id != id }, active)
    }

    /** Sets a variable; a secret's value goes to the secret store and "" to the file. */
    fun setVariable(envId: String, name: String, value: String, secret: Boolean) {
        val e = _state.value.environments.firstOrNull { it.id == envId } ?: return
        val vars = LinkedHashMap(e.variables)
        var names = e.secrets
        if (secret) {
            secrets.set(SecretStore.envKey(envId, name), value)
            vars[name] = ""
            if (name !in names) names = names + name
        } else {
            if (name in names) secrets.set(SecretStore.envKey(envId, name), null)
            vars[name] = value
            names = names - name
        }
        putEnvironments(_state.value.environments.map { if (it.id == envId) e.withVariables(vars).withSecrets(names) else it }, _state.value.activeEnvironmentId)
    }

    fun removeVariable(envId: String, name: String) {
        val e = _state.value.environments.firstOrNull { it.id == envId } ?: return
        secrets.set(SecretStore.envKey(envId, name), null)
        putEnvironments(_state.value.environments.map { if (it.id == envId) e.withVariables(e.variables - name).withSecrets(e.secrets - name) else it }, _state.value.activeEnvironmentId)
    }

    fun secretValue(envId: String, name: String): String? = secrets.get(SecretStore.envKey(envId, name))

    fun setGlobal(name: String, value: String, secret: Boolean) {
        val g = _state.value.globals
        val vars = LinkedHashMap(g.variables)
        var names = g.secrets
        if (secret) {
            secrets.set(SecretStore.globalKey(name), value); vars[name] = ""; if (name !in names) names = names + name
        } else {
            secrets.set(SecretStore.globalKey(name), null); vars[name] = value; names = names - name
        }
        applyRemoteGlobals(g.copy(variables = vars, secrets = names))
        changed()
    }

    fun removeGlobal(name: String) {
        val g = _state.value.globals
        secrets.set(SecretStore.globalKey(name), null)
        applyRemoteGlobals(g.copy(variables = g.variables - name, secrets = g.secrets - name))
        changed()
    }

    fun globalSecretValue(name: String): String? = secrets.get(SecretStore.globalKey(name))

    private fun environmentSecrets(e: Environment?): Map<String, String> =
        e?.secrets?.mapNotNull { n -> secrets.get(SecretStore.envKey(e.id, n))?.let { n to it } }?.toMap() ?: emptyMap()

    private fun globalSecrets(g: Globals): Map<String, String> = g.secrets.mapNotNull { n -> secrets.get(SecretStore.globalKey(n))?.let { n to it } }.toMap()

    /** Every secret value the active environment and globals hold (masked out of what the on-device model sees). */
    fun activeSecretValues(): List<String> = environmentSecrets(_state.value.activeEnvironment).values.toList() + globalSecrets(_state.value.globals).values

    /** A variable's value and where it comes from (the peek on a `${name}` chip). */
    fun peek(collectionId: String, name: String): Peek {
        val s = _state.value
        val c = s.collection(collectionId)
        val env = s.activeEnvironment
        scriptVariables[collectionId]?.get(name)?.let { return Peek(name, it, Scope.SCRIPT, false, "Set by a script while this app runs") }
        if (env != null && name in env.secrets) return Peek(name, secretValue(env.id, name), Scope.SECRET, true, env.name)
        env?.variables?.get(name)?.let { return Peek(name, it, Scope.ENVIRONMENT, false, env.name) }
        c?.variables?.get(name)?.let { return Peek(name, it, Scope.COLLECTION, false, c.name) }
        if (name in s.globals.secrets) return Peek(name, globalSecretValue(name), Scope.SECRET, true, "Globals")
        s.globals.variables[name]?.let { return Peek(name, it, Scope.GLOBALS, false, "Globals") }
        return Peek(name, null, null, false, if (env == null) "No environment picked" else "Not set in ${env.name}")
    }

    // ---- history

    fun addHistory(e: HistoryEntry) {
        val list = (listOf(e) + _state.value.history).take(HistoryEntry.MAX)
        store.saveHistory(list)
        _state.update { it.copy(history = list) }
    }

    fun deleteHistory(id: Long) {
        val list = _state.value.history.filter { it.id != id }
        store.saveHistory(list)
        _state.update { it.copy(history = list) }
    }

    fun clearHistory() {
        store.saveHistory(emptyList())
        _state.update { it.copy(history = emptyList()) }
    }

    /** A history entry opened again: a new request in My requests. */
    fun openHistory(id: Long): Pair<Collection, Request>? {
        val h = _state.value.history.firstOrNull { it.id == id } ?: return null
        val r = h.request.set("id", JsonPrimitive(Ids.next())).withFile(null)
        return newRequest(null, "", r)
    }

    // ---- sending

    /**
     * Sends [draft] (the editor's request, saved or not) as part of its collection, through
     * hitop-core: the environment, secrets, globals, the scripts' values of earlier sends, the
     * cookie jar. Adds a history entry. Blocking work runs on the IO dispatcher.
     */
    suspend fun send(collectionId: String, draft: Request, timeoutMs: Long = 0, history: Boolean = true): SendResult = withContext(Dispatchers.IO) {
        val c = _state.value.collection(collectionId) ?: Collection.new("Unsaved")
        val args = engineArgs(c, draft) { if (timeoutMs > 0) put("timeoutMs", timeoutMs); put("jar", store.loadJar()); put("sendId", sendIds.incrementAndGet()) }
        val result = try {
            parse(callAbandonable("send", args).jsonObject, collectionId)
        } catch (e: kotlinx.coroutines.CancellationException) {
            throw e
        } catch (e: Exception) {
            SendResult(null, emptyList(), "", 0, null, emptyList(), emptyList(), Errors.explain(e.message ?: e.toString()), draft.url, draft.method, emptyList(), "", emptyList())
        }
        val snapshot = result.status?.let {
            val cut = result.body.length > ResponseSnapshot.MAX_BODY
            ResponseSnapshot(it, result.headers, if (cut) result.body.substring(0, ResponseSnapshot.MAX_BODY) else result.body, cut, result.durationMs, result.size, result.tests)
        }
        if (history) addHistory(HistoryEntry(Ids.next(), System.currentTimeMillis(), c.name, draft, snapshot, result.error?.raw))
        result
    }

    /**
     * The engine can't stop inside an exchange, so a cancelled send leaves it to finish (or reach
     * its time limit) on its own thread; its response and cookies are dropped, as on the desktop.
     */
    private suspend fun callAbandonable(op: String, args: JsonObject): JsonElement = kotlinx.coroutines.suspendCancellableCoroutine { cont ->
        val id = args["sendId"]
        val done = java.util.concurrent.atomic.AtomicBoolean(false)
        Thread({
            val r = runCatching { Ffi.call(op, args) }
            done.set(true)
            cont.resumeWith(r)
        }, "hitop-$op").apply { isDaemon = true }.start()
        cont.invokeOnCancellation { if (id != null) runCatching { Ffi.call("oauthCancel", buildJsonObject { put("sendId", id) }) } }
        // OAuth 2.0's authorization code grant asks for a page to open while the send waits.
        val open = openSignIn
        if (id != null && open != null && args.str("collection").contains("authorization_code")) Thread({
            while (!done.get() && cont.isActive) {
                val page = runCatching { Ffi.call("oauthPending", buildJsonObject { put("sendId", id) }) as? JsonPrimitive }.getOrNull()?.takeIf { it !is JsonNull }?.content
                if (page != null) open(page)
                Thread.sleep(200)
            }
        }, "hitop-oauth").apply { isDaemon = true }.start()
    }

    private val sendIds = java.util.concurrent.atomic.AtomicLong(System.currentTimeMillis())

    /** Opens an OAuth 2.0 sign-in page (the app sets it). */
    @Volatile var openSignIn: ((String) -> Unit)? = null

    /** Forgets OAuth 2.0 tokens got by signing in (they are only in memory). */
    fun forgetTokens() { runCatching { Ffi.call("oauthForget") } }

    fun setCollectionAuth(id: String, a: Auth?) {
        val c = _state.value.collection(id) ?: return
        putCollection(c.withAuth(a))
    }

    /** What hitop-ffi's `send` and `snippet` take: the collection with [draft] in it, scopes, secrets. */
    /** The engine's arguments for [draft] in its collection (WebSocket, SSE, gRPC sessions use them). */
    fun requestArgs(collectionId: String, draft: Request, more: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit = {}): JsonObject =
        engineArgs(_state.value.collection(collectionId) ?: Collection.new("Unsaved"), draft, more)

    private fun engineArgs(c: Collection, draft: Request, more: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit = {}): JsonObject {
        val requests = if (c.request(draft.id) != null) c.requests.map { if (it.id == draft.id) draft else it } else c.requests + draft
        return scopeArgs(c, requests, _state.value.activeEnvironment, sessionValues = true) {
            put("request", requests.indexOfFirst { it.id == draft.id })
            more()
        }
    }

    /** A run's arguments: the collection as saved, [env], secrets and globals; script values start afresh (as a run). */
    fun runArgs(c: Collection, env: Environment?, more: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit = {}): JsonObject = scopeArgs(c, c.requests, env, sessionValues = false, more)

    /** The network settings for hitop-ffi's `setNetwork` (the app sets it); empty = the engine's defaults. */
    @Volatile var network: () -> JsonObject = { EmptyObject }
    /** The time limit of one request in ms (Settings); 0 = the engine's own. */
    @Volatile var timeoutMs: () -> Long = { 0 }
    private var networkSet: JsonObject = EmptyObject

    @Synchronized
    private fun applyNetwork() {
        val want = runCatching { network() }.getOrDefault(EmptyObject)
        if (want == networkSet) return
        // A setting that can't be used falls back to the defaults, never to less checking.
        if (runCatching { Ffi.call("setNetwork", want) }.isFailure) runCatching { Ffi.call("setNetwork") }
        networkSet = want
    }

    data class StoredCookie(val index: Int, val name: String, val domain: String, val path: String)

    fun cookies(): List<StoredCookie> = store.loadJar().mapIndexedNotNull { i, e -> (e as? JsonObject)?.let { StoredCookie(i, it.str("name"), it.str("domain"), it.str("path", "/")) } }
    fun deleteCookie(c: StoredCookie) = store.saveJar(JsonArray(store.loadJar().filterNot { e -> (e as? JsonObject)?.let { it.str("name") == c.name && it.str("domain") == c.domain && it.str("path", "/") == c.path } ?: false }))
    fun clearCookies() = store.saveJar(JsonArray(emptyList()))

    private fun scopeArgs(c: Collection, requests: List<Request>, env: Environment?, sessionValues: Boolean, more: kotlinx.serialization.json.JsonObjectBuilder.() -> Unit): JsonObject {
        val s = _state.value
        applyNetwork()
        return buildJsonObject {
            timeoutMs().takeIf { it > 0 }?.let { put("timeoutMs", it) }
            put("collection", c.forEngine(requests).toString())
            filesArg(requests)?.let { put("files", it) }
            env?.let { put("environment", it.forEngine()) }
            put("secrets", environmentSecrets(env).toJsonObject())
            put("globals", s.globals.variables.filterKeys { it !in s.globals.secrets }.toJsonObject())
            put("globalSecrets", globalSecrets(s.globals).toJsonObject())
            if (sessionValues) {
                put("scriptVariables", JsonObject((scriptVariables[c.id] ?: emptyMap<String, String?>()).mapValues { (_, v) -> v?.let(::JsonPrimitive) ?: JsonNull }))
                put("scriptGlobals", JsonObject(scriptGlobals.mapValues { (_, v) -> v?.let(::JsonPrimitive) ?: JsonNull }))
                put("context", contexts[c.id] ?: EmptyObject)
            }
            more()
        }
    }

    /** The request as code (curl, fetch, python, go, java, rust); secrets stay `{{NAME}}`. */
    suspend fun snippet(collectionId: String, draft: Request, lang: String): String = withContext(Dispatchers.IO) {
        val c = _state.value.collection(collectionId) ?: Collection.new("Unsaved")
        try {
            Ffi.call("snippet", engineArgs(c, draft) { put("lang", lang) }).jsonObject.str("code")
        } catch (e: Exception) {
            e.message ?: "No snippet"
        }
    }

    private fun parse(o: JsonObject, collectionId: String): SendResult {
        o.obj("scriptVariables")?.forEach { (k, v) -> scriptVariables.getOrPut(collectionId) { LinkedHashMap() }[k] = (v as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content }
        o.obj("scriptGlobals")?.forEach { (k, v) -> scriptGlobals[k] = (v as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content }
        o.obj("context")?.let { contexts[collectionId] = it }
        (o["jar"] as? JsonArray)?.let { store.saveJar(it) }
        val resp = o.obj("response")
        fun pairs(e: JsonElement?): List<Pair<String, String>> = (e as? JsonArray)?.mapNotNull { (it as? JsonArray)?.let { a -> (a[0] as JsonPrimitive).content to (a[1] as JsonPrimitive).content } } ?: emptyList()
        val timings = resp?.obj("timings")?.let { t ->
            Timings(t.str("dnsMs", "0").toLong(), t.str("connectMs", "0").toLong(), t.str("waitMs", "0").toLong(), t.str("downloadMs", "0").toLong(), t.str("totalMs", "0").toLong(), t.bool("reused", false))
        }
        val sent = o.obj("sent")
        val url = sent?.str("url") ?: o.str("url")
        val host = runCatching { java.net.URI(url).host }.getOrNull().orEmpty()
        val jar = (o["jar"] as? JsonArray)?.mapNotNull { it as? JsonObject }?.map {
            CookieRow(it.str("name"), it.str("value"), it.str("domain"), it.str("path", "/"), it.strOrNull("expires")?.toLongOrNull(), it.bool("secure", false))
        }?.filter { host.isNotEmpty() && (host == it.domain || host.endsWith("." + it.domain)) } ?: emptyList()
        return SendResult(
            status = resp?.str("status")?.toIntOrNull(),
            headers = pairs(resp?.get("headers")),
            body = resp?.str("body").orEmpty(),
            durationMs = resp?.str("durationMs", "0")?.toLongOrNull() ?: o.str("durationMs", "0").toLong(),
            timings = timings,
            redirects = resp?.arr("redirects")?.mapNotNull { (it as? JsonObject)?.let { h -> Redirect(h.str("status", "0").toInt(), h.str("url"), h.str("location")) } } ?: emptyList(),
            tests = o.arr("tests")?.mapNotNull { (it as? JsonObject)?.let(TestResult::of) } ?: emptyList(),
            error = o.strOrNull("error")?.let(Errors::explain),
            url = url,
            method = o.str("method"),
            sentHeaders = pairs(sent?.get("headers")),
            sentBody = sent?.str("body").orEmpty(),
            cookies = jar,
        )
    }

    /** Script-set values of this session (the peek shows them; tests read them). */
    fun scriptValues(collectionId: String): Map<String, String?> = scriptVariables[collectionId].orEmpty()
}
