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
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.sync.SyncCore

/** A3: sharing a collection end-to-end encrypted (against the fake cloud's workspaces), people, invite, view only. */
@RunWith(AndroidJUnit4::class)
class SharedFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private fun signIn() {
        compose.waitForTag("account-button")
        compose.onNodeWithTag("account-button").performClick()
        compose.waitForTag("sign-in")
        compose.onNodeWithTag("sign-in").performClick()
        compose.waitForTag("continue-github")
        compose.onNodeWithTag("continue-github").performClick()
        compose.waitForTag("account-email")
        compose.waitForTag("fingerprint")
        compose.onNodeWithTag("back").performClick()
    }

    @Test
    fun shareInviteAndViewOnly() {
        Harness.launchSeeded().use {
            signIn()
            compose.waitForTag("collections")
            compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("collection-menu-webhooks"))
            compose.onNodeWithTag("collection-menu-webhooks").performClick()
            compose.onNodeWithTag("share-webhooks").performClick()
            compose.waitForTag("share-now")
            compose.onNodeWithTag("share-now").performClick()
            val fake = TestGraph.fakeCloud!!
            compose.waitUntil(20_000) { fake.workspaces.values.any { it.blobs.size >= 7 } }
            compose.waitForTag("my-fingerprint")
            compose.waitFor(hasTestTag("member-${tech.rws8.hitop.cloud.FakeCloud.EMAIL}"))
            compose.screenshot("12-share-people")
            compose.onNodeWithTag("invite").performClick()
            compose.waitForTag("invite-code")
            val parts = AppGraph.parts(ApplicationProvider.getApplicationContext())
            val fp = SyncCore.normalize(parts.shared.fingerprint!!)
            compose.waitFor(hasText(".$fp", substring = true))
            // The server never saw a request: every blob is sealed.
            val ws = fake.workspaces.values.first()
            assertTrue(ws.blobs.values.none { it.third.orEmpty().contains("Register endpoint") })
            compose.screenshot("13-invite-code")
            // The workspace makes this account a viewer: the collection turns view only.
            ws.viewerRole = true
            runBlocking { parts.shared.refreshWorkspaces() }
            assertTrue(parts.library.isReadOnly("1002"))
            compose.waitFor(hasText("Shared · View only", substring = true))
            // A viewer's edits don't land.
            parts.library.renameCollection("1002", "renamed by a viewer")
            assertTrue(parts.library.state.value.collection("1002")!!.name == "webhooks")
        }
    }
}
