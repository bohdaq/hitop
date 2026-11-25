import Foundation
import Observation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
#if canImport(Security)
import Security
#endif

/// What the person chose on the Settings screen (settings.json; this device only, never synced).
/// For requests, never for HITOP Cloud. There is no setting that skips verification.
struct AppSettings: Codable, Hashable, Sendable {
    /// A certificate authority added here (base64url DER), beside the system's.
    struct Authority: Codable, Hashable, Sendable, Identifiable {
        var name: String
        var der: String
        var id: String { der }
    }

    /// A client certificate in the Keychain, sent to `host` only.
    struct ClientCertificate: Codable, Hashable, Sendable, Identifiable {
        var name: String
        var host: String
        /// The Keychain's persistent reference (base64url); not the certificate or its key.
        var reference: String
        var id: String { host }
    }

    /// Seconds one request may take.
    var timeout = 60
    var authorities: [Authority] = []
    var clientCertificates: [ClientCertificate] = []
    /// HITOP Cloud also syncs while the app is closed.
    var backgroundSync = true

    static let timeouts = [10, 30, 60, 120, 300]

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        timeout = min(600, max(1, c.value("timeout", 60)))
        authorities = c.value("authorities", [])
        clientCertificates = c.value("clientCertificates", [])
        backgroundSync = c.value("backgroundSync", true)
    }

    /// A host as people type it ("https://API.example:8443/x" → "api.example").
    static func host(_ text: String) -> String? {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty, let h = URL(string: t.contains("://") ? t : "https://\(t)")?.host, !h.isEmpty else { return nil }
        return h.lowercased()
    }
}

/// The settings file and the only place that changes it.
@Observable
final class SettingsStore: @unchecked Sendable {
    @ObservationIgnored private let file: URL?
    @ObservationIgnored private let lock = NSLock()
    @ObservationIgnored private var current = AppSettings()
    /// What views read (changed on the main actor only).
    private(set) var shown = AppSettings()
    /// The file is there but can't be used: requests stop and say so, since it may hold the
    /// certificate authorities a server needs.
    private(set) var problem: String?

    init(root: URL?) {
        file = root?.appendingPathComponent("settings.json")
        load()
    }

    /// Also after the phone was locked when the app started.
    func load() {
        guard let file, FileManager.default.fileExists(atPath: file.path) else { return }
        do {
            let s = try JSONDecoder().decode(AppSettings.self, from: Data(contentsOf: file))
            try Self.applyToCore(s)
            lock.withLock { current = s }
            shown = s
            problem = nil
        } catch {
            problem = "The settings can't be read: \(error.localizedDescription)"
        }
    }

    var value: AppSettings { lock.withLock { current } }

    /// Checks, uses and saves; nothing changes on an error.
    @MainActor
    func update(_ change: (inout AppSettings) -> Void) throws {
        if let problem { throw Uploads.Failure(message: problem) }
        var next = value
        change(&next)
        guard next != value else { return }
        let before = value
        try Self.applyToCore(next)
        do {
            if let file {
                try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
                try JSONEncoder().encode(next).write(to: file, options: [.atomic])
            }
        } catch {
            try? Self.applyToCore(before)
            throw error
        }
        lock.withLock { current = next }
        shown = next
    }

    /// Starts over when the file can't be used.
    @MainActor
    func reset() throws {
        if let file { try? FileManager.default.removeItem(at: file) }
        problem = nil
        try Self.applyToCore(AppSettings())
        lock.withLock { current = AppSettings() }
        shown = AppSettings()
    }

    /// gRPC and OAuth token requests go through hitop-core: the same added authorities.
    private static func applyToCore(_ s: AppSettings) throws {
        _ = try SyncCore.call("networkRoots", ["roots": .array(s.authorities.map { .string($0.der) })])
    }

    /// The certificates of a PEM or DER file, each usable as an authority.
    static func authorities(in data: Data, name: String) throws -> [AppSettings.Authority] {
        let found = (try SyncCore.call("certificates", ["data": .string(CloudCrypto.base64url([UInt8](data)))]).array ?? []).compactMap(\.string)
        return try found.enumerated().map { i, der in
            #if canImport(Security)
            guard let bytes = CloudCrypto.unbase64url(der), let c = SecCertificateCreateWithData(nil, Data(bytes) as CFData) else {
                throw Uploads.Failure(message: "\(name) holds something that isn't a certificate.")
            }
            let label = SecCertificateCopySubjectSummary(c) as String? ?? name
            #else
            let label = found.count > 1 ? "\(name) (\(i + 1))" : name
            #endif
            return AppSettings.Authority(name: label, der: der)
        }
    }
}

