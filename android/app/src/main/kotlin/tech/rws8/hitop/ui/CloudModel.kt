package tech.rws8.hitop.ui

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import tech.rws8.hitop.cloud.AuthMethods
import tech.rws8.hitop.cloud.CloudDevice
import tech.rws8.hitop.cloud.CloudException
import tech.rws8.hitop.cloud.CloudUsage
import tech.rws8.hitop.cloud.DeviceLimit
import tech.rws8.hitop.cloud.HitopCloud
import tech.rws8.hitop.cloud.LibrarySync
import tech.rws8.hitop.platform.SignInPlatform

/** The sign-in sheet's and the Account screen's state over [HitopCloud] and [LibrarySync]. */
class CloudModel(val cloud: HitopCloud, val sync: LibrarySync, private val platform: SignInPlatform, private val scope: CoroutineScope) {
    var methods by mutableStateOf<AuthMethods?>(null)
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)
    /** The server didn't take the app's return address: paste the code its page shows. */
    var pasteFor by mutableStateOf<String?>(null)
    var deviceLimit by mutableStateOf<DeviceLimit?>(null)
    var devices by mutableStateOf<List<CloudDevice>>(emptyList())
    var usage by mutableStateOf<CloudUsage?>(null)
    var showSignIn by mutableStateOf(false)

    private fun run(block: suspend () -> Unit): Unit = launchRun(block)

    private fun launchRun(block: suspend () -> Unit) {
        scope.launch {
            busy = true
            error = null
            try {
                block()
            } catch (l: DeviceLimit) {
                deviceLimit = l
            } catch (e: CloudException) {
                error = e.message
            } catch (e: Exception) {
                error = e.message ?: e.toString()
            } finally {
                busy = false
            }
        }
    }

    fun loadMethods(server: String = cloud.settings.value.server) = run { methods = cloud.authMethods(server) ?: AuthMethods() }

    fun setServer(server: String) {
        try {
            cloud.setServer(server)
            loadMethods()
        } catch (e: CloudException) {
            error = e.message
        }
    }

    fun signIn(context: Context, kind: String) = run {
        val server = cloud.settings.value.server
        if (kind == "google" && platform.googleNative && methods?.googleNative == true) {
            val nonce = cloud.googleNonce(server)
            val token = platform.googleIdToken(context, nonce)
            if (token != null) {
                cloud.finishGoogleNative(server, token, nonce)
                signedIn()
            }
        } else {
            browser(context, kind, server)
        }
    }

    private suspend fun browser(context: Context, kind: String, server: String) {
        val (url, paste) = cloud.startBrowserSignIn(kind, server)
        if (paste) pasteFor = kind
        platform.openBrowser(context, url) { back -> handleReturn(back) }
    }

    /** `hitop://sign-in?sso=<code>` (the intent filter) or a pasted code. */
    fun handleReturn(text: String) {
        val code = HitopCloud.code(text)
        if (code == null) {
            error = "That isn't a sign-in code."
            return
        }
        run {
            cloud.finishBrowserSignIn(code)
            signedIn()
        }
    }

    private suspend fun signedIn() {
        pasteFor = null
        showSignIn = false
        refresh()
    }

    fun removeAtLimit(id: String) = run {
        val l = deviceLimit
        if (l != null) deviceLimit = cloud.removeDevice(id, l)
    }

    fun retryAtLimit() = run {
        val l = deviceLimit
        if (l != null) {
            deviceLimit = null
            cloud.retry(l)
            signedIn()
        }
    }

    /** The Account screen's lists: devices, organizations, the current organization's usage. */
    fun refresh() = run {
        if (!cloud.signedIn) return@run Unit
        devices = cloud.devices() ?: emptyList()
        cloud.loadOrgs()
        usage = cloud.currentOrg?.let { cloud.usage(it.id) }
    }

    fun syncNow() = run { sync.sync(); sync.flush() }
    fun signOut() = run { cloud.signOut(); devices = emptyList(); usage = null }
    fun renameDevice(id: String, name: String) = run { cloud.renameDevice(id, name); devices = cloud.devices() ?: devices }
    fun removeDevice(id: String, force: Boolean) = run { cloud.removeDevice(id, force); devices = cloud.devices() ?: devices }
    fun selectOrg(id: String?) = run { cloud.selectOrg(id); usage = cloud.currentOrg?.let { cloud.usage(it.id) } }
    fun upload(collectionId: String) = run { sync.upload(collectionId) }

    /** Deletes the HITOP Cloud account; [done] after it went (the screen closes). */
    fun deleteAccount(done: () -> Unit) = run {
        cloud.deleteAccount()
        devices = emptyList()
        usage = null
        done()
    }
}
