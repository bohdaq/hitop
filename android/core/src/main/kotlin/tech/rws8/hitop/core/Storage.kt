package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import java.io.File

/**
 * The app's files under `<files dir>/hitop-library/` (the iOS app's layout):
 * `collections/<id>.json` = `{formatVersion, updatedAt, collection}` (the web app's JSON, one file
 * per collection so sync can push and pull them one by one), `environments.json`
 * `{environments, activeEnvironmentId}`, `globals.json` `{variables, secrets}`, `history.json`,
 * `state.json` (pins, flags), `cookies.json` (the jar, hitop-core's shape). Writes go to a
 * temporary file first, then replace the old one.
 */
class LibraryStore(val root: File) {
    private val collectionsDir = File(root, "collections")

    /** Files that exist but couldn't be read: never overwritten, set aside first (`<name>.damaged-<time>`). */
    private val unreadable = LinkedHashSet<File>()
    val damaged: List<String> get() = synchronized(unreadable) { unreadable.map { it.relativeTo(root).path } }

    fun isDamaged(name: String) = name in damaged
    fun isCollectionDamaged(id: String) = isDamaged("collections/${fileName(id)}")

    /** Moves the damaged files aside, once whoever syncs knows about them. */
    fun setAsideDamaged() = synchronized(unreadable) {
        unreadable.toList().forEach { f -> if (runCatching { SafeFiles.setAside(f) }.isSuccess) unreadable.remove(f) }
    }

    private fun <T> read(f: File, parse: (String) -> T?): T? {
        if (!f.exists()) return null
        val v = runCatching { parse(f.readText()) }.getOrNull()
        if (v == null) synchronized(unreadable) { unreadable.add(f) }
        return v
    }

    init {
        collectionsDir.mkdirs()
    }

    fun file(name: String) = File(root, name)

    fun loadCollections(): List<Collection> = collectionsDir.listFiles { f -> f.name.endsWith(".json") }
        ?.sortedBy { it.name }
        ?.mapNotNull { f ->
            read(f) { t -> HitopJson.parseToJsonElement(t).jsonObject.obj("collection")?.let(::Collection) }
        }
        ?.sortedBy { order(it) }
        ?: emptyList()

    private fun order(c: Collection): Long = c.json.strOrNull("order")?.toLongOrNull() ?: Long.MAX_VALUE

    fun saveCollection(c: Collection) {
        val doc = buildJsonObject {
            put("formatVersion", 1)
            put("updatedAt", System.currentTimeMillis())
            put("collection", c.json)
        }
        write(File(collectionsDir, fileName(c.id)), doc)
    }

    fun deleteCollection(id: String) {
        File(collectionsDir, fileName(id)).delete()
    }

    private fun fileName(id: String) = id.replace(Regex("[^A-Za-z0-9_.-]"), "_") + ".json"

    fun readJson(name: String): JsonElement? = read(file(name)) { HitopJson.parseToJsonElement(it) }

    fun write(name: String, value: JsonElement) = write(file(name), value)

    private fun write(f: File, value: JsonElement) {
        synchronized(unreadable) { if (unreadable.remove(f)) SafeFiles.setAside(f) }
        SafeFiles.write(f, value.toString())
    }

    fun loadEnvironments(): Pair<List<Environment>, String?> {
        val o = readJson("environments.json") as? JsonObject ?: return emptyList<Environment>() to null
        return (o.arr("environments")?.mapNotNull { (it as? JsonObject)?.let(::Environment) } ?: emptyList()) to o.strOrNull("activeEnvironmentId")
    }

    fun saveEnvironments(list: List<Environment>, active: String?) = write("environments.json", buildJsonObject {
        put("environments", JsonArray(list.map { it.json }))
        if (active != null) put("activeEnvironmentId", active)
    })

    fun loadGlobals(): Globals = Globals.of(readJson("globals.json") as? JsonObject)
    fun saveGlobals(g: Globals) = write("globals.json", g.toJson())

    fun loadHistory(): List<HistoryEntry> = (readJson("history.json") as? JsonArray)?.mapNotNull { (it as? JsonObject)?.let(HistoryEntry::of) } ?: emptyList()
    fun saveHistory(list: List<HistoryEntry>) = write("history.json", JsonArray(list.map { it.toJson() }))