#if canImport(Security)
/// Client certificates in the Keychain.
enum ClientIdentity {
    /// Imports a .p12 file; the Keychain's reference to what it holds.
    static func add(_ p12: Data, password: String) throws -> (name: String, reference: String) {
        var items: CFArray?
        let status = SecPKCS12Import(p12 as CFData, [kSecImportExportPassphrase as String: password] as CFDictionary, &items)
        guard status == errSecSuccess else {
            throw Uploads.Failure(message: status == errSecAuthFailed ? "That isn't the file's password." : "The file can't be read as a .p12 certificate (\(status)).")
        }
        guard let first = (items as? [[String: Any]])?.first, let any = first[kSecImportItemIdentity as String] else {
            throw Uploads.Failure(message: "The file holds no certificate with its key.")
        }
        let identity = any as! SecIdentity
        var certificate: SecCertificate?
        SecIdentityCopyCertificate(identity, &certificate)
        let name = certificate.flatMap { SecCertificateCopySubjectSummary($0) as String? } ?? "Certificate"
        var out: CFTypeRef?
        let add: [String: Any] = [kSecValueRef as String: identity, kSecReturnPersistentRef as String: true,
                                  kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly]
        let added = SecItemAdd(add as CFDictionary, &out)
        if added == errSecDuplicateItem {
            throw Uploads.Failure(message: "\(name) is in HITOP already: remove it first to use it for another server.")
        }
        guard added == errSecSuccess, let reference = out as? Data else {
            throw Uploads.Failure(message: "The Keychain didn't take the certificate (\(added)).")
        }
        return (name, CloudCrypto.base64url([UInt8](reference)))
    }

    static func remove(_ reference: String) {
        guard let d = CloudCrypto.unbase64url(reference) else { return }
        SecItemDelete([kSecValuePersistentRef as String: Data(d)] as CFDictionary)
    }

    static func load(_ reference: String) -> SecIdentity? {
        guard let d = CloudCrypto.unbase64url(reference) else { return nil }
        var out: CFTypeRef?
        let q: [String: Any] = [kSecValuePersistentRef as String: Data(d), kSecClass as String: kSecClassIdentity, kSecReturnRef as String: true]
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let out, CFGetTypeID(out) == SecIdentityGetTypeID() else { return nil }
        return (out as! SecIdentity)
    }
}
#endif

/// A session's answers to TLS questions, with the settings of when it started.
final class TLSDelegate: NSObject, URLSessionDelegate, @unchecked Sendable {
    let settings: AppSettings

    init(_ settings: AppSettings) { self.settings = settings }

    #if canImport(Security)
    func urlSession(_ session: URLSession, didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping @Sendable (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        let (disposition, credential) = answer(challenge)
        completionHandler(disposition, credential)
    }

    func answer(_ challenge: URLAuthenticationChallenge) -> (URLSession.AuthChallengeDisposition, URLCredential?) {
        let space = challenge.protectionSpace
        switch space.authenticationMethod {
        case NSURLAuthenticationMethodServerTrust:
            guard !settings.authorities.isEmpty, let trust = space.serverTrust, Self.trusts(trust, settings.authorities) else { break }
            return (.useCredential, URLCredential(trust: trust))
        case NSURLAuthenticationMethodClientCertificate:
            guard let c = settings.clientCertificates.first(where: { $0.host == space.host.lowercased() }), let identity = ClientIdentity.load(c.reference) else { break }
            return (.useCredential, URLCredential(identity: identity, certificates: nil, persistence: .forSession))
        default:
            break
        }
        // The system's own decision (an untrusted server is refused).
        return (.performDefaultHandling, nil)
    }

    /// The server's chain, name and dates are right with the system's authorities and these.
    static func trusts(_ trust: SecTrust, _ authorities: [AppSettings.Authority]) -> Bool {
        let anchors = authorities.compactMap { CloudCrypto.unbase64url($0.der).flatMap { SecCertificateCreateWithData(nil, Data($0) as CFData) } }
        guard !anchors.isEmpty,
              SecTrustSetAnchorCertificates(trust, anchors as CFArray) == errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(trust, false) == errSecSuccess else { return false }
        return SecTrustEvaluateWithError(trust, nil)
    }
    #endif
}
