package tech.rws8.hitop.platform

import android.content.Context
import androidx.annotation.StringRes

/** The app's text (res/values/strings_ui.xml) outside composition too: notifications, click handlers. */
object Strings {
    @Volatile private var app: Context? = null

    fun install(context: Context) {
        app = context.applicationContext
    }

    fun get(@StringRes id: Int, args: Array<out Any?>): String {
        val c = checkNotNull(app) { "HitopApp sets the strings" }
        return if (args.isEmpty()) c.getString(id) else c.getString(id, *args)
    }
}

/** The text [id] with [args] in it. */
fun tr(@StringRes id: Int, vararg args: Any?): String = Strings.get(id, args)
