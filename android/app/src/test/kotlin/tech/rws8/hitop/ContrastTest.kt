package tech.rws8.hitop

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import org.junit.Assert.assertTrue
import org.junit.Test
import tech.rws8.hitop.ui.theme.Hitop
import tech.rws8.hitop.ui.theme.MethodColors

/** A6: every text colour on every ground it is drawn on meets WCAG AA (4.5:1 for normal text). */
class ContrastTest {
    private fun ratio(a: Color, b: Color): Double {
        val l1 = a.luminance() + 0.05
        val l2 = b.luminance() + 0.05
        return maxOf(l1, l2).toDouble() / minOf(l1, l2)
    }

    @Test
    fun textTokensMeetAA() {
        val grounds = mapOf("ground" to Hitop.ground, "surface" to Hitop.surface, "surfaceHigh" to Hitop.surfaceHigh, "code" to Hitop.code, "codeDeep" to Hitop.codeDeep, "pane" to Hitop.pane)
        val texts = mapOf(
            "text" to Hitop.text, "textSecondary" to Hitop.textSecondary, "textMuted" to Hitop.textMuted, "textFaint" to Hitop.textFaint, "accent" to Hitop.accent,
            "success" to Hitop.success, "error" to Hitop.error, "warning" to Hitop.warning, "jsonKey" to Hitop.jsonKey, "jsonString" to Hitop.jsonString, "jsonNumber" to Hitop.jsonNumber, "jsonMuted" to Hitop.jsonMuted,
        ) + listOf("GET", "POST", "PUT", "PATCH", "DELETE", "WS", "SSE", "GQL", "GRPC").associate { "method $it" to MethodColors.of(it) }
        val failures = ArrayList<String>()
        for ((tn, t) in texts) for ((gn, g) in grounds) ratio(t, g).let { if (it < 4.5) failures.add("$tn on $gn: ${"%.2f".format(it)}") }
        // Pills and chips: their own grounds.
        listOf(Triple("success pill", Hitop.success, Hitop.successContainer), Triple("error pill", Hitop.error, Hitop.errorContainer), Triple("selected chip", Hitop.onSelected, Hitop.selected),
            Triple("send button", Hitop.onAccent, Hitop.accent), Triple("insight", Hitop.onInsight, Hitop.insight), Triple("error text", Hitop.errorText, Hitop.errorContainer)) .forEach { (n, t, g) -> ratio(t, g).let { if (it < 4.5) failures.add("$n: ${"%.2f".format(it)}") } }
        listOf("GET", "POST", "PUT", "PATCH", "DELETE").forEach { m -> ratio(MethodColors.of(m), MethodColors.container(m)).let { if (it < 4.5) failures.add("method pill $m: ${"%.2f".format(it)}") } }
        assertTrue(failures.joinToString("\n"), failures.isEmpty())
    }
}
