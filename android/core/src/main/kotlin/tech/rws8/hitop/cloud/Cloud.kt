package tech.rws8.hitop.cloud

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.core.EmptyObject
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.SecretStore
import tech.rws8.hitop.core.bool
import tech.rws8.hitop.core.obj
import tech.rws8.hitop.core.str
import tech.rws8.hitop.core.strOrNull
import tech.rws8.hitop.ffi.Ffi
import java.io.File
import java.net.HttpURLConnection
import java.net.URI
import java.net.URL
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64

/*
 * HITOP Cloud (the sync server, sync-server/): sign-in, this device's key, devices, organizations,
 * plan usage, account deletion. The iOS app's Cloud.swift and the web app's sync.js call for call.
 * Store mode everywhere (no price, plan offer, upgrade or billing link): server texts go through
 * [StoreText.withoutPlanLink], `upgradeUrl` is never read.
 */

data class CloudRequest(val method: String, val url: String, val headers: Map<String, String>, val body: ByteArray?)

data class CloudResponse(val status: Int, val body: ByteArray) {
    val json: JsonObject? get() = runCatching { HitopJson.parseToJsonElement(String(body, Charsets.UTF_8)) as? JsonObject }.getOrNull()
}

fun interface CloudTransport {
    /** Throws when there is no answer at all (offline, refused); any HTTP status is an answer. */
    suspend fun send(request: CloudRequest): CloudResponse
}

/** HttpURLConnection: on Android and on the JVM, no extra dependency. */
class UrlConnectionTransport(private val timeoutMs: Int = 20_000) : CloudTransport {
    override suspend fun send(request: CloudRequest): CloudResponse = withContext(Dispatchers.IO) {
        val c = URL(request.url).openConnection() as HttpURLConnection
        try {
            c.requestMethod = request.method
            c.connectTimeout = timeoutMs
            c.readTimeout = timeoutMs
            c.instanceFollowRedirects = false
            request.headers.forEach { (k, v) -> c.setRequestProperty(k, v) }
            if (request.body != null) {
                c.doOutput = true
                c.setFixedLengthStreamingMode(request.body.size)
                c.outputStream.use { it.write(request.body) }
            }
            val status = c.responseCode
            val bytes = (if (status >= 400) c.errorStream else c.inputStream)?.use { it.readBytes() } ?: ByteArray(0)
            CloudResponse(status, bytes)
        } finally {
            c.disconnect()
        }
    }
}

class CloudException(val status: Int, val code: String, override val message: String) : Exception(message) {
    companion object {
        fun unreachable(why: String) = CloudException(0, "unreachable", "Couldn't reach HITOP Cloud ($why).")
        val notSignedIn = CloudException(0, "not_signed_in", "Sign in to HITOP Cloud first.")
        fun badServer(why: String) = CloudException(0, "bad_server", why)
    }
}

/** ST1 store mode: what the server says about plans, without where to see, change or buy one. */
object StoreText {
    private const val PLANS = "(?:Pro|Team|Business|Enterprise)"
    fun withoutPlanLink(text: String): String {
        var s = text
        fun replace(p: String, with: String) { s = Regex(p, RegexOption.IGNORE_CASE).replace(s, with) }
        replace("\\s*See or change the plan at \\S+", "")
        replace("\\bcome with $PLANS(?:, $PLANS)*(?:,? and $PLANS)? plans?\\b", "aren't part of this plan")
        replace(",?\\s*https?://[^\\s)]*/(?:plan|pricing|billing|checkout)\\b[^\\s)]*", "")
        replace("\\s+([.,;)])", "$1")
        return s.trim()
    }
}

data class CloudAccount(val server: String, val email: String, val userId: String, val keyId: String, val deviceName: String) {
    fun toJson() = buildJsonObject { put("server", server); put("email", email); put("userId", userId); put("keyId", keyId); put("deviceName", deviceName) }

    companion object {
        fun of(o: JsonObject?) = o?.let { CloudAccount(it.str("server"), it.str("email"), it.str("userId"), it.str("keyId"), it.str("deviceName")) }
    }
}

