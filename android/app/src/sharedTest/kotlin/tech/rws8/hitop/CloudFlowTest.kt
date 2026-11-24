package tech.rws8.hitop

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.Harness.waitForText
import tech.rws8.hitop.platform.AppGraph

/** A2: HITOP Cloud sign-in, the Account screen and library sync, against the in-app fake cloud. */
@RunWith(AndroidJUnit4::class)
class CloudFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private fun signIn(kind: String) {
        compose.waitForTag("account-button")
        compose.onNodeWithTag("account-button").performClick()
        compose.waitForTag("sign-in")
        compose.screenshot("06-account-signed-out")
        compose.onNodeWithTag("sign-in").performClick()
        compose.waitForTag("continue-$kind")
        compose.screenshot("07-sign-in")
        compose.onNodeWithTag("continue-$kind").performClick()
    }

    private fun noPurchaseWords(where: String) {
        Harness.PURCHASE_WORDS.forEach { w ->
            assertTrue("$w on $where", compose.onAllNodesWithText(w, substring = true).fetchSemanticsNodes().isEmpty())
        }
    }

    @Test
    fun signInWithGitHubSyncsTheLibrary() {
        Harness.launchSeeded().use {
            signIn("github")
            compose.waitForTag("account-email")
            compose.onNodeWithText("ada@example.com").assertExists()
            compose.waitForText("HITOP on Pixel (this phone)")
            compose.waitForTag("usage")
            compose.screenshot("08-account")
            noPurchaseWords("account")
            compose.onNodeWithTag("back").performClick()
            val fake = TestGraph.fakeCloud!!
            compose.waitUntil(20_000) { fake.items.containsKey("collection/1001") && fake.items.containsKey("environments/all") }
            compose.waitFor(hasText("13 requests · synced", substring = true))
            compose.onNodeWithText("Personal · synced").assertExists()
            compose.screenshot("09-synced-collections")
            // The environments' secret values travelled apart from their data.
            assertTrue(!fake.items.getValue("environments/all").data.toString().contains("tok_staging_8f2"))
            assertTrue(fake.items.getValue("environments/all").secrets.toString().contains("tok_staging_8f2"))
        }
    }

    @Test
    fun googleSignsInWithTheNativeAccountPicker() {
        Harness.launchSeeded().use {
            signIn("google")
            compose.waitForTag("account-email")
            assertTrue(TestGraph.fakeCloud!!.calls.contains("POST /v1/auth/google/native/finish"))
        }
    }

    @Test
    fun aConflictKeepsTheirsAndACopyOfMine() {
        Harness.launchSeeded().use {
            signIn("github")
            compose.waitForTag("account-email")
            compose.onNodeWithTag("back").performClick()
            val fake = TestGraph.fakeCloud!!
            compose.waitUntil(20_000) { fake.items.containsKey("collection/1002") }
            fake.conflictOnNextCollectionWrite = true
            AppGraph.library(androidx.test.core.app.ApplicationProvider.getApplicationContext()).renameCollection("1002", "webhooks v2")
            compose.waitForTag("notice", 30_000)
            compose.onNodeWithText("“webhooks v2” changed on another device. Kept theirs and saved yours as “webhooks v2 (this phone)”.").assertExists()
            compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("collection-webhooks v2 (this phone)"))
            compose.screenshot("10-conflict-copy")
        }
    }

    @Test
    fun theDeviceLimitAsksToRemoveOne() {
        Harness.launchSeeded().use {
            val fake = TestGraph.fakeCloud!!
            fake.deviceLimit = 1
            fake.addDevice("Old phone")
            signIn("github")
            compose.waitForTag("device-limit")
            compose.onNodeWithText("This account allows 1 devices.").assertExists()
            compose.screenshot("11-device-limit")
            noPurchaseWords("device limit")
            compose.onNodeWithText("Remove").performClick()
            compose.waitUntil(10_000) { compose.onAllNodesWithText("Old phone").fetchSemanticsNodes().isEmpty() }
            compose.onNodeWithTag("limit-sign-in").performClick()
            compose.waitForTag("account-email")
        }
    }

    @Test
    fun deletingTheAccountSignsOutAndKeepsTheCollections() {
        Harness.launchSeeded().use {
            signIn("github")
            compose.waitForTag("account-email")
            compose.onNodeWithTag("account").performScrollToNode(hasTestTag("delete-account"))
            compose.onNodeWithTag("delete-account").performClick()
            compose.onNodeWithTag("confirm-delete-account").performClick()
            compose.waitForTag("sign-in")
            assertTrue(TestGraph.fakeCloud!!.userDeleted)
            compose.onNodeWithTag("back").performClick()
            compose.waitForTag("collection-shop-api")
            compose.onNodeWithText("On this phone").assertExists()
        }
    }
}
