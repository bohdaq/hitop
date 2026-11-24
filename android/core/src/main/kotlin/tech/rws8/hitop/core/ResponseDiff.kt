package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** A response to compare: the current one, a saved example, or a history entry's. */
data class ComparedResponse(val label: String, val status: Int?, val headers: Map<String, String>, val body: String) {
    companion object {
        private fun headers(rows: List<Pair<String, String>>) = rows.associate { it.first.lowercase() to it.second }
        fun of(label: String, r: SendResult) = ComparedResponse(label, r.status, headers(r.headers), r.body)
        fun of(label: String, r: ResponseSnapshot) = ComparedResponse(label, r.status, headers(r.headers), r.body)

        /** A request's saved example (free-form: headers as rows or as a map, a body of any JSON type). */
        fun ofExample(label: String, e: JsonObject): ComparedResponse {
            val h = when (val v = e["headers"]) {
                is JsonArray -> headers(v.mapNotNull { (it as? JsonObject)?.let { r -> r.str("name") to r.str("value") } })
                is JsonObject -> v.entries.associate { (k, x) -> k.lowercase() to ((x as? JsonPrimitive)?.content ?: x.toString()) }
                else -> emptyMap()
            }
            val body = when (val b = e["body"]) {
                null, is JsonNull -> ""
                is JsonPrimitive -> b.content
                else -> PrettyJson.encodeToString(JsonElement.serializer(), b)
            }
            return ComparedResponse(label, e.str("status").toIntOrNull(), h, body)
        }
    }
}

/** The web app's response diff (src/diff.js): JSON by path (key order ignored), other bodies by line, headers by name. */
object ResponseDiff {
    enum class Kind { ADDED, REMOVED, CHANGED, SAME }
    data class Change(val kind: Kind, val path: String, val before: String? = null, val after: String? = null)

    /** Headers that differ on every response. */
    private val VOLATILE = setOf("date", "age", "expires", "x-request-id")

    private fun child(path: String, key: String) = if (Regex("[A-Za-z_$][\\w$]*").matches(key)) "$path.$key" else "$path[${JsonPrimitive(key)}]"

    private fun equal(a: JsonElement, b: JsonElement): Boolean {
        if (a !is JsonPrimitive || b !is JsonPrimitive) return a == b
        if (a.isString || b.isString || a is JsonNull || b is JsonNull) return a == b
        val (x, y) = a.content.toBigDecimalOrNull() to b.content.toBigDecimalOrNull()
        return if (x != null && y != null) x.compareTo(y) == 0 else a.content == b.content
    }

    fun json(a: JsonElement, b: JsonElement, path: String = "$", out: MutableList<Change> = ArrayList()): List<Change> {
        when {
            a is JsonObject && b is JsonObject -> (a.keys + b.keys).sorted().forEach { k ->
                val (x, y) = a[k] to b[k]
                when {
                    y == null -> out.add(Change(Kind.REMOVED, child(path, k), x.toString()))
                    x == null -> out.add(Change(Kind.ADDED, child(path, k), after = y.toString()))
                    else -> json(x, y, child(path, k), out)
                }
            }
            a is JsonArray && b is JsonArray -> (0 until maxOf(a.size, b.size)).forEach { i ->
                when {
                    i >= b.size -> out.add(Change(Kind.REMOVED, "$path[$i]", a[i].toString()))
                    i >= a.size -> out.add(Change(Kind.ADDED, "$path[$i]", after = b[i].toString()))
                    else -> json(a[i], b[i], "$path[$i]", out)
                }
            }
            !equal(a, b) -> out.add(Change(Kind.CHANGED, path, a.toString(), b.toString()))
        }
        return out
    }

    /** By line (longest common subsequence); past [limit] cells the middle is removed then added. */
    fun lines(a: String, b: String, limit: Long = 1_000_000): List<Change> {
        val x = if (a.isEmpty()) emptyList() else a.split(Regex("\\r?\\n"))
        val y = if (b.isEmpty()) emptyList() else b.split(Regex("\\r?\\n"))
        fun same(t: String) = Change(Kind.SAME, "", t, t)
        fun removed(t: String) = Change(Kind.REMOVED, "", t)
        fun added(t: String) = Change(Kind.ADDED, "", after = t)
        var start = 0
        while (start < x.size && start < y.size && x[start] == y[start]) start++
        var (ex, ey) = x.size to y.size
        while (ex > start && ey > start && x[ex - 1] == y[ey - 1]) { ex--; ey-- }
        val mx = x.subList(start, ex)
        val my = y.subList(start, ey)
        val mid = ArrayList<Change>()
        if (mx.size.toLong() * my.size > limit) {
            mx.mapTo(mid, ::removed)
            my.mapTo(mid, ::added)
        } else {
            val t = Array(mx.size + 1) { IntArray(my.size + 1) }
            for (i in mx.indices.reversed()) for (j in my.indices.reversed()) t[i][j] = if (mx[i] == my[j]) t[i + 1][j + 1] + 1 else maxOf(t[i + 1][j], t[i][j + 1])
            var (i, j) = 0 to 0
            while (i < mx.size && j < my.size) when {
                mx[i] == my[j] -> { mid.add(same(mx[i])); i++; j++ }
                t[i + 1][j] >= t[i][j + 1] -> mid.add(removed(mx[i++]))
                else -> mid.add(added(my[j++]))
            }
            while (i < mx.size) mid.add(removed(mx[i++]))
            while (j < my.size) mid.add(added(my[j++]))
        }
        return x.take(start).map(::same) + mid + x.drop(ex).map(::same)
    }

    data class Result(val status: Change?, val headers: List<Change>, val body: List<Change>, val json: Boolean) {
        val same: Boolean get() = status == null && headers.isEmpty() && body.none { it.kind != Kind.SAME }
    }

    private fun parse(t: String): JsonElement? = if (t.trimStart().firstOrNull()?.let { it in "{[" } == true) runCatching { HitopJson.parseToJsonElement(t) }.getOrNull() else null

    fun compare(a: ComparedResponse, b: ComparedResponse): Result {
        val headers = (a.headers.keys + b.headers.keys).filter { it !in VOLATILE }.sorted().mapNotNull { k ->
            val (x, y) = a.headers[k] to b.headers[k]
            when {
                x == y -> null
                y == null -> Change(Kind.REMOVED, k, x)
                x == null -> Change(Kind.ADDED, k, after = y)
                else -> Change(Kind.CHANGED, k, x, y)
            }
        }
        val (ja, jb) = parse(a.body) to parse(b.body)
        val isJson = ja != null && jb != null
        return Result(
            if (a.status == b.status) null else Change(Kind.CHANGED, "status", a.status?.toString() ?: "none", b.status?.toString() ?: "none"),
            headers,
            if (isJson) json(ja!!, jb!!) else lines(a.body, b.body),
            isJson,
        )
    }
}
