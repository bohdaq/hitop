package tech.rws8.hitop.platform

import android.content.Context
import android.os.Build
import tech.rws8.hitop.BuildConfig
import java.io.File

/**
 * A crash leaves a report on this phone (crash-report.txt) that the person may look at and share
 * by hand; nothing is sent. It holds class and method names only: exception messages may have
 * addresses, variables or secrets in them, so they are left out.
 */
object CrashReport {
    private fun file(context: Context) = File(context.filesDir, "crash-report.txt")

    fun install(context: Context) {
        val app = context.applicationContext
        val before = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { t, e ->
            runCatching { file(app).writeText(text(e)) }
            before?.uncaughtException(t, e)
        }
    }

    fun text(e: Throwable): String = buildString {
        appendLine("HITOP ${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE}) · Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT}) · ${Build.MANUFACTURER} ${Build.MODEL}")
        var x: Throwable? = e
        var depth = 0
        while (x != null && depth < 6) {
            appendLine((if (depth == 0) "" else "Caused by ") + x.javaClass.name)
            x.stackTrace.take(60).forEach { appendLine("  at ${it.className}.${it.methodName}(${it.fileName ?: "?"}:${it.lineNumber})") }
            x = x.cause.takeIf { it !== x }
            depth++
        }
    }

    fun pending(context: Context): String? = runCatching { file(context).takeIf { it.exists() }?.readText()?.takeIf { it.isNotBlank() } }.getOrNull()
    fun dismiss(context: Context) { file(context).delete() }
}
