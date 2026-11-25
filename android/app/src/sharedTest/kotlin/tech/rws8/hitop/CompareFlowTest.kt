package tech.rws8.hitop

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.ui.AppViewModel

/** B4: compare responses, saved examples, undo. */
@RunWith(AndroidJUnit4::class)
class CompareFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private fun ActivityScenario<MainActivity>.vm(block: (AppViewModel) -> Unit) = onActivity { block(ViewModelProvider(it)[AppViewModel::class.java]) }

    @Test
    fun compareWithAnExampleThenDeleteIt() {
        Harness.launchSeeded().use { s ->
            compose.waitForTag("collections")
            s.vm { vm ->
                vm.openRequest(vm.library.state.value.find("1103")!!.first.id, "1103")
                vm.edit("1103") { it.set("examples", JsonArray(listOf(buildJsonObject { put("name", "Before"); put("status", 200); put("body", "[]") }))) }
            }
            compose.waitForTag("send")
            compose.onNodeWithTag("send").performClick()
            compose.waitForTag("response-Compare")
            compose.onNodeWithTag("response-Compare").performClick()
            compose.waitForTag("diff")
            compose.waitFor(hasText("$[0]", substring = true))
            compose.screenshot("33-compare")
            s.vm { it.sheetFor = null }
            compose.waitUntil(20_000) { compose.onAllNodes(hasTestTag("response-sheet")).fetchSemanticsNodes().isEmpty() && compose.onAllNodes(hasTestTag("diff")).fetchSemanticsNodes().isEmpty() }
            compose.onNodeWithTag("request-more").performClick()
            compose.waitForTag("examples")
            compose.onNodeWithTag("examples").performClick()
            compose.waitForTag("delete-example-0")
            compose.onNodeWithTag("delete-example-0").performClick()
            compose.waitFor(hasText("No examples. Save one from a response."))
        }
    }

    @Test
    fun aDeletedRequestComesBackWithUndo() {
        Harness.launchSeeded().use { s ->
            compose.waitForTag("collections")
            s.vm { vm -> vm.undoable("Deleted Create order") { vm.library.deleteRequest("1104") } }
            compose.waitForTag("undo")
            s.vm { assertNull(it.library.state.value.find("1104")) }
            compose.onNodeWithTag("undo").performClick()
            compose.waitUntil(20_000) { compose.onAllNodes(hasTestTag("undo-bar")).fetchSemanticsNodes().isEmpty() }
            s.vm { vm ->
                assertNotNull(vm.library.state.value.find("1104"))
                assertEquals(null, vm.undo)
            }
        }
    }
}
