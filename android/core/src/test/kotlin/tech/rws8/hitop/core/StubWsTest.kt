package tech.rws8.hitop.core

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertTrue
import org.junit.Test

class StubWsTest {
    @Test
    fun theStubEchoes() = runBlocking {
        val (lib, _, _) = TestStub.library(seed = false)
        val c = lib.addCollection("x")
        lib.newRequest(c.id, template = Request.new("w", "ws://127.0.0.1:${TestStub.server.port}/v2/stock").set("kind", JsonPrimitive("websocket")).withWebSocket(listOf("stock.v2")))
        val s = WebSocketSession(lib, CoroutineScope(SupervisorJob() + Dispatchers.Default))
        s.connect(c.id, lib.state.value.collection(c.id)!!.requests.single())
        withTimeoutOrNull(10_000) { while (s.status.value != WebSocketSession.Status.OPEN && s.status.value != WebSocketSession.Status.CLOSED) delay(50) }
        assertTrue(s.log.value.toString(), s.status.value == WebSocketSession.Status.OPEN)
        s.send("hello")
        withTimeoutOrNull(5_000) { while (s.log.value.none { it.kind == LogLine.Kind.RECEIVED }) delay(50) }
        assertTrue(s.log.value.toString(), s.log.value.any { it.kind == LogLine.Kind.RECEIVED && it.text == "hello" })
    }
}
