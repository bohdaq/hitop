package tech.rws8.hitop.sync

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.core.EmptyObject
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.str
import java.util.Base64

/**
 * The published hitop sync test vectors (cli/crates/hitop-sync-crypto/vectors/v1.json, v2.json)
 * and the key acceptance policy (`accept_wraps`), through the same [SyncCore] ops the app calls.
 * Shared by core's JVM tests and the app's instrumented tests (the .so on the device).
 * [read] gives a vector file's text by name. Returns the number of cases checked.
 */
object VectorChecks {
    private fun b64(hex: String): String = Base64.getUrlEncoder().withoutPadding().encodeToString(hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray())
    private fun hex(b: String): String = Base64.getUrlDecoder().decode(b.trimEnd('=')).joinToString("") { "%02x".format(it) }
    private fun check(ok: Boolean, what: String) { if (!ok) throw AssertionError(what) }
    private fun fails(what: String, f: () -> Any?) = check(runCatching(f).isFailure, "$what: expected a refusal")

    fun run(read: (String) -> String): Int {
        var n = 0
        val v1 = HitopJson.parseToJsonElement(read("v1.json")).jsonObject
        val v2 = HitopJson.parseToJsonElement(read("v2.json")).jsonObject
        fun list(o: JsonObject, k: String) = (o[k] as JsonArray).map { it.jsonObject }

        for (b in list(v1, "blobs")) {
            val args = buildJsonObject { put("key", b64(b.str("key"))); put("workspace", b.str("workspaceId")); put("blob", b.str("blobId")); put("version", b["version"]!!); put("epoch", b["epoch"]!!) }
            fun with(k: String, v: String) = JsonObject(args + (k to JsonPrimitive(v)))
            check(hex(SyncCore.string("encryptBlob", JsonObject(with("plaintext", b64(b.str("plaintext"))) + ("nonce" to JsonPrimitive(b64(b.str("nonce"))))))) == b.str("envelope"), b.str("name"))
            check(hex(SyncCore.string("blobAad", args)) == b.str("aad"), "aad " + b.str("name"))
            check(hex(SyncCore.string("decryptBlob", with("envelope", b64(b.str("envelope"))))) == b.str("plaintext"), "decrypt " + b.str("name"))
            check(SyncCore.blobEpoch(b64(b.str("envelope"))).toString() == b.str("epoch"), "epoch " + b.str("name"))
            n++
        }
        for (b in list(v1, "blobFailures")) {
            fails(b.str("name")) { SyncCore.call("decryptBlob", buildJsonObject { put("key", b64(b.str("key"))); put("workspace", b.str("workspaceId")); put("blob", b.str("blobId")); put("version", b["version"]!!); put("epoch", b["epoch"]!!); put("envelope", b64(b.str("envelope"))) }) }
            n++
        }
        for (w in list(v1, "wraps")) {
            val secret = b64(w.str("recipientSecret"))
            check(SyncCore.publicKey(secret) == b64(w.str("recipientPublic")), "public " + w.str("name"))
            check(hex(SyncCore.string("wrapInfoLegacy", buildJsonObject { put("workspace", w.str("workspaceId")); put("epoch", w["epoch"]!!) })) == w.str("info"), "info " + w.str("name"))
            val key = SyncCore.string("unwrapLegacy", buildJsonObject { put("secret", secret); put("workspace", w.str("workspaceId")); put("epoch", w["epoch"]!!); put("wrapped", b64(w.str("wrapped"))) })
            check(hex(key) == w.str("workspaceKey"), "unwrap " + w.str("name"))
            check(SyncCore.wrapHeader(b64(w.str("wrapped"))).legacy, "legacy header")
            n++
        }
        for (w in list(v1, "wrapFailures")) {
            fails(w.str("name")) { SyncCore.call("unwrapLegacy", buildJsonObject { put("secret", b64(w.str("recipientSecret"))); put("workspace", w.str("workspaceId")); put("epoch", w["epoch"]!!); put("wrapped", b64(w.str("wrapped"))) }) }
            n++
        }
        for (w in list(v2, "wraps")) {
            val secret = b64(w.str("recipientSecret"))
            check(SyncCore.publicKey(secret) == b64(w.str("recipientPublic")), "v2 public " + w.str("name"))
            check(hex(SyncCore.string("wrapInfo", buildJsonObject { put("workspace", w.str("workspaceId")); put("epoch", w["epoch"]!!); put("head", w["head"]!!); put("senderKeyId", w.str("senderKeyId")); put("recipientKeyId", w.str("recipientKeyId")) })) == w.str("info"), "v2 info " + w.str("name"))
            val o = SyncCore.call("openWrap", buildJsonObject {
                put("secret", secret); put("recipientKeyId", w.str("recipientKeyId")); put("senderPublicKey", b64(w.str("senderPublic")))
                put("workspace", w.str("workspaceId")); put("epoch", w["epoch"]!!); put("wrapped", b64(w.str("wrapped")))
            }).jsonObject
            check(hex(o.str("key")) == w.str("workspaceKey"), "v2 open " + w.str("name"))
            check(o.str("sender") == w.str("senderKeyId") && o.str("head") == w.str("head"), "v2 sender/head " + w.str("name"))
            val h = SyncCore.wrapHeader(b64(w.str("wrapped")))
            check(!h.legacy && h.sender == w.str("senderKeyId"), "v2 header " + w.str("name"))
            n++
        }
        for (w in list(v2, "wrapFailures")) {
            fails(w.str("name")) {
                SyncCore.call("openWrap", buildJsonObject {
                    put("secret", b64(w.str("recipientSecret"))); put("recipientKeyId", w.str("recipientKeyId")); put("senderPublicKey", b64(w.str("senderPublic")))
                    put("workspace", w.str("workspaceId")); put("epoch", w["epoch"]!!); put("wrapped", b64(w.str("wrapped")))
                })
            }
            n++
        }
        for (c in list(v2, "commitments")) {
            check(SyncCore.string("keyCommitment", buildJsonObject { put("workspace", c.str("workspaceId")); put("epoch", c["epoch"]!!); put("key", b64(c.str("workspaceKey"))) }) == c.str("commitment"), "commitment")
            n++
        }
        return n + acceptWrapsPolicy()
    }

