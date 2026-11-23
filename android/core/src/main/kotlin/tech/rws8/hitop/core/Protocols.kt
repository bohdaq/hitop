package tech.rws8.hitop.core

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.ffi.Ffi

/*
 * WebSocket, Server-Sent Events and gRPC (the iOS app's Protocols/WebSocket/SSE/Grpc): the saved
 * shapes (`kind`, `websocket`, `grpc`, unknown keys kept), the method menu, and live sessions over
 * hitop-ffi (hitop-core's WebSocket and gRPC clients; SSE read as it arrives).
 */

data class WsMessage(val name: String, val content: String, val extra: JsonObject = EmptyObject) {
    fun toJson() = JsonObject(LinkedHashMap(extra).apply { put("name", JsonPrimitive(name)); put("content", JsonPrimitive(content)) })
}

data class GrpcFields(val service: String, val method: String, val message: String, val plaintext: Boolean)

val Request.subprotocols: List<String> get() = json.obj("websocket")?.stringList("subprotocols") ?: emptyList()
val Request.wsMessages: List<WsMessage>
    get() = (json.obj("websocket")?.get("messages") as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }.map { WsMessage(it.str("name"), it.str("content"), it.without("name", "content")) }
val Request.grpcFields: GrpcFields
    get() = json.obj("grpc").let { g -> GrpcFields(g?.str("service").orEmpty(), g?.str("method").orEmpty(), g?.str("message").orEmpty(), g?.bool("plaintext", false) ?: false) }

fun Request.withWebSocket(subprotocols: List<String> = this.subprotocols, messages: List<WsMessage> = this.wsMessages): Request {
    val w = LinkedHashMap(json.obj("websocket") ?: EmptyObject)
    if (subprotocols.isEmpty()) w.remove("subprotocols") else w["subprotocols"] = subprotocols.toJsonArray()
    if (messages.isEmpty()) w.remove("messages") else w["messages"] = JsonArray(messages.map { it.toJson() })
    return set("websocket", if (w.isEmpty()) null else JsonObject(w))
}

fun Request.withGrpc(f: GrpcFields): Request {
    val g = LinkedHashMap(json.obj("grpc") ?: EmptyObject)
    fun put(k: String, v: String) { if (v.isEmpty()) g.remove(k) else g[k] = JsonPrimitive(v) }
    put("service", f.service); put("method", f.method); put("message", f.message)
    if (f.plaintext) g["plaintext"] = JsonPrimitive(true) else g.remove("plaintext")
    return set("grpc", if (g.isEmpty()) null else JsonObject(g))
}

/** The method menu (the web app's methodSelect): HTTP methods, then SSE, WebSocket, gRPC. */
object Modes {
    val ALL = listOf("GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "SSE", "WebSocket", "gRPC")

    fun of(r: Request): String = when {
        r.isWebSocket -> "WebSocket"
        r.isGrpc -> "gRPC"
        r.tag == "SSE" -> "SSE"
        else -> r.method
    }

    fun set(r: Request, mode: String): Request {
        val plain = r.set("kind", null)
        val noEvents = { x: Request -> x.withHeaders(x.headers.filterNot { it.name.equals("Accept", true) && it.value.contains("text/event-stream") }) }
        return when (mode) {
            "WebSocket" -> r.set("kind", JsonPrimitive("websocket"))
            "gRPC" -> r.set("kind", JsonPrimitive("grpc"))
            "SSE" -> plain.withMethod("GET").withHeaders(plain.headers.filterNot { it.name.equals("Accept", true) } + Header("Accept", "text/event-stream"))
            "GET" -> if (of(r) == "SSE") noEvents(plain).withMethod("GET") else plain.withMethod("GET")
            else -> plain.withMethod(mode)
        }
    }
}

/** One Server-Sent Event. */
data class SseEvent(val event: String, val id: String?, val data: String)

/** The web app's `parseEvents` rules, fed bytes split anywhere: a blank line ends an event, `data:` lines join with \n. */
class SseParser {
    private val buffer = StringBuilder()
    private var event: String? = null
    private var id: String? = null
    private val data = ArrayList<String>()

