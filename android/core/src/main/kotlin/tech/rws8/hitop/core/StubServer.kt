package tech.rws8.hitop.core

import java.io.BufferedInputStream
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.URI
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.concurrent.thread

/**
 * A local stand-in for the network (UI tests, screenshots, JVM tests): an HTTP proxy on
 * 127.0.0.1 that answers the seeded shop-api hosts (the iOS app's StubURLProtocol answers).
 * hitop-ffi's `useHttpProxy` points hitop-core's client at it, so requests still go through the
 * whole engine (variables, auth, scripts, cookies, timings) and only the far end is fake.
 */
class StubServer {
    data class Seen(val method: String, val url: String, val headers: List<Pair<String, String>>, val body: String) {
        fun header(name: String) = headers.firstOrNull { it.first.equals(name, true) }?.second
    }

    data class Answer(val status: Int, val headers: List<Pair<String, String>>, val body: ByteArray)

    private val socket = ServerSocket(0, 50, InetAddress.getByName("127.0.0.1"))
    val port: Int get() = socket.localPort
    val proxy: String get() = "http://127.0.0.1:$port"
    val seen = CopyOnWriteArrayList<Seen>()
    @Volatile private var running = true

    fun start(): StubServer {
        thread(name = "hitop-stub", isDaemon = true) {
            while (running) {
                val s = runCatching { socket.accept() }.getOrNull() ?: break
                thread(isDaemon = true) { runCatching { serve(s) }; runCatching { s.close() } }
            }
        }
        return this
    }

    fun stop() {
        running = false
        socket.close()
    }

    private fun serve(s: Socket) {
        val input = BufferedInputStream(s.getInputStream())
        val line = readLine(input) ?: return
        val parts = line.split(' ')
        if (parts.size < 2) return
        val method = parts[0]
        val target = parts[1]
        val headers = ArrayList<Pair<String, String>>()
        while (true) {
            val h = readLine(input) ?: break
            if (h.isEmpty()) break
            val i = h.indexOf(':')
            if (i > 0) headers.add(h.substring(0, i).trim() to h.substring(i + 1).trim())
        }
        val length = headers.firstOrNull { it.first.equals("content-length", true) }?.second?.toIntOrNull() ?: 0
        val body = ByteArray(length)
        var read = 0
        while (read < length) {
            val n = input.read(body, read, length - read)
            if (n < 0) break
            read += n
        }
        // A proxy request's target is absolute ("http://staging.shop.example/v2/orders"); a direct one is a path.
        val host = headers.firstOrNull { it.first.equals("host", true) }?.second.orEmpty()
        val url = if (target.startsWith("http")) target else "http://$host$target"
        val seenRequest = Seen(method, url, headers, String(body, Charsets.UTF_8))
        seen.add(seenRequest)
        // A local server (end-to-end tests): passed through as a plain proxy, streamed back as it comes.
        if (headers.any { it.first.equals("upgrade", true) && it.second.equals("websocket", true) }) return webSocket(s, input, headers)
        val local = runCatching { URI(url) }.getOrNull()
        // Only a proxy request (absolute-form target) to a local server is passed through.
        if (target.startsWith("http") && local != null && (local.host == "127.0.0.1" || local.host == "localhost")) {
            Socket(local.host, if (local.port > 0) local.port else 80).use { up ->
                val path = (local.rawPath ?: "/").ifEmpty { "/" } + (local.rawQuery?.let { "?$it" } ?: "")
                val head = StringBuilder("$method $path HTTP/1.1\r\n")
                headers.filterNot { it.first.equals("proxy-connection", true) || it.first.equals("connection", true) }.forEach { (k, v) -> head.append("$k: $v\r\n") }
                head.append("Connection: close\r\n\r\n")
                up.getOutputStream().apply { write(head.toString().toByteArray()); write(body); flush() }
                val inp = up.getInputStream()
                val out = s.getOutputStream()
                val buf = ByteArray(8192)
                while (true) {
                    val n = runCatching { inp.read(buf) }.getOrDefault(-1)
                    if (n < 0) break
                    out.write(buf, 0, n)
                    out.flush()
                }
            }
            return
        }
        val path = runCatching { URI(url).path }.getOrNull()
        if (path == "/v2/stock/live") return ticker(s)
        val answer = answer(seenRequest)
        val out = s.getOutputStream()
        val head = StringBuilder("HTTP/1.1 ${answer.status} ${reason(answer.status).ifEmpty { "Status" }}\r\n")
        answer.headers.forEach { (k, v) -> head.append("$k: $v\r\n") }
        head.append("Content-Length: ${answer.body.size}\r\nConnection: close\r\n\r\n")
        out.write(head.toString().toByteArray(Charsets.UTF_8))
        out.write(answer.body)
        out.flush()
    }

