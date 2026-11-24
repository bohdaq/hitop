package tech.rws8.hitop

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.lifecycle.ViewModelProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.ui.AppViewModel

/** B2: every auth type the web app has can be set up. */
@RunWith(AndroidJUnit4::class)
class AuthFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @Test
    fun oauthSigV4AndDigestHaveForms() {
        Harness.launchSeeded().use { s ->
            compose.waitForTag("collections")
            s.onActivity { a -> ViewModelProvider(a)[AppViewModel::class.java].let { vm -> vm.openRequest(vm.library.state.value.find("1103")!!.first.id, "1103") } }
            compose.waitForTag("editor-tab-Auth")
            compose.onNodeWithTag("editor-tab-Auth").performScrollTo().performClick()
            compose.waitForTag("chip-OAuth 2.0")
            compose.onNodeWithTag("chip-OAuth 2.0").performScrollTo().performClick()
            compose.waitForTag("auth-tokenUrl")
            compose.onNodeWithTag("chip-Sign in (code + PKCE)").performScrollTo().performClick()
            compose.waitForTag("auth-authUrl")
            compose.waitForTag("oauth-forget")
            compose.screenshot("31-oauth")
            compose.onNodeWithTag("chip-AWS SigV4").performScrollTo().performClick()
            compose.waitForTag("auth-secretKey")
            compose.onNodeWithTag("chip-Digest").performScrollTo().performClick()
            compose.waitForTag("auth-password")
            s.onActivity { a -> assertEquals("digest", ViewModelProvider(a)[AppViewModel::class.java].draft("1103")!!.auth!!.type) }
        }
    }
}
