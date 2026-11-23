package tech.rws8.hitop.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.rws8.hitop.R

/**
 * HITOP dark, as drafted (android/design, owner approved 2026-11-22): dark only, fixed
 * brand colours, never the wallpaper's. Every colour the screens use is a token here.
 */
@Immutable
object Hitop {
    // Grounds and surfaces
    val ground = Color(0xFF0B0B0E)
    val surface = Color(0xFF15151B)
    val surfaceHigh = Color(0xFF1C1C24)
    val code = Color(0xFF111117)
    val codeDeep = Color(0xFF0F0F14)
    val pane = Color(0xFF101016)
    val border = Color(0xFF2C2A38)
    val divider = Color(0xFF22212C)
    val dividerSoft = Color(0xFF1E1D27)

    // Text (each checked against ground and surface for WCAG AA in ContrastTest)
    val text = Color(0xFFECEAF4)
    val textSecondary = Color(0xFFC9C6D6)
    val textMuted = Color(0xFF9C98AE)
    val textFaint = Color(0xFF8A869C)

    // Accent
    val accent = Color(0xFFA99CFF)
    val onAccent = Color(0xFF1A1530)
    val accentContainer = Color(0xFF2A2540)
    val selected = Color(0xFF3A3358)
    val onSelected = Color(0xFFE4DEFF)
    val teal = Color(0xFF6EE7C8)
    val tealContainer = Color(0xFF1F2A2A)
    val logoStart = Color(0xFF667EEA)
    val logoEnd = Color(0xFF764BA2)
    val logo = Brush.linearGradient(listOf(logoStart, logoEnd))

    // States
    val success = Color(0xFF4ADE80)
    val successContainer = Color(0xFF12301F)
    val error = Color(0xFFF87171)
    val errorContainer = Color(0xFF2B1618)
    val errorText = Color(0xFFFCA5A5)
    val warning = Color(0xFFF59E8B)
    val insight = Color(0xFF1F1C2E)
    val onInsight = Color(0xFFDCD8EA)

    // JSON highlighting
    val jsonKey = Color(0xFF8AB4FF)
    val jsonString = Color(0xFFFDE68A)
    val jsonNumber = Color(0xFFF9A8D4)
    val jsonMuted = Color(0xFF8A869C)

    // Timing bar
    val timingDns = Color(0xFF7DD3FC)
    val timingTls = Color(0xFFC4B5FD)
    val timingWait = Color(0xFFFDBA74)
    val timingDownload = Color(0xFF4ADE80)
}

/** Method colours from the drafts; the container is the method pill's ground (POST: #3A2E12). */
object MethodColors {
    fun of(method: String): Color = when (method.uppercase()) {
        "GET" -> Color(0xFF4ADE80)
        "POST" -> Color(0xFFFBBF24)
        "PUT" -> Color(0xFF60A5FA)
        "PATCH" -> Color(0xFFC084FC)
        "DELETE" -> Color(0xFFF87171)
        "WS" -> Color(0xFF7CC7FF)
        "SSE" -> Color(0xFFF5D76E)
        "GQL" -> Color(0xFFFF8AD8)
        "GRPC" -> Color(0xFF6EE7C8)
        else -> Hitop.accent
    }

    fun container(method: String): Color = of(method).copy(alpha = 0.16f).compositeOver(Hitop.ground)
}

private fun Color.compositeOver(bg: Color): Color {
    val a = alpha
    return Color(red * a + bg.red * (1 - a), green * a + bg.green * (1 - a), blue * a + bg.blue * (1 - a), 1f)
}

val Geist = FontFamily(
    Font(R.font.geist_regular, FontWeight.Normal),
    Font(R.font.geist_medium, FontWeight.Medium),
    Font(R.font.geist_semibold, FontWeight.SemiBold),
    Font(R.font.geist_bold, FontWeight.Bold),
)