    /** `/v2/stock/live`: a tick every 0.4 s until the client goes away. */
    private fun ticker(s: Socket) {
        val out = s.getOutputStream()
        out.write("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n".toByteArray())
        var n = 0
        while (running) {
            n++
            val ok = runCatching {
                out.write("event: tick\nid: $n\ndata: {\"sku\":\"TEE-0193\",\"stock\":${120 - n}}\n\n".toByteArray())
                out.flush()
            }.isSuccess
            if (!ok || n > 600) break
            Thread.sleep(400)
        }
    }

    /**
     * A WebSocket echo (RFC 6455, enough for tests): echoes text and binary, answers `bin` with
     * bytes 0x00 0xff, closes with 4001 "bye" on `close`; picks the first subprotocol offered.
     */
    private fun webSocket(s: Socket, input: InputStream, headers: List<Pair<String, String>>) {
        fun h(n: String) = headers.firstOrNull { it.first.equals(n, true) }?.second
        val key = h("Sec-WebSocket-Key") ?: return
        val accept = java.util.Base64.getEncoder().encodeToString(java.security.MessageDigest.getInstance("SHA-1").digest((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").toByteArray()))
        val protocol = h("Sec-WebSocket-Protocol")?.split(',')?.firstOrNull()?.trim()
        val out = s.getOutputStream()
        out.write(("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: $accept\r\n" + (protocol?.let { "Sec-WebSocket-Protocol: $it\r\n" } ?: "") + "\r\n").toByteArray())
        out.flush()
        fun frame(opcode: Int, payload: ByteArray) {
            val head = java.io.ByteArrayOutputStream()
            head.write(0x80 or opcode)
            when {
                payload.size < 126 -> head.write(payload.size)
                payload.size < 65536 -> { head.write(126); head.write(payload.size shr 8); head.write(payload.size and 0xff) }
                else -> { head.write(127); for (i in 7 downTo 0) head.write(((payload.size.toLong() shr (8 * i)) and 0xff).toInt()) }
            }
            out.write(head.toByteArray()); out.write(payload); out.flush()
        }
        fun readFully(n: Int): ByteArray? {
            val b = ByteArray(n)
            var r = 0
            while (r < n) { val k = input.read(b, r, n - r); if (k < 0) return null; r += k }
            return b
        }
        while (running) {
            val h2 = readFully(2) ?: return
            val opcode = h2[0].toInt() and 0x0f
            val masked = (h2[1].toInt() and 0x80) != 0
            var len = (h2[1].toInt() and 0x7f).toLong()
            if (len == 126L) len = readFully(2)!!.let { ((it[0].toInt() and 0xff) shl 8 or (it[1].toInt() and 0xff)).toLong() }
            else if (len == 127L) len = readFully(8)!!.fold(0L) { a, b -> (a shl 8) or (b.toLong() and 0xff) }
            val mask = if (masked) readFully(4) else null
            val payload = readFully(len.toInt()) ?: return
            if (mask != null) for (i in payload.indices) payload[i] = (payload[i].toInt() xor mask[i % 4].toInt()).toByte()
            when (opcode) {
                1 -> {
                    val text = String(payload, Charsets.UTF_8)
                    when (text) {
                        "bin" -> frame(2, byteArrayOf(0, 0xff.toByte()))
                        "close" -> { frame(8, byteArrayOf((4001 shr 8).toByte(), (4001 and 0xff).toByte()) + "bye".toByteArray()); return }
                        else -> frame(1, payload)
                    }
                }
                2 -> frame(2, payload)
                8 -> { frame(8, payload.take(2).toByteArray()); return }
                9 -> frame(10, payload)
            }
        }
    }

    private fun readLine(input: InputStream): String? {
        val buf = ByteArrayOutputStream()
        while (true) {
            val b = input.read()
            if (b < 0) return if (buf.size() == 0) null else buf.toString(Charsets.UTF_8.name())
            if (b == '\n'.code) break
            if (b != '\r'.code) buf.write(b)
        }
        return buf.toString(Charsets.UTF_8.name())
    }

    companion object {
        val ORDER_EVENTS = ": order events for shop-api\nevent: order.created\nid: 1\ndata: {\"id\":\"ord_8KQ2mZ41\",\"status\":\"processing\",\"total\":6480}\n\nevent: order.packed\nid: 2\ndata: {\"id\":\"ord_8KQ2mZ41\",\"status\":\"packed\"}\n\nevent: order.shipped\nid: 3\ndata: {\"id\":\"ord_8KQ2mZ41\",\"carrier\":\"DHL\",\ndata: \"tracking\":\"JD0146 0001 2345\"}\n\n"

        /** The catalog's schema (Query.products → Product), as an introspection answer. */
        val INTROSPECTION = """{"data":{"__schema":{"queryType":{"name":"Query"},"mutationType":null,"subscriptionType":null,"types":[""" +
            """{"kind":"OBJECT","name":"Query","fields":[{"name":"products","args":[{"name":"first","type":{"kind":"SCALAR","name":"Int"}}],"type":{"kind":"LIST","ofType":{"kind":"OBJECT","name":"Product"}}},{"name":"product","args":[{"name":"sku","type":{"kind":"NON_NULL","ofType":{"kind":"SCALAR","name":"String"}}}],"type":{"kind":"OBJECT","name":"Product"}}]},""" +
            """{"kind":"OBJECT","name":"Product","fields":[{"name":"sku","args":[],"type":{"kind":"SCALAR","name":"String"}},{"name":"name","args":[],"type":{"kind":"SCALAR","name":"String"}},{"name":"stock","args":[],"type":{"kind":"SCALAR","name":"Int"}},{"name":"rating","args":[],"type":{"kind":"SCALAR","name":"Float"}}]},""" +
            """{"kind":"SCALAR","name":"String"},{"kind":"SCALAR","name":"Int"},{"kind":"SCALAR","name":"Float"}]}}}"""

        private val base = listOf(
            "Content-Type" to "application/json; charset=utf-8",
            "Date" to "Tue, 17 Nov 2026 10:42:00 GMT",
            "Server" to "shop-api",
            "X-Request-Id" to "req_7f3a91c2",
            "Cache-Control" to "no-store",
        )

        private fun json(status: Int, text: String, extra: List<Pair<String, String>> = emptyList()) =
            Answer(status, base + extra, text.toByteArray(Charsets.UTF_8))

        fun answer(r: Seen): Answer {
            val uri = runCatching { URI(r.url) }.getOrNull()
            val path = uri?.path ?: "/"
            val host = uri?.host.orEmpty()
            val m = r.method
            if (host.startsWith("unreachable")) return Answer(503, listOf("Content-Type" to "text/plain"), "upstream unavailable".toByteArray())
            if (host.startsWith("httpbin")) {
                if (path.startsWith("/status/")) {
                    val code = path.removePrefix("/status/").toIntOrNull() ?: 200
                    return Answer(code, listOf("Content-Type" to "text/plain"), if (code == 418) "I'm a teapot".toByteArray() else ByteArray(0))
                }
                return json(200, "{\"method\":\"$m\",\"url\":\"${r.url}\",\"ok\":true}")
            }
            return when {
                m == "POST" && path == "/v2/orders" -> json(201,
                    """{"id":"ord_8KQ2mZ41","status":"processing","total":{"amount":6480,"currency":"EUR"},"items":[{"sku":"TEE-0193","qty":2},{"sku":"CAP-0007","qty":1}],"customer":{"id":"cus_42","name":"Ada","tier":"gold"},"links":{"track":"/v2/orders/ord_8KQ2mZ41/tracking"},"createdAt":"2026-11-17T10:42Z"}""",
                    listOf("Location" to "/v2/orders/ord_8KQ2mZ41", "Set-Cookie" to "session=s_91x; Path=/; HttpOnly"))
                m == "GET" && path == "/v2/orders" -> json(200, """[{"id":"ord_8KQ2mZ41","status":"processing","total":6480},{"id":"ord_7HP1aX02","status":"shipped","total":2190}]""")
                m == "POST" && path == "/v2/auth/login" -> json(200, """{"token":"tok_live_5Yq","expiresIn":3600}""", listOf("Set-Cookie" to "session=s_91x; Path=/; HttpOnly"))
                path == "/echo" -> json(200, "{\"method\":\"$m\",\"path\":\"$path\",\"env\":\"${r.header("X-Env").orEmpty()}\",\"ok\":true}")
                m == "GET" && path == "/v2/orders/stream" -> Answer(200, listOf("Content-Type" to "text/event-stream", "Cache-Control" to "no-store"), ORDER_EVENTS.toByteArray())
                m == "POST" && path == "/graphql" && r.body.contains("__schema") -> json(200, INTROSPECTION)
                m == "POST" && path == "/graphql" -> json(200, """{"data":{"products":[{"sku":"TEE-0193","name":"HITOP tee","stock":120,"rating":4.8},{"sku":"CAP-0007","name":"HITOP cap","stock":64,"rating":4.6}]}}""")
                path.startsWith("/v2/orders/") -> if (m == "DELETE") json(409, """{"error":"order already shipped"}""") else json(200, """{"id":"ord_8KQ2mZ41","status":"processing"}""")
                else -> json(200, "{\"method\":\"$m\",\"path\":\"$path\",\"host\":\"$host\",\"ok\":true}")
            }
        }
    }
}
