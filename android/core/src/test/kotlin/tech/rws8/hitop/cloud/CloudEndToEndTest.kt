package tech.rws8.hitop.cloud

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import tech.rws8.hitop.core.Library
import tech.rws8.hitop.core.LibraryStore
import tech.rws8.hitop.core.MemorySecretStore
import tech.rws8.hitop.core.Request
import tech.rws8.hitop.core.str
import java.io.File
import java.nio.file.Files
import java.security.SecureRandom

/**
 * The app's core against a real hitop sync server (HITOP_SYNC_SERVER_BIN, built by CI; skipped
 * without it unless HITOP_E2E_REQUIRED is set): sign in by a mailed code (the server logs its
 * mail), push the library from one phone, read it back through the API and on a second phone,
 * a conflict copy, a tombstone, devices, sign-out and in as the same device, organizations and
 * usage, native Google sign-in switched off, account deletion.
 */
class CloudEndToEndTest {
    private var process: Process? = null
    private lateinit var dir: File
    private val log = StringBuffer()
    private var server = ""
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @Before
    fun start() {
        val bin = System.getenv("HITOP_SYNC_SERVER_BIN").orEmpty()
        if (bin.isEmpty()) {
            if (System.getenv("HITOP_E2E_REQUIRED") != null) throw AssertionError("HITOP_SYNC_SERVER_BIN must be set")
            assumeTrue("HITOP_SYNC_SERVER_BIN not set", false)
        }
        dir = Files.createTempDirectory("hitop-android-e2e").toFile()
        val port = 20000 + SecureRandom().nextInt(40000)
        server = "http://127.0.0.1:$port"
        val key = ByteArray(32).also(SecureRandom()::nextBytes).joinToString("") { "%02x".format(it) }
        val pb = ProcessBuilder(bin).directory(dir).redirectErrorStream(true)
        pb.environment().putAll(mapOf(
            "RWS_CONFIG_IP" to "127.0.0.1", "RWS_CONFIG_PORT" to "$port", "HITOP_SYNC_PUBLIC_URL" to server,
            "HITOP_SYNC_RP_ID" to "localhost", "HITOP_SYNC_RP_ORIGINS" to "http://localhost:$port",
            "HITOP_SYNC_DATABASE" to "sqlite:${File(dir, "sync.db").path}", "HITOP_SYNC_MAIL" to "log", "HITOP_SYNC_HOST_KEY" to key,
        ))
        val p = pb.start()
        process = p
        Thread { p.inputStream.bufferedReader().forEachLine { log.append(it).append('\n') } }.apply { isDaemon = true }.start()
        runBlocking {
            repeat(150) {
                val ok = runCatching { UrlConnectionTransport(2000).send(CloudRequest("GET", "$server/healthz", emptyMap(), null)).status == 200 }.getOrDefault(false)
                if (ok) return@runBlocking
                delay(100)
            }
            throw AssertionError("the sync server didn't start: $log")
        }
    }

    @After
    fun stop() {
        process?.destroy()
        process?.waitFor()
        if (::dir.isInitialized) dir.deleteRecursively()
    }

    private suspend fun mailedCode(email: String, after: Int): String {
        repeat(100) {
            val parts = log.toString().split("mail to $email:")
            if (parts.size - 1 > after) {
                parts.last().lines().firstOrNull { it.contains("/signin#") }?.let { return it.trim().substringAfterLast('#') }
            }
            delay(100)
        }
        throw AssertionError("no mail to $email: $log")
    }

    inner class Phone(name: String) {
        val secrets = MemorySecretStore()
        val library = Library(LibraryStore(Files.createTempDirectory("lib").toFile()), secrets).also { it.load() }
        val cloud = HitopCloud(UrlConnectionTransport(), CloudCredentials(secrets), MemoryCloudSettingsStore(CloudSettings(server = server)), deviceName = name)
        val sync = LibrarySync(cloud, library, null, scope).also { it.pushDelayMs = 3_600_000; it.retryDelayMs = 3_600_000 }

        suspend fun signIn(email: String) {
            val before = log.toString().split("mail to $email:").size - 1
            cloud.startEmailSignIn(server, email)
            cloud.finishEmailSignIn(mailedCode(email, before))
            sync.sync()
        }
    }

    private class Cli(val bin: String, val home: File, val server: String) {
        data class Out(val code: Int, val out: String, val err: String)

