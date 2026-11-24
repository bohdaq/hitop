package tech.rws8.hitop.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** What the person chose on the Settings screen (settings.json; this phone only, never synced). */
data class AppSettings(
    /** Time limit of one request in ms; 0 = the engine's 30 s. */
    val timeoutMs: Long = 0,
    /** Certificate authorities installed on this phone by its user, for requests (never HITOP Cloud). */
    val trustUserCas: Boolean = false,
    /** The phone's HTTP proxy, for requests. */
    val systemProxy: Boolean = false,
    val librarySync: Boolean = true,
    val backgroundSync: Boolean = true,
) {
    fun toJson() = buildJsonObject {
        put("timeoutMs", timeoutMs); put("trustUserCas", trustUserCas); put("systemProxy", systemProxy); put("librarySync", librarySync); put("backgroundSync", backgroundSync)
    }

    companion object {
        val TIMEOUTS = listOf(0L, 10_000L, 60_000L, 120_000L, 300_000L)
        fun of(o: JsonObject?) = if (o == null) AppSettings() else AppSettings(
            (o.strOrNull("timeoutMs")?.toLongOrNull() ?: 0).coerceIn(0, 600_000), o.bool("trustUserCas", false), o.bool("systemProxy", false), o.bool("librarySync", true), o.bool("backgroundSync", true),
        )
    }
}

class SettingsStore(private val store: LibraryStore) {
    private val _state = MutableStateFlow(AppSettings.of(store.readJson("settings.json") as? JsonObject))
    val state: StateFlow<AppSettings> = _state.asStateFlow()
    val value: AppSettings get() = _state.value

    /** Called after a change. */
    var onChange: ((AppSettings) -> Unit)? = null

    fun update(change: (AppSettings) -> AppSettings) {
        val next = change(_state.value)
        if (next == _state.value) return
        store.write("settings.json", next.toJson())
        _state.value = next
        onChange?.invoke(next)
    }
}
