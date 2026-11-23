package tech.rws8.hitop.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull

/** One JSON configuration for files and the FFI: unknown keys are data, never errors. */
val HitopJson = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
    encodeDefaults = true
}

/** Pretty JSON as the web app's export writes it (two spaces). */
val PrettyJson = Json {
    prettyPrint = true
    prettyPrintIndent = "  "
}

val EmptyObject = JsonObject(emptyMap())

/** A string value, or [default] when the key is missing, null or not a string/number/bool. */
fun JsonObject.str(key: String, default: String = ""): String {
    val p = this[key] as? JsonPrimitive ?: return default
    if (p is JsonNull) return default
    return p.contentOrNull ?: default
}

fun JsonObject.strOrNull(key: String): String? {
    val p = this[key] as? JsonPrimitive ?: return null
    if (p is JsonNull) return null
    return p.contentOrNull
}

fun JsonObject.bool(key: String, default: Boolean): Boolean = (this[key] as? JsonPrimitive)?.booleanOrNull ?: default

fun JsonObject.obj(key: String): JsonObject? = this[key] as? JsonObject

fun JsonObject.arr(key: String): JsonArray? = this[key] as? JsonArray

/** This object with [key] set to [value] (removed when null), every other key kept in place. */
fun JsonObject.with(key: String, value: JsonElement?): JsonObject {
    val m = LinkedHashMap(this)
    if (value == null) m.remove(key) else m[key] = value
    return JsonObject(m)
}

fun JsonObject.with(key: String, value: String?): JsonObject = with(key, value?.let { JsonPrimitive(it) })

fun JsonObject.without(vararg keys: String): JsonObject = JsonObject(LinkedHashMap(this).apply { keys.forEach { remove(it) } })

/** `{name: string}` maps (variables): non-string values become their JSON text. */
fun JsonObject.stringMap(key: String): Map<String, String> {
    val o = obj(key) ?: return emptyMap()
    return o.mapValues { (_, v) -> if (v is JsonPrimitive && v !is JsonNull) v.content else if (v is JsonNull) "" else v.toString() }
}

fun Map<String, String>.toJsonObject(): JsonObject = JsonObject(mapValues { JsonPrimitive(it.value) })

fun JsonObject.stringList(key: String): List<String> =
    arr(key)?.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p !is JsonNull }?.content } ?: emptyList()

fun List<String>.toJsonArray(): JsonArray = JsonArray(map { JsonPrimitive(it) })