        fun run(vararg args: String): Out {
            val pb = ProcessBuilder(listOf(bin) + args).directory(home)
            pb.environment().putAll(mapOf(
                "HOME" to home.path, "XDG_CONFIG_HOME" to File(home, "config").path, "HITOP_KEYCHAIN" to "off",
                "HITOP_SYNC_KEY_FILE" to File(home, "sync-key.json").path, "HITOP_SYNC_SERVER" to server, "NO_COLOR" to "1",
            ))
            pb.environment().remove("HITOP_SYNC_RUNNER")
            val p = pb.start()
            var err = ""
            val t = Thread { err = p.errorStream.bufferedReader().readText() }.also { it.start() }
            val out = p.inputStream.bufferedReader().readText()
            t.join()
            return Out(p.waitFor(), out, err)
        }

        fun ok(vararg args: String): String {
            val r = run(*args)
            assertEquals("hitop ${args.joinToString(" ")}\n${r.out}\n${r.err}", 0, r.code)
            return r.out + r.err
        }
    }

    private fun requestFile(folder: File, name: String): File =
        File(folder, "requests").listFiles()!!.first { it.name.endsWith(".yaml") && it.readText().contains("name: $name\n") }

    private fun edit(f: File, from: String, to: String) {
        val t = f.readText()
        assertTrue(t, t.contains(from))
        f.writeText(t.replace(from, to))
    }

