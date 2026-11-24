package tech.rws8.hitop

import android.content.Context
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.core.AppSettings
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.ui.AppViewModel
import tech.rws8.hitop.ui.Screen

/** B3: Settings. */
@RunWith(AndroidJUnit4::class)
class SettingsFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @Test
    fun settingsAreKeptAndCookiesCanBeDeleted() {
        Harness.launchSeeded().use { s ->
            compose.waitForTag("collections")
            val parts = AppGraph.parts(ApplicationProvider.getApplicationContext<Context>())
            val (c, r) = parts.library.state.value.find("1104")!!
            runBlocking { parts.library.send(c.id, r) }
            assertEquals(1, parts.library.cookies().size)
            // Nothing that loosens the network is on unless turned on.
            assertFalse(parts.settings.value.trustUserCas || parts.settings.value.systemProxy)
            assertTrue(parts.network.args(AppSettings()).isEmpty())
            s.onActivity { a -> ViewModelProvider(a)[AppViewModel::class.java].push(Screen.Settings) }
            compose.waitForTag("settings-list")
            compose.onNodeWithTag("chip-60 s").performClick()
            compose.onNodeWithTag("settings-list").performScrollToNode(hasTestTag("trust-user-cas"))
            compose.onNodeWithTag("trust-user-cas").performClick()
            compose.onNodeWithTag("settings-list").performScrollToNode(hasTestTag("library-sync"))
            compose.onNodeWithTag("library-sync").performClick()
            compose.screenshot("32-settings")
            compose.onNodeWithTag("settings-list").performScrollToNode(hasTestTag("clear-cookies"))
            compose.onNodeWithTag("clear-cookies").performClick()
            compose.onNodeWithTag("settings-list").performScrollToNode(hasText("No cookies are stored."))
            compose.waitFor(hasText("No cookies are stored."))
            assertTrue(parts.library.cookies().isEmpty())
            val kept = tech.rws8.hitop.core.SettingsStore(tech.rws8.hitop.core.LibraryStore(java.io.File(ApplicationProvider.getApplicationContext<Context>().filesDir, "hitop-library"))).value
            assertEquals(AppSettings(timeoutMs = 60_000, trustUserCas = true, librarySync = false), kept)
            assertFalse(parts.sync.checkAccount())
            assertEquals(200, runBlocking { parts.library.send(c.id, parts.library.state.value.find("1103")!!.second) }.status)
        }
    }
}
