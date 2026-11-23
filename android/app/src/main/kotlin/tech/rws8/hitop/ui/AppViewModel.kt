package tech.rws8.hitop.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.launch
import tech.rws8.hitop.core.Library
import tech.rws8.hitop.core.Request
import tech.rws8.hitop.core.SendResult

enum class Tab(val label: String) { COLLECTIONS("Collections"), HISTORY("History"), ENVIRONMENTS("Environments"), RUNS("Runs") }

sealed interface Screen {
    data class Request(val collectionId: String, val requestId: String) : Screen
    data class EnvironmentDetail(val envId: String) : Screen
    data object Globals : Screen
}

/**
 * The UI's state over the [Library]: which tab and screen, the editors' unsaved requests (they
 * survive navigation while the app runs, like iOS's one editor per request), responses and sends
 * in flight.
 */
class AppViewModel(val library: Library) : ViewModel() {
    var tab by mutableStateOf(Tab.COLLECTIONS)
    val stack = mutableStateListOf<Screen>()
    val drafts = mutableStateMapOf<String, Request>()
    val responses = mutableStateMapOf<String, SendResult>()
    val sending = mutableStateMapOf<String, Boolean>()
    /** The request whose response sheet is open. */
    var sheetFor by mutableStateOf<String?>(null)

    val top: Screen? get() = stack.lastOrNull()

    fun push(s: Screen) { stack.add(s) }
    fun pop(): Boolean = if (stack.isEmpty()) false else { stack.removeAt(stack.lastIndex); true }

    fun openRequest(collectionId: String, requestId: String) = push(Screen.Request(collectionId, requestId))

    fun newRequest(collectionId: String? = null, folder: String = "") {
        val (c, r) = library.newRequest(collectionId, folder)
        tab = Tab.COLLECTIONS
        openRequest(c.id, r.id)
    }

    fun openHistory(id: Long) {
        val (c, r) = library.openHistory(id) ?: return
        openRequest(c.id, r.id)
    }

    /** The editor's request: the unsaved one, else the saved one. */
    fun draft(requestId: String): Request? = drafts[requestId] ?: library.state.value.find(requestId)?.second

    fun edit(requestId: String, change: (Request) -> Request) {
        val r = draft(requestId) ?: return
        drafts[requestId] = change(r)
    }

    fun isDirty(requestId: String): Boolean {
        val d = drafts[requestId] ?: return false
        return d != library.state.value.find(requestId)?.second
    }

    fun save(collectionId: String, requestId: String) {
        val d = draft(requestId) ?: return
        library.saveRequest(collectionId, d)
        drafts.remove(requestId)
    }

    fun discard(requestId: String) { drafts.remove(requestId) }

    fun send(collectionId: String, requestId: String) {
        val raw = draft(requestId) ?: return
        // A request still named from its URL goes into the history under that name.
        val d = if (requestId in library.state.value.autoNamed) raw.withName(tech.rws8.hitop.core.AutoName.of(raw)) else raw
        if (sending[requestId] == true) return
        sending[requestId] = true
        viewModelScope.launch {
            try {
                responses[requestId] = library.send(collectionId, d)
                sheetFor = requestId
            } finally {
                sending.remove(requestId)
            }
        }
    }
}
