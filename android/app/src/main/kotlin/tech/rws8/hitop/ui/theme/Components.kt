package tech.rws8.hitop.ui.theme

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** The method as a coloured mono label (rows) or pill (editor). */
@Composable
fun MethodTag(method: String, modifier: Modifier = Modifier, pill: Boolean = false) {
    val label = method.uppercase()
    if (pill) {
        Box(
            modifier.clip(RoundedCornerShape(12.dp)).background(MethodColors.container(label)).heightIn(min = 40.dp).padding(horizontal = 12.dp),
            contentAlignment = Alignment.Center,
        ) { Text(label, style = HitopType.method.copy(fontSize = 13.sp), color = MethodColors.of(label)) }
    } else {
        Text(label, modifier.widthIn(min = 56.dp), style = HitopType.method, color = MethodColors.of(label), maxLines = 1)
    }
}

fun statusColor(status: Int): Color = when {
    status == 0 -> Hitop.textMuted
    status < 400 -> Hitop.success
    else -> Hitop.error
}

/** "201 Created" on its state colour. */
@Composable
fun StatusPill(text: String, ok: Boolean, modifier: Modifier = Modifier) {
    Box(
        modifier.clip(RoundedCornerShape(8.dp)).background(if (ok) Hitop.successContainer else Hitop.errorContainer).heightIn(min = 32.dp).padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) { Text(text, style = HitopType.mono.copy(fontWeight = FontWeight.SemiBold), color = if (ok) Hitop.success else Hitop.error) }
}

/** The drafts' filter chips: selected = filled #3A3358, others outlined. */
@Composable
fun HitopChip(label: String, selected: Boolean, modifier: Modifier = Modifier, color: Color? = null) {
    val shape = RoundedCornerShape(8.dp)
    Box(
        modifier
            .clip(shape)
            .then(if (selected) Modifier.background(Hitop.selected) else Modifier.border(1.dp, Hitop.border, shape))
            .heightIn(min = 32.dp)
            .padding(horizontal = 14.dp),
        contentAlignment = Alignment.Center,
    ) { Text(label, style = HitopType.label, color = color ?: if (selected) Hitop.onSelected else Hitop.textSecondary) }
}

/** A dot in an environment's colour. */
@Composable
fun Dot(color: Color, size: Int = 8) {
    Box(Modifier.size(size.dp).clip(CircleShape).background(color))
}

/** The HITOP mark: the logo gradient disc with an H. */
@Composable
fun LogoMark(size: Int = 40, modifier: Modifier = Modifier) {
    Box(modifier.size(size.dp).clip(CircleShape).background(Hitop.logo), contentAlignment = Alignment.Center) {
        Text("H", color = Color.White, fontFamily = Geist, fontWeight = FontWeight.Bold, fontSize = (size * 0.38f).sp)
    }
}

@Composable
fun SpacedRow(modifier: Modifier = Modifier, gap: Int = 8, content: @Composable () -> Unit) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(gap.dp), verticalAlignment = Alignment.CenterVertically) { content() }
}