data class CloudOrg(val id: String, val name: String, val personal: Boolean, val role: String, val plan: String) {
    fun toJson() = buildJsonObject { put("id", id); put("name", name); put("personal", personal); put("role", role); put("plan", plan) }

    companion object {
        fun of(o: JsonObject) = CloudOrg(o.str("id"), o.str("name"), o.bool("personal", false), o.str("role"), o.str("plan", "free"))
    }
}

/** cloud.json: everything about HITOP Cloud that isn't secret. */
data class CloudSettings(
    val server: String = DEFAULT_SERVER,
    val account: CloudAccount? = null,
    /** Signed out: the device this app was for an account (signing in again = the same device). */
    val device: CloudAccount? = null,
    /** The organization the Collections screen shows (null: the personal one). */
    val org: String? = null,
    val orgs: List<CloudOrg> = emptyList(),
) {
    fun toJson(): JsonObject = buildJsonObject {
        put("server", server)
        account?.let { put("account", it.toJson()) }
        device?.let { put("device", it.toJson()) }
        org?.let { put("org", it) }
        put("orgs", JsonArray(orgs.map { it.toJson() }))
    }

    companion object {
        const val DEFAULT_SERVER = "https://sync.hitop.rws8.tech"
        fun of(o: JsonObject?): CloudSettings = if (o == null) CloudSettings() else CloudSettings(
            o.str("server", DEFAULT_SERVER), CloudAccount.of(o.obj("account")), CloudAccount.of(o.obj("device")), o.strOrNull("org"),
            (o["orgs"] as? JsonArray)?.mapNotNull { (it as? JsonObject)?.let(CloudOrg::of) } ?: emptyList(),
        )
    }
}

interface CloudSettingsStore {
    fun load(): CloudSettings
    fun save(s: CloudSettings)
}

class FileCloudSettingsStore(private val file: File) : CloudSettingsStore {
    override fun load() = CloudSettings.of(runCatching { HitopJson.parseToJsonElement(file.readText()).jsonObject }.getOrNull())
    override fun save(s: CloudSettings) {
        tech.rws8.hitop.core.SafeFiles.write(file, s.toJson().toString())
    }
}

class MemoryCloudSettingsStore(var value: CloudSettings = CloudSettings()) : CloudSettingsStore {
    override fun load() = value
    override fun save(s: CloudSettings) { value = s }
}

/** The session and the device's secret key, in the secret store (`android/cloud/<host>`). */
class CloudCredentials(private val store: SecretStore) {
    data class Secret(val session: String?, val secret: String)

    private fun key(server: String) = "android/cloud/" + (runCatching { URI(server).host }.getOrNull() ?: server)

    fun load(server: String): Secret? = store.get(key(server))?.let { t ->
        runCatching { HitopJson.parseToJsonElement(t).jsonObject }.getOrNull()?.let { Secret(it.strOrNull("session"), it.str("secret")) }
    }

    fun save(server: String, s: Secret?) = store.set(key(server), s?.let { buildJsonObject { it.session?.let { v -> put("session", v) }; put("secret", it.secret) }.toString() })
}

data class AuthMethods(val google: Boolean = false, val googleNative: Boolean = false, val github: Boolean = false, val apple: Boolean = false, val sso: Boolean = false) {
    /** In the order the sign-in sheet shows them. */
    val providers: List<String> get() = listOfNotNull("google".takeIf { google }, "github".takeIf { github }, "apple".takeIf { apple })

    companion object {
        fun name(kind: String) = when (kind) { "google" -> "Google"; "github" -> "GitHub"; "apple" -> "Apple"; "sso" -> "company SSO"; else -> kind }
    }
}

data class CloudDevice(val id: String, val name: String, val createdAt: Long?, val lastUsedAt: Long?, val workspaces: Int, val thisDevice: Boolean)

/** A sign-in that would add a device past the account's limit: nothing was added. */
class DeviceLimit(val server: String, val session: String, val userId: String, val email: String, val max: Int, val devices: List<CloudDevice>) :
    Exception("This account allows $max devices. Remove one to sign in here.")

