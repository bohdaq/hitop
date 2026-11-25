package tech.rws8.hitop

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextReplacement
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import tech.rws8.hitop.Harness.screenshot
import tech.rws8.hitop.Harness.waitFor
import tech.rws8.hitop.Harness.waitForTag

/** A5: WebSocket, Server-Sent Events, GraphQL schema + suggestions and gRPC on the request screen. */
@RunWith(AndroidJUnit4::class)
class ProtocolFlowTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private fun open(name: String) {
        compose.waitForTag("collections")
        compose.onNodeWithTag("collections").performScrollToNode(hasTestTag("request-$name"))
        compose.onNodeWithTag("request-$name", useUnmergedTree = true).performClick()
        compose.waitForTag("send")
    }

    @Test
    fun webSocketConnectSendSavedAndDisconnect() {
        Harness.launchSeeded().use {
            open("Live stock")
            compose.onNodeWithTag("editor-tab-Messages").performClick()
            compose.screenshot("18-websocket-request")
            compose.onNodeWithTag("send").performClick()
            compose.waitFor(hasText("Connected · stock.v2"))
            compose.onNodeWithTag("ws-saved-subscribe").performClick()
            // Sent and echoed: a line that arrives later would move the buttons under the next tap.
            compose.waitUntil(15_000) { compose.onAllNodes(hasText("{\"op\": \"subscribe\", \"symbol\": \"TEE-0193\"}")).fetchSemanticsNodes().size == 2 }
            compose.waitForIdle()
            compose.onNodeWithTag("ws-saved-binary").performClick()
            compose.waitFor(hasText("binary 2 bytes", substring = true))
            compose.screenshot("19-websocket-log")
            compose.onNodeWithTag("ws-disconnect").performClick()
            compose.waitFor(hasText("Not connected"))
        }
    }

    @Test
    fun eventStreamsLiveAndStop() {
        Harness.launchSeeded().use {
            open("Order feed")
            compose.onNodeWithTag("send").performClick()
            compose.waitFor(hasText("Ended · 3 events"), 20_000)
            compose.screenshot("20-sse-events")
            compose.onNodeWithTag("sse-events").performScrollToNode(hasText("order.shipped", substring = true))
        }
        Harness.launchSeeded().use {
            open("Stock ticks")
            compose.onNodeWithTag("send").performClick()
            // At least two ticks while live: the stub ticks every 0.4 s, so the count can pass 2 between polls.
            compose.waitFor(androidx.compose.ui.test.SemanticsMatcher("live with 2+ events") { node ->
                node.config.getOrElseNullable(androidx.compose.ui.semantics.SemanticsProperties.Text) { null }.orEmpty().any { t ->
                    Regex("""Live · (\d+) events""").find(t.text)?.groupValues?.get(1)?.toInt()?.let { it >= 2 } == true
                }
            }, 20_000)
            compose.screenshot("21-sse-live")
            compose.onNodeWithTag("sse-stop").performClick()
            compose.waitFor(hasText("Stopped ·", substring = true), 20_000)
        }
    }

    @Test
    fun graphqlSchemaAndSuggestions() {
        Harness.launchSeeded().use {
            open("Catalog query")
            compose.onNodeWithTag("editor-tab-Body").performClick()
            compose.onNodeWithTag("load-schema").performClick()
            compose.waitForTag("schema-loaded", 20_000)
            compose.onNodeWithTag("GraphQL query").performTextReplacement("query Catalog(\$first: Int) {\n  products(first: \$first) {\n    ")
            compose.waitForTag("suggest-stock")
            compose.screenshot("22-graphql-suggestions")
            compose.onNodeWithTag("suggest-stock").performClick()
            compose.waitFor(hasText("    stock", substring = true))
        }
    }

    @Test
    fun grpcFieldsAndAnUnreachableServer() {
        Harness.launchSeeded().use {
            open("Say hello")
            compose.waitForTag("grpc-load")
            compose.screenshot("23-grpc-request")
            compose.onNodeWithTag("send").performClick()
            compose.waitForTag("grpc-call-error", 30_000)
        }
    }
}
