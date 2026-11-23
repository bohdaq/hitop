package tech.rws8.hitop.platform

import android.content.Context
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.core.Library
import tech.rws8.hitop.core.LibraryStore
import tech.rws8.hitop.core.SecretStore
import tech.rws8.hitop.core.StubServer
import tech.rws8.hitop.ffi.Ffi
import java.io.File

/** The app's objects, made once per process. UI tests replace them with [useSeed] before launching. */
object AppGraph {
    @Volatile private var library: Library? = null
    @Volatile var stub: StubServer? = null
        private set

    fun library(context: Context): Library = library ?: synchronized(this) {
        library ?: make(context.applicationContext, seed = false).also { library = it }
    }

    private fun make(context: Context, seed: Boolean, secrets: SecretStore? = null): Library {
        val store = LibraryStore(File(context.filesDir, "hitop-library"))
        val lib = Library(store, secrets ?: KeystoreSecretStore(File(context.filesDir, "secrets.json")))
        lib.load()
        if (seed) lib.seedShopApi() else lib.seedFirstVisit()
        return lib
    }

    /**
     * UI tests and screenshots: a fresh library with the drafts' shop-api, and every http://
     * request answered by the in-process [StubServer] (hitop-core's client goes through it as a
     * proxy). Call before the first send of the process.
     */
    @Synchronized
    fun useSeed(context: Context, secrets: SecretStore? = null) {
        val app = context.applicationContext
        File(app.filesDir, "hitop-library").deleteRecursively()
        File(app.filesDir, "secrets.json").delete()
        if (stub == null) {
            stub = StubServer().start().also { s -> Ffi.call("useHttpProxy", buildJsonObject { put("proxy", s.proxy) }) }
        }
        library = make(app, seed = true, secrets)
    }
}
