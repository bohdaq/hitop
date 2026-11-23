package tech.rws8.hitop

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.performTextReplacement
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.Harness.waitForText

/** A1: the seeded shop-api sends through hitop-core (the stub answers) and shows its responses. */
@RunWith(AndroidJUnit4::class)
class CoreClientTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private fun open(name: String) {
        compose.waitForTag("collections")
        compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("request-$name"))
        compose.onNodeWithTag("request-$name", useUnmergedTree = true).performClick()
        compose.waitForTag("send")
    }

    @Test
    fun createOrderSends201AndPassesItsFourTests() {
        Harness.launchSeeded().use {
            compose.waitForTag("collections")
            Harness.screenshot("01-collections")
            open("Create order")
            compose.onNodeWithTag("editor-tab-Body").performClick()
            Harness.screenshot("02-request")
            compose.onNodeWithTag("send").performClick()
            compose.waitForText("201 Created")
            compose.onNodeWithText("Tests 4/4").assertIsDisplayed()
            Harness.screenshot("03-response")
            compose.onNodeWithTag("response-tab-Tests").performClick()
            listOf("Status is 201", "Total equals sum of items", "Responds under 1 s", "Has a tracking link").forEach { compose.onNodeWithText(it).assertExists() }
            compose.onNodeWithTag("response-tab-Cookies").performClick()
            compose.onNodeWithText("session").assertExists()
            compose.onNodeWithTag("response-tab-Headers").performClick()
            compose.onNodeWithText("/v2/orders/ord_8KQ2mZ41").assertExists()
        }
    }

    @Test
    fun aNewRequestSendsSavesAndShowsInHistory() {
        Harness.launchSeeded().use {
            compose.waitForTag("new-request")
            compose.onNodeWithTag("new-request").performClick()
            compose.waitForTag("URL")
            compose.onNodeWithTag("URL").performTextReplacement("http://httpbin.shop.example/anything?x=1")
            compose.waitFor(hasText("GET /anything"))
            compose.onNodeWithTag("send").performClick()
            compose.waitForText("200 OK")
            compose.onNodeWithTag("close-response").performClick()
            compose.onNodeWithTag("save").performClick()
            compose.onNodeWithTag("back").performClick()
            compose.waitForTag("collections")
            compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("request-GET /anything"))
            compose.onNodeWithTag("tab-History").performClick()
            compose.waitForTag("history")
            compose.onNodeWithTag("history").performScrollToNode(hasTestTag("history-GET /anything"))
            Harness.screenshot("04-history")
            compose.onNodeWithTag("history-GET /anything").performClick()
            compose.waitForTag("send")
            compose.onNodeWithTag("request-title").assertExists()
        }
    }

    @Test
    fun environmentsChangeWhereRequestsGoAndNoneIsExplained() {
        Harness.launchSeeded().use {
            compose.waitForTag("environment-pill")
            compose.onNodeWithTag("environment-pill").performClick()
            compose.onNodeWithTag("env-option-Production").performClick()
            open("Cancel order")
            compose.onNodeWithTag("send").performClick()
            compose.waitForText("409 Conflict")
            compose.onNodeWithTag("close-response").performClick()
            compose.onNodeWithTag("environment-pill").performClick()
            compose.onNodeWithText("No environment").performClick()
            compose.onNodeWithTag("send").performClick()
            compose.waitForTag("error-card")
            compose.onNodeWithText("\${baseUrl} has no value").assertExists()
        }
    }

    @Test
    fun aVariableChipShowsItsValueAndSecretsAreMasked() {
        Harness.launchSeeded().use {
            open("Create order")
            compose.onNodeWithTag("var-baseUrl").performClick()
            compose.waitForTag("peek")
            compose.onNodeWithText("baseUrl = http://staging.shop.example · from Staging").assertExists()
            compose.onNodeWithTag("var-baseUrl").performClick()
            compose.onNodeWithTag("back").performClick()
            compose.onNodeWithTag("tab-Environments").performClick()
            compose.waitForTag("env-open-Staging")
            Harness.screenshot("05-environments")
            compose.onNodeWithTag("env-open-Staging").performClick()
            compose.waitForTag("value-token")
            compose.onNodeWithText("tok_staging_8f2").assertDoesNotExist()
            compose.onNodeWithTag("show-token").performClick()
            compose.waitForText("tok_staging_8f2")
        }
    }

    @Test
    fun noScreenSellsAnything() {
        Harness.launchSeeded().use {
            compose.waitForTag("collections")
            val screens = listOf("tab-Collections", "tab-History", "tab-Environments", "tab-Runs")
            screens.forEach { tab ->
                compose.onNodeWithTag(tab).performClick()
                compose.waitForIdle()
                Harness.PURCHASE_WORDS.forEach { w ->
                    assertTrue("$w on $tab", compose.onAllNodesWithText(w, substring = true).fetchSemanticsNodes().isEmpty())
                }
            }
        }
    }
}
