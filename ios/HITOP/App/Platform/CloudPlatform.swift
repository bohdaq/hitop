import AuthenticationServices
import CryptoKit
import Foundation
import UIKit

/// The device key pair: X25519 from CryptoKit, the raw 32-byte keys as base64url (the encoding of
/// hitop-sync-crypto's `DeviceKeyPair`, so I2b's shared workspaces can use the same key).
struct X25519KeyMaker: DeviceKeyMaker {
    func make() -> DeviceKey {
        let k = Curve25519.KeyAgreement.PrivateKey()
        return DeviceKey(secret: CloudCrypto.base64url([UInt8](k.rawRepresentation)),
                         publicKey: CloudCrypto.base64url([UInt8](k.publicKey.rawRepresentation)))
    }
}

/// Opens the provider's sign-in page and gives back the address it ends on (`hitop://sign-in?sso=`).
@MainActor
protocol WebSignIn {
    /// nil: the person closed the page (in the paste flow, after copying the code).
    func open(_ url: URL) async throws -> URL?
}

/// ASWebAuthenticationSession: the system's sign-in sheet, sharing Safari's cookies (so a
/// person already signed in to Google or GitHub only approves), handing `hitop://` back to this app.
@MainActor
final class SystemWebSignIn: NSObject, WebSignIn, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func open(_ url: URL) async throws -> URL? {
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<URL?, Error>) in
            let s = ASWebAuthenticationSession(url: url, callback: .customScheme("hitop")) { callback, error in
                if let error = error as? ASWebAuthenticationSessionError, error.code == .canceledLogin {
                    cont.resume(returning: nil)
                } else if let error {
                    cont.resume(throwing: error)
                } else {
                    cont.resume(returning: callback)
                }
            }
            s.presentationContextProvider = self
            s.prefersEphemeralWebBrowserSession = false
            session = s
            if !s.start() { cont.resume(throwing: CloudError.unreachable("the sign-in page didn't open")) }
        }
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            return scenes.flatMap(\.windows).first { $0.isKeyWindow } ?? ASPresentationAnchor()
        }
    }
}

/// UI tests: the provider "approves" at once. The fake server's page address carries the code it
/// will hand over; the return address carries it back, as ASWebAuthenticationSession would.
@MainActor
final class StubWebSignIn: WebSignIn {
    /// The paste flow: the page shows the code, the person copies it and closes the page.
    nonisolated(unsafe) static var lastCode: String?

    func open(_ url: URL) async throws -> URL? {
        try await Task.sleep(for: .milliseconds(300))
        let code = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == "code" }?.value ?? ""
        Self.lastCode = code
        return URL(string: "hitop://sign-in?sso=\(code)")
    }
}

enum DeviceInfo {
    /// "this iPhone" / "this iPad": the conflict copy's name.
    @MainActor static var label: String { UIDevice.current.userInterfaceIdiom == .pad ? "this iPad" : "this iPhone" }

    /// The Devices list's name for this device.
    @MainActor static var name: String { UIDevice.current.userInterfaceIdiom == .pad ? "HITOP on iPad" : "HITOP on iPhone" }
}
