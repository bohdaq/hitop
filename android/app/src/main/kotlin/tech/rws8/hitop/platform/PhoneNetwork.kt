package tech.rws8.hitop.platform

import android.content.Context
import android.net.ConnectivityManager
import android.util.Base64
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.core.AppSettings
import java.security.KeyStore

/** The phone's own network setup, for hitop-ffi's `setNetwork`; only what Settings turned on. */
class PhoneNetwork(private val context: Context) {
    /** Reading the certificates takes a moment: once per process (a new one needs a restart of HITOP or the switch off and on). */
    private var roots: List<String>? = null

    @Synchronized
    fun forget() { roots = null }

    @Synchronized
    fun userRoots(): List<String> = roots ?: runCatching {
        val ks = KeyStore.getInstance("AndroidCAStore").apply { load(null) }
        ks.aliases().toList().filter { it.startsWith("user:") }.mapNotNull { ks.getCertificate(it)?.encoded }.map { Base64.encodeToString(it, Base64.NO_WRAP) }
    }.getOrDefault(emptyList()).also { roots = it }

    fun args(s: AppSettings): JsonObject = buildJsonObject {
        if (s.trustUserCas) userRoots().takeIf { it.isNotEmpty() }?.let { put("roots", JsonArray(it.map(::JsonPrimitive))) }
        if (s.systemProxy) context.getSystemService(ConnectivityManager::class.java)?.defaultProxy?.let { p ->
            if (!p.host.isNullOrEmpty() && p.port in 1..65535) {
                put("proxy", "http://${if (':' in p.host) "[${p.host}]" else p.host}:${p.port}")
                put("noProxy", (p.exclusionList.orEmpty().map { it.removePrefix("*.") } + listOf("localhost", "127.0.0.1", "::1")).joinToString(","))
            }
        }
    }
}
