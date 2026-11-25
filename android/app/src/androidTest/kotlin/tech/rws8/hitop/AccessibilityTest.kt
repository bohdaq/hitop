package tech.rws8.hitop

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.accessibility.enableAccessibilityChecks
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.tryPerformAccessibilityChecks
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag

/** A6: Accessibility Test Framework checks (touch targets, labels, contrast, ...) on every main screen, on the device. */
@RunWith(AndroidJUnit4::class)
class AccessibilityTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @Test
    fun mainScreensPassTheChecks() {
        Harness.launchSeeded().use {
            compose.enableAccessibilityChecks()
            compose.waitForTag("collections")
            compose.onNodeWithTag("collections").tryPerformAccessibilityChecks()
            compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("request-Create order"))
            compose.onNodeWithTag("request-Create order", useUnmergedTree = true).performClick()
            compose.waitForTag("send")
            listOf("Params", "Headers", "Body", "Auth", "Scripts", "Tests").forEach { compose.onNodeWithTag("editor-tab-$it").performScrollTo().performClick() }
            compose.onNodeWithTag("send").performClick()
            compose.waitFor(hasText("201 Created"))
            listOf("Body", "Headers", "Tests", "Cookies").forEach { compose.onNodeWithTag("response-tab-$it").performClick() }
            compose.onNodeWithTag("close-response").performClick()
            compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("response-sheet")).fetchSemanticsNodes().isEmpty() }
            compose.onNodeWithTag("back").performClick()
            listOf("History", "Environments", "Runs", "Collections").forEach { compose.onNodeWithTag("tab-$it").performClick(); compose.waitForIdle() }
            compose.onNodeWithTag("account-button").performClick()
            compose.waitForTag("account")
            compose.onNodeWithTag("account").tryPerformAccessibilityChecks()
        }
    }
}
