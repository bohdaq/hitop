package tech.rws8.hitop

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.platform.Widgets

/** A4: a collection run (the foreground service on a device), its result, report, history and the widget snapshot. */
@RunWith(AndroidJUnit4::class)
class RunFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @Test
    fun runShopApiSeeTheFailureAndTheReport() {
        Harness.launchSeeded().use {
            compose.waitForTag("tab-Runs")
            compose.onNodeWithTag("tab-Runs").performClick()
            compose.waitForTag("run-shop-api")
            compose.onNodeWithTag("run-shop-api").performClick()
            compose.waitForTag("start-run")
            compose.screenshot("14-run-options")
            compose.onNodeWithTag("start-run").performClick()
            compose.waitForTag("open-report", 60_000)
            compose.onNodeWithText("11/12").assertExists()
            compose.waitForTag("failure-card")
            compose.onNodeWithText("HTTP 409", substring = true).assertExists()
            compose.screenshot("15-run-result")
            compose.onNodeWithTag("open-report").performClick()
            compose.waitForTag("report")
            compose.onNodeWithTag("report").performScrollToNode(hasText("HTTP 409"))
            compose.screenshot("16-run-report")
            compose.onNodeWithTag("back").performClick()
            compose.onNodeWithTag("back").performClick()
            compose.waitFor(hasTestTag("history-run-shop-api"))
            compose.screenshot("17-runs")
            // The widgets' snapshot: the run, names and counts only.
            val ctx = ApplicationProvider.getApplicationContext<android.content.Context>()
            compose.waitUntil(10_000) { Widgets.snapshot(ctx).lastRun != null }
            val s = Widgets.snapshot(ctx)
            assertEquals(11, s.lastRun!!.passed)
            assertEquals(12, s.lastRun!!.total)
            val text = java.io.File(ctx.filesDir, "widget-snapshot.json").readText()
            assertTrue(!text.contains("tok_staging") && !text.contains("shop.example"))
            assertEquals(1, AppGraph.parts(ctx).runs.history.value.size)
        }
    }

    @Test
    fun stopAtTheFirstFailure() {
        Harness.launchSeeded().use {
            compose.waitForTag("collections")
            compose.onNodeWithTag("run-button-shop-api").performClick()
            compose.waitForTag("bail")
            compose.onNodeWithTag("bail").performClick()
            compose.onNodeWithTag("start-run").performClick()
            compose.waitForTag("open-report", 60_000)
            compose.onNodeWithText("6/7").assertExists()
            Harness.PURCHASE_WORDS.forEach { w -> assertTrue(compose.onAllNodes(hasText(w, substring = true)).fetchSemanticsNodes().isEmpty()) }
        }
    }
}
