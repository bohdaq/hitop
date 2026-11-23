package tech.rws8.hitop.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.ffi.Ffi

/** One finished run: counts, its first failure, and hitop-core's report.json text verbatim. */
data class RunRecord(
    val id: Long,
    val collectionId: String,
    val collectionName: String,
    val environment: String?,
    val startedAt: Long,
    val durationMs: Long,
    val total: Int,
    val passed: Int,
    val bailed: Boolean,
    val cancelled: Boolean,
    val firstFailure: String?,
    val reportJson: String,
) {
    val ok: Boolean get() = total > 0 && passed == total
    fun toJson() = buildJsonObject {
        put("id", id); put("collectionId", collectionId); put("collectionName", collectionName); environment?.let { put("environment", it) }
        put("startedAt", startedAt); put("durationMs", durationMs); put("total", total); put("passed", passed); put("bailed", bailed); put("cancelled", cancelled)
        firstFailure?.let { put("firstFailure", it) }; put("reportJson", reportJson)
    }

    companion object {
        const val MAX = 30
        fun of(o: JsonObject) = RunRecord(
            o.str("id", "0").toLong(), o.str("collectionId"), o.str("collectionName"), o.strOrNull("environment"), o.str("startedAt", "0").toLong(),
            o.str("durationMs", "0").toLong(), o.str("total", "0").toInt(), o.str("passed", "0").toInt(), o.bool("bailed", false), o.bool("cancelled", false),
            o.strOrNull("firstFailure"), o.str("reportJson"),
        )
    }
}

/** A step of a run as it finishes (the live list and the notification). */
data class RunStep(val iteration: Int, val name: String, val method: String, val status: Int?, val passed: Boolean, val durationMs: Long, val error: String?, val tests: List<TestResult>)

/** The run going on: live progress. */
data class RunProgress(
    val collectionId: String,
    val collectionName: String,
    val environment: String?,
    val total: Int,
    val steps: List<RunStep> = emptyList(),
    val startedAt: Long = System.currentTimeMillis(),
    val finished: RunRecord? = null,
    val reportJunit: String? = null,
    val reportHtml: String? = null,
) {
    val done: Int get() = steps.size
    val passed: Int get() = steps.count { it.passed }
    val failed: Int get() = steps.count { !it.passed }
}

/** hitop-core's report.json (reportVersion 1, spec/run-report.md) read for the screens. */
data class RunReport(val json: JsonObject) {
    data class Request(val name: String, val method: String, val url: String, val status: Int?, val durationMs: Long, val passed: Boolean, val error: String?, val assertions: List<TestResult>)

    val iterations: List<List<Request>>
        get() = (json["collections"] as? JsonArray).orEmpty().flatMap { c -> ((c as JsonObject)["iterations"] as? JsonArray).orEmpty() }.map { it as JsonObject }.map { i ->
            (i["requests"] as? JsonArray).orEmpty().map { it as JsonObject }.map { r ->
                Request(
                    r.str("name"), r.str("method"), r.str("url"), r.strOrNull("status")?.toIntOrNull(), r.str("durationMs", "0").toLongOrNull() ?: 0, r.bool("passed", false), r.strOrNull("error"),
                    (r["assertions"] as? JsonArray).orEmpty().map { a -> (a as JsonObject).let { TestResult(it.str("name"), it.bool("passed", false), it.str("message")) } },
                )
            }
        }

    val summary: JsonObject get() = json.obj("summary") ?: EmptyObject
    fun count(k: String, which: String) = summary.obj(k)?.str(which, "0")?.toIntOrNull() ?: 0

    companion object {
        fun parse(text: String) = runCatching { RunReport(HitopJson.parseToJsonElement(text).jsonObject) }.getOrNull()
    }
}

/**
 * Runs (one at a time) through hitop-ffi's `runStart`/`runNext`/`runCancel`: the collection as
 * `hitop run` runs it. Keeps the last [RunRecord.MAX] runs in `runs.json`.
 */
