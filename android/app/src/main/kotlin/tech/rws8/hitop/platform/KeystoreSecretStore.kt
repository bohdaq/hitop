package tech.rws8.hitop.platform

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import tech.rws8.hitop.core.HitopJson
import tech.rws8.hitop.core.SecretStore
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Secret values encrypted with an AES-256-GCM key that lives in the Android Keystore (it never
 * leaves it; hardware-backed where the phone has it). The ciphertexts are in `secrets.json` in
 * the app's private files, apart from everything the app syncs or exports; backups are off.
 */
class KeystoreSecretStore(private val file: File) : SecretStore {
    private val alias = "hitop-secrets"
    private var cache: MutableMap<String, String>? = null

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getEntry(alias, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    private fun values(): MutableMap<String, String> {
        cache?.let { return it }
        val out = HashMap<String, String>()
        if (file.exists()) {
            runCatching {
                val o = HitopJson.parseToJsonElement(file.readText()).jsonObject
                val k = key()
                o.forEach { (name, v) ->
                    val bytes = Base64.decode((v as JsonPrimitive).content, Base64.NO_WRAP)
                    val c = Cipher.getInstance("AES/GCM/NoPadding")
                    c.init(Cipher.DECRYPT_MODE, k, GCMParameterSpec(128, bytes, 0, 12))
                    out[name] = String(c.doFinal(bytes, 12, bytes.size - 12), Charsets.UTF_8)
                }
            }
        }
        cache = out
        return out
    }

    private fun persist(m: Map<String, String>) {
        val k = key()
        val o = JsonObject(m.mapValues { (_, v) ->
            val c = Cipher.getInstance("AES/GCM/NoPadding")
            c.init(Cipher.ENCRYPT_MODE, k)
            JsonPrimitive(Base64.encodeToString(c.iv + c.doFinal(v.toByteArray(Charsets.UTF_8)), Base64.NO_WRAP))
        })
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(o.toString())
        tmp.renameTo(file)
    }

    @Synchronized override fun get(key: String): String? = values()[key]

    @Synchronized override fun set(key: String, value: String?) {
        val m = values()
        if (value == null) m.remove(key) else m[key] = value
        persist(m)
    }
}
