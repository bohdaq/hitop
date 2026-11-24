package tech.rws8.hitop.cloud

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.core.EmptyObject
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.str
import tech.rws8.hitop.core.strOrNull
import java.net.URI

/**
 * An in-memory HITOP Cloud for UI tests and screenshots (`https://cloud.hitop.test`), answering
 * with sync-server's shapes and rules for what the app calls: methods, browser and native Google
 * sign-in (codes it hands to [approve]), /v1/me, device keys with a limit, devices, organizations,
 * usage, the library with versions, `baseVersion` 409s, tombstones, sealed secrets, logout and
 * account deletion. The real server is covered by CloudEndToEndTest.
 */
class FakeCloud : CloudTransport {
    companion object {
        const val SERVER = "https://cloud.hitop.test"
        const val EMAIL = "ada@example.com"
    }

    /** A workspace (shared collection): wraps by (key id, epoch), blobs, a change log, invites. */
    class Workspace(val id: String, var epoch: Int, val org: String?) {
        val wraps = HashMap<Pair<String, Int>, String>()
        val blobs = LinkedHashMap<String, Triple<Long, JsonElement, String?>>() // id → (version, vv, content or null when deleted)
        val log = ArrayList<Pair<Long, String>>() // seq → blob id
        val invites = ArrayList<String>()
        var viewerRole = false
    }

    val workspaces = LinkedHashMap<String, Workspace>()
    private var seq = 0L

    data class Item(val kind: String, val id: String, var version: Long, var data: JsonElement, var secrets: JsonElement, var deleted: Boolean = false)

    private var nextVersion = 1L
    private var nextKey = 1
    private val sessions = HashMap<String, String>() // session → user id
    private val keys = LinkedHashMap<String, Pair<String, String>>() // key id → (name, public key)
    private val codes = HashMap<String, String>() // code → challenge
    private val nonces = HashSet<String>()
    val items = LinkedHashMap<String, Item>()
    var deviceLimit = 5
    var userDeleted = false
    /** UI tests: the next PUT of a collection gets a 409 after the server's copy changed (another device). */
    var conflictOnNextCollectionWrite = false
    val calls = ArrayList<String>()

    /** What a provider page does when the person approves: the one-time code for the app's return address. */
    @Synchronized
    fun approve(): String = "code-${codes.size + 1}".also { codes[it] = "" }

    /** Another device of the account (device-limit tests). */
    @Synchronized
    fun addDevice(name: String) { keys["key-${nextKey++}"] = name to "pk" }

    @Synchronized
    fun nativeIdToken(nonce: String) = "fake-id-token.$nonce"

    override suspend fun send(request: CloudRequest): CloudResponse = handle(request)

