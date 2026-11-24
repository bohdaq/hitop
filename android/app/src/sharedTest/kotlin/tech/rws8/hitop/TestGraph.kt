package tech.rws8.hitop

import android.content.Context
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import tech.rws8.hitop.cloud.CloudCredentials
import tech.rws8.hitop.cloud.CloudSettings
import tech.rws8.hitop.cloud.FakeCloud
import tech.rws8.hitop.cloud.HitopCloud
import tech.rws8.hitop.cloud.MemoryCloudSettingsStore
import tech.rws8.hitop.core.FakeAssistant
import tech.rws8.hitop.core.SecretStore
import tech.rws8.hitop.core.StubServer
import tech.rws8.hitop.ffi.Ffi
import tech.rws8.hitop.platform.AppGraph
import tech.rws8.hitop.platform.SignInPlatform
import java.io.File

/**
 * UI tests and screenshots: a fresh library with the drafts' shop-api, every http:// request
 * answered by the in-process [StubServer], and HITOP Cloud answered by a [FakeCloud].
 */
object TestGraph {
    /** The assistant's answers are [FakeAssistant]'s; whether the model is there. */
    @Volatile var assistantReady = true
    @Volatile var stub: StubServer? = null
        private set
    @Volatile var fakeCloud: FakeCloud? = null
        private set

    @Synchronized
    fun useSeed(context: Context, secrets: SecretStore? = null) {
        val app = context.applicationContext
        File(app.filesDir, "hitop-library").deleteRecursively()
        File(app.filesDir, "secrets.json").delete()
        if (stub == null) {
            stub = StubServer().start().also { s -> Ffi.call("useHttpProxy", buildJsonObject { put("proxy", s.proxy) }) }
        }
        val fake = FakeCloud().also { fakeCloud = it }
        AppGraph.use(AppGraph.make(
            app, secrets,
            cloudFor = { vault -> HitopCloud(fake, CloudCredentials(vault), MemoryCloudSettingsStore(CloudSettings(server = FakeCloud.SERVER)), deviceName = "HITOP on Pixel") },
            signIn = FakeSignIn(fake),
            assistant = { FakeAssistant(assistantReady) },
            background = false,
            seed = { it.seedShopApi(stub?.let { s -> "ws://127.0.0.1:${s.port}" }) },
        ))
    }
}

/** UI tests: the fake cloud approves at once and hands the code back, as the real return would. */
class FakeSignIn(private val fake: FakeCloud) : SignInPlatform {
    override fun openBrowser(context: Context, url: String, onReturn: (String) -> Unit) = onReturn("hitop://sign-in?sso=${fake.approve()}")
    override val googleNative = true
    override suspend fun googleIdToken(context: Context, nonce: String) = fake.nativeIdToken(nonce)
}
