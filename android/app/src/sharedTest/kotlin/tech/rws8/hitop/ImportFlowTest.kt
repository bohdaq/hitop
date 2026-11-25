package tech.rws8.hitop

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.hasTestTag
import androidx.lifecycle.ViewModelProvider
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.platform.Incoming
import tech.rws8.hitop.ui.AppViewModel

/** B1: Share to HITOP, import and export. */
@RunWith(AndroidJUnit4::class)
class ImportFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private fun share(text: String, block: (ActivityScenario<MainActivity>) -> Unit) {
        Harness.launchSeeded().close()
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        ActivityScenario.launch<MainActivity>(Intent(ctx, MainActivity::class.java).setAction(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)).use(block)
    }

    private fun ActivityScenario<MainActivity>.vm(block: (AppViewModel) -> Unit) = onActivity { block(ViewModelProvider(it)[AppViewModel::class.java]) }

    @Test
    fun aSharedAddressBecomesARequestThatIsNotSent() = share("https://shared.shop.example/items?x=1") { s ->
        compose.waitForTag("send")
        s.vm { vm ->
            val r = vm.top as tech.rws8.hitop.ui.Screen.Request
            assertEquals("https://shared.shop.example/items?x=1", vm.draft(r.requestId)!!.url)
            assertTrue(vm.responses.isEmpty() && vm.sending.isEmpty())
        }
    }

    @Test
    fun aSharedCurlCommandOpensAsARequestWhenAsked() = share("curl -X POST https://shared.shop.example/c -d 'a=1'") { s ->
        compose.waitForTag("from-text-make")
        compose.onNodeWithTag("from-text-make").performClick()
        compose.waitForTag("send")
        s.vm { vm -> assertEquals("POST", vm.draft((vm.top as tech.rws8.hitop.ui.Screen.Request).requestId)!!.method) }
    }

    @Test
    fun sharedJsonShowsWhatItHoldsThenImports() = share("""{"id":1,"name":"Shared one","requests":[{"id":2,"name":"R","url":"https://x.test","method":"GET","headers":[],"body":""}]}""") { s ->
        compose.waitForTag("import-plan")
        compose.waitFor(hasText("Shared one · 1 request"))
        compose.screenshot("30-import")
        compose.onNodeWithTag("import-apply").performClick()
        compose.waitUntil(20_000) { compose.onAllNodes(hasTestTag("import-plan")).fetchSemanticsNodes().isEmpty() }
        s.vm { vm -> assertEquals(1, vm.library.state.value.collections.count { it.name == "Shared one" }) }
    }

    @Test
    fun importOpensFromTheListAndExportOffersThreeFormats() {
        Harness.launchSeeded().use {
            compose.waitForTag("collections")
            compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("import"))
            compose.onNodeWithTag("import").performClick()
            compose.waitForTag("import-file")
            it.vm { vm -> vm.importing = false }
            compose.waitUntil(20_000) { compose.onAllNodes(hasTestTag("import-file")).fetchSemanticsNodes().isEmpty() }
            compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("collection-menu-shop-api"))
            compose.onNodeWithTag("collection-menu-shop-api").performClick()
            compose.waitForTag("export-shop-api")
            compose.onNodeWithTag("export-shop-api").performClick()
            compose.waitForTag("export-zip")
            compose.waitForTag("export-hitop")
            compose.waitForTag("export-postman")
        }
    }

    @Test
    fun onlyOtherAppsContentAddressesAreRead() {
        val ctx = ApplicationProvider.getApplicationContext<Context>()
        assertFalse(Incoming.allowed(ctx, Uri.parse("file:///data/data/${ctx.packageName}/files/secrets.json")))
        assertFalse(Incoming.allowed(ctx, Uri.parse("content://${ctx.packageName}.files/reports/x")))
        assertFalse(Incoming.allowed(ctx, Uri.parse("content://0@${ctx.packageName}.files/reports/x")))
        assertTrue(Incoming.allowed(ctx, Uri.parse("content://com.android.providers.downloads.documents/document/1")))
    }
}
