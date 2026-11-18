import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HitopKit
import Observation

// HITOP Cloud: the account (sign-in, devices, organizations, plan usage) against a hitop sync
// server (sync-server/), as the web app's `sync.js` SyncClient does it. The personal library sync
// is `LibrarySync`. Shared end-to-end encrypted workspaces are I2b.
//
// What stays on this device:
//   cloud.json (hitop-library/)  server setting, the signed-in account {server, email, userId,
//                                keyId, deviceName}, the device kept after signing out, the
//                                current organization
//   the Keychain                 `ios/cloud/<host>` = {session, secret}: the session token and the
//                                device's X25519 secret key (base64url, as hitop-sync-crypto's)

// MARK: - JSON

extension JSONAny {
    subscript(key: String) -> JSONAny? {
        if case let .object(o) = self { return o[key] }
        return nil
    }

    var string: String? { if case let .string(s) = self { return s }; return nil }
    var bool: Bool? { if case let .bool(b) = self { return b }; return nil }
    var int: Int? { if case let .number(n) = self, n.rounded() == n, abs(n) < 9.0e15 { return Int(n) }; return nil }
    var array: [JSONAny]? { if case let .array(a) = self { return a }; return nil }
    var object: [String: JSONAny]? { if case let .object(o) = self { return o }; return nil }
    var isNull: Bool { self == .null }

    /// Any Encodable as JSON (`SavedCollection` → the web app's JSON object).
    static func from<T: Encodable>(_ value: T) throws -> JSONAny {
        try JSONDecoder().decode(JSONAny.self, from: JSONEncoder().encode(value))
    }

    func decode<T: Decodable>(_ type: T.Type) throws -> T {
        try JSONDecoder().decode(T.self, from: JSONEncoder().encode(self))
    }

    /// Bytes with sorted keys: the same value always gives the same bytes (sync fingerprints).
    var canonical: Data {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return (try? e.encode(self)) ?? Data()
    }
}

// MARK: - Transport

struct CloudRequest: Sendable {
    var method: String
    var url: URL
    var headers: [String: String]
    var body: Data?
}

struct CloudResponse: Sendable {
    var status: Int
    var body: Data

    var json: JSONAny? { body.isEmpty ? nil : try? JSONDecoder().decode(JSONAny.self, from: body) }
}

/// The HTTP layer: URLSession in the app, a fake server in tests (`FakeCloud`).
protocol CloudTransport: Sendable {
    func send(_ request: CloudRequest) async throws -> CloudResponse
}

final class URLSessionCloudTransport: CloudTransport, @unchecked Sendable {
    private let session: URLSession

    init(configuration: URLSessionConfiguration = .ephemeral) {
        configuration.timeoutIntervalForRequest = 30
        configuration.urlCache = nil
        configuration.httpShouldSetCookies = false
        session = URLSession(configuration: configuration)
    }

    func send(_ request: CloudRequest) async throws -> CloudResponse {
        var r = URLRequest(url: request.url)
        r.httpMethod = request.method
        for (k, v) in request.headers { r.setValue(v, forHTTPHeaderField: k) }
        r.httpBody = request.body
        return try await withCheckedThrowingContinuation { cont in
            session.dataTask(with: r) { data, response, error in
                if let error {
                    cont.resume(throwing: error)
                } else {
                    cont.resume(returning: CloudResponse(status: (response as? HTTPURLResponse)?.statusCode ?? 0, body: data ?? Data()))
                }
            }.resume()
        }
    }
}

// MARK: - Errors and text

enum CloudError: Error, LocalizedError, Equatable {
    /// The server answered with an error (`{error, message}`).
    case http(status: Int, code: String, message: String)
    /// No answer (offline, wrong address, TLS...).
    case unreachable(String)
    case notSignedIn
    /// The server address isn't usable.
    case badServer(String)

    var errorDescription: String? {
        switch self {
        case let .http(status, code, message):
            if status == 401 && code != "invalid_code" { return "Your HITOP Cloud sign-in has ended: sign in again." }
            return message.isEmpty ? "HTTP \(status)" : message
        case let .unreachable(why): return "Couldn't reach HITOP Cloud (\(why))."
        case .notSignedIn: return "Sign in to HITOP Cloud first."
        case let .badServer(why): return why
        }
    }

