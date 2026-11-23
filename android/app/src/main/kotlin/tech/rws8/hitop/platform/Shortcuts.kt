package tech.rws8.hitop.platform

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import tech.rws8.hitop.MainActivity
import tech.rws8.hitop.R

/**
 * Launcher and Assistant shortcuts ("Run shop-api"): one per recently run collection, opening
 * `hitop://run/<collection id>`. Google Assistant offers pushed dynamic shortcuts by their labels.
 */
object Shortcuts {
    fun update(context: Context) {
        val s = Widgets.snapshot(context)
        val list = s.collections.take(4).map { c ->
            ShortcutInfoCompat.Builder(context, "run-${c.collectionId}")
                .setShortLabel("Run ${c.name}")
                .setLongLabel("Run ${c.name} in HITOP")
                .setIcon(IconCompat.createWithResource(context, R.drawable.ic_notification))
                .setIntent(Intent(Intent.ACTION_VIEW, Uri.parse("hitop://run/${c.collectionId}"), context, MainActivity::class.java))
                .build()
        }
        runCatching { list.forEach { ShortcutManagerCompat.pushDynamicShortcut(context, it) } }
    }
}
