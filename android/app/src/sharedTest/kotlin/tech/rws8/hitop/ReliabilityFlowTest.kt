package tech.rws8.hitop

import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.ui.AppViewModel
import java.util.concurrent.CountDownLatch

/** B0: unsaved edits outlive the activity, and a send can be cancelled. */
@RunWith(AndroidJUnit4::class)
class ReliabilityFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private fun ActivityScenario<MainActivity>.vm(block: (AppViewModel) -> Unit) = onActivity { block(ViewModelProvider(it)[AppViewModel::class.java]) }

    @Test
    fun unsavedEditsAreThereInANewProcess() {
        Harness.launchSeeded().use { s ->
            compose.waitForTag("collections")
            s.vm { vm ->
                val (c, _) = vm.library.state.value.find("1103")!!
                vm.openRequest(c.id, "1103")
                vm.edit("1103") { it.withUrl("http://staging.shop.example/edited") }
            }
            compose.waitFor(hasText("unsaved", substring = true))
        }
        // A new view model over the same files.
        ActivityScenario.launch(MainActivity::class.java).use { s ->
            compose.waitForTag("collections")
            s.vm { vm ->
                assertEquals("http://staging.shop.example/edited", vm.draft("1103")!!.url)
                vm.discard("1103")
            }
        }
    }

    @Test
    fun sendCanBeCancelled() {
        Harness.launchSeeded().use { s ->
            compose.waitForTag("collections")
            val latch = CountDownLatch(1)
            AppGraph.stub!!.hold = latch
            try {
                s.vm { vm ->
                    val (c, _) = vm.library.state.value.find("1103")!!
                    vm.openRequest(c.id, "1103")
                    vm.edit("1103") { it.withUrl("http://staging.shop.example/hold") }
                }
                compose.waitForTag("send")
                compose.onNodeWithTag("send").performClick()
                compose.waitUntil(20_000) { AppGraph.stub!!.seen.any { it.url.endsWith("/hold") } }
                compose.waitFor(hasText("Cancel"))
                compose.onNodeWithTag("send").performClick()
                compose.waitFor(hasText("Send"))
                s.vm { assertNull(it.responses["1103"]) }
            } finally {
                latch.countDown()
            }
        }
    }
}
