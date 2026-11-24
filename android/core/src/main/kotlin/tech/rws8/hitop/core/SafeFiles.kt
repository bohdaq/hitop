package tech.rws8.hitop.core

import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption

/**
 * Whole-file writes that a crash or a full disk can't leave half done (a synced temporary file,
 * then an atomic replace), and a way to keep a file that can't be read instead of losing it.
 */
object SafeFiles {
    fun write(f: File, text: String) {
        f.parentFile?.mkdirs()
        val tmp = File(f.parentFile, f.name + ".tmp")
        try {
            FileOutputStream(tmp).use { out ->
                out.write(text.toByteArray(Charsets.UTF_8))
                out.fd.sync()
            }
            Files.move(tmp.toPath(), f.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        } catch (e: java.io.IOException) {
            tmp.delete()
            throw e
        }
    }

    /** Moves an unreadable file to `<name>.damaged-<time>` next to it; the new file, or null when there is none. */
    fun setAside(f: File): File? {
        if (!f.exists()) return null
        var to = File(f.parentFile, "${f.name}.damaged-${System.currentTimeMillis()}")
        var n = 1
        while (to.exists()) to = File(f.parentFile, "${f.name}.damaged-${System.currentTimeMillis()}-${n++}")
        Files.move(f.toPath(), to.toPath())
        return to
    }
}