    /**
     * A shared collection between the phone's core (Ada) and the hitop CLI (Grace, HITOP_BIN):
     * share, invite with the fingerprint, accept, add, link, nothing to push on either side, a
     * hand-written request kept byte for byte, edits both ways, a conflict on each side, removal →
     * rotation → Grace's old key opens nothing written after it, and her CLI can't pull.
     */
    @Test
    fun aSharedCollectionWithTheCLI() = runBlocking {
        val bin = System.getenv("HITOP_BIN").orEmpty()
        if (bin.isEmpty()) {
            if (System.getenv("HITOP_E2E_REQUIRED") != null) throw AssertionError("HITOP_BIN (the hitop CLI) must be set")
            assumeTrue("HITOP_BIN not set", false)
        }
        val ada = Phone("HITOP on Pixel")
        val shared = tech.rws8.hitop.sync.SharedSync(ada.cloud, ada.library, tech.rws8.hitop.sync.MemorySharedSyncStore(), scope).also { it.pushDelayMs = 3_600_000 }
        val c = ada.library.addCollection("Shop")
        ada.library.newRequest(c.id, template = Request.new("List orders", "https://internal.example.test/orders"))
        ada.library.newRequest(c.id, template = Request.new("Create order", "https://internal.example.test/orders", "POST").withBody("{\"item\": 1}"))
        ada.signIn("ada@example.com")
        shared.share(c.id)
        val ws = shared.link(c.id)!!.workspace
        assertTrue(ada.library.isShared(c.id))

        val home = File(dir, "grace").also { it.mkdirs() }
        val grace = Cli(bin, home, server)
        val g = "grace@example.com"
        val before = log.toString().split("mail to $g:").size - 1
        grace.ok("sync", "login", "--email", g, "--code")
        val signedIn = grace.ok("sync", "login", "--email", g, "--code", mailedCode(g, before))
        val graceFp = signedIn.lines().mapNotNull { it.split("key fingerprint ").getOrNull(1) }.first().trim()

        val (code, _) = shared.invite(c.id, g, "editor")
        assertTrue(code.endsWith(tech.rws8.hitop.sync.SyncCore.normalize(shared.fingerprint!!)))
        val accepted = grace.ok("sync", "accept", code)
        assertTrue(accepted, accepted.contains(shared.fingerprint!!))
        val waiting = shared.members(c.id).invites.first { it.acceptedBy != null }.acceptedBy!!
        assertEquals(listOf(graceFp), waiting.keys.map { it.fingerprint })
        shared.trust(waiting.keys)
        val keys = shared.members(c.id).invites.first { it.acceptedBy != null }.acceptedBy!!.keys
        shared.addMember(c.id, g, "editor", keys)

        val folder = File(home, "shop")
        val linked = grace.ok("sync", "link", ws, "-c", folder.path)
        assertTrue(linked, linked.contains("trusted device"))
        assertTrue(File(folder, "hitop.yaml").readText().contains("Shop"))
        val listFile = requestFile(folder, "List orders")
        requestFile(folder, "Create order")

        // Nothing to push on either side: both write the folder the same way.
        assertEquals(emptyList<String>(), shared.syncNow(c.id).pushed)
        assertTrue(grace.ok("sync", "push", "-c", folder.path).contains("Up to date"))

        // A hand-written request (no id, her own file name): taken, nothing sent back, bytes kept.
        val hand = "name: Hand made\nmethod: GET\nurl: https://hand.example.test\n"
        val handFile = File(folder, "requests/hand.yaml").also { it.writeText(hand) }
        grace.ok("sync", "push", "-c", folder.path)
        shared.syncNow(c.id)
        assertTrue(ada.library.state.value.collection(c.id)!!.requests.any { it.name == "Hand made" })
        assertEquals(emptyList<String>(), shared.syncNow(c.id).pushed)
        grace.ok("sync", "pull", "-c", folder.path)
        assertEquals(hand, handFile.readText())

        // Edits both ways.
        edit(listFile, "url: https://internal.example.test/orders", "url: https://internal.example.test/v2/orders")
        assertTrue(grace.ok("sync", "push", "-c", folder.path).contains("pushed"))
        shared.syncNow(c.id)
        fun req(name: String) = ada.library.state.value.collection(c.id)!!.requests.first { it.name == name }
        assertEquals("https://internal.example.test/v2/orders", req("List orders").url)
        ada.library.saveRequest(c.id, req("Create order").withBody("{\"item\": 2}"))
        assertEquals(1, shared.syncNow(c.id).pushed.size)
        grace.ok("sync", "pull", "-c", folder.path)
        assertTrue(requestFile(folder, "Create order").readText().contains("item"))

        // Both change List orders, the CLI second: she takes theirs (the phone's).
        ada.library.saveRequest(c.id, req("List orders").withUrl("https://ada.example.test/orders"))
        shared.syncNow(c.id)
        edit(listFile, "url: https://internal.example.test/v2/orders", "url: https://grace.example.test/orders")
        val conflict = grace.run("sync", "push", "-c", folder.path)
        assertEquals(conflict.out + conflict.err, 1, conflict.code)
        assertTrue(conflict.out, conflict.out.contains("CONFLICT"))
        grace.ok("sync", "resolve", "requests/${listFile.name}", "--theirs", "-c", folder.path)
        assertTrue(listFile.readText().contains("https://ada.example.test/orders"))
        // The phone second, keeping mine.
        edit(listFile, "url: https://ada.example.test/orders", "url: https://grace2.example.test/orders")
        grace.ok("sync", "push", "-c", folder.path)
        ada.library.saveRequest(c.id, req("List orders").withUrl("https://ada2.example.test/orders"))
        assertEquals(1, shared.syncNow(c.id).conflicts.size)
        val k = shared.conflicts.value[c.id]!!.first()
        assertTrue(k.theirs!!.contains("grace2"))
        shared.resolve(c.id, k.path, tech.rws8.hitop.sync.SharedSync.Choice.MINE)
        grace.ok("sync", "pull", "-c", folder.path)
        assertTrue(listFile.readText().contains("https://ada2.example.test/orders"))

        // Grace's epoch-1 key as her device holds it, before she is removed.
        val secret = tech.rws8.hitop.core.HitopJson.parseToJsonElement(File(home, "sync-key.json").readText()).jsonObject
        val account = tech.rws8.hitop.core.HitopJson.parseToJsonElement(File(home, "config/hitop/sync/account.json").readText()).jsonObject
        val graceKey = account.str("keyId")
        val wraps = UrlConnectionTransport().send(CloudRequest("GET", "$server/v1/workspaces/$ws/wraps?keyId=$graceKey", mapOf("Authorization" to "Bearer ${secret.str("session")}", "X-Hitop-Device" to graceKey), null)).json!!
        val pin = tech.rws8.hitop.sync.SyncCore.publicKey(ada.cloud.deviceSecret()!!)
        val ring = tech.rws8.hitop.sync.SyncCore.acceptWraps(secret.str("secret"), graceKey, ws, 1,
            (wraps["wraps"] as JsonArray).map { it.jsonObject.let { w -> w.str("epoch").toInt() to w.str("wrapped") } }, mapOf(ada.cloud.account!!.keyId to pin), emptyMap(), JsonObject(emptyMap()))
        val old = ring.keys.getValue(1)

        // Removed: a new key, every file sealed again, her old key opens none of them.
        val graceId = shared.members(c.id).members.first { it.email == g }.userId
        assertEquals(2, shared.removeMember(c.id, graceId))
        ada.library.saveRequest(c.id, req("Create order").withBody("{\"after removal\": true}"))
        shared.syncNow(c.id)
        val changes = ada.cloud.call("GET", "/v1/workspaces/$ws/changes?since=0").json!!
        var opened = 0
        for (ch in (changes["changes"] as JsonArray).map { it.jsonObject }.filter { !it.containsKey("deleted") || it.str("deleted") != "true" }) {
            val b = ada.cloud.call("GET", "/v1/workspaces/$ws/blobs/${ch.str("id")}").json!!
            assertEquals(2, tech.rws8.hitop.sync.SyncCore.blobEpoch(b.str("content")))
            assertTrue(runCatching { tech.rws8.hitop.sync.SyncCore.open(old, ws, ch.str("id"), b.str("version").toLong(), b.str("content")) }.isFailure)
            opened++
        }
        assertTrue(opened >= 3)
        val refused = grace.run("sync", "pull", "-c", folder.path)
        assertTrue(refused.out + refused.err, refused.code != 0)
        assertFalse(requestFile(folder, "Create order").readText().contains("after removal"))
    }