    fun feed(text: String): List<SseEvent> {
        buffer.append(text.replace("\r\n", "\n").replace('\r', '\n'))
        val out = ArrayList<SseEvent>()
        while (true) {
            val nl = buffer.indexOf("\n")
            if (nl < 0) break
            val line = buffer.substring(0, nl)
            buffer.delete(0, nl + 1)
            if (line.isEmpty()) {
                if (data.isNotEmpty()) out.add(SseEvent(event ?: "message", id, data.joinToString("\n")))
                event = null; data.clear()
                continue
            }
            if (line.startsWith(":")) continue
            val field = line.substringBefore(':')
            val value = line.substringAfter(':', "").removePrefix(" ")
            when (field) {
                "event" -> event = value
                "data" -> data.add(value)
                "id" -> id = value
            }
        }
        return out
    }

    /** What's left when the stream ends (an event without its blank line). */
    fun finish(): List<SseEvent> = feed("\n\n")
}

/** A line of a WebSocket or stream log. */
data class LogLine(val at: Long, val kind: Kind, val text: String) {
    enum class Kind { SENT, RECEIVED, INFO, ERROR }
}

/** A WebSocket request's open connection (memory only, up to 500 lines). */
class WebSocketSession(private val library: Library, private val scope: CoroutineScope) {
    enum class Status { IDLE, CONNECTING, OPEN, CLOSED }

    private val _status = MutableStateFlow(Status.IDLE)
    val status: StateFlow<Status> = _status.asStateFlow()
    private val _log = MutableStateFlow<List<LogLine>>(emptyList())
    val log: StateFlow<List<LogLine>> = _log.asStateFlow()
    var protocol: String? = null
        private set
    private var id: Long? = null
    private var job: Job? = null

    private fun add(kind: LogLine.Kind, text: String) = _log.update { (it + LogLine(System.currentTimeMillis(), kind, text)).takeLast(500) }

    fun connect(collectionId: String, request: Request) {
        if (_status.value == Status.CONNECTING || _status.value == Status.OPEN) return
        _status.value = Status.CONNECTING
        add(LogLine.Kind.INFO, "Connecting to ${request.url}")
        job = scope.launch(Dispatchers.IO) {
            val sid = try {
                Ffi.call("wsConnect", library.requestArgs(collectionId, request)).jsonObject.str("id").toLong()
            } catch (e: Exception) {
                add(LogLine.Kind.ERROR, e.message ?: e.toString())
                _status.value = Status.CLOSED
                return@launch
            }
            id = sid
            while (isActive) {
                val r = Ffi.call("wsNext", buildJsonObject { put("id", sid); put("waitMs", 300) }).jsonObject
                for (e in (r["events"] as? JsonArray).orEmpty().map { it.jsonObject }) {
                    when (e.str("type")) {
                        "opened" -> { protocol = e.strOrNull("protocol"); _status.value = Status.OPEN; add(LogLine.Kind.INFO, "Connected" + (protocol?.let { " · $it" } ?: "")) }
                        "message" -> add(LogLine.Kind.RECEIVED, e.strOrNull("text") ?: "binary ${e.str("size")} bytes · ${e.str("base64")}")
                        "closed" -> add(LogLine.Kind.INFO, "Closed" + (e.strOrNull("code")?.let { " $it" } ?: "") + e.str("reason").let { if (it.isEmpty()) "" else " $it" })
                        "error" -> add(LogLine.Kind.ERROR, e.str("message"))
                    }
                }
                if (r.bool("done", false)) break
            }
            id = null
            _status.value = Status.CLOSED
        }
    }

