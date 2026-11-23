package tech.rws8.hitop.sync

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import tech.rws8.hitop.core.Collection
import tech.rws8.hitop.core.EmptyObject
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.bool
import tech.rws8.hitop.core.obj
import tech.rws8.hitop.core.str
import tech.rws8.hitop.core.strOrNull
import tech.rws8.hitop.ffi.Ffi

/**
 * hitop sync's client logic, the Rust every hitop client runs (SyncFFI's operations through
 * hitop-ffi: hitop-core's sync plan and folder format, hitop-sync-crypto's wraps, `accept_wraps`,
 * blobs, shared environment values). Nothing cryptographic is written in Kotlin. Binary values are
 * base64url strings.
 */
object SyncCore {
    fun call(op: String, args: JsonObject = EmptyObject): JsonElement = Ffi.call(op, args)
    fun string(op: String, args: JsonObject = EmptyObject): String = (call(op, args) as JsonPrimitive).content

    data class DeviceKey(val secret: String, val publicKey: String)

    fun keygen(): DeviceKey = call("keygen").jsonObject.let { DeviceKey(it.str("secret"), it.str("publicKey")) }
    fun publicKey(secret: String) = string("publicKey", buildJsonObject { put("secret", secret) })
    /** "62bd a406 …": eight groups of four hex digits. */
    fun fingerprint(publicKey: String) = string("fingerprint", buildJsonObject { put("publicKey", publicKey) })
    fun normalize(fingerprint: String) = runCatching { string("normalizeFingerprint", buildJsonObject { put("fingerprint", fingerprint) }) }.getOrDefault("")
    fun grouped(fp: String) = normalize(fp).chunked(4).joinToString(" ")
    fun newKey() = string("newKey")
    fun randomId() = string("randomId")

    fun sealWrap(secret: String, sender: String, recipient: String, recipientPublicKey: String, workspace: String, epoch: Int, head: Int, key: String) =
        string("sealWrap", buildJsonObject {
            put("secret", secret); put("senderKeyId", sender); put("recipientKeyId", recipient); put("recipientPublicKey", recipientPublicKey)
            put("workspace", workspace); put("epoch", epoch); put("head", head); put("key", key)
        })

    data class WrapHeader(val legacy: Boolean, val sender: String?, val head: Int?)

    fun wrapHeader(wrapped: String): WrapHeader = call("wrapHeader", buildJsonObject { put("wrapped", wrapped) }).jsonObject.let {
        WrapHeader(it.bool("legacy", true), it.strOrNull("sender"), it.strOrNull("head")?.toIntOrNull())
    }

    /** A refused key (`accept_wraps`'s error): untrustedSender, forged, keyChanged, rollback, legacyRefused, missingKey or malformed. */
    class KeyRefusal(val workspace: String, val kind: String, val text: String, val epoch: Int?, val senderKeyId: String?, val fingerprint: String?) :
        Exception(
            when (kind) {
                "missingKey" -> "This device has no key for the shared collection yet: a member opens it on their device to hand the key over."
                "untrustedSender" -> text
                else -> if (kind in listOf("forged", "keyChanged", "rollback")) "Security warning: $text" else text
            },
        ) {
        val security: Boolean get() = kind in listOf("forged", "keyChanged", "rollback")
        override fun equals(other: Any?) = other is KeyRefusal && other.workspace == workspace && other.kind == kind && other.senderKeyId == senderKeyId && other.epoch == epoch
        override fun hashCode() = listOf(workspace, kind, senderKeyId, epoch).hashCode()
    }

    data class KeyNotice(val kind: String, val epoch: Int?, val senderKeyId: String?, val fingerprint: String?, val message: String)

    data class Keyring(val keys: Map<Int, String>, val current: Int, val details: List<KeyNotice>, val newPins: Map<String, String>, val memory: JsonElement, val legacy: Boolean)

    fun acceptWraps(secret: String, keyId: String, workspace: String, currentEpoch: Int, wraps: List<Pair<Int, String>>, pinned: Map<String, String>, listed: Map<String, String>, memory: JsonElement): Keyring {
        val r = call("acceptWraps", buildJsonObject {
            put("secret", secret); put("keyId", keyId); put("workspace", workspace); put("currentEpoch", currentEpoch)
            put("wraps", buildJsonArray { wraps.forEach { (e, w) -> add(buildJsonObject { put("epoch", e); put("wrapped", w) }) } })
            put("pinned", JsonObject(pinned.mapValues { JsonPrimitive(it.value) }))
            put("listed", JsonObject(listed.mapValues { JsonPrimitive(it.value) }))
            put("memory", memory)
        }).jsonObject
        if (!r.bool("ok", false)) {
            val e = r.obj("error") ?: EmptyObject
            throw KeyRefusal(workspace, e.str("kind", "malformed"), e.str("message", "the workspace key was refused"), e.strOrNull("epoch")?.toIntOrNull(), e.strOrNull("senderKeyId"), e.strOrNull("fingerprint"))
        }
        val keys = r.obj("keys")?.mapNotNull { (e, k) -> e.toIntOrNull()?.let { it to (k as JsonPrimitive).content } }?.toMap() ?: emptyMap()
        val details = (r["notices"] as? JsonArray)?.mapNotNull { n -> (n as? JsonObject)?.let { KeyNotice(it.str("kind"), it.strOrNull("epoch")?.toIntOrNull(), it.strOrNull("senderKeyId"), it.strOrNull("fingerprint"), it.str("message")) } } ?: emptyList()
        return Keyring(keys, r.str("current", "0").toInt(), details, r.obj("newPins")?.mapValues { (it.value as JsonPrimitive).content } ?: emptyMap(), r["memory"] ?: EmptyObject, r.bool("legacy", false))
    }

    fun blobEpoch(envelope: String): Int = string("blobEpoch", buildJsonObject { put("envelope", envelope) }).toInt()

    fun seal(key: String, workspace: String, blob: String, version: Long, epoch: Int, path: String, text: String) = string("seal", buildJsonObject {
        put("key", key); put("workspace", workspace); put("blob", blob); put("version", version); put("epoch", epoch); put("path", path); put("text", text)
    })

    fun open(key: String, workspace: String, blob: String, version: Long, envelope: String): Pair<String, String> =
        call("open", buildJsonObject { put("key", key); put("workspace", workspace); put("blob", blob); put("version", version); put("envelope", envelope) }).jsonObject.let { it.str("path") to it.str("text") }

    /** A collection → its folder files, as the CLI writes them. */
    fun toFiles(c: Collection): Map<String, String> {
        val folder = buildJsonObject { put("collection", c.forEngine()); put("environments", JsonArray(emptyList())) }
        return call("toFiles", buildJsonObject { put("folder", folder.toString()) }).jsonObject.mapValues { (it.value as JsonPrimitive).content }
    }

    /** Files → the collection they hold (`name` when hitop.yaml has none). */
    fun readFiles(files: Map<String, String>, name: String): Collection {
        val text = string("readFiles", buildJsonObject { put("files", JsonObject(files.mapValues { JsonPrimitive(it.value) })); put("name", name) })
        return Collection(HitopJson.parseToJsonElement(text).jsonObject.obj("collection") ?: EmptyObject)
    }

    fun isSyncedPath(path: String) = runCatching { call("isSyncedPath", buildJsonObject { put("path", path) }).jsonPrimitive.content == "true" }.getOrDefault(false)
}
