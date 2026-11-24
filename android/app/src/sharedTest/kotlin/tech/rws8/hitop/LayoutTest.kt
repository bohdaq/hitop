package tech.rws8.hitop

import androidx.compose.ui.test.DeviceConfigurationOverride
import androidx.compose.ui.test.FontScale
import androidx.compose.ui.test.ForcedSize
import androidx.compose.ui.test.then
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performKeyInput
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.pressKey
import androidx.compose.ui.test.withKeyDown
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.core.MemorySecretStore
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.ui.AppViewModel
import tech.rws8.hitop.ui.HitopRoot
import tech.rws8.hitop.ui.theme.HitopTheme

/** A6: phone, foldable and tablet widths; large text; keyboard shortcuts. */
@RunWith(AndroidJUnit4::class)
class LayoutTest {
    @get:Rule
    val compose = createComposeRule()

    private fun show(size: DpSize, fontScale: Float = 1f) {
        val ctx = ApplicationProvider.getApplicationContext<android.content.Context>()
        AppGraph.useSeed(ctx, if (Harness.robolectric) MemorySecretStore() else null)
        AppGraph.runsInProcess = Harness.robolectric
        val p = AppGraph.parts(ctx)
        val vm = AppViewModel(p.library, p.cloud, p.sync, p.shared, p.signIn, p.runs, p.settings, p.network, p.assistant)
        compose.setContent {
            DeviceConfigurationOverride(DeviceConfigurationOverride.ForcedSize(size) then DeviceConfigurationOverride.FontScale(fontScale)) {
                HitopTheme { HitopRoot(vm) }
            }
        }
    }

    private fun openCreateOrder() {
        compose.waitForTag("collections")
        compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("request-Create order"))
        compose.onNodeWithTag("request-Create order", useUnmergedTree = true).performClick()
        compose.waitForTag("send")
    }

    @Test
    fun tabletShowsThreePanesAndCtrlEnterSends() {
        show(DpSize(1280.dp, 800.dp))
        compose.waitForTag("wide-THREE")
        openCreateOrder()
        compose.onNodeWithTag("root").performKeyInput { withKeyDown(Key.CtrlLeft) { pressKey(Key.Enter) } }
        compose.waitFor(hasText("201 Created"))
        compose.onNodeWithTag("pane-response").assertExistsSafely()
        compose.screenshot("27-tablet")
    }

    @Test
    fun foldableShowsListAndRequest() {
        show(DpSize(700.dp, 900.dp))
        compose.waitForTag("wide-TWO")
        openCreateOrder()
        compose.onNodeWithTag("send").performClick()
        compose.waitFor(hasText("201 Created"))
        compose.screenshot("28-foldable")
    }

    @Test
    fun phoneAtTwiceTheTextSize() {
        show(DpSize(412.dp, 915.dp), fontScale = 2f)
        compose.waitForTag("collections")
        compose.waitForTag("tab-Collections")
        openCreateOrder()
        compose.onNodeWithTag("send").performClick()
        compose.waitFor(hasText("201 Created"))
        compose.screenshot("29-large-text")
    }

    private fun androidx.compose.ui.test.SemanticsNodeInteraction.assertExistsSafely() = assertExists()
}