    @Test
    fun theAppsCoreAgainstARealSyncServer() = runBlocking {
        val email = "android-e2e@example.com"
        val methods = Phone("probe").cloud.authMethods(server)
        assertEquals(emptyList<String>(), methods!!.providers)
        assertFalse(methods.googleNative)
        val probe = Phone("probe")
        val e = runCatching { probe.cloud.googleNonce(server) }.exceptionOrNull() as CloudException
        assertEquals("google_native_off", e.code)

        // Phone one: a collection and an environment with a secret, then signed in: pushed.
        val one = Phone("HITOP on Pixel")
        val shop = one.library.addCollection("shop-api")
        one.library.newRequest(shop.id, template = Request.new("List orders", "\${baseUrl}/v2/orders"))
        val env = one.library.addEnvironment("Staging")
        one.library.setVariable(env.id, "baseUrl", "https://staging.shop.example", secret = false)
        one.library.setVariable(env.id, "token", "tok_e2e_secret", secret = true)
        one.signIn(email)
        one.sync.flush()
        assertNull(one.sync.status.value.error)
        assertEquals(LibrarySync.Badge.SYNCED, one.sync.badges.value[shop.id])
        val listed = one.cloud.call("GET", "/v1/library").json!!
        val items = (listed["items"] as JsonArray).map { it.jsonObject }
        assertTrue(items.any { it.str("kind") == "collection" && (it["data"] as JsonObject).str("name") == "shop-api" })
        // The secret value went apart from the data (sealed by the server's host key).
        val envItem = items.first { it.str("kind") == "environments" }
        assertFalse(envItem["data"].toString().contains("tok_e2e_secret"))

        // Phone two: the same account brings everything, the secret included.
        val two = Phone("HITOP on Tablet")
        two.signIn(email)
        val shop2 = two.library.state.value.collections.first { it.name == "shop-api" }
        assertEquals("List orders", shop2.requests.single().name)
        val env2 = two.library.state.value.environments.first { it.name == "Staging" }
        assertEquals("tok_e2e_secret", two.library.secretValue(env2.id, "token"))
        assertEquals("", env2.variables["token"])

        // Both change the collection: the second write keeps the server's and a copy of its own.
        one.library.renameCollection(shop.id, "shop-api v2")
        one.sync.flush()
        two.library.renameCollection(shop2.id, "shop-api (tablet edit)")
        two.sync.flush()
        val names = two.library.state.value.collections.map { it.name }
        assertTrue(names.toString(), "shop-api v2" in names && "shop-api (tablet edit) (this phone)" in names)
        assertTrue(two.sync.notices.value.first().contains("changed on another device"))

        // A delete leaves a tombstone: the other phone drops the collection on its next pull.
        two.library.deleteCollection(shop2.id)
        two.sync.flush()
        one.sync.pull()
        assertNull(one.library.state.value.collection(shop.id))

        // Devices: both, this one marked.
        val devices = one.cloud.devices()!!
        assertEquals(setOf("HITOP on Pixel", "HITOP on Tablet"), devices.map { it.name }.toSet())
        assertTrue(devices.first { it.name == "HITOP on Pixel" }.thisDevice)

        // Signing out and in again is the same device (no new key).
        val keyId = one.cloud.account!!.keyId
        one.cloud.signOut()
        assertFalse(one.cloud.signedIn)
        one.signIn(email)
        assertEquals(keyId, one.cloud.account!!.keyId)
        assertEquals(2, one.cloud.devices()!!.size)

        // Organizations and usage: the personal one, neutral words only.
        val orgs = one.cloud.loadOrgs()
        val personal = orgs.first { it.personal }
        val usage = one.cloud.usage(personal.id)
        assertNotNull(usage.planName)
        val words = usage.meters.joinToString { it.label + it.text + it.note }
        listOf("Upgrade", "Buy", "price", "€", "$").forEach { assertFalse(words, words.contains(it)) }

        // Deleting the account: everything about it is forgotten here, the collections stay.
        val kept = one.library.state.value.collections.size
        one.cloud.deleteAccount()
        assertFalse(one.cloud.signedIn)
        assertNull(one.cloud.settings.value.device)
        assertEquals(kept, one.library.state.value.collections.size)
        val gone = runCatching { two.cloud.call("GET", "/v1/me") }.exceptionOrNull() as CloudException
        assertEquals(401, gone.status)
    }
}
