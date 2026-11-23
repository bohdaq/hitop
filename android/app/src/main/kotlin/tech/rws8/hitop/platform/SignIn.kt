package tech.rws8.hitop.platform

import android.content.Context
import android.net.Uri
import androidx.browser.customtabs.CustomTabsIntent
import androidx.credentials.CredentialManager
import androidx.credentials.CustomCredential
import androidx.credentials.GetCredentialRequest
import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.credentials.exceptions.NoCredentialException
import com.google.android.libraries.identity.googleid.GetGoogleIdOption
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import tech.rws8.hitop.BuildConfig

/** What signing in needs from Android: a browser tab for provider pages, Google's account picker. */
interface SignInPlatform {
    /** Opens a provider's page; it comes back to `hitop://sign-in?sso=<code>` (MainActivity). */
    fun openBrowser(context: Context, url: String, onReturn: (String) -> Unit)

    /** Whether this build can ask Google for an ID token (a server client id is built in). */
    val googleNative: Boolean

    /** Google's account picker → an ID token for HITOP Cloud with [nonce]; null when cancelled. */
    suspend fun googleIdToken(context: Context, nonce: String): String?
}

class AndroidSignIn : SignInPlatform {
    override fun openBrowser(context: Context, url: String, onReturn: (String) -> Unit) {
        CustomTabsIntent.Builder().setShowTitle(true).build().launchUrl(context, Uri.parse(url))
    }

    override val googleNative: Boolean get() = BuildConfig.GOOGLE_SERVER_CLIENT_ID.isNotEmpty()

    override suspend fun googleIdToken(context: Context, nonce: String): String? {
        val option = GetGoogleIdOption.Builder()
            .setServerClientId(BuildConfig.GOOGLE_SERVER_CLIENT_ID)
            .setFilterByAuthorizedAccounts(false)
            .setAutoSelectEnabled(false)
            .setNonce(nonce)
            .build()
        return try {
            val result = CredentialManager.create(context).getCredential(context, GetCredentialRequest.Builder().addCredentialOption(option).build())
            val c = result.credential
            if (c is CustomCredential && c.type == GoogleIdTokenCredential.TYPE_GOOGLE_ID_TOKEN_CREDENTIAL) GoogleIdTokenCredential.createFrom(c.data).idToken else null
        } catch (_: GetCredentialCancellationException) {
            null
        } catch (_: NoCredentialException) {
            throw IllegalStateException("No Google account on this phone: add one in Settings, or continue with GitHub.")
        }
    }
}

/** UI tests: the fake cloud approves at once and hands the code back, as the real return would. */
class FakeSignIn(private val fake: tech.rws8.hitop.cloud.FakeCloud) : SignInPlatform {
    override fun openBrowser(context: Context, url: String, onReturn: (String) -> Unit) = onReturn("hitop://sign-in?sso=${fake.approve()}")
    override val googleNative = true
    override suspend fun googleIdToken(context: Context, nonce: String) = fake.nativeIdToken(nonce)
}
