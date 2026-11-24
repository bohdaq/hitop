package tech.rws8.hitop

import android.graphics.Bitmap
import android.graphics.Canvas
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isRoot
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.core.MemorySecretStore
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.ui.AppViewModel
import tech.rws8.hitop.ui.HitopRoot
import tech.rws8.hitop.ui.ResponseContent
import tech.rws8.hitop.ui.Screen
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.HitopTheme
import java.io.File

/**
 * The Play listing's screenshots (android/store/screenshots/<device>/), at sizes Play takes: phone
 * 1080×1920, 7-inch tablet 2880×1800, 10-inch tablet 3840×2400 (both sides 320–3840 px, the long
 * side at most twice the short one). Rendered by Robolectric from the seeded sample (no real data).
 * Also the feature graphic (1024×500). JPEG: Play takes no alpha channel in these.
 * Writes the files only with -Dhitop.storeScreenshots=<dir> (Gradle property hitop.storeScreenshots);
 * otherwise it checks the sizes.
 */
@RunWith(AndroidJUnit4::class)
class StoreScreenshots {
    @get:Rule
    val compose = createComposeRule()

    private val out = System.getProperty("hitop.storeScreenshots")?.takeIf { it.isNotBlank() }?.let(::File)

    /**
     * The app on the seeded sample. With [responseFullScreen] a response, once there, is drawn on its
     * own (the sheet's content, as the tablet's response pane): Robolectric can't capture a sheet's window.
     */
    private fun show(responseFullScreen: Boolean = false) {
        val ctx = ApplicationProvider.getApplicationContext<android.content.Context>()
        AppGraph.useSeed(ctx, MemorySecretStore())
        AppGraph.runsInProcess = true
        val p = AppGraph.parts(ctx)
        val vm = AppViewModel(p.library, p.cloud, p.sync, p.shared, p.signIn, p.runs, p.settings, p.network, p.assistant)
        compose.setContent {
            HitopTheme {
                val top = vm.top as? Screen.Request
                val res = top?.let { vm.responses[it.requestId] }
                if (responseFullScreen && top != null && res != null)
                    ResponseContent(vm, top.collectionId, top.requestId, res, close = {}, modifier = Modifier.fillMaxSize().background(Hitop.surface).padding(top = 24.dp))
                else HitopRoot(vm)
            }
        }
    }

    /** Every window (the screen, then sheets and dialogs over it) drawn into one picture. */
    private fun ComposeContentTestRule.shot(device: String, name: String, width: Int, height: Int) {
        waitForIdle()
        val roots = onAllNodes(isRoot())
        val images = (0 until roots.fetchSemanticsNodes().size).map { roots[it].captureToImage().asAndroidBitmap() }
        val base = images.first().copy(Bitmap.Config.ARGB_8888, true)
        val canvas = Canvas(base)
        images.drop(1).forEach { canvas.drawBitmap(it, 0f, (base.height - it.height).toFloat().coerceAtLeast(0f), null) }
        assertEquals("$device/$name width", width, base.width)
        assertEquals("$device/$name height", height, base.height)
        out?.let { dir -> File(dir, device).apply { mkdirs() }.resolve("$name.jpg").outputStream().use { base.compress(Bitmap.CompressFormat.JPEG, 95, it) } }
    }

