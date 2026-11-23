package tech.rws8.hitop.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ProtocolsTest {
    @Test
    fun sseEventsSplitAnywhere() {
        val p = SseParser()
        val text = ": comment\nevent: order.created\nid: 1\ndata: {\"a\":1}\n\nevent: tick\ndata: one\ndata: two\n\ndata: plain\n\n"
        val out = text.chunked(3).flatMap { p.feed(it) }
        assertEquals(listOf(SseEvent("order.created", "1", "{\"a\":1}"), SseEvent("tick", "1", "one\ntwo"), SseEvent("message", "1", "plain")), out)
    }

    @Test
    fun theMethodMenuSwitchesKinds() {
        val r = Request.new(url = "https://x.test/a").withHeaders(listOf(Header("Accept", "application/json")))
        val sse = Modes.set(r, "SSE")
        assertEquals("SSE", Modes.of(sse))
        assertEquals(listOf(Header("Accept", "text/event-stream")), sse.headers)
        assertEquals("GET", Modes.of(Modes.set(sse, "GET")))
        assertTrue(Modes.set(sse, "GET").headers.isEmpty())
        assertEquals("WebSocket", Modes.of(Modes.set(r, "WebSocket")))
        val g = Modes.set(r, "gRPC").withGrpc(GrpcFields("a.B", "C", "{}", true))
        assertEquals("gRPC", Modes.of(g))
        assertEquals(GrpcFields("a.B", "C", "{}", true), g.grpcFields)
        assertEquals("GET", Modes.of(Modes.set(g, "GET")))
    }

    @Test
    fun websocketShapesKeepUnknownKeys() {
        val r = Request(HitopJson.parseToJsonElement("""{"id":1,"name":"w","url":"wss://x","method":"GET","headers":[],"body":"","kind":"websocket","websocket":{"subprotocols":["a"],"messages":[{"name":"m","content":"c","color":"red"}],"autoConnect":true}}""") as kotlinx.serialization.json.JsonObject)
        val edited = r.withWebSocket(messages = r.wsMessages + WsMessage("n", "d"))
        val text = edited.json.toString()
        assertTrue(text, text.contains("\"autoConnect\":true") && text.contains("\"color\":\"red\"") && text.contains("\"name\":\"n\""))
    }

    private val schemaJson = """{"data":{"__schema":{"queryType":{"name":"Query"},"mutationType":{"name":"Mutation"},"subscriptionType":null,"types":[
        {"kind":"OBJECT","name":"Query","fields":[{"name":"products","args":[{"name":"first","type":{"kind":"SCALAR","name":"Int"}}],"type":{"kind":"NON_NULL","ofType":{"kind":"LIST","ofType":{"kind":"OBJECT","name":"Product"}}}},{"name":"order","args":[],"type":{"kind":"OBJECT","name":"Order"}}]},
        {"kind":"OBJECT","name":"Product","fields":[{"name":"sku","args":[],"type":{"kind":"SCALAR","name":"String"}},{"name":"stock","args":[],"type":{"kind":"SCALAR","name":"Int"}},{"name":"supplier","args":[],"type":{"kind":"OBJECT","name":"Supplier"}}]},
        {"kind":"OBJECT","name":"Supplier","fields":[{"name":"name","args":[],"type":{"kind":"SCALAR","name":"String"}}]},
        {"kind":"OBJECT","name":"Order","fields":[{"name":"id","args":[],"type":{"kind":"SCALAR","name":"ID"}}]},
        {"kind":"OBJECT","name":"Mutation","fields":[{"name":"cancel","args":[],"type":{"kind":"OBJECT","name":"Order"}}]}]}}}"""

    @Test
    fun suggestionsFollowTheCursor() {
        val s = GraphQLSchema.parse(schemaJson)
        assertEquals("[Product]!", s.types["Query"]!!.fields.first().type)
        fun at(text: String) = s.suggestions(text, text.length).map { it.name }
        assertEquals(listOf("order", "products"), at("query Catalog(\$first: Int) {\n  "))
        assertEquals(listOf("sku", "stock", "supplier"), at("{ products(first: 2) { "))
        assertEquals(listOf("stock"), at("{ products(first: 2) { st"))
        assertEquals(listOf("name"), at("{ products { supplier { "))
        assertEquals(listOf("cancel"), at("mutation { "))
        assertEquals(listOf("sku", "stock", "supplier"), at("{ products { supplier { name } "))
        assertEquals(listOf("id"), at("fragment F on Order { "))
        assertEquals(emptyList<String>(), at("{ products { \"str { "))
    }

    @Test
    fun schemaErrorsInWords() {
        val e = runCatching { GraphQLSchema.parse("""{"data":{}}""") }.exceptionOrNull()!!
        assertTrue(e.message!!.contains("introspection turned off"))
        assertEquals("not allowed", runCatching { GraphQLSchema.parse("""{"errors":[{"message":"not allowed"}]}""") }.exceptionOrNull()!!.message)
    }
}