    suspend fun send(text: String) = withContext(Dispatchers.IO) {
        val sid = id ?: return@withContext
        runCatching { Ffi.call("wsSend", buildJsonObject { put("id", sid); put("text", text) }) }.onSuccess { add(LogLine.Kind.SENT, text) }.onFailure { add(LogLine.Kind.ERROR, it.message ?: "") }
    }

    suspend fun sendSaved(name: String) = withContext(Dispatchers.IO) {
        val sid = id ?: return@withContext
        runCatching { (Ffi.call("wsSendSaved", buildJsonObject { put("id", sid); put("name", name) }) as JsonPrimitive).content }
            .onSuccess { add(LogLine.Kind.SENT, it) }.onFailure { add(LogLine.Kind.ERROR, it.message ?: "") }
    }

    suspend fun sendBinary(base64: String) = withContext(Dispatchers.IO) {
        val sid = id ?: return@withContext
        runCatching { Ffi.call("wsSend", buildJsonObject { put("id", sid); put("base64", base64) }) }.onSuccess { add(LogLine.Kind.SENT, "binary · $base64") }
    }

    fun close() {
        val sid = id ?: return
        runCatching { Ffi.call("wsClose", buildJsonObject { put("id", sid) }) }
    }

    fun clear() { _log.value = emptyList() }
}

/** A request read as Server-Sent Events: live until it ends or Stop. */
class SseSession(private val library: Library, private val scope: CoroutineScope) {
    data class State(val running: Boolean = false, val status: Int? = null, val events: List<Pair<Long, SseEvent>> = emptyList(), val error: String? = null, val stopped: Boolean = false)

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()
    private var id: Long? = null

    fun start(collectionId: String, request: Request) {
        if (_state.value.running) return
        _state.value = State(running = true)
        scope.launch(Dispatchers.IO) {
            val parser = SseParser()
            val sid = try {
                Ffi.call("sseStart", library.requestArgs(collectionId, request)).jsonObject.str("id").toLong()
            } catch (e: Exception) {
                _state.value = State(error = Errors.explain(e.message ?: "").headline + ": " + e.message)
                return@launch
            }
            id = sid
            while (true) {
                val r = Ffi.call("sseNext", buildJsonObject { put("id", sid); put("waitMs", 300) }).jsonObject
                for (e in (r["events"] as? JsonArray).orEmpty().map { it.jsonObject }) {
                    when (e.str("type")) {
                        "head" -> _state.update { it.copy(status = e.strOrNull("status")?.toIntOrNull()) }
                        "chunk" -> parser.feed(e.str("text")).let { evs -> _state.update { s -> s.copy(events = s.events + evs.map { System.currentTimeMillis() to it }) } }
                        "end" -> parser.finish().let { evs -> _state.update { s -> s.copy(events = s.events + evs.map { System.currentTimeMillis() to it }, stopped = e.bool("cancelled", false)) } }
                        "error" -> _state.update { it.copy(error = e.str("message")) }
                    }
                }
                if (r.bool("done", false)) break
            }
            id = null
            _state.update { it.copy(running = false) }
        }
    }

    fun stop() {
        val sid = id ?: return
        runCatching { Ffi.call("sseCancel", buildJsonObject { put("id", sid) }) }
    }
}

/** gRPC through hitop-core's client (SyncFFI's grpc* ops): reflection, unary and server streaming. */
class GrpcSession(private val library: Library, private val scope: CoroutineScope) {
    data class Call(val running: Boolean = false, val messages: List<String> = emptyList(), val headers: List<Pair<String, String>> = emptyList(), val status: String? = null, val statusMessage: String? = null, val error: String? = null)

