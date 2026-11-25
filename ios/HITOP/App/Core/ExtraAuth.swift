import Foundation
import HitopKit

/// Opens a provider's sign-in page (ASWebAuthenticationSession in the app). `cancelled` is called
/// when the person closes it.
@MainActor
protocol OAuthBrowser: AnyObject, Sendable {
    func open(_ url: URL, cancelled: @escaping @MainActor () -> Void) throws
    func close()
}

/// OAuth 2.0, AWS Signature and Digest: the presets HitopKit doesn't have, through hitop-core
/// (SyncFFI `auth.rs`), hooked into HitopKit's runner. Each request's own preset, else its collection's.
struct ExtraAuth: Sendable {
    /// Request id → its preset.
    var presets: [UInt64: AuthSpec]
    /// Where a person signs in; nil (runs, intents): a token that is there, or an error.
    var browser: OAuthBrowser?

    static let sent: Set<String> = ["oauth2", "awsv4", "digest"]

    init(_ requests: [SavedRequest], collectionAuth: AuthSpec?, browser: OAuthBrowser? = nil) {
        self.browser = browser
        presets = [:]
        for r in requests { if let a = r.auth ?? collectionAuth, case .other = a { presets[r.id] = a } }
    }

    func install(_ options: inout RunOptions) {
        guard !presets.isEmpty else { return }
        options.authorize = { try await authorize($0, $1, $2) }
        options.retry = { try await retry($0, $1, $2, $3) }
    }

    /// The preset with its variables filled in, as hitop-core reads it.
    private func filled(_ r: HitopKit.Request, _ variables: [String: String]) throws -> (type: String, json: JSONAny)? {
        guard case let .other(type, fields)? = presets[r.id] else { return nil }
        guard Self.sent.contains(type) else { throw HTTPError(description: "\(type) auth isn't supported here: the request wasn't sent") }
        var out: [String: JSONAny] = ["type": .string(type)]
        for (k, v) in fields {
            if let s = v.string {
                out[k] = .string(try HitopKitBridge.fill(s, variables))
            } else {
                out[k] = v
            }
        }
        return (type, .object(out))
    }

    private static func pairs(_ headers: [(String, String)]) -> JSONAny { .array(headers.map { .array([.string($0.0), .string($0.1)]) }) }

    private static func headers(_ j: JSONAny) -> [(String, String)] {
        (j.array ?? []).compactMap { p in p.array.flatMap { $0.count == 2 ? ($0[0].string ?? "", $0[1].string ?? "") : nil } }
    }

    private func authorize(_ r: HitopKit.Request, _ out: OutgoingRequest, _ variables: [String: String]) async throws -> OutgoingRequest {
        guard let (type, auth) = try filled(r, variables) else { return out }
        var out = out
        switch type {
        case "awsv4":
            out.headers = Self.headers(try SyncCore.call("authSign", [
                "auth": auth, "method": .string(out.method), "url": .string(out.url), "headers": Self.pairs(out.headers),
                "body": .string(CloudCrypto.base64url([UInt8](out.body))),
            ]))
        case "oauth2":
            // A header set by hand wins, as everywhere in hitop.
            guard !out.headers.contains(where: { $0.0.lowercased() == "authorization" && !$0.1.isEmpty }) else { return out }
            out.headers.append(("Authorization", "Bearer \(try await token(auth))"))
        default:
            break
        }
        return out
    }

    private func retry(_ r: HitopKit.Request, _ out: OutgoingRequest, _ response: HTTPResponse, _ variables: [String: String]) async throws -> OutgoingRequest? {
        guard response.status == 401, let challenge = response.headers["www-authenticate"], let (type, auth) = try filled(r, variables), type == "digest" else { return nil }
        let j = try SyncCore.call("authDigest", ["auth": auth, "challenge": .string(challenge), "method": .string(out.method), "url": .string(out.url), "headers": Self.pairs(out.headers)])
        guard !j.isNull else { return nil }
        var again = out
        again.headers = Self.headers(j)
        return again
    }

    // MARK: OAuth 2.0

    /// Off the caller's thread: the token request blocks.
    private static func call(_ op: String, _ args: [String: JSONAny]) async throws -> JSONAny {
        try await Task.detached { try SyncCore.call(op, args) }.value
    }

    private func token(_ auth: JSONAny) async throws -> String {
        var j = try await Self.call("oauthToken", ["auth": auth])
        if j["needsSignIn"]?.bool == true {
            guard let browser else {
                throw HTTPError(description: "OAuth 2.0: sign in first by sending one request that uses it, then run the collection")
            }
            try await Self.signIn(auth, browser)
            j = try await Self.call("oauthToken", ["auth": auth])
        }
        guard let t = j["token"]?.string else { throw HTTPError(description: "OAuth 2.0: no token") }
        return t
    }

    @MainActor
    private static func signIn(_ auth: JSONAny, _ browser: OAuthBrowser) async throws {
        let id = try SyncCore.call("oauthSignIn", ["auth": auth])["id"] ?? .null
        let cancel: @MainActor () -> Void = { _ = try? SyncCore.call("oauthSignInCancel", ["id": id]) }
        defer {
            browser.close()
            cancel()
        }
        while true {
            try Task.checkCancellation()
            let n = try SyncCore.call("oauthSignInNext", ["id": id])
            if let e = n["error"]?.string { throw HTTPError(description: e) }
            if n["done"]?.bool == true { return }
            if let page = n["page"]?.string, let url = URL(string: page) { try browser.open(url, cancelled: cancel) }
            try await Task.sleep(for: .milliseconds(150))
        }
    }

    static func forgetTokens() { _ = try? SyncCore.call("oauthForget") }
}