data class UsageMeter(val id: String, val label: String, val used: Long, val max: Long?, val level: String, val text: String, val note: String) {
    val fraction: Float get() = if (max == null || max <= 0) 0f else minOf(1f, used.toFloat() / max)
}

/** `GET /v1/orgs/:org/usage` with the web app's store-build wording. */
data class CloudUsage(val plan: String, val planInForce: String, val meters: List<UsageMeter>) {
    val planName: String get() = PLAN_NAMES[planInForce] ?: planInForce.replaceFirstChar { it.uppercase() }
    val ended: Boolean get() = planInForce != plan

    companion object {
        val PLAN_NAMES = mapOf("free" to "Free", "pro" to "Pro", "team" to "Team", "business" to "Business", "enterprise" to "Enterprise")

        private fun bytes(n: Long): String {
            val gb = 1024.0 * 1024 * 1024
            val mb = 1024.0 * 1024
            return if (n >= gb) String.format(java.util.Locale.ROOT, if (n >= 10 * gb) "%.0f GB" else "%.1f GB", n / gb) else "${Math.round(n / mb)} MB"
        }

        fun of(j: JsonObject): CloudUsage {
            val plan = j.str("plan", "free")
            val inForce = j.str("planInForce", plan)
            val u = j.obj("usage") ?: EmptyObject
            val l = j.obj("limits") ?: EmptyObject
            fun n(o: JsonObject, k: String) = o.strOrNull(k)?.toDoubleOrNull()?.toLong()
            val rows = listOf(
                Triple("Seats", "seats", "seats"), Triple("Shared collections", "workspaces", "workspaces"), Triple("Storage", "storage", "storageBytes"),
                Triple("Devices", "devices", "devices"), Triple("CI runners", "runners", "runners"),
            )
            val meters = rows.mapNotNull { (label, id, key) ->
                val used = n(u, key) ?: return@mapNotNull null
                val max = n(l, key)
                if (max == 0L && used == 0L) return@mapNotNull null
                val fmt: (Long) -> String = if (id == "storage") ::bytes else { v -> v.toString() }
                val percent = if (max != null && max > 0) minOf(100, Math.round(used.toDouble() / max * 100).toInt()) else 0
                val lastOne = max != null && max >= 2 && id != "storage" && max - used == 1L
                val level = when { max == null -> "ok"; used >= max -> "full"; percent >= 80 || lastOne -> "warn"; else -> "ok" }
                val text = if (max != null) "${fmt(used)} of ${fmt(max)}" else "${fmt(used)} · no limit"
                val note = when (level) {
                    "full" -> if (id == "seats") "Full: every seat is taken" else "Full: new ones are refused until there is room"
                    "warn" -> if (lastOne) "Nearly full: 1 left" else "Nearly full"
                    else -> ""
                }
                UsageMeter(id, label, used, max, level, text, note)
            }
            return CloudUsage(plan, inForce, meters)
        }
    }
}

/** PKCE for the browser sign-ins: a 64-hex verifier and base64url(SHA-256) of it. */
object CloudCrypto {
    private val random = SecureRandom()
    fun verifier(): Pair<String, String> {
        val bytes = ByteArray(32).also(random::nextBytes)
        val v = bytes.joinToString("") { "%02x".format(it) }
        val challenge = Base64.getUrlEncoder().withoutPadding().encodeToString(MessageDigest.getInstance("SHA-256").digest(v.toByteArray()))
        return v to challenge
    }
}

/** This device's X25519 key (hitop-sync-crypto through hitop-ffi): base64url secret and public key. */
fun interface DeviceKeyMaker {
    fun make(): Pair<String, String>

    companion object {
        val Ffi = DeviceKeyMaker {
            val k = tech.rws8.hitop.ffi.Ffi.call("keygen").jsonObject
            k.str("secret") to k.str("publicKey")
        }
    }
}

