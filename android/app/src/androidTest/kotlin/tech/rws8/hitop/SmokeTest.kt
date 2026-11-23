package tech.rws8.hitop

import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.assertTextEquals
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SmokeTest {
    @get:Rule
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun theEngineAnswersThroughJni() {
        compose.waitUntil(20_000) { compose.onAllNodes(hasText("Engine ready", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithTag("engine").assertTextEquals("Engine ready · script answer 42")
        Screenshots.take("00-a0-theme")
    }
}
