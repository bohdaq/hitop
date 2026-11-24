package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ModelsTest {
    private val webCollection = """
        {"id":18446744073709551615,"name":"web","order":3,"variables":{"n":1,"s":"x"},
         "auth":{"type":"oauth2","grant":"client_credentials","tokenUrl":"https://t.example/token"},
         "requests":[{"id":"req-legacy","name":"Get","url":"https://a.example/x","method":"get",
           "headers":[{"key":"Accept","value":"*/*"},{"name":"","value":""}],"body":"","preRequestScript":"","postRequestScript":"",
           "examples":[{"name":"ok","status":200}],"openapi":{"operationId":"getX"},"settings":{"timeoutMs":500},
           "graphql":{"variables":{"first":2}},"weird":[1,2,3]}],
         "comments":{"kept":true}}
    """.trimIndent()

    @Test
    fun unknownKeysAndExactIdsSurviveAnEdit() {
        val c = Collection(HitopJson.parseToJsonElement(webCollection).jsonObject)
        assertEquals("18446744073709551615", c.id)
        val r = c.requests.single()
        assertEquals("req-legacy", r.id)
        assertEquals("GET", r.method)
        assertEquals(listOf(Header("Accept", "*/*")), r.headers)
        assertTrue(r.graphqlVariables.contains("\"first\": 2"))
        val edited = c.replacing(r.withUrl("https://a.example/y")).withName("web 2")
        val text = edited.json.toString()
        listOf("\"order\":3", "\"oauth2\"", "\"examples\"", "\"openapi\"", "\"settings\"", "\"weird\":[1,2,3]", "\"comments\"", "18446744073709551615").forEach {
            assertTrue("$it kept in $text", text.contains(it))
        }
        assertEquals("1", edited.variables["n"])
        assertTrue(Auth(c.json.obj("auth")!!).editable)
        assertFalse(Auth.of("hawk").editable)
    }

    @Test
    fun theEngineSeesTypedFields() {
        val c = Collection(HitopJson.parseToJsonElement(webCollection).jsonObject)
        val e = c.forEngine()
        val r = (e["requests"] as kotlinx.serialization.json.JsonArray)[0] as JsonObject
        assertEquals("GET", r.str("method"))
        assertEquals("[{\"name\":\"Accept\",\"value\":\"*/*\",\"enabled\":true}]", r["headers"].toString())
        assertEquals("1", (e["variables"] as JsonObject).str("n"))
    }

    @Test
    fun bodyKindsAsTheWebAppDecides() {
        val r = Request.new()
        assertEquals(BodyKind.NONE, r.bodyKind)
        assertEquals(BodyKind.JSON, r.withBody("{\"a\":1}").bodyKind)
        assertEquals(BodyKind.TEXT, r.withBody("hello").bodyKind)
        assertEquals(BodyKind.FORM, r.withBodyKind(BodyKind.FORM).bodyKind)
        assertEquals(BodyKind.GRAPHQL, r.withBody("{ a }").withBodyKind(BodyKind.GRAPHQL).bodyKind)
        assertEquals("GQL", r.withBodyKind(BodyKind.GRAPHQL).tag)
        assertEquals("SSE", r.withHeaders(listOf(Header("Accept", "text/event-stream"))).tag)
    }

    @Test
    fun autoNames() {
        assertEquals("GET /users", AutoName.of(Request.new(url = "\${baseUrl}/users?x=1")))
        assertEquals("POST api.example.com", AutoName.of(Request.new(url = "https://api.example.com/", method = "POST")))
        assertEquals("Untitled", AutoName.of(Request.new()))
    }

    @Test
    fun variableSegments() {
        val s = Variables.segments("\${baseUrl}/v2/orders/\${ orderId }?x=1")
        assertEquals(listOf("baseUrl", null, "orderId", null), s.map { it.variable })
        assertEquals(listOf("baseUrl", "orderId"), Variables.names("\${baseUrl}/\${orderId}/\${baseUrl}"))
    }

    @Test
    fun errorsInWords() {
        assertEquals("\${token} has no value", Errors.explain("variable 'token' is not defined").headline)
        assertTrue(Errors.explain("error sending request: Connection refused (os error 111)").headline.contains("refused"))
        assertNull(null)
    }
}