    /** The editors' unsaved requests by id (they outlive the app's process). */
    fun loadDrafts(): Map<String, Request> = (readJson("drafts.json") as? JsonObject)?.mapNotNull { (k, v) -> (v as? JsonObject)?.let { k to Request(it) } }?.toMap() ?: emptyMap()
    fun saveDrafts(d: Map<String, Request>) {
        if (d.isEmpty() && !file("drafts.json").exists()) return
        write("drafts.json", JsonObject(d.mapValues { it.value.json }))
    }

    fun loadState(): JsonObject = readJson("state.json") as? JsonObject ?: EmptyObject
    fun saveState(o: JsonObject) = write("state.json", o)

    fun loadJar(): JsonArray = readJson("cookies.json") as? JsonArray ?: JsonArray(emptyList())
    fun saveJar(j: JsonArray) = write("cookies.json", j)
}

/** Secret values, kept apart from every file the app syncs (Android: Keystore-encrypted). */
interface SecretStore {
    fun get(key: String): String?
    fun set(key: String, value: String?)

    companion object {
        fun envKey(envId: String, name: String) = "android/$envId/$name"
        fun globalKey(name: String) = "android/global/$name"
    }
}

class MemorySecretStore : SecretStore {
    private val values = HashMap<String, String>()
    @Synchronized override fun get(key: String) = values[key]
    @Synchronized override fun set(key: String, value: String?) {
        if (value == null) values.remove(key) else values[key] = value
    }
    @Synchronized fun all(): Map<String, String> = HashMap(values)
}

/** A response kept with its history entry (body cut at 100 kB, secrets masked by the engine). */
data class ResponseSnapshot(
    val status: Int,
    val headers: List<Pair<String, String>>,
    val body: String,
    val truncated: Boolean,
    val durationMs: Long,
    val size: Long,
    val tests: List<TestResult>,
) {
    fun toJson(): JsonObject = buildJsonObject {
        put("status", status)
        put("headers", JsonArray(headers.map { (k, v) -> JsonArray(listOf(JsonPrimitive(k), JsonPrimitive(v))) }))
        put("body", body)
        if (truncated) put("truncated", true)
        put("durationMs", durationMs)
        put("size", size)
        put("tests", JsonArray(tests.map { it.toJson() }))
    }

    companion object {
        const val MAX_BODY = 100_000
        fun of(o: JsonObject): ResponseSnapshot = ResponseSnapshot(
            o.str("status", "0").toIntOrNull() ?: 0,
            o.arr("headers")?.mapNotNull { (it as? JsonArray)?.let { a -> (a.getOrNull(0) as? JsonPrimitive)?.content.orEmpty() to (a.getOrNull(1) as? JsonPrimitive)?.content.orEmpty() } } ?: emptyList(),
            o.str("body"),
            o.bool("truncated", false),
            o.str("durationMs", "0").toLongOrNull() ?: 0,
            o.str("size", "0").toLongOrNull() ?: 0,
            o.arr("tests")?.mapNotNull { (it as? JsonObject)?.let(TestResult::of) } ?: emptyList(),
        )
    }
}

data class TestResult(val name: String, val pass: Boolean, val detail: String = "") {
    fun toJson(): JsonObject = buildJsonObject { put("name", name); put("pass", pass); if (detail.isNotEmpty()) put("detail", detail) }

    companion object {
        fun of(o: JsonObject) = TestResult(o.str("name"), o.bool("pass", false), o.str("detail"))
    }
}

/** One send: the request as it was (secrets as `${name}`, never values) and its response. */
data class HistoryEntry(
    val id: Long,
    val timestamp: Long,
    val collectionName: String,
    val request: Request,
    val response: ResponseSnapshot?,
    val error: String? = null,
) {
    fun toJson(): JsonObject = buildJsonObject {
        put("id", id)
        put("timestamp", timestamp)
        put("collectionName", collectionName)
        put("request", request.json)
        response?.let { put("response", it.toJson()) }
        error?.let { put("error", it) }
    }

    companion object {
        const val MAX = 100
        fun of(o: JsonObject): HistoryEntry? {
            val r = o.obj("request") ?: return null
            return HistoryEntry(
                o.str("id", "0").toLongOrNull() ?: 0,
                o.str("timestamp", "0").toLongOrNull() ?: 0,
                o.str("collectionName"),
                Request(r),
                o.obj("response")?.let(ResponseSnapshot::of),
                o.strOrNull("error"),
            )
        }
    }
}
