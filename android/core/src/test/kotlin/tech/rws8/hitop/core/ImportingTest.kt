package tech.rws8.hitop.core

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class ImportingTest {
    private fun fixture(p: String) = File("../../cli/fixtures/$p").readBytes()

    private fun zip(files: Map<String, ByteArray>): ByteArray = ByteArrayOutputStream().also { o -> ZipOutputStream(o).use { z -> files.forEach { (n, b) -> z.putNextEntry(ZipEntry(n)); z.write(b); z.closeEntry() } } }.toByteArray()

    @Test
    fun postmanCollectionAndEnvironment() {
        val (lib, _, secrets) = TestStub.library(seed = false)
        val plan = Importing.read(fixture("launch-gate/shop.postman_collection.json"))
        assertEquals("postman", plan.kind)
        val id = lib.applyImport(plan).single()
        assertTrue(lib.state.value.collection(id)!!.requests.isNotEmpty())
        lib.applyImport(Importing.read("""{"name":"S","values":[{"key":"a","value":"1"},{"key":"t","value":"s3","type":"secret"}]}"""))
        val e = lib.state.value.environments.single()
        assertEquals(mapOf("a" to "1", "t" to ""), e.variables)
        assertEquals("s3", secrets.get(SecretStore.envKey(e.id, "t")))
    }

    @Test
    fun anExportImportedAgainIsAnotherCollectionWithItsOwnIds() {
        val (lib, _, _) = TestStub.library()
        val c = lib.state.value.collections.first { it.name == "shop-api" }
        val ids = lib.state.value.collections.flatMap { it.requests }.map { it.id }.toSet()
        for (bytes in listOf(Importing.exportJson(c, "hitop").toByteArray(), Importing.exportZip(c))) {
            val newId = lib.applyImport(Importing.read(bytes, "shop.zip")).single()
            val added = lib.state.value.collection(newId)!!
            assertEquals(c.requests.map { it.name }.sorted(), added.requests.map { it.name }.sorted())
            assertEquals(c.name, added.name)
            assertTrue(added.requests.none { it.id in ids })
        }
        assertEquals(1, lib.state.value.collections.count { it.id == c.id })
        assertEquals("postman", Importing.read(Importing.exportJson(c, "postman")).kind)
    }

    @Test
    fun zipsAreBoundedAndMustBeCollectionFolders() {
        val one = ByteArray(1024 * 1024)
        assertTrue(runCatching { Importing.unzip(zip((0..30).associate { "f$it" to one })) }.exceptionOrNull()!!.message!!.contains("too large"))
        assertTrue(runCatching { Importing.read(zip(mapOf("a.txt" to "x".toByteArray()))) }.exceptionOrNull()!!.message!!.contains("hitop.yaml"))
        val c = Collection.new("Nested", listOf(Request.new("One", "https://a.test")))
        val nested = Importing.unzip(Importing.exportZip(c)).mapKeys { "top/${it.key}" }.mapValues { it.value.toByteArray() }
        assertEquals("Nested", Importing.read(zip(nested)).collections.single().name)
        assertTrue(runCatching { Importing.read("hello") }.isFailure)
    }

    @Test
    fun specSyncKeepsIdsEditsAndUnknownKeys() {
        val spec = "openapi: 3.0.0\ninfo: {title: P}\nservers: [{url: 'https://x.test/v1'}]\npaths:\n  /a: {get: {operationId: a, summary: First, responses: {}}}\n  /c: {get: {operationId: c, responses: {}}}\n"
        val (lib, _, _) = TestStub.library(seed = false)
        val id = lib.applyImport(Importing.read(spec, "https://x.test/spec.yaml")).single()
        var c = lib.state.value.collection(id)!!
        assertEquals("https://x.test/spec.yaml", Importing.specSource(c))
        // An old web app id (text) and a key the app doesn't know.
        val first = c.requests.first { it.url.endsWith("/a") }
        c = c.withRequests(c.requests.map { if (it.id == first.id) it.set("id", JsonPrimitive("req-old")).set("mine", JsonPrimitive("kept")).withPost("pm.test('x', () => {})") else it })
        val next = spec.replace("summary: First", "summary: Renamed").replace("/c: {get: {operationId: c", "/d: {get: {operationId: d")
        val s = Importing.syncSpec(c, next, prune = false)
        assertEquals(1, s.added.size)
        assertEquals(1, s.removed.size)
        val a = s.collection.requests.first { it.url.endsWith("/a") }
        assertEquals("req-old", a.id)
        assertEquals("kept", a.json.str("mine"))
        assertTrue(a.postRequestScript.contains("pm.test"))
        assertEquals(id, s.collection.id)
        assertEquals(3, s.collection.requests.size)
        assertEquals(2, Importing.syncSpec(c, next, prune = true).collection.requests.size)
        assertTrue(Importing.syncSpec(s.collection, next, prune = false).added.isEmpty())
    }

    @Test
    fun aPickedFileIsSentAndStaysOnThisPhone() = runBlocking {
        val (lib, store, _) = TestStub.library()
        val c = lib.state.value.collections.first { it.name == "shop-api" }
        val r0 = Request.new("Upload", "\${baseUrl}/upload", "POST").withBodyKind(BodyKind.MULTIPART)
        val extra = lib.addUpload(r0.id, "a \"b\".txt", "text/plain", "line one\r\nüñí".byteInputStream())
        val r = r0.withFormFields(listOf(FormRow("doc", "", "file", true, extra), FormRow("note", "hi")))
        lib.saveRequest(c.id, r)
        assertEquals(200, lib.send(c.id, r).status)
        val body = TestStub.server.seen.last { it.url.endsWith("/upload") }.body
        assertTrue(body, body.contains("name=\"doc\"; filename=\"a %22b%22.txt\"\r\nContent-Type: text/plain\r\n\r\nline one\r\nüñí\r\n"))
        // Another request naming the same file gets nothing.
        val other = r.set("id", JsonPrimitive(Ids.next()))
        assertTrue(lib.send(c.id, other).error!!.raw.contains("isn't on this phone"))
        assertFalse(Importing.exportJson(lib.state.value.collection(c.id)!!, "hitop").contains("fileToken"))
        lib.pruneUploads()
        assertTrue(lib.hasUpload(r.id, extra.str("fileToken")))
        lib.deleteRequest(r.id)
        lib.pruneUploads()
        assertFalse(store.file("uploads/${extra.str("fileToken")}").exists())
        assertNull(runCatching { lib.addUpload("1", "big", "", ByteArray(20).inputStream(), max = 10) }.getOrNull())
    }
}
