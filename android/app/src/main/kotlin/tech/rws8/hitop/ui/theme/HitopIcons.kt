package tech.rws8.hitop.ui.theme

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

/** The drafts' icons (24-unit stroke paths, the web app's set), tinted by `Icon`. */
object HitopIcons {
    private fun icon(name: String, vararg paths: String, filled: Boolean = false): ImageVector =
        ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f).apply {
            paths.forEach { d ->
                addPath(
                    pathData = addPathNodes(d),
                    fill = if (filled) SolidColor(Color.Black) else null,
                    stroke = if (filled) null else SolidColor(Color.Black),
                    strokeLineWidth = 2f,
                    strokeLineCap = StrokeCap.Round,
                    strokeLineJoin = StrokeJoin.Round,
                )
            }
        }.build()

    val Search = icon("search", "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z", "M20 20l-3.5-3.5")
    val Folder = icon("folder", "M3 7h6l2 2h10v10H3z")
    val History = icon("history", "M12 7v5l3 2M3 12a9 9 0 1 0 3-6.7M3 4v4h4")
    val Environments = icon("environments", "M4 6h16M4 12h10M4 18h6")
    val Run = icon("run", "M7 4v16l13-8z", filled = true)
    val Plus = icon("plus", "M12 5v14M5 12h14")
    val Back = icon("back", "M15 5l-7 7 7 7")
    val Save = icon("save", "M5 4h11l3 3v13H5z", "M8 4v5h7V4M8 20v-6h8v6")
    val More = icon("more", "M12 3.2a1.8 1.8 0 1 0 0 3.6a1.8 1.8 0 1 0 0-3.6z", "M12 10.2a1.8 1.8 0 1 0 0 3.6a1.8 1.8 0 1 0 0-3.6z", "M12 17.2a1.8 1.8 0 1 0 0 3.6a1.8 1.8 0 1 0 0-3.6z", filled = true)
    val ChevronDown = icon("chevron-down", "M6 9l6 6 6-6")
    val ChevronRight = icon("chevron-right", "M9 6l6 6-6 6")
    val Sparkle = icon("sparkle", "M12 3l2.5 5.5L20 11l-5.5 2.5L12 19l-2.5-5.5L4 11l5.5-2.5z")
    val Send = icon("send", "M3 20.5L21 12 3 3.5v6.5l12 2-12 2z", filled = true)
    val Close = icon("close", "M6 6l12 12M18 6L6 18")
    val People = icon("people", "M9 5a3 3 0 1 0 0 6a3 3 0 1 0 0-6z", "M3 19c0-3 3-5 6-5s6 2 6 5M15 14c3 0 6 1.5 6 4", "M17 6.5a2.5 2.5 0 1 0 0 5a2.5 2.5 0 1 0 0-5z")
    val Refresh = icon("refresh", "M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5")
    val Trash = icon("trash", "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13")
    val Copy = icon("copy", "M8 8h12v12H8z", "M4 16V4h12")
    val Share = icon("share", "M12 3v12M7 8l5-5 5 5M5 14v6h14v-6")
    val Pin = icon("pin", "M9 4h6l-1 6 4 4H6l4-4z", "M12 14v6")
    val Eye = icon("eye", "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z", "M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z")
    val Check = icon("check", "M5 12l5 5 9-10")
    val Code = icon("code", "M8 7l-5 5 5 5M16 7l5 5-5 5")
}
