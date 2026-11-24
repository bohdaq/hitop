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
    data object Account : Screen
    data object Settings : Screen
    data class Run(val collectionId: String) : Screen
    data class Report(val id: Long) : Screen
    data class Request(val collectionId: String, val requestId: String) : Screen
    data class EnvironmentDetail(val envId: String) : Screen
    data object Globals : Screen
}

/**
 * The UI's state over the [Library]: which tab and screen, the editors' unsaved requests (they
 * survive navigation while the app runs, like iOS's one editor per request), responses and sends
 * in flight.
 */
class AppViewModel(val library: Library, cloud: tech.rws8.hitop.cloud.HitopCloud, sync: tech.rws8.hitop.cloud.LibrarySync, val shared: tech.rws8.hitop.sync.SharedSync, signIn: tech.rws8.hitop.platform.SignInPlatform, val runs: tech.rws8.hitop.core.RunCenter, val settings: tech.rws8.hitop.core.SettingsStore, val network: tech.rws8.hitop.platform.PhoneNetwork, val assistant: tech.rws8.hitop.core.LanguageAssistant) : ViewModel() {
    /** The on-device model is there (Explain, Suggest a test, request from text); hidden when not. */
    var aiReady by mutableStateOf(false)
        private set

    init {
        runCatching { library.pruneUploads() }
        viewModelScope.launch { aiReady = runCatching { assistant.availability() }.getOrNull() is tech.rws8.hitop.core.LanguageAssistant.Availability.Ready }
    }

    /** The import sheet is open, and what it starts with (a file or text shared from another app). */
    var importing by mutableStateOf(false)
    var importSource by mutableStateOf<ImportSource?>(null)
    /** Text shared from another app, for a new request (nothing is sent until the person taps Send). */
    var sharedText by mutableStateOf<String?>(null)

    fun receive(shared: tech.rws8.hitop.platform.Incoming.Shared) {
        when (shared) {
            is tech.rws8.hitop.platform.Incoming.Shared.File -> { importSource = ImportSource.File(shared.uri); importing = true }
            is tech.rws8.hitop.platform.Incoming.Shared.Text -> {
                val t = shared.text
                when {
                    tech.rws8.hitop.core.CurlImport.looksLikeCurl(t) -> sharedText = t
                    Regex("https?://\\S+|wss?://\\S+").matches(t) -> {
                        val (c, r) = library.newRequest()
                        openRequest(c.id, r.id)
                        edit(r.id) { it.withUrl(t) }
                    }
                    t.startsWith("{") || t.startsWith("[") || Regex("(?m)^(openapi|swagger):").containsMatchIn(t) -> { importSource = ImportSource.Text(t); importing = true }
                    else -> sharedText = t
                }
            }
        }
    }

    /** The last delete or move, while it can be taken back. */
    data class Undo(val label: String, val undo: () -> Unit)
    var undo by mutableStateOf<Undo?>(null)

    /** A delete or move with Undo. */
    fun undoable(label: String, change: () -> Unit) {
        undo = library.undoable(change)?.let { Undo(label, it) }
    }

    /** What a response can be compared with: the request's examples, then its history. */
    fun compareSources(requestId: String): List<tech.rws8.hitop.core.ComparedResponse> {
        val examples = draft(requestId)?.examples.orEmpty().filterIsInstance<kotlinx.serialization.json.JsonObject>().mapIndexed { i, e -> tech.rws8.hitop.core.ComparedResponse.ofExample("Example: ${(e["name"] as? kotlinx.serialization.json.JsonPrimitive)?.content ?: (i + 1)}", e) }
        val history = library.state.value.history.filter { it.request.id == requestId && it.response != null }.map { h ->
            tech.rws8.hitop.core.ComparedResponse.of("${tech.rws8.hitop.cloud.LibrarySync.ago(h.timestamp)} · ${h.response!!.status}", h.response!!)
        }
        // The newest history entry is the response on screen.
        return examples + history.drop(1)
    }

    fun secretValues(): List<String> = library.activeSecretValues()

    /** Starts a collection run: the foreground service on a device, in-process under Robolectric. */
    fun startRun(context: android.content.Context, collectionId: String, iterations: Int = 1, bail: Boolean = false, data: tech.rws8.hitop.core.RunData? = null) {
        if (runs.running) return
        runs.setData(collectionId, data)
        if (tech.rws8.hitop.platform.AppGraph.runsInProcess) {
            viewModelScope.launch {
                runs.run(collectionId, iterations, bail)
                tech.rws8.hitop.platform.Widgets.refresh(context.applicationContext)
            }
        } else {
            tech.rws8.hitop.platform.RunService.start(context, collectionId, iterations, bail)
        }
    }

    val cloud = CloudModel(cloud, sync, signIn, viewModelScope)

    var tab by mutableStateOf(Tab.COLLECTIONS)
    val stack = mutableStateListOf<Screen>()
    val drafts = mutableStateMapOf<String, Request>().apply { putAll(library.loadDrafts()) }
    private var draftsJob: kotlinx.coroutines.Job? = null
    private val sends = HashMap<String, kotlinx.coroutines.Job>()

    private fun draftsChanged() {
        draftsJob?.cancel()
        draftsJob = viewModelScope.launch { kotlinx.coroutines.delay(400); writeDrafts() }
    }

