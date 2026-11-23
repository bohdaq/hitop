package tech.rws8.hitop.core

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import tech.rws8.hitop.sync.SyncCore
import java.io.File
import java.nio.file.Files

/** A4: collection runs through hitop-core (the stub answers), the report, and `hitop run` on the same collection. */
class RunsTest {
    private fun center(): Triple<Library, RunCenter, LibraryStore> {
        val (lib, store, _) = TestStub.library()
        return Triple(lib, RunCenter(lib, store, "test"), store)
    }

    @Test
    fun shopApiInStaging() = runBlocking {
        val (_, runs, store) = center()
        val steps = ArrayList<Int>()
        runs.onProgress = { steps.add(it.done) }
        val r = runs.run("1001")!!
        assertEquals(12, r.total)
        assertEquals(11, r.passed)
        assertTrue(r.firstFailure!!, r.firstFailure!!.startsWith("DELETE Cancel order · HTTP 409"))
        assertEquals((0..12).toList(), steps.distinct())
        val report = RunReport.parse(r.reportJson)!!
        assertEquals("1", report.json.str("reportVersion"))
        assertEquals(12, report.count("requests", "total"))
        assertEquals(11, report.count("requests", "passed"))
        assertEquals(listOf("Status is 201", "Total equals sum of items", "Responds under 1 s", "Has a tracking link"), report.iterations[0].first { it.name == "Create order" }.assertions.map { it.name })
        assertFalse(report.iterations[0].any { it.name == "Order events" })
        // Nothing secret in the report or the history.
        assertFalse(r.reportJson.contains("tok_staging_8f2"))
        assertFalse(store.file("runs.json").readText().contains("tok_staging_8f2"))
        assertEquals(1, runs.history.value.size)
    }

    @Test
    fun bailStopsAtTheFirstFailureAndIterationsRepeat() = runBlocking {
        val (_, runs, _) = center()
        val bailed = runs.run("1001", bail = true)!!
        assertEquals(7, bailed.total)
        assertTrue(bailed.bailed)
        assertTrue(RunReport.parse(bailed.reportJson)!!.json.bool("bailed", false))
        val twice = runs.run("1001", iterations = 2)!!
        assertEquals(24, twice.total)
        assertEquals(2, RunReport.parse(twice.reportJson)!!.iterations.size)
    }

    /** The app's report and `hitop run`'s report of the same collection through the same stub are the same run. */
    @Test
    fun theReportIsWhatHitopRunWrites() = runBlocking {
        val bin = System.getenv("HITOP_BIN").orEmpty()
        assumeTrue("HITOP_BIN not set", bin.isNotEmpty())
        val (lib, runs, _) = center()
        val mine = RunReport.parse(runs.run("1001")!!.reportJson)!!
        val dir = Files.createTempDirectory("cli-run").toFile()
        val folder = File(dir, "shop-api")
        SyncCore.toFiles(lib.state.value.collection("1001")!!).forEach { (p, t) -> File(folder, p).apply { parentFile.mkdirs(); writeText(t) } }
        val env = File(dir, "staging.yaml").apply { writeText("name: Staging\nvariables:\n  baseUrl: http://staging.shop.example\n  customerId: '42'\n") }
        val pb = ProcessBuilder(bin, "run", folder.path, "--env-file", env.path, "--secret", "token=tok_staging_8f2", "--secret", "password=hunter2", "--reporter", "json", "--out", File(dir, "out").path)
        pb.environment().putAll(mapOf("HTTP_PROXY" to TestStub.server.proxy, "http_proxy" to TestStub.server.proxy, "HITOP_KEYCHAIN" to "off", "HOME" to dir.path, "XDG_CONFIG_HOME" to dir.path, "NO_COLOR" to "1"))
        pb.environment().remove("NO_PROXY")
        pb.redirectErrorStream(true)
        val p = pb.start()
        val out = p.inputStream.bufferedReader().readText()
        assertEquals(out, 1, p.waitFor())
        val theirs = RunReport(HitopJson.parseToJsonElement(File(dir, "out/report.json").readText()).jsonObject)
        fun shape(r: RunReport) = r.iterations.map { it.map { q -> listOf(q.name, q.method, q.url, q.status, q.passed, q.error, q.assertions.map { a -> a.name to a.pass }) } }
        assertEquals(shape(theirs), shape(mine))
        assertEquals(theirs.summary.toString(), mine.summary.toString())
        assertEquals(theirs.json.keys, mine.json.keys)
    }

    @Test
    fun cancelStopsAfterTheRequestInFlight() = runBlocking {
        val (_, runs, _) = center()
        runs.onProgress = { if (it.done == 2) runs.cancel() }
        val r = runs.run("1001")!!
        assertTrue(r.cancelled)
        assertTrue(r.total in 2..3)
        assertEquals(JsonArray::class, (RunReport.parse(r.reportJson)!!.json["collections"])!!::class)
        @Suppress("UNUSED_VARIABLE") val unused: JsonObject? = null
    }
}
