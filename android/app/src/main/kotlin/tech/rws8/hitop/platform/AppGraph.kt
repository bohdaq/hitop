package tech.rws8.hitop.platform

import android.content.Context
import android.os.Build
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.coroutines.launch
import tech.rws8.hitop.cloud.CloudCredentials
import tech.rws8.hitop.cloud.CloudSettings
import tech.rws8.hitop.cloud.FakeCloud
import tech.rws8.hitop.cloud.FileCloudSettingsStore
import tech.rws8.hitop.cloud.HitopCloud
import tech.rws8.hitop.cloud.LibrarySync
import tech.rws8.hitop.cloud.MemoryCloudSettingsStore
import tech.rws8.hitop.cloud.UrlConnectionTransport
import tech.rws8.hitop.core.Library
import tech.rws8.hitop.core.LibraryStore
import tech.rws8.hitop.core.SecretStore
import tech.rws8.hitop.core.RunCenter
import tech.rws8.hitop.core.StubServer
import tech.rws8.hitop.sync.FileSharedSyncStore
import tech.rws8.hitop.sync.SharedSync
import tech.rws8.hitop.ffi.Ffi
import java.io.File

/** The app's objects, made once per process. UI tests replace them with [useSeed] before launching. */
object AppGraph {
    class Parts(val library: Library, val cloud: HitopCloud, val sync: LibrarySync, val shared: SharedSync, val signIn: SignInPlatform, val runs: RunCenter, val assistant: tech.rws8.hitop.core.LanguageAssistant)

    /** UI tests: the assistant's answers (FakeAssistant) and whether the model is there. */
    @Volatile var assistantReady = true

    /** Robolectric runs collections in-process (no foreground service there); devices use RunService. */
    @Volatile var runsInProcess = false

    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    @Volatile private var parts: Parts? = null
    @Volatile var stub: StubServer? = null
        private set
    @Volatile var fakeCloud: FakeCloud? = null
        private set

    fun parts(context: Context): Parts = parts ?: synchronized(this) {
        parts ?: make(context.applicationContext, seed = false).also { parts = it }
    }

    fun library(context: Context) = parts(context).library

    private fun deviceName() = "HITOP on ${Build.MODEL?.takeIf { it.isNotBlank() } ?: "Android"}"

    private fun make(context: Context, seed: Boolean, secrets: SecretStore? = null, fake: FakeCloud? = null): Parts {
        val root = File(context.filesDir, "hitop-library")
        val store = LibraryStore(root)
        val vault = secrets ?: KeystoreSecretStore(File(context.filesDir, "secrets.json"))
        val lib = Library(store, vault)
        lib.load()
        if (seed) lib.seedShopApi(stub?.let { "ws://127.0.0.1:${it.port}" }) else lib.seedFirstVisit()
        val cloud = if (fake != null) {
            HitopCloud(fake, CloudCredentials(vault), MemoryCloudSettingsStore(CloudSettings(server = FakeCloud.SERVER)), deviceName = "HITOP on Pixel")
        } else {
            HitopCloud(UrlConnectionTransport(), CloudCredentials(vault), FileCloudSettingsStore(File(root, "cloud.json")), deviceName = deviceName())
        }
        val sync = LibrarySync(cloud, lib, File(root, "library-sync.json"), scope)
        val shared = SharedSync(cloud, lib, FileSharedSyncStore(root), scope)
        lib.orgForNew = { cloud.orgForNew }
        cloud.onAccountChange = {
            sync.refreshBadges()
            shared.applyRoles()
            scope.launch { sync.sync(); shared.syncAll() }
        }
        val runs = RunCenter(lib, store, tech.rws8.hitop.BuildConfig.VERSION_NAME)
        val assistant = if (fake != null) tech.rws8.hitop.core.FakeAssistant(assistantReady) else GeminiNano()
        return Parts(lib, cloud, sync, shared, if (fake != null) FakeSignIn(fake) else AndroidSignIn(), runs, assistant)
    }

    /**
     * UI tests and screenshots: a fresh library with the drafts' shop-api, every http:// request
     * answered by the in-process [StubServer], and HITOP Cloud answered by a [FakeCloud].
     */
    @Synchronized
    fun useSeed(context: Context, secrets: SecretStore? = null) {
        val app = context.applicationContext
        File(app.filesDir, "hitop-library").deleteRecursively()
        File(app.filesDir, "secrets.json").delete()
        if (stub == null) {
            stub = StubServer().start().also { s -> Ffi.call("useHttpProxy", buildJsonObject { put("proxy", s.proxy) }) }
        }
        val fake = FakeCloud().also { fakeCloud = it }
        parts = make(app, seed = true, secrets, fake)
    }
}
