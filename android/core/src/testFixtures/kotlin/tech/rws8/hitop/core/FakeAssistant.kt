package tech.rws8.hitop.core

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/** Deterministic stand-in for tests and screenshots (no model). */
class FakeAssistant(private val ready: Boolean = true) : LanguageAssistant {
    override suspend fun availability() = if (ready) LanguageAssistant.Availability.Ready else LanguageAssistant.Availability.Unavailable("This phone doesn't have the on-device model.")

    override fun stream(prompt: String): Flow<String> = flow {
        val status = Regex("Response: (\\d+)").find(prompt)?.groupValues?.get(1) ?: "no"
        "The server answered $status: the request worked and the body holds the order with its items and total. Nothing looks unusual.".split(" ").forEach { emit("$it ") }
    }

    override suspend fun answer(prompt: String): String = when {
        prompt.startsWith("Suggest") -> """[{"kind":"status","value":201},{"kind":"header","name":"Content-Type","contains":"json"},{"kind":"equals","path":"total.amount","value":6480},{"kind":"exists","path":"links.track"},{"kind":"equals","path":"total.amount","value":1},{"kind":"type","path":"items","type":"array"}]"""
        else -> """{"name":"List orders","method":"GET","url":"${'$'}{baseUrl}/v2/orders?status=open","headers":[{"name":"Accept","value":"application/json"}],"body":""}"""
    }
}
