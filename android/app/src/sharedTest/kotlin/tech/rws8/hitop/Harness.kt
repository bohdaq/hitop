package tech.rws8.hitop

import android.content.Context
import android.graphics.Bitmap
import android.os.Build
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.isRoot
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.services.storage.TestStorage
import tech.rws8.hitop.core.MemorySecretStore
import tech.rws8.hitop.platform.AppGraph

/** Shared by the emulator (androidTest) and Robolectric (test) runs of the UI tests. */
object Harness {
    val robolectric: Boolean get() = Build.FINGERPRINT == "robolectric"

    /** The drafts' shop-api, every http:// request answered by the in-app stub, then the app. */
    fun launchSeeded(): ActivityScenario<MainActivity> {
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        TestGraph.useSeed(ctx, if (robolectric) MemorySecretStore() else null)
        AppGraph.runsInProcess = robolectric
        // The run notification's permission, granted as a person would on the first run: the system
        // dialog would otherwise take the taps meant for the app (Android 13+ devices).
        if (!robolectric && Build.VERSION.SDK_INT >= 33) {
            InstrumentationRegistry.getInstrumentation().uiAutomation.grantRuntimePermission(ctx.packageName, android.Manifest.permission.POST_NOTIFICATIONS)
        }
        return ActivityScenario.launch(MainActivity::class.java)
    }

    fun ComposeTestRule.waitFor(matcher: SemanticsMatcher, timeoutMs: Long = 20_000) {
        try {
            waitUntil(timeoutMs) { onAllNodes(matcher, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        } catch (e: androidx.compose.ui.test.ComposeTimeoutException) {
            // What was there instead: a timeout alone says nothing about the cause.
            val shown = runCatching {
                onAllNodes(androidx.compose.ui.test.SemanticsMatcher.keyIsDefined(androidx.compose.ui.semantics.SemanticsProperties.Text), useUnmergedTree = true).fetchSemanticsNodes()
                    .flatMap { n -> n.config[androidx.compose.ui.semantics.SemanticsProperties.Text].map { it.text } }.joinToString(" | ").take(1500)
            }.getOrElse { "unknown ($it)" }
            throw AssertionError("not shown after $timeoutMs ms: ${matcher.description}; on screen: $shown", e)
        }
    }

    fun ComposeTestRule.waitForTag(tag: String, timeoutMs: Long = 20_000) = waitFor(hasTestTag(tag), timeoutMs)
    fun ComposeTestRule.waitForText(text: String, timeoutMs: Long = 20_000) = waitFor(hasText(text, substring = true), timeoutMs)

    /** A screen capture into the test storage (CI artifact "screenshots-api*"); none under Robolectric. */
    fun ComposeTestRule.screenshot(name: String) {
        if (robolectric) return
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        waitForIdle()
        instrumentation.waitForIdleSync()
        // The system capture shows every window (sheets, dialogs, system bars); some emulator
        // images refuse it (API 37 with SwiftShader), then the top Compose window is captured.
        val bitmap = instrumentation.uiAutomation.takeScreenshot()
            ?: runCatching { onAllNodes(isRoot()).let { roots -> roots[roots.fetchSemanticsNodes().size - 1] }.captureToImage().asAndroidBitmap() }.getOrNull()
            ?: return
        TestStorage().openOutputFile("screenshots/api${Build.VERSION.SDK_INT}/$name.png").use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    /** Store mode: words no screen may show (nothing is sold in the app). */
    val PURCHASE_WORDS = listOf("Upgrade", "Buy", "Purchase", "Pricing", "per month", "/mo", "Subscribe", "Trial", "Checkout", "Billing", "€", "$9", "Pro plan")
}