    /** Unsaved edits to disk now (the app goes to the background: Android may end its process). */
    fun writeDrafts() {
        draftsJob?.cancel()
        runCatching { library.saveDrafts(drafts.toMap()) }
    }
    val responses = mutableStateMapOf<String, SendResult>()
    val sending = mutableStateMapOf<String, Boolean>()
    /** The request whose response sheet is open. */
    var sheetFor by mutableStateOf<String?>(null)
    /** The request whose response is being explained (its own sheet; the response sheet steps aside). */
    var explainFor by mutableStateOf<String?>(null)
    /** The request whose live sheet (WebSocket, SSE, gRPC) is open. */
    var liveFor by mutableStateOf<String?>(null)

    private val wsSessions = HashMap<String, tech.rws8.hitop.core.WebSocketSession>()
    private val sseSessions = HashMap<String, tech.rws8.hitop.core.SseSession>()
    private val grpcSessions = HashMap<String, tech.rws8.hitop.core.GrpcSession>()
    val schemas = mutableStateMapOf<String, tech.rws8.hitop.core.GraphQLSchema>()

    fun ws(id: String) = wsSessions.getOrPut(id) { tech.rws8.hitop.core.WebSocketSession(library, viewModelScope) }
    fun sse(id: String) = sseSessions.getOrPut(id) { tech.rws8.hitop.core.SseSession(library, viewModelScope) }
    fun grpc(id: String) = grpcSessions.getOrPut(id) { tech.rws8.hitop.core.GrpcSession(library, viewModelScope) }
    fun collectionOf(requestId: String) = library.state.value.find(requestId)?.first?.id

    /** Introspection through the normal send path (kept out of the history); an error's text, or null. */
    suspend fun loadSchema(collectionId: String, r: Request): String? {
        val res = library.send(collectionId, tech.rws8.hitop.core.GraphQLSchema.introspectionRequest(r), history = false)
        res.error?.let { return it.headline }
        return try {
            schemas[r.id] = tech.rws8.hitop.core.GraphQLSchema.parse(res.body)
            null
        } catch (e: Exception) {
            "Couldn't read the schema: ${e.message}"
        }
    }

    /** Send for live kinds: connect a WebSocket, read an event stream, make a gRPC call. */
    fun startLive(collectionId: String, requestId: String) {
        val r = draft(requestId) ?: return
        when {
            r.isWebSocket -> ws(requestId).connect(collectionId, r)
            r.isGrpc -> grpc(requestId).start(collectionId, r)
            else -> sse(requestId).start(collectionId, r)
        }
        liveFor = requestId
    }

    override fun onCleared() {
        wsSessions.values.forEach { it.close() }
        sseSessions.values.forEach { it.stop() }
        grpcSessions.values.forEach { it.cancel() }
    }

    val top: Screen? get() = stack.lastOrNull()

    fun push(s: Screen) { stack.add(s) }
    fun pop(): Boolean = if (stack.isEmpty()) false else { stack.removeAt(stack.lastIndex); true }

    fun openRequest(collectionId: String, requestId: String) {
        // One request screen at a time (the wide layout shows the newest in its middle pane).
        if (top is Screen.Request) stack.removeAt(stack.lastIndex)
        push(Screen.Request(collectionId, requestId))
    }

    /** Ctrl+K: the Collections tab with its search field focused. */
    var searchFocus by mutableStateOf(0)
    fun showSearch() {
        if (top !is Screen.Request) stack.clear()
        tab = Tab.COLLECTIONS
        searchFocus++
    }

    /** Ctrl+Enter: send (or connect/call) the request on screen. */
    fun sendCurrent() {
        val r = top as? Screen.Request ?: return
        val d = draft(r.requestId) ?: return
        if (d.isWebSocket || d.isGrpc || d.tag == "SSE") startLive(r.collectionId, r.requestId) else send(r.collectionId, r.requestId)
    }

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
        library.state.value.find(requestId)?.first?.let { if (library.isReadOnly(it.id)) return }
        drafts[requestId] = change(r)
        draftsChanged()
    }

    fun isDirty(requestId: String): Boolean {
        val d = drafts[requestId] ?: return false
        return d != library.state.value.find(requestId)?.second
    }

    fun save(collectionId: String, requestId: String) {
        val d = draft(requestId) ?: return
        library.saveRequest(collectionId, d)
        drafts.remove(requestId)
        draftsChanged()
    }

    fun discard(requestId: String) { drafts.remove(requestId); draftsChanged() }

    fun send(collectionId: String, requestId: String) {
        val raw = draft(requestId) ?: return
        // A request still named from its URL goes into the history under that name.
        val d = if (requestId in library.state.value.autoNamed) raw.withName(tech.rws8.hitop.core.AutoName.of(raw)) else raw
        if (sending[requestId] == true) return
        sending[requestId] = true
        sends[requestId] = viewModelScope.launch {
            try {
                responses[requestId] = library.send(collectionId, d)
                sheetFor = requestId
            } finally {
                sending.remove(requestId)
                sends.remove(requestId)
            }
        }
    }

    /** Stops waiting for the answer; nothing of it is kept. */
    fun cancelSend(requestId: String) { sends[requestId]?.cancel() }
}