    @Synchronized
    fun handle(r: CloudRequest): CloudResponse {
        val uri = URI(r.url)
        val path = uri.path
        val query = uri.rawQuery.orEmpty()
        calls.add("${r.method} $path")
        val body = r.body?.let { runCatching { HitopJson.parseToJsonElement(String(it, Charsets.UTF_8)) as? JsonObject }.getOrNull() } ?: EmptyObject
        val session = r.headers["Authorization"]?.removePrefix("Bearer ")
        val user = session?.let { sessions[it] }
        fun ok(status: Int = 200, j: JsonElement = EmptyObject) = CloudResponse(status, j.toString().toByteArray())
        fun err(status: Int, code: String, message: String, more: JsonObject.() -> JsonObject = { this }) =
            CloudResponse(status, buildJsonObject { put("error", code); put("message", message) }.more().toString().toByteArray())

        when {
            path == "/v1/auth/methods" -> return ok(j = buildJsonObject { put("google", true); put("googleNative", true); put("github", true); put("apple", true) })
            path.matches(Regex("/v1/auth/(google|github|apple)/start")) -> {
                val kind = path.split('/')[3]
                return ok(j = buildJsonObject { put("url", "https://$kind.example/authorize?client=hitop") })
            }
            path.matches(Regex("/v1/auth/(google|github|apple)/finish")) -> {
                if (codes.remove(body.str("code")) == null) return err(401, "invalid_code", "this sign-in code is invalid, used or expired; sign in again")
                return signIn()
            }
            path == "/v1/auth/google/native/start" -> return ok(j = buildJsonObject { put("nonce", "nonce-${nonces.size + 1}".also { nonces.add(it) }) })
            path == "/v1/auth/google/native/finish" -> {
                val n = body.str("nonce")
                if (!nonces.remove(n) || body.str("idToken") != nativeIdToken(n)) return err(401, "bad_token", "the ID token was refused")
                return signIn()
            }
        }
        if (user == null || userDeleted) return err(401, "unauthorized", "sign in again")
        return when {
            path == "/v1/me" && r.method == "GET" -> ok(j = buildJsonObject {
                put("id", user); put("email", EMAIL); put("devices", true)
                put("keys", JsonArray(keys.map { (id, k) -> buildJsonObject { put("id", id); put("name", k.first); put("publicKey", k.second) } }))
            })
            path == "/v1/me" && r.method == "DELETE" -> {
                if (body.str("confirm") != EMAIL) return err(400, "confirm", "confirm with the account's address")
                userDeleted = true
                ok(j = buildJsonObject { put("deleted", true) })
            }
            path == "/v1/me/keys" && r.method == "POST" -> {
                if (keys.size >= deviceLimit) return err(402, "limit", "This plan allows $deviceLimit devices. See or change the plan at https://cloud.hitop.test/orgs/p/plan") {
                    JsonObject(this + mapOf("limit" to JsonPrimitive("devices"), "max" to JsonPrimitive(deviceLimit), "upgradeUrl" to JsonPrimitive("https://cloud.hitop.test/plan")))
                }
                val id = "key-${nextKey++}"
                keys[id] = body.str("name") to body.str("publicKey")
                ok(201, buildJsonObject { put("id", id) })
            }
            path.startsWith("/v1/me/keys/") && r.method == "DELETE" -> { keys.remove(path.removePrefix("/v1/me/keys/")); ok(204) }
            path.startsWith("/v1/me/keys/") && r.method == "PUT" -> {
                val id = path.removePrefix("/v1/me/keys/")
                keys[id]?.let { keys[id] = body.str("name") to it.second }
                ok()
            }
            path == "/v1/me/devices" -> ok(j = buildJsonObject {
                val mine = r.headers["X-Hitop-Device"]
                put("devices", JsonArray(keys.map { (id, k) -> buildJsonObject { put("id", id); put("name", k.first); put("createdAt", 1_794_900_000); put("lastUsedAt", 1_794_912_000); put("workspaces", 0); put("thisDevice", id == mine) } }))
            })
            path == "/v1/auth/logout" -> { sessions.remove(session); ok() }
            path == "/v1/orgs" -> ok(j = buildJsonObject {
                put("orgs", buildJsonArray {
                    add(buildJsonObject { put("id", "personal-$user"); put("name", "Personal"); put("personal", true); put("role", "owner"); put("plan", "free") })
                    add(buildJsonObject { put("id", "acme"); put("name", "Acme"); put("personal", false); put("role", "member"); put("plan", "team") })
                })
            })
            path.matches(Regex("/v1/orgs/[^/]+/usage")) -> ok(j = buildJsonObject {
                put("plan", "free"); put("planInForce", "free")
                put("usage", buildJsonObject { put("devices", keys.size); put("storageBytes", 52_428_800); put("workspaces", 1) })
                put("limits", buildJsonObject { put("devices", deviceLimit); put("storageBytes", 104_857_600); put("workspaces", 3) })
            })
            path == "/v1/workspaces" && r.method == "GET" -> ok(j = buildJsonObject {
                put("workspaces", JsonArray(workspaces.values.map { w -> buildJsonObject {
                    put("id", w.id); put("role", if (w.viewerRole) "viewer" else "owner"); put("epoch", w.epoch)
                    put("org", buildJsonObject { put("id", w.org ?: "personal-$user"); put("name", if (w.org == null) "Personal" else "Acme"); put("personal", w.org == null) })
                } }))
            })
            path == "/v1/workspaces" && r.method == "POST" -> {
                val w = Workspace(body.str("id"), 1, body.strOrNull("org"))
                (body["wraps"] as? JsonArray)?.forEach { x -> (x as JsonObject).let { w.wraps[it.str("keyId") to 1] = it.str("wrapped") } }
                workspaces[w.id] = w
                ok(201, buildJsonObject { put("id", w.id) })
            }
            path.startsWith("/v1/workspaces/") -> workspace(r, path.removePrefix("/v1/workspaces/").split('/'), body, query, user, ::ok, ::err)
            path == "/v1/invites/accept" -> err(404, "not_found", "no such invite")
            path == "/v1/library" && r.method == "GET" -> {
                val since = Regex("since=(\\d+)").find(query)?.groupValues?.get(1)?.toLong() ?: 0
                ok(j = buildJsonObject {
                    put("items", JsonArray(items.values.filter { it.version > since }.map { itemJson(it) }))
                    put("cursor", nextVersion - 1)
                    put("secretsStored", true)
                })
            }
            path.startsWith("/v1/library/") -> {
                val parts = path.removePrefix("/v1/library/").split('/')
                val key = "${parts[0]}/${parts[1]}"
                val existing = items[key]
                val base = if (r.method == "DELETE") Regex("baseVersion=(\\d+)").find(query)?.groupValues?.get(1)?.toLong() ?: 0 else body.strOrNull("baseVersion")?.toLongOrNull() ?: 0
                if (parts[0] == "collection" && conflictOnNextCollectionWrite && existing != null && r.method == "PUT") {
                    conflictOnNextCollectionWrite = false
                    val d = existing.data as JsonObject
                    existing.data = JsonObject(d + ("name" to JsonPrimitive(d.str("name") + " (edited elsewhere)")))
                    existing.version = nextVersion++
                }
                if ((existing?.version ?: 0) != base) return CloudResponse(409, buildJsonObject { put("error", "conflict"); put("current", existing?.let { itemJson(it) } ?: JsonNull) }.toString().toByteArray())
                val v = nextVersion++
                if (r.method == "DELETE") {
                    existing?.let { it.deleted = true; it.version = v; it.data = JsonNull }
                } else {
                    items[key] = Item(parts[0], parts[1], v, body["data"] ?: JsonNull, body["secrets"] ?: EmptyObject)
                }
                ok(j = buildJsonObject { put("version", v); put("secretsStored", true) })
            }
            else -> err(404, "not_found", "no such endpoint ${r.method} $path")
        }
    }

