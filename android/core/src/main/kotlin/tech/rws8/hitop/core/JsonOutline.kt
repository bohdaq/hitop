package tech.rws8.hitop.core

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * A JSON body as foldable lines (the response sheet's Body tab): each container can be folded
 * to one line ("{ 2 keys }", "[ 3 items ]"). By default the root stays open; containers below it
 * are folded when the whole body would be longer than [OPEN_LINES] lines, and a one-key wrapper
 * (GraphQL's `data`) is never folded at the top.
 */
object JsonOutline {
    const val OPEN_LINES = 14

    enum class Kind { KEY_ONLY, STRING, NUMBER, BOOL, NULL, OPEN, CLOSE, SUMMARY }

    data class Line(
        val depth: Int,
        val key: String?,
        val text: String,
        val kind: Kind,
        /** The container this line opens or summarises ("" = root), for folding. */
        val path: String? = null,
        val folded: Boolean = false,
        val comma: Boolean = false,
    )

    fun parse(text: String): JsonElement? = runCatching { HitopJson.parseToJsonElement(text) }.getOrNull()?.takeIf { it is JsonObject || it is JsonArray }

    /** The paths folded when the body is first shown. */
    fun defaultFolded(root: JsonElement): Set<String> {
        if (count(root) <= OPEN_LINES) return emptySet()
        val out = HashSet<String>()
        val start = if (root is JsonObject && root.size == 1 && root.values.first().let { it is JsonObject || it is JsonArray }) {
            // A wrapper: fold one level further in.
            root.values.first() to "/" + root.keys.first()
        } else root to ""
        children(start.first, start.second).forEach { (p, v) -> if (v is JsonObject || v is JsonArray) out.add(p) }
        return out
    }

    private fun children(e: JsonElement, path: String): List<Pair<String, JsonElement>> = when (e) {
        is JsonObject -> e.entries.map { "$path/${it.key}" to it.value }
        is JsonArray -> e.mapIndexed { i, v -> "$path/$i" to v }
        else -> emptyList()
    }

    private fun count(e: JsonElement): Int = when (e) {
        is JsonObject -> 2 + e.values.sumOf { count(it) }
        is JsonArray -> 2 + e.sumOf { count(it) }
        else -> 1
    }

    fun lines(root: JsonElement, folded: Set<String>): List<Line> {
        val out = ArrayList<Line>()
        fun walk(e: JsonElement, key: String?, depth: Int, path: String, comma: Boolean) {
            when (e) {
                is JsonObject, is JsonArray -> {
                    val isObj = e is JsonObject
                    val size = if (e is JsonObject) e.size else (e as JsonArray).size
                    if (path in folded || size == 0) {
                        val summary = when {
                            size == 0 -> if (isObj) "{}" else "[]"
                            isObj -> "{ $size ${if (size == 1) "key" else "keys"} }"
                            else -> "[ $size ${if (size == 1) "item" else "items"} ]"
                        }
                        out.add(Line(depth, key, summary, Kind.SUMMARY, path.takeIf { size > 0 }, folded = size > 0, comma = comma))
                    } else {
                        out.add(Line(depth, key, if (isObj) "{" else "[", Kind.OPEN, path))
                        val kids = children(e, path)
                        kids.forEachIndexed { i, (p, v) -> walk(v, if (isObj) p.substringAfterLast('/') else null, depth + 1, p, i < kids.size - 1) }
                        out.add(Line(depth, null, if (isObj) "}" else "]", Kind.CLOSE, comma = comma))
                    }
                }
                is JsonNull -> out.add(Line(depth, key, "null", Kind.NULL, comma = comma))
                is JsonPrimitive -> out.add(
                    when {
                        e.isString -> Line(depth, key, "\"" + escape(e.content) + "\"", Kind.STRING, comma = comma)
                        e.content == "true" || e.content == "false" -> Line(depth, key, e.content, Kind.BOOL, comma = comma)
                        else -> Line(depth, key, e.content, Kind.NUMBER, comma = comma)
                    },
                )
            }
        }
        walk(root, null, 0, "", false)
        return out
    }

    private fun escape(s: String) = s.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n")

    /** The body pretty-printed (Raw view and Copy keep the bytes as they came). */
    fun pretty(text: String): String = parse(text)?.let { PrettyJson.encodeToString(JsonElement.serializer(), it) } ?: text
}