val GeistMono = FontFamily(
    Font(R.font.geist_mono_regular, FontWeight.Normal),
    Font(R.font.geist_mono_medium, FontWeight.Medium),
    Font(R.font.geist_mono_semibold, FontWeight.SemiBold),
)

/** Type styles of the drafts (sizes in sp, so they follow the system font scale). */
object HitopType {
    val screenTitle = TextStyle(fontFamily = Geist, fontWeight = FontWeight.SemiBold, fontSize = 22.sp, lineHeight = 28.sp)
    val title = TextStyle(fontFamily = Geist, fontWeight = FontWeight.SemiBold, fontSize = 18.sp, lineHeight = 24.sp)
    val cardTitle = TextStyle(fontFamily = Geist, fontWeight = FontWeight.SemiBold, fontSize = 16.sp, lineHeight = 22.sp)
    val body = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Normal, fontSize = 15.sp, lineHeight = 21.sp)
    val bodySmall = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Normal, fontSize = 14.sp, lineHeight = 20.sp)
    val caption = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Normal, fontSize = 13.sp, lineHeight = 18.sp)
    val label = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Medium, fontSize = 13.sp, lineHeight = 18.sp)
    val navLabel = TextStyle(fontFamily = Geist, fontWeight = FontWeight.Medium, fontSize = 12.sp, lineHeight = 16.sp)
    val method = TextStyle(fontFamily = GeistMono, fontWeight = FontWeight.SemiBold, fontSize = 12.sp, lineHeight = 16.sp)
    val mono = TextStyle(fontFamily = GeistMono, fontWeight = FontWeight.Normal, fontSize = 14.sp, lineHeight = 22.sp)
    val monoSmall = TextStyle(fontFamily = GeistMono, fontWeight = FontWeight.Medium, fontSize = 12.sp, lineHeight = 16.sp)
    val button = TextStyle(fontFamily = Geist, fontWeight = FontWeight.SemiBold, fontSize = 16.sp, lineHeight = 22.sp)
}

private val colors = darkColorScheme(
    primary = Hitop.accent,
    onPrimary = Hitop.onAccent,
    primaryContainer = Hitop.accentContainer,
    onPrimaryContainer = Hitop.accent,
    secondary = Hitop.teal,
    onSecondary = Hitop.ground,
    secondaryContainer = Hitop.selected,
    onSecondaryContainer = Hitop.onSelected,
    background = Hitop.ground,
    onBackground = Hitop.text,
    surface = Hitop.ground,
    onSurface = Hitop.text,
    surfaceVariant = Hitop.surfaceHigh,
    onSurfaceVariant = Hitop.textMuted,
    surfaceContainerLowest = Hitop.ground,
    surfaceContainerLow = Hitop.surface,
    surfaceContainer = Hitop.surface,
    surfaceContainerHigh = Hitop.surfaceHigh,
    surfaceContainerHighest = Hitop.surfaceHigh,
    outline = Hitop.border,
    outlineVariant = Hitop.divider,
    error = Hitop.error,
    onError = Hitop.ground,
    errorContainer = Hitop.errorContainer,
    onErrorContainer = Hitop.errorText,
)

private val typography = Typography(
    headlineSmall = HitopType.screenTitle,
    titleLarge = HitopType.screenTitle,
    titleMedium = HitopType.title,
    titleSmall = HitopType.cardTitle,
    bodyLarge = HitopType.body,
    bodyMedium = HitopType.bodySmall,
    bodySmall = HitopType.caption,
    labelLarge = HitopType.label,
    labelMedium = HitopType.navLabel,
    labelSmall = HitopType.navLabel,
)

private val shapes = Shapes(
    extraSmall = RoundedCornerShape(8.dp),
    small = RoundedCornerShape(12.dp),
    medium = RoundedCornerShape(16.dp),
    large = RoundedCornerShape(24.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

@Composable
fun HitopTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = colors, typography = typography, shapes = shapes, content = content)
}