    /** `accept_wraps` cases (hitop-sync-crypto tests_v2.rs): pinned sender, forged, untrusted, invite fingerprint, legacy in strict mode. */
    private fun acceptWrapsPolicy(): Int {
        val ws = "3f6c1a9e0b7d4c2a8e5f1b3d7a9c0e24"
        val owner = SyncCore.keygen()
        val me = SyncCore.keygen()
        val eve = SyncCore.keygen()
        val k1 = SyncCore.newKey()
        fun wrap(from: SyncCore.DeviceKey, fromId: String, epoch: Int, key: String) = SyncCore.sealWrap(from.secret, fromId, "me", me.publicKey, ws, epoch, epoch, key)
        // A pinned sender's key is accepted and remembered.
        val ring = SyncCore.acceptWraps(me.secret, "me", ws, 1, listOf(1 to wrap(owner, "owner", 1, k1)), mapOf("owner" to owner.publicKey), emptyMap(), EmptyObject)
        check(ring.keys[1] == k1, "pinned sender's key")
        // A wrap claiming the owner but sealed by someone else: forged.
        val forged = runCatching { SyncCore.acceptWraps(me.secret, "me", ws, 1, listOf(1 to wrap(eve, "owner", 1, SyncCore.newKey())), mapOf("owner" to owner.publicKey), emptyMap(), EmptyObject) }.exceptionOrNull()
        check(forged is SyncCore.KeyRefusal && forged.security, "forged refused as a security warning: $forged")
        // An unpinned sender, listed by the server: refused with its fingerprint to compare.
        val untrusted = runCatching { SyncCore.acceptWraps(me.secret, "me", ws, 1, listOf(1 to wrap(owner, "owner", 1, k1)), emptyMap(), mapOf("owner" to owner.publicKey), EmptyObject) }.exceptionOrNull()
        check(untrusted is SyncCore.KeyRefusal && untrusted.kind == "untrustedSender" && untrusted.fingerprint != null, "untrusted sender: $untrusted")
        // The invite code's fingerprint pins that sender.
        val fp = SyncCore.normalize(SyncCore.fingerprint(owner.publicKey))
        val invited = SyncCore.acceptWraps(me.secret, "me", ws, 1, listOf(1 to wrap(owner, "owner", 1, k1)), emptyMap(), mapOf("owner" to owner.publicKey),
            buildJsonObject { put("strict", true); put("expectedSenders", JsonArray(listOf(JsonPrimitive(fp)))) })
        check(invited.keys[1] == k1 && invited.newPins["owner"] == owner.publicKey, "invite fingerprint pins the inviter")
        // Strict memory refuses an unauthenticated (format 1) wrap.
        val legacy = SyncCore.string("wrapLegacy", buildJsonObject { put("publicKey", me.publicKey); put("workspace", ws); put("epoch", 1); put("key", k1) })
        val strict = runCatching { SyncCore.acceptWraps(me.secret, "me", ws, 1, listOf(1 to legacy), mapOf("owner" to owner.publicKey), emptyMap(), buildJsonObject { put("strict", true) }) }.exceptionOrNull()
        check(strict is SyncCore.KeyRefusal, "strict refuses legacy: $strict")
        return 5
    }
}