    private val _services = MutableStateFlow<List<String>>(emptyList())
    val services: StateFlow<List<String>> = _services.asStateFlow()
    private val _methods = MutableStateFlow<Map<String, List<String>>>(emptyMap())
    val methods: StateFlow<Map<String, List<String>>> = _methods.asStateFlow()
    val source = MutableStateFlow<String?>(null)
    val schemaError = MutableStateFlow<String?>(null)
    private val _call = MutableStateFlow(Call())
    val call: StateFlow<Call> = _call.asStateFlow()
    private var id: Long? = null

    private fun target(collectionId: String, request: Request): JsonObject = Ffi.call("grpcTarget", library.requestArgs(collectionId, request)).jsonObject

    private fun server(t: JsonObject, timeoutMs: Long = 10_000) = buildJsonObject {
        put("address", t.str("address")); put("plaintext", t.bool("plaintext", false)); put("metadata", t["metadata"] ?: JsonArray(emptyList())); put("timeoutMs", timeoutMs)
    }

    /** The server's services and methods through reflection. */
    suspend fun load(collectionId: String, request: Request) = withContext(Dispatchers.IO) {
        schemaError.value = null
        try {
            val t = target(collectionId, request)
            val r = Ffi.call("grpcList", buildJsonObject { put("server", server(t)) }).jsonObject
            source.value = r.strOrNull("source")
            val list = (r["services"] as? JsonArray).orEmpty().map { it.jsonObject }
            _services.value = list.map { it.str("name") }
            _methods.value = list.associate { s -> s.str("name") to (s["methods"] as? JsonArray).orEmpty().map { m -> (m as? JsonObject)?.str("name") ?: (m as JsonPrimitive).content } }
        } catch (e: Exception) {
            val m = e.message.orEmpty()
            schemaError.value = if (m.contains("reflection", true) || m.contains("Unimplemented", true)) "This server has no reflection service: type the service, method and message by hand." else m
        }
    }

    /** A message template for the method (its fields with empty values), as pretty JSON text. */
    suspend fun template(collectionId: String, request: Request, symbol: String): String? = withContext(Dispatchers.IO) {
        runCatching { Ffi.call("grpcDescribe", buildJsonObject { put("server", server(target(collectionId, request))); put("symbol", symbol) }).jsonObject.str("template") }.getOrNull()
    }

    fun start(collectionId: String, request: Request) {
        if (_call.value.running) return
        _call.value = Call(running = true)
        scope.launch(Dispatchers.IO) {
            val cid = try {
                val t = target(collectionId, request)
                Ffi.call("grpcStart", buildJsonObject {
                    put("server", server(t)); put("service", t.str("service")); put("method", t.str("method")); put("message", t.str("message").ifBlank { "{}" }); put("maxTimeMs", 60_000)
                }).jsonObject.str("id").toLong()
            } catch (e: Exception) {
                _call.value = Call(error = e.message)
                return@launch
            }
            id = cid
            while (true) {
                val r = Ffi.call("grpcNext", buildJsonObject { put("id", cid); put("waitMs", 300) }).jsonObject
                for (e in (r["events"] as? JsonArray).orEmpty().map { it.jsonObject }) {
                    when (e.str("type")) {
                        "headers" -> _call.update { it.copy(headers = metadata(e["metadata"])) }
                        "message" -> _call.update { it.copy(messages = it.messages + JsonOutline.pretty(e.str("data"))) }
                        "status" -> _call.update { it.copy(status = e.str("name"), statusMessage = e.str("message")) }
                        "error" -> _call.update { it.copy(error = e.str("message")) }
                    }
                }
                if (r.bool("done", false)) break
            }
            id = null
            _call.update { it.copy(running = false) }
        }
    }

    private fun metadata(e: JsonElement?): List<Pair<String, String>> = (e as? JsonArray).orEmpty().mapNotNull { (it as? JsonArray)?.let { a -> (a[0] as JsonPrimitive).content to (a[1] as JsonPrimitive).content } }

    fun cancel() {
        val cid = id ?: return
        runCatching { Ffi.call("grpcCancel", buildJsonObject { put("id", cid) }) }
    }
}
