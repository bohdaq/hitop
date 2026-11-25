package tech.rws8.hitop.core

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test

/**
 * A5 end to end: WebSocket, Server-Sent Events and gRPC through hitop-ffi against
 * `cargo build -p hitop --example test_servers` (HITOP_TEST_SERVERS_BIN; skipped without it unless
 * HITOP_E2E_REQUIRED): the CLI's ws echo server, hitop-core's Greeter (reflection v1, v1alpha,
 * none) and an HTTP server with event streams.
 */
class ProtocolEndToEndTest {
    private var process: Process? = null
    private lateinit var servers: kotlinx.serialization.json.JsonObject
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @Before
    fun start() {
        val bin = System.getenv("HITOP_TEST_SERVERS_BIN").orEmpty()
        if (bin.isEmpty()) {
            if (System.getenv("HITOP_E2E_REQUIRED") != null) throw AssertionError("HITOP_TEST_SERVERS_BIN must be set")
            assumeTrue("HITOP_TEST_SERVERS_BIN not set", false)
        }
        val p = ProcessBuilder(bin).redirectError(ProcessBuilder.Redirect.DISCARD).start()
        process = p
        servers = HitopJson.parseToJsonElement(p.inputStream.bufferedReader().readLine()).jsonObject
    }

    @After
    fun stop() {
        process?.destroy()
    }

    private fun library(requests: List<Request>): Pair<Library, String> {
        val (lib, _, _) = TestStub.library(seed = false)
        val c = lib.addCollection("realtime")
        requests.forEach { lib.newRequest(c.id, template = it) }
        return lib to c.id
    }

    private suspend fun until(what: String, f: () -> Boolean) = withTimeout(15_000) { while (!f()) delay(50) }.also { assertTrue(what, f()) }

    @Test
    fun webSocketEchoesTextAndBinaryAndCloses() = runBlocking {
        val ws = Request.new("Echo", "ws://${servers.str("ws")}/?token=\${token}").set("kind", kotlinx.serialization.json.JsonPrimitive("websocket"))
            .withWebSocket(listOf("echo.v1"), listOf(WsMessage("hello", "hi \${who}")))
            .withHeaders(listOf(Header("X-Client", "hitop-android")))
        val (lib, cid) = library(listOf(ws))
        lib.state.value.collection(cid)!!.let { lib.applyRemote(it.withVariables(mapOf("who" to "Ada", "token" to "t1"))) }
        val r = lib.state.value.collection(cid)!!.requests.single()
        val s = WebSocketSession(lib, scope)
        s.connect(cid, r)
        until("open") { s.status.value == WebSocketSession.Status.OPEN }
        s.sendSaved("hello")
        until("echo") { s.log.value.any { it.kind == LogLine.Kind.RECEIVED && it.text == "hi Ada" } }
        s.send("bin")
        until("binary") { s.log.value.any { it.kind == LogLine.Kind.RECEIVED && it.text.startsWith("binary 2 bytes") } }
        s.send("close")
        until("closed 4001") { s.status.value == WebSocketSession.Status.CLOSED && s.log.value.any { it.text == "Closed 4001 bye" } }
    }

    @Test
    fun aRefusedConnectionIsAnError() = runBlocking {
        val (lib, cid) = library(listOf(Request.new("Nowhere", "ws://127.0.0.1:9/").set("kind", kotlinx.serialization.json.JsonPrimitive("websocket"))))
        val s = WebSocketSession(lib, scope)
        s.connect(cid, lib.state.value.collection(cid)!!.requests.single())
        until("closed with an error") { s.status.value == WebSocketSession.Status.CLOSED && s.log.value.any { it.kind == LogLine.Kind.ERROR } }
    }