    private fun openCreateOrder() {
        compose.waitForTag("collections")
        compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("request-Create order"))
        compose.onNodeWithTag("request-Create order", useUnmergedTree = true).performClick()
        compose.waitForTag("send")
    }

    @Test
    @Config(qualifiers = "w432dp-h768dp-400dpi")
    fun phone() {
        show()
        compose.waitForTag("collections")
        compose.shot("phone", "1-collections", 1080, 1920)
        openCreateOrder()
        compose.shot("phone", "2-request", 1080, 1920)
        compose.onNodeWithTag("back").performClick()
        compose.waitForTag("run-button-shop-api")
        compose.onNodeWithTag("run-button-shop-api").performClick()
        compose.waitForTag("start-run")
        compose.onNodeWithTag("start-run").performClick()
        compose.waitForTag("open-report", 60_000)
        compose.waitForTag("failure-card")
        compose.shot("phone", "4-run", 1080, 1920)
        compose.onNodeWithTag("back").performClick()
        compose.onNodeWithTag("tab-Environments").performClick()
        compose.waitForIdle()
        compose.shot("phone", "5-environments", 1080, 1920)
    }

    @Test
    @Config(qualifiers = "w432dp-h768dp-400dpi")
    fun phoneResponse() {
        show(responseFullScreen = true)
        openCreateOrder()
        compose.onNodeWithTag("send").performClick()
        compose.waitFor(hasText("201 Created"))
        compose.shot("phone", "3-response", 1080, 1920)
    }

    private fun tablet(device: String, width: Int, height: Int, layout: String) {
        show(responseFullScreen = false)
        compose.waitForTag(layout)
        openCreateOrder()
        if (layout == "wide-THREE") {
            compose.onNodeWithTag("send").performClick()
            compose.waitFor(hasText("201 Created"))
        }
        compose.shot(device, "1-request", width, height)
        if (layout != "wide-THREE") {
            compose.onNodeWithTag("send").performClick()
            compose.waitFor(hasText("201 Created"))
            compose.onNodeWithTag("close-response").performClick()
            compose.waitUntil(10_000) { compose.onAllNodes(hasTestTag("response-sheet")).fetchSemanticsNodes().isEmpty() }
        }
        compose.onNodeWithTag("rail-History").performClick()
        compose.waitForIdle()
        compose.shot(device, "2-history", width, height)
    }

    @Test
    @Config(qualifiers = "w1024dp-h500dp-land-mdpi")
    fun featureGraphic() {
        // The brand logo (brand/png/logo-512.png, rendered from brand/logo.svg); the test runs in android/app.
        val logo = android.graphics.BitmapFactory.decodeFile(File("../../brand/png/logo-512.png").absolutePath).asImageBitmap()
        compose.setContent {
            HitopTheme {
                androidx.compose.foundation.layout.Row(
                    Modifier.fillMaxSize().background(Hitop.ground).padding(horizontal = 72.dp),
                    verticalAlignment = androidx.compose.ui.Alignment.CenterVertically,
                    horizontalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(40.dp),
                ) {
                    androidx.compose.foundation.Image(logo, null, Modifier.size(200.dp))
                    androidx.compose.foundation.layout.Column(verticalArrangement = androidx.compose.foundation.layout.Arrangement.spacedBy(16.dp)) {
                        androidx.compose.material3.Text("HITOP", color = androidx.compose.ui.graphics.Color.White, style = tech.rws8.hitop.ui.theme.HitopType.screenTitle.copy(fontSize = androidx.compose.ui.unit.TextUnit(88f, androidx.compose.ui.unit.TextUnitType.Sp)))
                        androidx.compose.material3.Text("The API client your terminal, your CI and your AI agent can all drive.", color = Hitop.textSecondary, style = tech.rws8.hitop.ui.theme.HitopType.title.copy(fontSize = androidx.compose.ui.unit.TextUnit(30f, androidx.compose.ui.unit.TextUnitType.Sp), lineHeight = androidx.compose.ui.unit.TextUnit(38f, androidx.compose.ui.unit.TextUnitType.Sp)))
                    }
                }
            }
        }
        compose.shot(".", "feature-graphic", 1024, 500)
    }

    @Test
    @Config(qualifiers = "w960dp-h600dp-land-xxhdpi")
    fun tablet7() = tablet("tablet-7", 2880, 1800, "wide-TWO")

    @Test
    @Config(qualifiers = "w1280dp-h800dp-land-xxhdpi")
    fun tablet10() = tablet("tablet-10", 3840, 2400, "wide-THREE")
}