    private fun workspace(r: CloudRequest, parts: List<String>, body: JsonObject, query: String, user: String, ok: (Int, JsonElement) -> CloudResponse, err: (Int, String, String, JsonObject.() -> JsonObject) -> CloudResponse): CloudResponse {
        val w = workspaces[parts[0]] ?: return err(404, "not_found", "no such workspace") { this }
        fun keysJson() = JsonArray(keys.map { (id, k) -> buildJsonObject { put("id", id); put("name", k.first); put("publicKey", k.second); put("hasKey", w.wraps.containsKey(id to w.epoch)) } })
        val rest = parts.drop(1)
        return when {
            rest.isEmpty() && r.method == "GET" -> ok(200, buildJsonObject {
                put("id", w.id); put("epoch", w.epoch); put("role", if (w.viewerRole) "viewer" else "owner")
                put("members", JsonArray(listOf(buildJsonObject { put("userId", user); put("email", EMAIL); put("role", if (w.viewerRole) "viewer" else "owner"); put("runner", false); put("keys", keysJson()) })))
            })
            rest == listOf("wraps") && r.method == "GET" -> {
                val keyId = Regex("keyId=([^&]+)").find(query)?.groupValues?.get(1).orEmpty()
                ok(200, buildJsonObject {
                    put("epoch", w.epoch)
                    put("wraps", JsonArray(w.wraps.filterKeys { it.first == keyId }.map { (k, v) -> buildJsonObject { put("epoch", k.second); put("wrapped", v) } }))
                })
            }
            rest == listOf("wraps") && r.method == "POST" -> {
                val e = body.str("epoch", "1").toInt()
                (body["wraps"] as? JsonArray)?.forEach { x -> (x as JsonObject).let { w.wraps[it.str("keyId") to e] = it.str("wrapped") } }
                ok(200, buildJsonObject { put("stored", (body["wraps"] as? JsonArray)?.size ?: 0) })
            }
            rest == listOf("changes") -> {
                val since = Regex("since=(\\d+)").find(query)?.groupValues?.get(1)?.toLong() ?: 0
                val latest = w.log.filter { it.first > since }.groupBy { it.second }.map { (id, l) -> l.maxOf { it.first } to id }.sortedBy { it.first }
                ok(200, buildJsonObject {
                    put("seq", seq)
                    put("more", false)
                    put("changes", JsonArray(latest.map { (sq, id) -> val b = w.blobs.getValue(id); buildJsonObject { put("id", id); put("seq", sq); put("version", b.first); put("vv", b.second); if (b.third == null) put("deleted", true) } }))
                })
            }
            rest.size == 2 && rest[0] == "blobs" && r.method == "GET" -> {
                val b = w.blobs[rest[1]] ?: return err(404, "not_found", "no such blob") { this }
                ok(200, buildJsonObject { put("version", b.first); put("vv", b.second); b.third?.let { put("content", it) } })
            }
            rest.size == 2 && rest[0] == "blobs" && r.method == "PUT" -> {
                val id = rest[1]
                val version = body.str("version", "0").toLong()
                val current = w.blobs[id]?.first ?: 0
                if (version != current + 1) return err(409, "conflict", "the blob changed") { this }
                if (body.str("epoch", "0").toInt() != w.epoch) return err(409, "stale_epoch", "a newer key exists") { this }
                w.blobs[id] = Triple(version, body["vv"] ?: EmptyObject, body.strOrNull("content"))
                w.log.add(++seq to id)
                ok(200, buildJsonObject { put("version", version) })
            }
            rest == listOf("invites") && r.method == "POST" -> {
                val token = "inv${w.invites.size + 1}"
                w.invites.add(body.str("email"))
                ok(201, buildJsonObject { put("token", token); put("mailed", body.strOrNull("email") != null) })
            }
            rest == listOf("invites") && r.method == "GET" -> ok(200, buildJsonObject {
                put("invites", JsonArray(w.invites.mapIndexed { i, e -> buildJsonObject { put("id", "i$i"); put("email", e); put("role", "editor"); put("accepted", JsonNull) } }))
            })
            rest == listOf("env-shares") -> err(404, "not_found", "no shares") { this }
            else -> err(404, "not_found", "no such endpoint ${r.method} ${parts.joinToString("/")}") { this }
        }
    }

    private fun itemJson(it: Item) = buildJsonObject {
        put("kind", it.kind); put("id", it.id); put("version", it.version)
        if (it.deleted) put("deleted", true) else { put("data", it.data); put("secrets", it.secrets) }
    }

    private fun signIn(): CloudResponse {
        userDeleted = false
        val session = "session-${sessions.size + 1}"
        sessions[session] = "user-1"
        return CloudResponse(200, buildJsonObject { put("session", session); put("user", buildJsonObject { put("id", "user-1"); put("email", EMAIL) }) }.toString().toByteArray())
    }
}
