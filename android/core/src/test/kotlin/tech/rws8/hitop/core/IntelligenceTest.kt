package tech.rws8.hitop.core

import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** A6: what the model may see, suggested checks validated and run, requests from text and curl. */
class IntelligenceTest {
    @Test
    fun noSecretOrCredentialReachesTheModel() = runBlocking {
        val (lib, _, secrets) = TestStub.library()
        val (c, r) = lib.state.value.find("1101")!! // Log in: password in the body, bearer token
        val res = lib.send(c.id, r)
        val values = secrets.all().values
        val input = ModelInput.exchange(r, c.auth, res, values)
        listOf("tok_staging_8f2", "hunter2", "tok_live_5Yq").forEach { assertFalse("$it in\n$input", input.contains(it)) }
        assertTrue(input, input.contains("authorization: Bearer ••••") || input.contains("Authorization: Bearer ••••"))
        assertTrue(input, input.contains("\"password\": \"••••\""))
        assertEquals("https://x.test/a?api_key=••••&page=2", ModelInput.url("https://x.test/a?api_key=k-1&page=2", emptyList()))
        assertEquals("••••", ModelInput.header("Cookie", "session=s1", emptyList()))
    }

    @Test
    fun bodiesAreCutAndShaped() {
        val big = "{\"items\":[" + (1..50).joinToString(",") { "{\"id\":$it,\"name\":\"${"x".repeat(300)}\"}" } + "]}"
        val b = ModelInput.body(big, ModelInput.RESPONSE_CHARS, emptyList())
        assertTrue(b.length < ModelInput.RESPONSE_CHARS + 60)
        assertTrue(b, b.contains("… 47 more"))
        assertEquals("{items: [object], total: {amount: number}}", ModelInput.shape("{\"items\":[{\"id\":1}],\"total\":{\"amount\":1}}"))
    }

    @Test
    fun suggestedChecksAreValidatedAndPass() = runBlocking {
        val (lib, _, _) = TestStub.library()
        val (c, r) = lib.state.value.find("1104")!!
        val res = lib.send(c.id, r)
        val checks = TestScript.validate(TestScript.parse("Here you go: " + FakeAssistant().answer(Prompts.suggestTests("x"))), res)
        assertEquals(listOf("Status is 201", "Content-Type contains json", "total.amount equals 6480", "links.track is present", "items is a array"), checks.map { it.title })
        val script = TestScript.render(checks)
        val again = lib.send(c.id, r.withPost(script))
        assertEquals(5, again.tests.size)
        assertTrue(again.tests.joinToString { "${it.name}: ${it.detail}" }, again.tests.all { it.pass })
    }

    @Test
    fun theFakeAssistantStreams() = runBlocking {
        val text = FakeAssistant().stream(Prompts.explain("Response: 201 Created")).toList().joinToString("")
        assertTrue(text.contains("201"))
    }

    @Test
    fun requestsFromText() {
        val r = RequestDraft.parse("Sure! {\"name\":\"Create user\",\"method\":\"post\",\"url\":\"https://api.example.test/users\",\"headers\":[{\"name\":\"Content-Type\",\"value\":\"application/json\"},{\"name\":\"bad header\",\"value\":\"x\"}],\"body\":\"{\\\"name\\\":\\\"Ada\\\"}\"}")!!
        assertEquals("POST", r.method)
        assertEquals(listOf("Content-Type"), r.headers.map { it.name })
        assertEquals("{\n  \"name\": \"Ada\"\n}", r.body)
    }

    /** Every curl snippet hitop-core's goldens have reads back as the request it came from. */
    @Test
    fun curlFromTheSnippetGoldens() {
        val dir = File(System.getenv("HITOP_REPO") ?: "../..", "cli/crates/hitop-core/tests/golden/snippets")
        val cases = (HitopJson.parseToJsonElement(File(dir, "cases.json").readText()) as JsonArray).map { it.jsonObject }
        assertTrue(cases.size >= 7)
        for (case in cases) {
            val name = case.str("name")
            val parsed = CurlImport.parse(File(dir, "$name/curl.sh").readText())
            assertNotNull(name, parsed)
            val r = parsed!!.request
            assertEquals(name, case.obj("request")!!.str("method").uppercase(), r.method)
        }
        val post = CurlImport.parse(File(dir, "post-json-basic/curl.sh").readText())!!.request
        assertEquals("ada", post.auth!!.field("username"))
        assertEquals("p'w\"d", post.auth!!.field("password"))
        assertTrue(post.body, post.body.contains("\"city\": \"Kraków\"") && post.body.contains("it's \\\"quoted\\\""))
        val multipart = CurlImport.parse(File(dir, "multipart-apikey-header/curl.sh").readText())!!.request
        assertEquals(listOf("title" to "text", "avatar" to "file"), multipart.formFields.map { it.key to it.type })
        assertEquals("@not-a-file", multipart.formFields.first().value)
        val form = CurlImport.parse(File(dir, "urlencoded-apikey-query/curl.sh").readText())!!.request
        assertEquals(listOf("name" to "Ada Lovelace", "note" to "a&b=c/é"), form.formFields.map { it.key to it.value })
        val smart = CurlImport.parse("curl –X POST “https://x.test/a” -H ‘Accept: */*’ -k")!!
        assertEquals("https://x.test/a", smart.request.url)
        assertEquals(listOf("-k"), smart.ignored)
    }
}