class RunCenter(private val library: Library, private val store: LibraryStore, private val toolVersion: String = "1.0.0") {
    private val _current = MutableStateFlow<RunProgress?>(null)
    val current: StateFlow<RunProgress?> = _current.asStateFlow()
    private val _history = MutableStateFlow(load())
    val history: StateFlow<List<RunRecord>> = _history.asStateFlow()
    @Volatile private var runId: Long? = null

    /** Called on every change of the run going on (the notification) and at its end (widgets). */
    var onProgress: ((RunProgress) -> Unit)? = null

    private fun load(): List<RunRecord> = (store.readJson("runs.json") as? JsonArray)?.mapNotNull { (it as? JsonObject)?.let(RunRecord::of) } ?: emptyList()

    private fun save(list: List<RunRecord>) = store.write("runs.json", JsonArray(list.map { it.toJson() }))

    val running: Boolean get() = runId != null

    /** Runs [collectionId] (with the active environment); returns its record, or null when one runs already. */
    suspend fun run(collectionId: String, iterations: Int = 1, bail: Boolean = false, environmentId: String? = library.state.value.activeEnvironmentId): RunRecord? = withContext(Dispatchers.IO) {
        if (runId != null) return@withContext null
        val c = library.state.value.collection(collectionId) ?: return@withContext null
        val env = library.state.value.environments.firstOrNull { it.id == environmentId }
        val args = library.runArgs(c, env) {
            put("iterations", iterations); put("bail", bail); put("toolVersion", toolVersion); put("jar", store.loadJar())
        }
        val steps = c.requests.count { !it.isWebSocket && !it.isGrpc } * iterations
        var progress = RunProgress(c.id, c.name, env?.name, steps)
        _current.value = progress
        onProgress?.invoke(progress)
        val id = Ffi.call("runStart", args).jsonObject.str("id").toLong()
        runId = id
        try {
            while (true) {
                val r = Ffi.call("runNext", buildJsonObject { put("id", id); put("waitMs", 400) }).jsonObject
                for (e in (r["events"] as? JsonArray).orEmpty().map { it.jsonObject }) {
                    when (e.str("type")) {
                        "step" -> {
                            val s = e.obj("step") ?: EmptyObject
                            val resp = s.obj("response")
                            progress = progress.copy(steps = progress.steps + RunStep(
                                e.str("iteration", "1").toInt(), s.str("name"), s.str("method"), resp?.strOrNull("status")?.toIntOrNull(), s.bool("passed", false),
                                s.str("durationMs", "0").toLong(), s.strOrNull("error"), (s["tests"] as? JsonArray).orEmpty().map { TestResult.of(it.jsonObject) },
                            ))
                        }
                        "done" -> {
                            (e["jar"] as? JsonArray)?.let { store.saveJar(it) }
                            val failure = progress.steps.firstOrNull { !it.passed }?.let { f -> "${f.method} ${f.name} · " + (f.status?.let { "HTTP $it" } ?: f.error ?: "failed") + (f.tests.firstOrNull { !it.pass }?.let { " · ${it.name}" } ?: "") }
                            val record = RunRecord(Ids.next(), c.id, c.name, env?.name, progress.startedAt, System.currentTimeMillis() - progress.startedAt,
                                progress.steps.size, progress.passed, !e.bool("cancelled", false) && bail && progress.failed > 0 && progress.steps.size < steps, e.bool("cancelled", false), failure, e.str("reportJson"))
                            progress = progress.copy(finished = record, reportJunit = e.strOrNull("junit"), reportHtml = e.strOrNull("html"))
                            val list = (listOf(record) + _history.value).take(RunRecord.MAX)
                            save(list)
                            _history.value = list
                        }
                    }
                }
                _current.value = progress
                onProgress?.invoke(progress)
                if (r.bool("done", false)) break
            }
        } finally {
            runId = null
        }
        progress.finished
    }

    fun cancel() {
        val id = runId ?: return
        runCatching { Ffi.call("runCancel", buildJsonObject { put("id", id) }) }
    }

    fun delete(id: Long) {
        val list = _history.value.filter { it.id != id }
        save(list)
        _history.value = list
    }

    fun clear() {
        save(emptyList())
        _history.value = emptyList()
    }

    fun dismissCurrent() {
        if (runId == null) _current.value = null
    }
}
