package tech.rws8.hitop.platform

import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import androidx.core.content.FileProvider
import androidx.core.content.IntentCompat
import tech.rws8.hitop.core.Importing
import java.io.ByteArrayOutputStream
import java.io.File

/** Files and text from other apps (the file picker, Share) and files shared out. */
object Incoming {
    /** A person's file: only other apps' `content:` addresses, never this app's own files. */
    fun allowed(context: Context, uri: Uri) = uri.scheme == ContentResolver.SCHEME_CONTENT && uri.authority?.substringAfterLast('@')?.startsWith(context.packageName) == false

    fun name(context: Context, uri: Uri): String = runCatching {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
    }.getOrNull()?.takeIf { it.isNotBlank() } ?: "file"

    /** Blocking. At most [Importing.MAX_BYTES]. */
    fun read(context: Context, uri: Uri): ByteArray {
        require(allowed(context, uri)) { "This file can't be opened." }
        val out = ByteArrayOutputStream()
        (context.contentResolver.openInputStream(uri) ?: throw java.io.IOException("This file can't be opened.")).use { input ->
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n < 0) break
                require(out.size() + n <= Importing.MAX_BYTES) { "The file is larger than ${Importing.MAX_BYTES / 1024 / 1024} MB." }
                out.write(buf, 0, n)
            }
        }
        return out.toByteArray()
    }

    sealed interface Shared {
        data class Text(val text: String) : Shared
        data class File(val uri: Uri) : Shared
    }

    /** What another app shared with ACTION_SEND. */
    fun shared(context: Context, intent: Intent): Shared? {
        if (intent.action != Intent.ACTION_SEND) return null
        IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)?.takeIf { allowed(context, it) }?.let { return Shared.File(it) }
        return intent.getStringExtra(Intent.EXTRA_TEXT)?.trim()?.takeIf { it.isNotEmpty() && it.length <= Importing.MAX_BYTES }?.let { Shared.Text(it) }
    }

    /** A file made here, to another app (FileProvider, cache/reports). */
    fun share(context: Context, name: String, type: String, bytes: ByteArray, title: String) {
        val dir = File(context.cacheDir, "reports").apply { mkdirs() }
        val f = File(dir, name.replace(Regex("[^A-Za-z0-9._-]+"), "-")).apply { writeBytes(bytes) }
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", f)
        val send = Intent(Intent.ACTION_SEND).setType(type).putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        context.startActivity(Intent.createChooser(send, title))
    }
}
