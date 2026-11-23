package tech.rws8.hitop.core

/** The web app's FV1 naming rules (webapp/src/new-request.js). */
object AutoName {
    const val UNTITLED = "Untitled"
    const val MY_REQUESTS = "My requests"
    const val SAMPLE_NAME = "Sample: JSONPlaceholder"
    val SAMPLE_REQUESTS = listOf("List posts", "Get the first post", "Create a post")

    fun isMyRequests(c: Collection) = c.name == MY_REQUESTS || c.name == "Default"

    /** The part of a URL the name shows: its path, or the host when there is none. */
    fun urlWhere(text: String): String {
        var url = text.trim().replace(Regex("^[a-zA-Z][a-zA-Z0-9+.-]*://"), "").replace(Regex("[?#].*$", RegexOption.DOT_MATCHES_ALL), "")
        if (url.isEmpty()) return ""
        val lead = Regex("^(\\$\\{[^}]*\\}|\\{\\{[^}]*\\}\\})").find(url)
        val host = lead?.value ?: url.substringBefore('/')
        val path = url.substring(host.length)
        url = if (path.isNotEmpty() && path != "/") path else host
        return if (url.length > 48) url.substring(0, 47) + "…" else url
    }

    /** "GET /users", "GET api.example.com", "WS /socket", "GRPC SayHello"; "Untitled" with no URL. */
    fun of(r: Request): String {
        if (r.isGrpc) {
            val m = r.json.obj("grpc")?.str("method").orEmpty()
            val what = m.ifEmpty { urlWhere(r.url) }
            return if (what.isEmpty()) UNTITLED else "GRPC $what"
        }
        val where = urlWhere(r.url)
        if (where.isEmpty()) return UNTITLED
        return "${if (r.isWebSocket) "WS" else r.method} $where"
    }

    fun isBlank(r: Request) = r.url.isBlank() && r.body.isBlank() && r.preRequestScript.isBlank() && r.postRequestScript.isBlank() && r.kind == null

    fun isEmptyMyRequests(c: Collection) = isMyRequests(c) && c.requests.all(::isBlank) && c.variables.isEmpty() && c.folders.isEmpty()
}

/** A piece of text split into literal text and `${name}` references (the URL card, the body). */
data class Segment(val text: String, val variable: String? = null)

object Variables {
    private val ref = Regex("\\$\\{([^}]+)\\}")

    fun segments(text: String): List<Segment> {
        val out = ArrayList<Segment>()
        var at = 0
        for (m in ref.findAll(text)) {
            if (m.range.first > at) out.add(Segment(text.substring(at, m.range.first)))
            out.add(Segment(m.value, m.groupValues[1].trim()))
            at = m.range.last + 1
        }
        if (at < text.length) out.add(Segment(text.substring(at)))
        return out
    }

    fun names(text: String): List<String> = ref.findAll(text).map { it.groupValues[1].trim() }.distinct().toList()
}

/** Where a variable's value comes from, for the peek. */
enum class Scope(val label: String) { GLOBALS("Globals"), COLLECTION("Collection"), ENVIRONMENT("Environment"), SCRIPT("Set by a script"), SECRET("Secret") }

data class Peek(val name: String, val value: String?, val scope: Scope?, val secret: Boolean, val where: String)

/** Byte sizes as the response header shows them ("287 B", "1.4 kB", "2.1 MB"). */
fun formatSize(bytes: Long): String = when {
    bytes < 1000 -> "$bytes B"
    bytes < 1_000_000 -> String.format(java.util.Locale.ROOT, "%.1f kB", bytes / 1000.0)
    else -> String.format(java.util.Locale.ROOT, "%.1f MB", bytes / 1_000_000.0)
}

fun formatDuration(ms: Long): String = if (ms < 1000) "$ms ms" else String.format(java.util.Locale.ROOT, "%.1f s", ms / 1000.0)

/** HTTP reason phrases for the status pill ("201 Created"). */
fun reason(status: Int): String = when (status) {
    100 -> "Continue"; 101 -> "Switching Protocols"
    200 -> "OK"; 201 -> "Created"; 202 -> "Accepted"; 204 -> "No Content"; 206 -> "Partial Content"
    301 -> "Moved Permanently"; 302 -> "Found"; 303 -> "See Other"; 304 -> "Not Modified"; 307 -> "Temporary Redirect"; 308 -> "Permanent Redirect"
    400 -> "Bad Request"; 401 -> "Unauthorized"; 403 -> "Forbidden"; 404 -> "Not Found"; 405 -> "Method Not Allowed"; 408 -> "Request Timeout"
    409 -> "Conflict"; 410 -> "Gone"; 412 -> "Precondition Failed"; 413 -> "Content Too Large"; 415 -> "Unsupported Media Type"
    418 -> "I'm a teapot"; 422 -> "Unprocessable Content"; 429 -> "Too Many Requests"
    500 -> "Internal Server Error"; 501 -> "Not Implemented"; 502 -> "Bad Gateway"; 503 -> "Service Unavailable"; 504 -> "Gateway Timeout"
    else -> ""
}

/** An engine or network error as a headline and a next step (the web app's explainError). */
data class ExplainedError(val headline: String, val next: String, val raw: String)

object Errors {
    fun explain(raw: String): ExplainedError {
        val t = raw.lowercase()
        fun e(h: String, n: String) = ExplainedError(h, n, raw)
        Regex("variable '([^']+)' is not defined").find(raw)?.let {
            return e("\${${it.groupValues[1]}} has no value", "Pick an environment that sets it, or add it to the collection's variables.")
        }
        return when {
            Regex("variable[s]? .*not (set|defined)|unknown variable|undefined variable|missing variable|no value for").containsMatchIn(t) ->
                e("A variable has no value", "Pick an environment that sets it, or add it to the collection's variables.")
            t.contains("dns") || t.contains("failed to lookup") || t.contains("name or service not known") || t.contains("no address associated") || t.contains("nodename nor servname") ->
                e("The host name didn't resolve", "Check the URL's host, or that this phone is online.")
            t.contains("connection refused") ->
                e("The server refused the connection", "Nothing is listening at that address and port. Check the URL and that the server runs.")
            t.contains("timed out") || t.contains("timeout") || t.contains("deadline") ->
                e("The request took too long", "The server didn't answer in time. Try again, or raise the time limit.")
            t.contains("certificate") || t.contains("tls") || t.contains("ssl") || t.contains("handshake") ->
                e("The secure connection failed", "The server's certificate wasn't accepted (expired, self-signed, or for another host).")
            t.contains("relative url") || t.contains("builder error") || t.contains("invalid url") || t.contains("url") && t.contains("parse") || t.contains("empty host") ->
                e("The URL isn't valid", "Write a full address such as https://api.example.com/path.")
            t.contains("script") || t.contains("referenceerror") || t.contains("typeerror") || t.contains("syntaxerror") ->
                e("A script failed", "Fix the pre-request script; the details say where.")
            t.contains("network is unreachable") || t.contains("no route") || t.contains("offline") ->
                e("No network", "Check that this phone is online.")
            else -> e("The request failed", "The details below say what the engine reported.")
        }
    }
}
