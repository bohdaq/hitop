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
        val answer = answer(seenRequest)
        val out = s.getOutputStream()
        val head = StringBuilder("HTTP/1.1 ${answer.status} ${reason(answer.status).ifEmpty { "Status" }}\r\n")
        answer.headers.forEach { (k, v) -> head.append("$k: $v\r\n") }
        head.append("Content-Length: ${answer.body.size}\r\nConnection: close\r\n\r\n")
        out.write(head.toString().toByteArray(Charsets.UTF_8))
        out.write(answer.body)
        out.flush()
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
                m == "POST" && path == "/graphql" -> json(200, """{"data":{"products":[{"sku":"TEE-0193","name":"HITOP tee","stock":120,"rating":4.8},{"sku":"CAP-0007","name":"HITOP cap","stock":64,"rating":4.6}]}}""")
                path.startsWith("/v2/orders/") -> if (m == "DELETE") json(409, """{"error":"order already shipped"}""") else json(200, """{"id":"ord_8KQ2mZ41","status":"processing"}""")
                else -> json(200, "{\"method\":\"$m\",\"path\":\"$path\",\"host\":\"$host\",\"ok\":true}")
            }
        }
    }
}
