import AuthenticationServices
import UIKit

/// A provider's sign-in page for OAuth 2.0 requests. The answer comes to hitop-core's loopback
/// listener in this app, so the session's own callback is never used; the sheet is closed when the
/// sign-in is done. Its cookies are its own (nothing shared with Safari).
@MainActor
final class SystemOAuthBrowser: NSObject, OAuthBrowser, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func open(_ url: URL, cancelled: @escaping @MainActor () -> Void) throws {
        let s = ASWebAuthenticationSession(url: url, callback: .customScheme("hitop-oauth")) { _, _ in
            Task { @MainActor in cancelled() }
        }
        s.presentationContextProvider = self
        s.prefersEphemeralWebBrowserSession = true
        session = s
        if !s.start() { throw OAuthPageError() }
    }

    func close() {
        session?.cancel()
        session = nil
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            return scenes.flatMap(\.windows).first { $0.isKeyWindow } ?? ASPresentationAnchor()
        }
    }
}

struct OAuthPageError: LocalizedError {
    var errorDescription: String? { "OAuth 2.0: the sign-in page didn't open" }
}
