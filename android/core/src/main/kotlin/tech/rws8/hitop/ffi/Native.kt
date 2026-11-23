package tech.rws8.hitop.ffi

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

/**
 * The one JNI function of android/hitop-ffi (`libhitop_ffi.so`): an operation's name and its
 * arguments as JSON, answered with `{"ok": value}` or `{"error": message}`. Text crosses as UTF-8
 * bytes, never as JNI strings.
 */
object Native {
    init {
        System.loadLibrary("hitop_ffi")
    }

    @JvmStatic
    external fun call(op: ByteArray, args: ByteArray): ByteArray?
}

/** A refusal from hitop-ffi (its `error` text). */
class FfiException(message: String) : Exception(message)

/** Typed access to [Native.call]. Blocking: call it off the main thread. */
object Ffi {
    private val json = Json { ignoreUnknownKeys = true }

    fun callText(op: String, args: String): String {
        val out = Native.call(op.toByteArray(Charsets.UTF_8), args.toByteArray(Charsets.UTF_8))
            ?: throw FfiException("$op: no answer")
        return String(out, Charsets.UTF_8)
    }

    fun call(op: String, args: JsonObject = JsonObject(emptyMap())): JsonElement {
        val answer = json.parseToJsonElement(callText(op, args.toString())).jsonObject
        answer["error"]?.let { throw FfiException((it as? kotlinx.serialization.json.JsonPrimitive)?.content ?: it.toString()) }
        return answer["ok"] ?: throw FfiException("$op: no answer")
    }
}
