package tech.rws8.hitop.core

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files

class LibraryTest {
    @Test
    fun createOrderSendsThroughTheEngineAndPassesItsTests() = runBlocking {
        val (lib, store, _) = TestStub.library()
        val (c, r) = lib.state.value.find("1104")!!
        val result = lib.send(c.id, r)
        assertNull(result.error?.raw, result.error)
        assertEquals(201, result.status)
        assertEquals(listOf("Status is 201", "Total equals sum of items", "Responds under 1 s", "Has a tracking link"), result.tests.map { it.name })
        assertTrue(result.tests.joinToString { "${it.name}: ${it.detail}" }, result.tests.all { it.pass })
        assertEquals("http://staging.shop.example/v2/orders?expand=items", result.url)
        // The bearer token went out; nothing the app keeps or shows has it.
        val seen = TestStub.server.seen.last { it.url.endsWith("/v2/orders?expand=items") }
        assertEquals("Bearer tok_staging_8f2", seen.header("Authorization"))
        assertTrue(seen.body.contains("\"customerId\": 42"))
        assertTrue(result.sentHeaders.any { it.first.equals("authorization", true) && it.second == "Bearer ••••" })
        assertEquals("session", result.cookies.single().name)
        assertEquals(201, lib.state.value.history.first().response!!.status)
        val files = store.root.walkTopDown().filter { it.isFile }.joinToString("\n") { it.readText() }
        assertFalse(files.contains("tok_staging_8f2"))
        assertFalse(files.contains("hunter2"))
    }

    @Test
    fun scriptValuesCarryToTheNextSend() = runBlocking {
        val (lib, _, _) = TestStub.library()
        val (c, r) = lib.state.value.find("1103")!!
        val withScript = r.withPost("pm.environment.set('lastOrder', pm.response.json()[0].id)")
        lib.send(c.id, withScript)
        assertEquals("ord_8KQ2mZ41", lib.scriptValues(c.id)["lastOrder"])
        val next = Request.new(url = "\${baseUrl}/echo?o=\${lastOrder}")
        val result = lib.send(c.id, next)
        assertEquals(200, result.status)
        assertTrue(result.url, result.url.endsWith("o=ord_8KQ2mZ41"))
        assertEquals(Scope.SCRIPT, lib.peek(c.id, "lastOrder").scope)
    }

    @Test
    fun noEnvironmentIsAnExplainedError() = runBlocking {
        val (lib, _, _) = TestStub.library()
        lib.setActiveEnvironment(null)
        val (c, r) = lib.state.value.find("1103")!!
        val result = lib.send(c.id, r)
        assertNull(result.status)
        assertEquals("\${baseUrl} has no value", result.error!!.headline)
    }

    @Test
    fun cancelOrderIs409AndProductionIsAnotherHost() = runBlocking {
        val (lib, _, _) = TestStub.library()
        lib.setActiveEnvironment("env-production")
        val (c, r) = lib.state.value.find("1108")!!
        val result = lib.send(c.id, r)
        assertEquals(409, result.status)
        assertTrue(result.url.startsWith("http://api.shop.example/"))
    }

    @Test
    fun newRequestsAreNamedFromTheirUrlUntilSaved() {
        val (lib, _, _) = TestStub.library(seed = false)
        lib.seedFirstVisit()
        assertEquals(listOf(AutoName.SAMPLE_NAME, AutoName.MY_REQUESTS), lib.state.value.collections.map { it.name })
        val (c, r) = lib.newRequest()
        assertEquals(AutoName.MY_REQUESTS, c.name)
        val saved = lib.saveRequest(c.id, r.withUrl("https://api.example.com/users").withMethod("POST"))
        assertEquals("POST /users", saved.name)
        lib.renameRequest(saved.id, "Create user")
        assertEquals("Create user", lib.state.value.find(saved.id)!!.second.name)
    }

    @Test
    fun editsPersistAndKeepWhatTheAppDoesNotModel() {
        val dir = Files.createTempDirectory("lib").toFile()
        val store = LibraryStore(dir)
        val lib = Library(store, MemorySecretStore())
        lib.load()
        lib.seedShopApi()
        val (c, r) = lib.state.value.find("1110")!!
        lib.saveRequest(c.id, r.withName("Catalog"))
        lib.togglePin("1110")
        lib.addFolder(c.id, "admin")
        lib.moveRequest("1113", c.id, "admin")
        val again = Library(store, MemorySecretStore()).also { it.load() }
        val (c2, r2) = again.state.value.find("1110")!!
        assertEquals("Catalog", r2.name)
        assertEquals("catalog/catalog", r2.file)
        assertEquals("{ \"first\": 2 }", r2.graphqlVariables)
        assertTrue("1110" in again.state.value.pins)
        assertTrue("admin" in c2.allFolders)
        assertEquals("admin", c2.request("1113")!!.folder)
    }

    @Test
    fun secretsLiveInTheSecretStoreOnly() {
        val (lib, store, secrets) = TestStub.library()
        lib.setVariable("env-local", "apiKey", "k-123", secret = true)
        val e = lib.state.value.environments.first { it.id == "env-local" }
        assertEquals("", e.variables["apiKey"])
        assertTrue("apiKey" in e.secrets)
        assertEquals("k-123", secrets.get(SecretStore.envKey("env-local", "apiKey")))
        assertFalse(store.file("environments.json").readText().contains("k-123"))
        lib.setVariable("env-local", "apiKey", "plain", secret = false)
        assertNull(secrets.get(SecretStore.envKey("env-local", "apiKey")))
        val p = lib.peek("1001", "token")
        assertTrue(p.secret)
        assertEquals("tok_staging_8f2", p.value)
    }

    @Test
    fun historyOpensAsANewRequestInMyRequests() {
        val (lib, _, _) = TestStub.library()
        val h = lib.state.value.history.first()
        val (c, r) = lib.openHistory(h.id)!!
        assertEquals(AutoName.MY_REQUESTS, c.name)
        assertEquals(h.request.url, r.url)
        assertNotNull(lib.state.value.find(r.id))
    }
}
