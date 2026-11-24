package tech.rws8.hitop.platform

import android.content.Context
import android.os.Build
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import tech.rws8.hitop.cloud.CloudCredentials
import tech.rws8.hitop.cloud.FileCloudSettingsStore
import tech.rws8.hitop.cloud.HitopCloud
import tech.rws8.hitop.cloud.LibrarySync
import tech.rws8.hitop.cloud.UrlConnectionTransport
import tech.rws8.hitop.core.Library
import tech.rws8.hitop.core.LibraryStore
import tech.rws8.hitop.core.SecretStore
import tech.rws8.hitop.core.RunCenter
import tech.rws8.hitop.sync.FileSharedSyncStore
import tech.rws8.hitop.sync.SharedSync
import java.io.File

/** The app's objects, made once per process. UI tests replace them with [use] before launching. */
object AppGraph {
    class Parts(val library: Library, val cloud: HitopCloud, val sync: LibrarySync, val shared: SharedSync, val signIn: SignInPlatform, val runs: RunCenter, val settings: tech.rws8.hitop.core.SettingsStore, val network: PhoneNetwork, val assistant: tech.rws8.hitop.core.LanguageAssistant)

    /** Robolectric runs collections in-process (no foreground service there); devices use RunService. */
    @Volatile var runsInProcess = false

    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    @Volatile private var parts: Parts? = null
    /** A test's objects are in use: no background work is scheduled for them. */
    @Volatile var testing = false
        private set

    /** UI tests: these objects from now on. */
    fun use(p: Parts) {
        testing = true
        parts = p
    }

    fun parts(context: Context): Parts = parts ?: synchronized(this) {
        parts ?: make(context.applicationContext).also { parts = it }
    }

    fun library(context: Context) = parts(context).library

    private fun deviceName() = "HITOP on ${Build.MODEL?.takeIf { it.isNotBlank() } ?: "Android"}"

    /** Tests give their own secret store, cloud, sign-in and assistant, and a [seed]. */
    fun make(
        context: Context,
        secrets: SecretStore? = null,
        cloudFor: ((SecretStore) -> HitopCloud)? = null,
        signIn: SignInPlatform = AndroidSignIn(),
        assistant: () -> tech.rws8.hitop.core.LanguageAssistant = { GeminiNano() },
        background: Boolean = true,
        seed: (Library) -> Unit = { it.seedFirstVisit() },
    ): Parts {
        val root = File(context.filesDir, "hitop-library")
        val store = LibraryStore(root)
        val vault = secrets ?: KeystoreSecretStore(File(context.filesDir, "secrets.json"))
        val lib = Library(store, vault)
        lib.load()
        val settings = tech.rws8.hitop.core.SettingsStore(store)
        val network = PhoneNetwork(context)
        lib.network = { network.args(settings.value) }
        lib.timeoutMs = { settings.value.timeoutMs }
        seed(lib)
        val cloud = cloudFor?.invoke(vault) ?: HitopCloud(UrlConnectionTransport(), CloudCredentials(vault), FileCloudSettingsStore(File(root, "cloud.json")), deviceName = deviceName())
        val sync = LibrarySync(cloud, lib, File(root, "library-sync.json"), scope)
        sync.enabled = { settings.value.librarySync }
        settings.onChange = { s ->
            network.forget()
            sync.refreshBadges()
            if (s.librarySync) scope.launch { sync.sync() }
            if (background) runCatching { BackgroundWork.updateSync(context) }
        }
        sync.forgetDamaged(store)
        store.setAsideDamaged()
        val shared = SharedSync(cloud, lib, FileSharedSyncStore(root), scope)
        lib.orgForNew = { cloud.orgForNew }
        cloud.onAccountChange = {
            sync.refreshBadges()
            shared.applyRoles()
            scope.launch { sync.sync(); shared.syncAll() }
            if (background) runCatching { BackgroundWork.updateSync(context) }
        }
        val runs = RunCenter(lib, store, tech.rws8.hitop.BuildConfig.VERSION_NAME)
        if (background) runs.onSchedules = { runCatching { BackgroundWork.updateRuns(context) } }
        return Parts(lib, cloud, sync, shared, signIn, runs, settings, network, assistant())
    }
}