    var status: Int? { if case let .http(s, _, _) = self { return s }; return nil }
    var code: String? { if case let .http(_, c, _) = self { return c }; return nil }
}

/// ST1 store mode (App Store 3.1.1/3.1.3): what the server says about plans, without where to see,
/// change or buy one. The web app's `plan-link.js` `withoutPlanLink`; the app never reads
/// `upgradeUrl` and never shows a price, a plan offer or a purchase link.
enum StoreText {
    private static let planList = "(?:Pro|Team|Business|Enterprise)"

    static func withoutPlanLink(_ text: String) -> String {
        var s = text
        func replace(_ pattern: String, _ with: String) {
            guard let re = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) else { return }
            s = re.stringByReplacingMatches(in: s, range: NSRange(s.startIndex..., in: s), withTemplate: with)
        }
        replace(#"\s*See or change the plan at \S+"#, "")
        replace(#"\bcome with \#(planList)(?:, \#(planList))*(?:,? and \#(planList))? plans?\b"#, "aren't part of this plan")
        replace(#",?\s*https?://[^\s)]*/(?:plan|pricing|billing|checkout)\b[^\s)]*"#, "")
        replace(#"\s+([.,;)])"#, "$1")
        return s.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

// MARK: - Account model

struct CloudAccount: Codable, Hashable, Sendable {
    var server: String
    var email: String
    var userId: String
    var keyId: String
    var deviceName: String
}

/// The session and the device's secret key (Keychain).
struct CloudSecret: Codable, Hashable, Sendable {
    var session: String?
    var secret: String
}

struct CloudOrg: Codable, Hashable, Sendable, Identifiable {
    var id: String
    var name: String
    var personal: Bool
    var role: String
    var plan: String
}

/// cloud.json: everything about HITOP Cloud that isn't secret.
struct CloudSettings: Codable, Hashable, Sendable {
    static let defaultServer = "https://sync.hitop.rws8.tech"

    var server: String = CloudSettings.defaultServer
    /// Signed in: this account.
    var account: CloudAccount?
    /// Signed out: the device this app was for an account, kept so signing in to it again is the
    /// same device (not one more toward the device limit).
    var device: CloudAccount?
    /// The organization the Collections tab shows (nil: the personal one).
    var org: String?
    /// The account's organizations, as last fetched (the pill shows the current one's name).
    var orgs: [CloudOrg] = []
}

protocol CloudSettingsStore: Sendable {
    func load() -> CloudSettings
    func save(_ settings: CloudSettings) throws
}

struct FileCloudSettingsStore: CloudSettingsStore {
    let file: URL

    init(root: URL) { file = root.appendingPathComponent("cloud.json") }

    func load() -> CloudSettings {
        guard let data = try? Data(contentsOf: file) else { return CloudSettings() }
        return (try? JSONDecoder().decode(CloudSettings.self, from: data)) ?? CloudSettings()
    }

    func save(_ settings: CloudSettings) throws {
        let e = JSONEncoder()
        e.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try e.encode(settings).write(to: file, options: [.atomic])
    }
}

final class MemoryCloudSettingsStore: CloudSettingsStore, @unchecked Sendable {
    private let lock = NSLock()
    private var value: CloudSettings

    init(_ value: CloudSettings = CloudSettings()) { self.value = value }

    func load() -> CloudSettings { lock.lock(); defer { lock.unlock() }; return value }
    func save(_ settings: CloudSettings) throws { lock.lock(); defer { lock.unlock() }; value = settings }
}

/// Session + device secret per server host: the Keychain in the app (HitopKit's store, service
/// `hitop`, account `ios/cloud/<host>`), memory in tests.
struct CloudCredentials: Sendable {
    let store: SecretStore

    static func account(_ server: String) -> String { "ios/cloud/\(URL(string: server)?.host ?? server)" }

    func load(_ server: String) -> CloudSecret? {
        guard let text = (try? store.get(account: Self.account(server))) ?? nil, let data = text.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(CloudSecret.self, from: data)
    }

    func save(_ server: String, _ secret: CloudSecret?) {
        let account = Self.account(server)
        if let secret, let data = try? JSONEncoder().encode(secret) {
            try? store.set(account: account, value: String(decoding: data, as: UTF8.self))
        } else {
            _ = try? store.delete(account: account)
        }
    }
}

/// What the sign-in sheet can offer (`GET /v1/auth/methods`).
struct AuthMethods: Hashable, Sendable {
    var apple = false
    var google = false
    var github = false

    /// In the order the sheet shows them (Apple first on Apple's platforms).
    var providers: [String] { [apple ? "apple" : nil, google ? "google" : nil, github ? "github" : nil].compactMap { $0 } }

    static func name(_ kind: String) -> String {
        switch kind {
        case "apple": "Apple"
        case "google": "Google"
        case "github": "GitHub"
        default: kind
        }
    }
}

struct CloudDevice: Hashable, Sendable, Identifiable {
    var id: String
    var name: String
    var createdAt: Int?
    var lastUsedAt: Int?
    /// How many shared workspaces' keys it holds (I2b uses them).
    var workspaces: Int
    var thisDevice: Bool
}

/// A sign-in that would add a device past the plan's limit: nothing was added. Remove one of
/// `devices` (`removeDevice(_:at:)`) and `retry`. Neutral words only (store mode): no plan, no
/// price, no link.
struct DeviceLimit: Error, Sendable {
    var server: String
    var session: String
    var user: (id: String, email: String)
    var max: Int
    var devices: [CloudDevice]
}

/// One usage meter of the account's organization (`GET /v1/orgs/:org/usage`), as the web app's
/// `org.js` `meters` with the store build's wording.
struct UsageMeter: Hashable, Sendable, Identifiable {
    var id: String
    var label: String
    var used: Int
    var max: Int?
    var level: String
    var text: String
    var note: String

    var fraction: Double { guard let max, max > 0 else { return 0 }; return min(1, Double(used) / Double(max)) }
}

struct CloudUsage: Hashable, Sendable {
    var plan: String
    var planInForce: String
    var meters: [UsageMeter]

    static let planNames = ["free": "Free", "pro": "Pro", "team": "Team", "business": "Business", "enterprise": "Enterprise"]

    var planName: String { Self.planNames[planInForce] ?? planInForce.capitalized }
    var ended: Bool { planInForce != plan }

    init(plan: String, planInForce: String, meters: [UsageMeter]) {
        self.plan = plan
        self.planInForce = planInForce
        self.meters = meters
    }

    init(json j: JSONAny) {
        plan = j["plan"]?.string ?? "free"
        planInForce = j["planInForce"]?.string ?? plan
        let u = j["usage"] ?? .null
        let l = j["limits"] ?? .null
        func bytes(_ n: Int) -> String {
            let gb = 1024.0 * 1024 * 1024, mb = 1024.0 * 1024
            let d = Double(n)
            return d >= gb ? String(format: d >= 10 * gb ? "%.0f GB" : "%.1f GB", d / gb) : "\(Int((d / mb).rounded())) MB"
        }
        let rows: [(String, String, Int?, Int?, (Int) -> String)] = [
            ("Seats", "seats", u["seats"]?.int, l["seats"]?.int, String.init),
            ("Shared collections", "workspaces", u["workspaces"]?.int, l["workspaces"]?.int, String.init),
            ("Storage", "storage", u["storageBytes"]?.int, l["storageBytes"]?.int, bytes),
            ("Devices", "devices", u["devices"]?.int, l["devices"]?.int, String.init),
            ("CI runners", "runners", u["runners"]?.int, l["runners"]?.int, String.init),
        ]
        meters = rows.compactMap { label, id, used, max, fmt in
            guard let used, !(max == 0 && used == 0) else { return nil }
            let percent = max.map { $0 > 0 ? Swift.min(100, Int((Double(used) / Double($0) * 100).rounded())) : 0 } ?? 0
            let lastOne = max != nil && max! >= 2 && id != "storage" && max! - used == 1
            let level = max == nil ? "ok" : used >= max! ? "full" : percent >= 80 || lastOne ? "warn" : "ok"
            let text = max.map { "\(fmt(used)) of \(fmt($0))" } ?? "\(fmt(used)) · no limit"
            let note = level == "full" ? (id == "seats" ? "Full: every seat is taken" : "Full: new ones are refused until there is room")
                : level == "warn" ? (lastOne ? "Nearly full: 1 left" : "Nearly full") : ""
            return UsageMeter(id: id, label: label, used: used, max: max, level: level, text: text, note: note)
        }
    }
}

// MARK: - The client

@MainActor
@Observable
final class HitopCloud {
    private(set) var settings: CloudSettings
    /// The last sign-in or account error worth a line on the Account screen.
    var lastError: String?

    @ObservationIgnored let transport: CloudTransport
    @ObservationIgnored let credentials: CloudCredentials
    @ObservationIgnored let settingsStore: CloudSettingsStore
    @ObservationIgnored let keyMaker: DeviceKeyMaker
    /// This device's name for the Devices list ("HITOP on iPhone").
    @ObservationIgnored var deviceName: String
    /// Called after signing in or out (LibrarySync links or stops).
    @ObservationIgnored var onAccountChange: (@MainActor () -> Void)?
    @ObservationIgnored private var pendingBrowser: (server: String, verifier: String, kind: String)?
    @ObservationIgnored private var pendingEmail: (server: String, email: String)?

    init(transport: CloudTransport, credentials: CloudCredentials, settingsStore: CloudSettingsStore,
         keyMaker: DeviceKeyMaker = RandomKeyMaker(), deviceName: String = "HITOP on iPhone") {
        self.transport = transport
        self.credentials = credentials
        self.settingsStore = settingsStore
        self.keyMaker = keyMaker
        self.deviceName = deviceName
        settings = settingsStore.load()
        // An account without its session in the Keychain (restored to another phone, Keychain
        // cleared) is signed out.
        if let a = settings.account, credentials.load(a.server)?.session == nil {
            settings.device = a
            settings.account = nil
        }
    }

    var account: CloudAccount? { settings.account }
    var signedIn: Bool { settings.account != nil }
    /// The library's owner: "<server> <userId>" (LibrarySync starts again for another).
    var accountKey: String { settings.account.map { "\($0.server) \($0.userId)" } ?? "" }

    private func persist() {
        try? settingsStore.save(settings)
    }

    // MARK: Server setting

    /// A server address: https (http only for this machine), no trailing slash.
    static func normalize(_ server: String) throws -> String {
        var s = server.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.isEmpty { s = CloudSettings.defaultServer }
        while s.hasSuffix("/") { s.removeLast() }
        guard let u = URL(string: s), let scheme = u.scheme?.lowercased(), let host = u.host, !host.isEmpty else {
            throw CloudError.badServer("Enter the server's address, like https://sync.example.com")
        }
        let local = ["localhost", "127.0.0.1", "::1", "[::1]"].contains(host)
        guard scheme == "https" || (scheme == "http" && local) else {
            throw CloudError.badServer("The server must be https:// (http:// only for localhost)")
        }
        return s
    }

    func setServer(_ server: String) throws {
        settings.server = try Self.normalize(server)
        persist()
    }

    // MARK: HTTP

    /// One request; errors (≥ 400) come back as responses, a missing answer throws `.unreachable`.
    func http(_ server: String, _ method: String, _ path: String, body: JSONAny? = nil, session: String? = nil) async throws -> CloudResponse {
        guard let url = URL(string: server + path) else { throw CloudError.badServer("Bad server address \(server)") }
        var headers = ["Accept": "application/json"]
        if body != nil { headers["Content-Type"] = "application/json" }
        if let session {
            headers["Authorization"] = "Bearer \(session)"
            // DD3: which device this is, for this account's own session (sessions are tied to it,
            // its last use shows on the Devices list). A native app has no CORS preflight to fail,
            // so it goes to every server; older ones ignore it.
            if let a = settings.account, a.server == server { headers["X-Hitop-Device"] = a.keyId }
        }
        let request = CloudRequest(method: method, url: url, headers: headers, body: body.map { $0.canonical })
        do {
            return try await transport.send(request)
        } catch {
            throw CloudError.unreachable(error.localizedDescription)
        }
    }

    static func error(_ r: CloudResponse) -> CloudError {
        let j = r.json
        let message = StoreText.withoutPlanLink(j?["message"]?.string ?? "")
        return .http(status: r.status, code: j?["error"]?.string ?? "", message: message)
    }

    /// A request as the signed-in account; `allow`: error statuses returned instead of thrown.
    func call(_ method: String, _ path: String, body: JSONAny? = nil, allow: Set<Int> = []) async throws -> CloudResponse {
        guard let a = settings.account, let session = credentials.load(a.server)?.session else { throw CloudError.notSignedIn }
        let r = try await http(a.server, method, path, body: body, session: session)
        if (200..<300).contains(r.status) || allow.contains(r.status) { return r }
        throw Self.error(r)
    }

    // MARK: Sign-in

    /// The sign-in methods `server` offers; nil for a server older than HITOP Cloud C1b.
    func authMethods(_ server: String) async throws -> AuthMethods? {
        let r = try await http(try Self.normalize(server), "GET", "/v1/auth/methods")
        guard r.status == 200, let j = r.json else { return nil }
        return AuthMethods(apple: j["apple"]?.bool ?? false, google: j["google"]?.bool ?? false, github: j["github"]?.bool ?? false)
    }

    /// The iOS app's return address (sync-server `oidc::is_app`).
    nonisolated static let returnTo = "hitop://sign-in"

    /// Starts Continue with Apple/Google/GitHub: the provider page to open. `paste`: the server
    /// didn't take the app's return address (one older than this app), so its page shows a code to
    /// paste instead.
    func startBrowserSignIn(_ kind: String, server: String, returnTo: String? = HitopCloud.returnTo) async throws -> (url: URL, paste: Bool) {
        let server = try Self.normalize(server)
        let (verifier, challenge) = CloudCrypto.verifier()
        var body: [String: JSONAny] = ["challenge": .string(challenge)]
        if let returnTo { body["returnTo"] = .string(returnTo) }
        var r = try await http(server, "POST", "/v1/auth/\(kind)/start", body: .object(body))
        var paste = returnTo == nil
        if r.status == 400, returnTo != nil, (r.json?["message"]?.string ?? "").range(of: "returnTo", options: .caseInsensitive) != nil {
            body.removeValue(forKey: "returnTo")
            r = try await http(server, "POST", "/v1/auth/\(kind)/start", body: .object(body))
            paste = true
        }
        if r.status == 404 { throw CloudError.http(status: 404, code: "no_provider", message: "This server has no sign-in with \(AuthMethods.name(kind)).") }
        guard r.status == 200 else { throw Self.error(r) }
        // Only a web page goes to the browser.
        guard let text = r.json?["url"]?.string, let url = URL(string: text), ["https", "http"].contains(url.scheme?.lowercased() ?? "") else {
            throw CloudError.badServer("The server answered without a sign-in page address")
        }
        pendingBrowser = (server, verifier, kind)
        return (url, paste)
    }

    /// The code from `hitop://sign-in?sso=<code>` (or a pasted one, or a whole pasted URL).
    static func code(from text: String) -> String? {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let c = URLComponents(string: t), c.scheme != nil {
            if let q = c.queryItems?.first(where: { $0.name == "sso" })?.value, !q.isEmpty { return q }
            if let f = c.fragment, f.hasPrefix("sso=") { return String(f.dropFirst(4)) }
            return nil
        }
        return t.isEmpty ? nil : t
    }

    /// The one-time code back (from the return address or pasted) → signed in. Throws `DeviceLimit`
    /// when this would be one device too many.
    @discardableResult
    func finishBrowserSignIn(_ code: String) async throws -> Bool {
        guard let p = pendingBrowser else { throw CloudError.badServer("This sign-in wasn't started here, or finished already: start again") }
        let r = try await http(p.server, "POST", "/v1/auth/\(p.kind)/finish", body: .object(["code": .string(code.trimmingCharacters(in: .whitespacesAndNewlines)), "verifier": .string(p.verifier)]))
        guard r.status == 200, let j = r.json else { throw Self.error(r) }
        pendingBrowser = nil
        return try await completeSignIn(p.server, j)
    }

    /// Sign-in with a mailed code (`POST /v1/auth/email`): not in the app's UI (as the web app);
    /// the end-to-end test signs in this way against a real server.
    func startEmailSignIn(server: String, email: String) async throws {
        let server = try Self.normalize(server)
        let r = try await http(server, "POST", "/v1/auth/email", body: .object(["email": .string(email)]))
        guard r.status == 202 else { throw Self.error(r) }
        pendingEmail = (server, email)
    }

    @discardableResult
    func finishEmailSignIn(_ code: String) async throws -> Bool {
        guard let p = pendingEmail else { throw CloudError.badServer("Ask for a code first") }
        let token = String(code.trimmingCharacters(in: .whitespacesAndNewlines).split(separator: "#").last ?? "")
        let r = try await http(p.server, "POST", "/v1/auth/email/verify", body: .object(["token": .string(token)]))
        guard r.status == 200, let j = r.json else { throw Self.error(r) }
        pendingEmail = nil
        return try await completeSignIn(p.server, j)
    }

    /// After a sign-in (`{session, user: {id, email}}`): keep this device's key or register one.
    /// True when a new device was added.
    func completeSignIn(_ server: String, _ reply: JSONAny) async throws -> Bool {
        guard let session = reply["session"]?.string, let userId = reply["user"]?["id"]?.string else {
            throw CloudError.badServer("The server's sign-in answer has no session")
        }
        let email = reply["user"]?["email"]?.string ?? ""
        let me = try await http(server, "GET", "/v1/me", session: session)
        guard me.status == 200, let mj = me.json else { throw Self.error(me) }
        let keys = mj["keys"]?.array ?? []
        // The same account on this device again (signed in, or signed out since) keeps its key.
        let old = settings.account ?? settings.device
        if let old, old.userId == userId, old.server == server, let secret = credentials.load(server)?.secret,
           keys.contains(where: { $0["id"]?.string == old.keyId }) {
            credentials.save(server, CloudSecret(session: session, secret: secret))
            signedIn(CloudAccount(server: server, email: email, userId: userId, keyId: old.keyId, deviceName: old.deviceName))
            return false
        }
        let key = keyMaker.make()
        let added = try await http(server, "POST", "/v1/me/keys", body: .object(["name": .string(deviceName), "publicKey": .string(key.publicKey)]), session: session)
        if added.status == 402, added.json?["limit"]?.string == "devices" {
            let devices = keys.map { k in
                CloudDevice(id: k["id"]?.string ?? "", name: k["name"]?.string ?? "", workspaces: 0, thisDevice: false)
            }
            throw DeviceLimit(server: server, session: session, user: (userId, email), max: added.json?["max"]?.int ?? keys.count, devices: devices)
        }
        guard added.status == 201, let keyId = added.json?["id"]?.string else { throw Self.error(added) }
        credentials.save(server, CloudSecret(session: session, secret: key.secret))
        signedIn(CloudAccount(server: server, email: email, userId: userId, keyId: keyId, deviceName: deviceName))
        return true
    }

    private func signedIn(_ a: CloudAccount) {
        settings.account = a
        settings.device = nil
        settings.server = a.server
        if settings.org != nil && !settings.orgs.contains(where: { $0.id == settings.org }) { settings.org = nil }
        lastError = nil
        persist()
        onAccountChange?()
    }

    /// At the device limit: removes one of the account's devices with the pending session (409
    /// `key_in_use`: it holds keys to shared collections). Returns the limit without it.
    func removeDevice(_ id: String, at limit: DeviceLimit) async throws -> DeviceLimit {
        let r = try await http(limit.server, "DELETE", "/v1/me/keys/\(id)", session: limit.session)
        guard r.status == 204 || r.status == 200 || r.status == 404 else { throw Self.error(r) }
        var next = limit
        next.devices.removeAll { $0.id == id }
        return next
    }

    /// ...then the sign-in again (or `DeviceLimit` once more).
    @discardableResult
    func retry(_ limit: DeviceLimit) async throws -> Bool {
        try await completeSignIn(limit.server, .object(["session": .string(limit.session), "user": .object(["id": .string(limit.user.id), "email": .string(limit.user.email)])]))
    }

    /// Ends the session; this device's key stays (without the session), so signing in to the same
    /// account again is the same device.
    func signOut() async {
        guard let a = settings.account else { return }
        _ = try? await call("POST", "/v1/auth/logout", allow: [401])
        if let s = credentials.load(a.server) {
            credentials.save(a.server, CloudSecret(session: nil, secret: s.secret))
        }
        settings.device = a
        settings.account = nil
        settings.org = nil
        settings.orgs = []
        persist()
        onAccountChange?()
    }

    /// The server ended the session (401): signed out here too, the device kept.
    func sessionEnded() {
        guard let a = settings.account else { return }
        if let s = credentials.load(a.server) { credentials.save(a.server, CloudSecret(session: nil, secret: s.secret)) }
        settings.device = a
        settings.account = nil
        lastError = "Your HITOP Cloud sign-in has ended: sign in again."
        persist()
        onAccountChange?()
    }

    // MARK: Devices (DD3)

    /// The account's devices, oldest first; nil from a server without the list.
    func devices() async throws -> [CloudDevice]? {
        let r = try await call("GET", "/v1/me/devices", allow: [404])
        guard r.status == 200, let j = r.json else { return nil }
        return (j["devices"]?.array ?? []).map { d in
            CloudDevice(id: d["id"]?.string ?? "", name: d["name"]?.string ?? "", createdAt: d["createdAt"]?.int, lastUsedAt: d["lastUsedAt"]?.int,
                        workspaces: d["workspaces"]?.int ?? 0, thisDevice: d["thisDevice"]?.bool ?? (d["id"]?.string == settings.account?.keyId))
        }
    }

    func renameDevice(_ id: String, to name: String) async throws {
        let n = String(name.trimmingCharacters(in: .whitespacesAndNewlines).prefix(64))
        guard !n.isEmpty else { return }
        _ = try await call("PUT", "/v1/me/keys/\(id)", body: .object(["name": .string(n)]))
        if settings.account?.keyId == id {
            settings.account?.deviceName = n
            persist()
        }
    }

    /// Removes another of the account's devices (its sessions end too). `force` for one holding
    /// shared workspaces' keys (409 `key_in_use` otherwise).
    func removeDevice(_ id: String, force: Bool = false) async throws {
        _ = try await call("DELETE", "/v1/me/keys/\(id)?untied=true\(force ? "&force=true" : "")")
    }

    // MARK: Organizations and plan usage

    @discardableResult
    func loadOrgs() async throws -> [CloudOrg] {
        let r = try await call("GET", "/v1/orgs", allow: [404])
        guard r.status == 200, let j = r.json else { return [] }
        let orgs = (j["orgs"]?.array ?? []).map { o in
            CloudOrg(id: o["id"]?.string ?? "", name: o["name"]?.string ?? "", personal: o["personal"]?.bool ?? false,
                     role: o["role"]?.string ?? "", plan: o["plan"]?.string ?? "free")
        }
        settings.orgs = orgs
        if let current = settings.org, !orgs.contains(where: { $0.id == current }) { settings.org = nil }
        persist()
        return orgs
    }

    var personalOrg: CloudOrg? { settings.orgs.first { $0.personal } }

    /// The organization the Collections tab shows.
    var currentOrg: CloudOrg? { settings.orgs.first { $0.id == settings.org } ?? personalOrg }

    func selectOrg(_ id: String?) {
        settings.org = id == personalOrg?.id ? nil : id
        persist()
    }

    /// The organization a new collection is filed under: nil for the personal one.
    var orgForNew: String? { signedIn ? settings.org : nil }

    func usage(_ org: String) async throws -> CloudUsage {
        let r = try await call("GET", "/v1/orgs/\(org)/usage")
        return CloudUsage(json: r.json ?? .null)
    }
}
