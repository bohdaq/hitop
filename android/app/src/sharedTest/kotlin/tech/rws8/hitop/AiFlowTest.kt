package tech.rws8.hitop

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.platform.AppGraph

/** A6: on-device AI (the assistant stand-in in tests): explain, suggest a test that passes, request from curl; hidden without a model. */
@RunWith(AndroidJUnit4::class)
class AiFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @After
    fun reset() { AppGraph.assistantReady = true }

    private fun sendCreateOrder() {
        compose.waitForTag("collections")
        compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("request-Create order"))
        compose.onNodeWithTag("request-Create order", useUnmergedTree = true).performClick()
        compose.waitForTag("send")
        compose.onNodeWithTag("send").performClick()
        compose.waitFor(hasText("201 Created"))
    }

    @Test
    fun explainSuggestAndAddATest() {
        Harness.launchSeeded().use {
            sendCreateOrder()
            compose.waitForTag("explain")
            compose.onNodeWithTag("explain").performClick()
            compose.waitFor(hasText("The server answered 201", substring = true))
            compose.screenshot("24-explain")
            compose.onNodeWithTag("suggest-test").performClick()
            compose.waitForTag("suggested-script")
            compose.waitFor(hasText("pm.test(\"total.amount equals 6480\"", substring = true))
            compose.screenshot("25-suggested-test")
            compose.onNodeWithTag("add-test").performClick()
            compose.waitForTag("close-response")
            compose.onNodeWithTag("close-response").performClick()
            compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("response-sheet")).fetchSemanticsNodes().isEmpty() }
            compose.onNodeWithTag("editor-tab-Tests").performScrollTo().performClick()
            compose.waitFor(hasText("links.track is present", substring = true))
        }
    }

    @Test
    fun aRequestFromCurlNeedsNoModel() {
        AppGraph.assistantReady = false
        Harness.launchSeeded().use {
            compose.waitForTag("from-text-button")
            compose.onNodeWithTag("from-text-button").performClick()
            compose.onNodeWithTag("from-text").performTextReplacement("curl -X POST 'https://api.example.test/items' -H 'Content-Type: application/json' --data-raw '{\"name\":\"tee\"}' -k")
            compose.screenshot("26-from-curl")
            compose.onNodeWithTag("from-text-make").performClick()
            compose.waitForTag("send")
            compose.waitFor(hasText("https://api.example.test/items", substring = true))
            compose.onNodeWithTag("send").performClick()
            compose.waitForTag("close-response", 30_000)
            // Without the model there is no Explain.
            assertTrue(compose.onAllNodes(hasTestTag("explain")).fetchSemanticsNodes().isEmpty())
        }
    }
}