class HitopCloud(
    private val transport: CloudTransport,
    private val credentials: CloudCredentials,
    private val settingsStore: CloudSettingsStore,
    private val keyMaker: DeviceKeyMaker = DeviceKeyMaker.Ffi,
    var deviceName: String = "HITOP on Android",
) {
    private val _settings = MutableStateFlow(settingsStore.load())
    val settings: StateFlow<CloudSettings> = _settings.asStateFlow()
    val lastError = MutableStateFlow<String?>(null)

    /** After signing in or out (LibrarySync links or stops). */
    var onAccountChange: (() -> Unit)? = null
    /** After the account was deleted (shared-collection links forget it). */
    var onAccountDeleted: ((CloudAccount) -> Unit)? = null

    private var pendingBrowser: Triple<String, String, String>? = null // server, verifier, kind
    private var pendingEmail: String? = null // server

    init {
        // An account whose session is gone (restored to another phone, store cleared) is signed out.
        val s = _settings.value
        if (s.account != null && credentials.load(s.account.server)?.session == null) {
            update { it.copy(device = it.account, account = null) }
        }
    }

    val account: CloudAccount? get() = _settings.value.account
    val signedIn: Boolean get() = account != null
    /** The library's owner: "<server> <userId>". */
    val accountKey: String get() = account?.let { "${it.server} ${it.userId}" } ?: ""

    private fun update(f: (CloudSettings) -> CloudSettings) {
        _settings.value = f(_settings.value)
        settingsStore.save(_settings.value)
    }

    fun setServer(server: String) = update { it.copy(server = normalize(server)) }

    // ---- HTTP

    suspend fun http(server: String, method: String, path: String, body: JsonElement? = null, session: String? = null): CloudResponse {
        val headers = LinkedHashMap<String, String>()
        headers["Accept"] = "application/json"
        if (body != null) headers["Content-Type"] = "application/json"
        if (session != null) {
            headers["Authorization"] = "Bearer $session"
            // DD3: which device this is, for this account's own session.
            account?.takeIf { it.server == server }?.let { headers["X-Hitop-Device"] = it.keyId }
        }
        return try {
            transport.send(CloudRequest(method, server + path, headers, body?.toString()?.toByteArray(Charsets.UTF_8)))
        } catch (e: Exception) {
            throw CloudException.unreachable(e.message ?: e.toString())
        }
    }

    /** A request as the signed-in account; [allow]: error statuses returned instead of thrown. */
    suspend fun call(method: String, path: String, body: JsonElement? = null, allow: Set<Int> = emptySet()): CloudResponse {
        val a = account ?: throw CloudException.notSignedIn
        val session = credentials.load(a.server)?.session ?: throw CloudException.notSignedIn
        val r = http(a.server, method, path, body, session)
        if (r.status in 200..299 || r.status in allow) return r
        throw error(r)
    }

    // ---- sign-in

    suspend fun authMethods(server: String): AuthMethods? {
        val r = http(normalize(server), "GET", "/v1/auth/methods")
        val j = r.json ?: return null
        if (r.status != 200) return null
        return AuthMethods(j.bool("google", false), j.bool("googleNative", false), j.bool("github", false), j.bool("apple", false), j.bool("sso", false))
    }

    /**
     * Starts Continue with Google/GitHub/Apple in a Custom Tab: the provider page to open. `paste`:
     * the server didn't take the app's return address (an older server), so its page shows a code.
     */
    suspend fun startBrowserSignIn(kind: String, server: String, returnTo: String? = RETURN_TO, email: String? = null): Pair<String, Boolean> {
        val srv = normalize(server)
        val (verifier, challenge) = CloudCrypto.verifier()
        fun body(withReturn: Boolean) = buildJsonObject { put("challenge", challenge); email?.takeIf { kind == "sso" && it.isNotBlank() }?.let { put("email", it.trim()) }; if (withReturn && returnTo != null) put("returnTo", returnTo) }
        var r = http(srv, "POST", "/v1/auth/$kind/start", body(true))
        var paste = returnTo == null
        if (r.status == 400 && returnTo != null && r.json?.str("message").orEmpty().contains("returnTo", ignoreCase = true)) {
            r = http(srv, "POST", "/v1/auth/$kind/start", body(false))
            paste = true
        }
        if (r.status == 404 && kind == "sso") throw CloudException(404, "no_sso", if (email.isNullOrBlank()) "This server has no single sign-on." else "There is no single sign-on for this address's organization here.")
        if (r.status == 404) throw CloudException(404, "no_provider", "This server has no sign-in with ${AuthMethods.name(kind)}.")
        if (r.status != 200) throw error(r)
        val url = r.json?.strOrNull("url")?.takeIf { it.startsWith("https://") || it.startsWith("http://") }
            ?: throw CloudException.badServer("The server answered without a sign-in page address")
        pendingBrowser = Triple(srv, verifier, kind)
        return url to paste
    }

    /** The one-time code back (from `hitop://sign-in?sso=` or pasted) → signed in. Throws [DeviceLimit]. */
    suspend fun finishBrowserSignIn(code: String): Boolean {
        val (server, verifier, kind) = pendingBrowser ?: throw CloudException.badServer("This sign-in wasn't started here, or finished already: start again")
        val r = http(server, "POST", "/v1/auth/$kind/finish", buildJsonObject { put("code", code.trim()); put("verifier", verifier) })
        val j = r.json
        if (r.status != 200 || j == null) throw error(r)
        pendingBrowser = null
        return completeSignIn(server, j)
    }

    /** Native Google sign-in, step 1: a nonce for Credential Manager (the server keeps it for one use). */
    suspend fun googleNonce(server: String): String {
        val r = http(normalize(server), "POST", "/v1/auth/google/native/start", EmptyObject)
        if (r.status != 200) throw error(r)
        return r.json?.strOrNull("nonce") ?: throw CloudException.badServer("The server answered without a nonce")
    }

    /** Step 2: the Google ID token Credential Manager gave → signed in. */
    suspend fun finishGoogleNative(server: String, idToken: String, nonce: String): Boolean {
        val srv = normalize(server)
        val r = http(srv, "POST", "/v1/auth/google/native/finish", buildJsonObject { put("idToken", idToken); put("nonce", nonce) })
        val j = r.json
        if (r.status != 200 || j == null) throw error(r)
        return completeSignIn(srv, j)
    }

    /** Sign-in with a mailed code: not in the app's screens (as the web app); the end-to-end test uses it. */
    suspend fun startEmailSignIn(server: String, email: String) {
        val srv = normalize(server)
        val r = http(srv, "POST", "/v1/auth/email", buildJsonObject { put("email", email) })
        if (r.status != 202) throw error(r)
        pendingEmail = srv
    }

    suspend fun finishEmailSignIn(code: String): Boolean {
        val server = pendingEmail ?: throw CloudException.badServer("Ask for a code first")
        val token = code.trim().split('#').last()
        val r = http(server, "POST", "/v1/auth/email/verify", buildJsonObject { put("token", token) })
        val j = r.json
        if (r.status != 200 || j == null) throw error(r)
        pendingEmail = null
        return completeSignIn(server, j)
    }

    /** After a sign-in (`{session, user: {id, email}}`): this device's key kept or registered. True when a device was added. */
    suspend fun completeSignIn(server: String, reply: JsonObject): Boolean {
        val session = reply.strOrNull("session") ?: throw CloudException.badServer("The server's sign-in answer has no session")
        val user = reply.obj("user") ?: throw CloudException.badServer("The server's sign-in answer has no user")
        val userId = user.str("id")
        val email = user.str("email")
        val me = http(server, "GET", "/v1/me", session = session)
        val mj = me.json
        if (me.status != 200 || mj == null) throw error(me)
        val keys = (mj["keys"] as? JsonArray)?.mapNotNull { it as? JsonObject } ?: emptyList()
        val old = account ?: _settings.value.device
        val oldSecret = old?.let { credentials.load(server)?.secret }
        if (old != null && old.userId == userId && old.server == server && oldSecret != null && keys.any { it.str("id") == old.keyId }) {
            credentials.save(server, CloudCredentials.Secret(session, oldSecret))
            signedIn(CloudAccount(server, email, userId, old.keyId, old.deviceName))
            return false
        }
        val (secret, publicKey) = keyMaker.make()
        val added = http(server, "POST", "/v1/me/keys", buildJsonObject { put("name", deviceName); put("publicKey", publicKey) }, session)
        val aj = added.json
        if (added.status == 402 && aj?.str("limit") == "devices") {
            val devices = keys.map { CloudDevice(it.str("id"), it.str("name"), null, null, 0, false) }
            throw DeviceLimit(server, session, userId, email, aj.strOrNull("max")?.toIntOrNull() ?: keys.size, devices)
        }
        val keyId = aj?.strOrNull("id")
        if (added.status != 201 || keyId == null) throw error(added)
        credentials.save(server, CloudCredentials.Secret(session, secret))
        signedIn(CloudAccount(server, email, userId, keyId, deviceName))
        return true
    }

    private fun signedIn(a: CloudAccount) {
        update { s -> s.copy(account = a, device = null, server = a.server, org = s.org.takeIf { o -> s.orgs.any { it.id == o } }) }
        lastError.value = null
        onAccountChange?.invoke()
    }

    /** At the device limit: removes one of the account's devices with the pending session. */
    suspend fun removeDevice(id: String, at: DeviceLimit): DeviceLimit {
        val r = http(at.server, "DELETE", "/v1/me/keys/$id", session = at.session)
        if (r.status !in listOf(200, 204, 404)) throw error(r)
        return DeviceLimit(at.server, at.session, at.userId, at.email, at.max, at.devices.filter { it.id != id })
    }

    suspend fun retry(at: DeviceLimit): Boolean = completeSignIn(at.server, buildJsonObject {
        put("session", at.session)
        put("user", buildJsonObject { put("id", at.userId); put("email", at.email) })
    })

    /** Ends the session; this device's key stays, so signing in to the same account again is the same device. */
    suspend fun signOut() {
        val a = account ?: return
        runCatching { call("POST", "/v1/auth/logout", allow = setOf(401)) }
        credentials.load(a.server)?.let { credentials.save(a.server, CloudCredentials.Secret(null, it.secret)) }
        update { it.copy(device = a, account = null, org = null, orgs = emptyList()) }
        onAccountChange?.invoke()
    }

    /** `DELETE /v1/me {confirm: <address>}`, then everything about the account is forgotten here; collections stay. */
    suspend fun deleteAccount() {
        val a = account ?: throw CloudException.notSignedIn
        call("DELETE", "/v1/me", buildJsonObject { put("confirm", a.email) })
        credentials.save(a.server, null)
        update { it.copy(account = null, device = null, org = null, orgs = emptyList()) }
        lastError.value = null
        onAccountDeleted?.invoke(a)
        onAccountChange?.invoke()
    }

    /** The server ended the session (401): signed out here too, the device kept. */
    fun sessionEnded() {
        val a = account ?: return
        credentials.load(a.server)?.let { credentials.save(a.server, CloudCredentials.Secret(null, it.secret)) }
        update { it.copy(device = a, account = null) }
        lastError.value = "Your HITOP Cloud sign-in has ended: sign in again."
        onAccountChange?.invoke()
    }

    /** This device's secret key (shared collections, A3). */
    fun deviceSecret(): String? = account?.let { credentials.load(it.server)?.secret }

    // ---- devices

    suspend fun devices(): List<CloudDevice>? {
        val r = call("GET", "/v1/me/devices", allow = setOf(404))
        val j = r.json ?: return null
        if (r.status != 200) return null
        return (j["devices"] as? JsonArray)?.mapNotNull { it as? JsonObject }?.map { d ->
            CloudDevice(d.str("id"), d.str("name"), d.strOrNull("createdAt")?.toLongOrNull(), d.strOrNull("lastUsedAt")?.toLongOrNull(),
                d.strOrNull("workspaces")?.toIntOrNull() ?: 0, (d["thisDevice"] as? JsonPrimitive)?.content?.toBooleanStrictOrNull() ?: (d.str("id") == account?.keyId))
        } ?: emptyList()
    }

    suspend fun renameDevice(id: String, name: String) {
        val n = name.trim().take(64)
        if (n.isEmpty()) return
        call("PUT", "/v1/me/keys/$id", buildJsonObject { put("name", n) })
        if (account?.keyId == id) update { it.copy(account = it.account?.copy(deviceName = n)) }
    }

    /** Removes another of the account's devices; [force] for one holding shared workspaces' keys (409 otherwise). */
    suspend fun removeDevice(id: String, force: Boolean = false) {
        call("DELETE", "/v1/me/keys/$id?untied=true${if (force) "&force=true" else ""}")
    }

    // ---- organizations and usage

    suspend fun loadOrgs(): List<CloudOrg> {
        val r = call("GET", "/v1/orgs", allow = setOf(404))
        val j = r.json ?: return emptyList()
        if (r.status != 200) return emptyList()
        val orgs = (j["orgs"] as? JsonArray)?.mapNotNull { (it as? JsonObject)?.let(CloudOrg::of) } ?: emptyList()
        update { s -> s.copy(orgs = orgs, org = s.org.takeIf { o -> orgs.any { it.id == o } }) }
        return orgs
    }

    val personalOrg: CloudOrg? get() = _settings.value.orgs.firstOrNull { it.personal }
    val currentOrg: CloudOrg? get() = _settings.value.let { s -> s.orgs.firstOrNull { it.id == s.org } } ?: personalOrg

    fun selectOrg(id: String?) = update { it.copy(org = if (id == personalOrg?.id) null else id) }

    /** The organization a new collection is filed under: null for the personal one. */
    val orgForNew: String? get() = if (signedIn) _settings.value.org else null

    suspend fun usage(org: String): CloudUsage = CloudUsage.of(call("GET", "/v1/orgs/$org/usage").json ?: EmptyObject)

    companion object {
        /** The app's return address (sync-server `oidc::is_app`, an Android intent filter). */
        const val RETURN_TO = "hitop://sign-in"

        /** A server address: https (http only for this machine), no trailing slash. */
        fun normalize(server: String): String {
            var s = server.trim().ifEmpty { CloudSettings.DEFAULT_SERVER }.trimEnd('/')
            val u = runCatching { URI(s) }.getOrNull()
            val scheme = u?.scheme?.lowercase()
            val host = u?.host
            if (u == null || scheme == null || host.isNullOrEmpty()) throw CloudException.badServer("Enter the server's address, like https://sync.example.com")
            val local = host in listOf("localhost", "127.0.0.1", "::1", "[::1]", "10.0.2.2")
            if (!(scheme == "https" || (scheme == "http" && local))) throw CloudException.badServer("The server must be https:// (http:// only for this device)")
            s = s.trimEnd('/')
            return s
        }

        /** The code from `hitop://sign-in?sso=<code>`, a pasted code, or a whole pasted URL. */
        fun code(from: String): String? {
            val t = from.trim()
            if (t.isEmpty()) return null
            val u = runCatching { URI(t) }.getOrNull()
            if (u?.scheme != null) {
                u.rawQuery?.split('&')?.firstOrNull { it.startsWith("sso=") }?.removePrefix("sso=")?.takeIf { it.isNotEmpty() }?.let { return java.net.URLDecoder.decode(it, "UTF-8") }
                u.rawFragment?.takeIf { it.startsWith("sso=") }?.let { return it.removePrefix("sso=") }
                return null
            }
            return t
        }

        fun error(r: CloudResponse): CloudException {
            val j = r.json
            return CloudException(r.status, j?.str("error").orEmpty(), StoreText.withoutPlanLink(j?.str("message").orEmpty().ifEmpty { "HITOP Cloud answered ${r.status}" }))
        }
    }
}

@Suppress("unused")
private val keepNull: JsonElement = JsonNull