    @Test
    fun serverSentEventsLiveAndStop() = runBlocking {
        val http = servers.str("http")
        val (lib, cid) = library(listOf(
            Modes.set(Request.new("Events", "http://$http/events"), "SSE"),
            Modes.set(Request.new("Live", "http://$http/live"), "SSE"),
        ))
        val (events, live) = lib.state.value.collection(cid)!!.requests
        val s = SseSession(lib, scope)
        s.start(cid, events)
        until("three events, ended") { !s.state.value.running && s.state.value.events.size == 3 }
        assertEquals(200, s.state.value.status)
        val l = SseSession(lib, scope)
        l.start(cid, live)
        until("live events") { l.state.value.events.size >= 2 }
        l.stop()
        until("stopped") { !l.state.value.running && l.state.value.stopped }
    }

    @Test
    fun grpcReflectionUnaryStreamingAndNoReflection() = runBlocking {
        fun call(address: String, method: String, message: String) = Request.new(method, address).set("kind", kotlinx.serialization.json.JsonPrimitive("grpc"))
            .withGrpc(GrpcFields("hitop.test.Greeter", method, message, plaintext = true)).withHeaders(listOf(Header("x-tenant", "\${tenant}")))
        val (lib, cid) = library(listOf(
            call(servers.str("grpc"), "SayHello", "{\"name\": \"\${who}\"}"),
            call(servers.str("grpc"), "StreamHellos", "{\"name\": \"Ada\", \"times\": 3}"),
            call(servers.str("grpcV1alpha"), "SayHello", "{\"name\": \"Ada\"}"),
            call(servers.str("grpcNone"), "SayHello", "{\"name\": \"Ada\"}"),
            call(servers.str("grpc"), "Chat", "{\"name\": \"\${who}\"}"),
        ))
        lib.state.value.collection(cid)!!.let { lib.applyRemote(it.withVariables(mapOf("who" to "Grace", "tenant" to "acme"))) }
        val (hello, stream, alpha, none) = lib.state.value.collection(cid)!!.requests
        val g = GrpcSession(lib, scope)
        g.load(cid, hello)
        assertTrue(g.services.value.toString(), "hitop.test.Greeter" in g.services.value)
        assertEquals("reflection v1", g.source.value)
        assertTrue(g.template(cid, hello, "hitop.test.Greeter.SayHello")!!.contains("name"))
        g.start(cid, hello)
        until("unary OK") { !g.call.value.running && g.call.value.status == "OK" }
        assertTrue(g.call.value.messages.single().contains("Grace"))
        val st = GrpcSession(lib, scope)
        st.start(cid, stream)
        until("three messages") { !st.call.value.running && st.call.value.messages.size == 3 }
        val a = GrpcSession(lib, scope)
        a.load(cid, alpha)
        assertEquals("reflection v1alpha", a.source.value)
        val n = GrpcSession(lib, scope)
        n.load(cid, none)
        assertTrue(n.schemaError.value!!, n.schemaError.value!!.contains("reflection"))
        // One at a time: each answer before the next message.
        val chat = lib.state.value.collection(cid)!!.requests[4]
        val o = GrpcSession(lib, scope)
        o.load(cid, chat)
        o.keepOpen.value = true
        o.start(cid, chat)
        until("the first answer") { o.call.value.messages.size == 1 }
        assertTrue(o.call.value.open && o.call.value.messages[0].contains("Grace"))
        o.send(cid, chat.withGrpc(chat.grpcFields.copy(message = "{nope")))
        assertTrue(o.call.value.toString(), o.call.value.notSent != null && o.call.value.running)
        o.send(cid, chat.withGrpc(chat.grpcFields.copy(message = "{\"name\": \"Two\"}")))
        until("the second answer") { o.call.value.messages.size == 2 }
        assertTrue(o.call.value.messages[1].contains("Two"))
        o.finish()
        until("OK") { !o.call.value.running && o.call.value.status == "OK" }
        @Suppress("UNUSED_VARIABLE") val unused = buildJsonObject { put("x", 1) }
    }
}
