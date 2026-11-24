package tech.rws8.hitop

import android.content.Context
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.platform.CrashReport

/** B7: a crash report stays on the phone and holds no messages. */
@RunWith(AndroidJUnit4::class)
class CrashReportTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    @Test
    fun theReportHasNamesOnlyAndIsShownOnce() {
        val text = CrashReport.text(IllegalStateException("https://secret.test?token=abc", RuntimeException("tok_s3cret")))
        assertTrue(text.contains("java.lang.IllegalStateException") && text.contains("Caused by java.lang.RuntimeException") && text.contains("CrashReportTest"))
        assertFalse(text.contains("secret.test") || text.contains("tok_s3cret"))
        Harness.launchSeeded().close()
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        java.io.File(ctx.filesDir, "crash-report.txt").writeText(text)
        ActivityScenario.launch(MainActivity::class.java).use {
            compose.waitForTag("crash-text")
            compose.onNodeWithTag("crash-dismiss").performClick()
            compose.waitUntil(20_000) { compose.onAllNodes(hasTestTag("crash-text")).fetchSemanticsNodes().isEmpty() }
            assertNull(CrashReport.pending(ctx))
        }
    }
}
