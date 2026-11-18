import Foundation

/// An in-memory hitop sync server: the unit tests' HTTP layer and, behind `StubURLProtocol`
/// (host `cloud.hitop.test`), the UI tests' HITOP Cloud. It answers what the app uses with the
/// shapes and rules of sync-server (`auth.rs` sessions, `oidc.rs` start/finish with the
/// verifier, `devices.rs`, `limits.rs` 402 `devices`, `library.rs` versions, cursor, 409 with
/// `current`, tombstones, `deny_unknown_fields` bodies, `orgs.rs`). The end-to-end test checks the
/// same client against the real server.
final class FakeCloud: CloudTransport, @unchecked Sendable {
    static let host = "cloud.hitop.test"
    static let server = "https://\(host)"

    struct Item {
        var kind: String
        var id: String
        var version: Int
        var seq: Int
        var data: JSONAny
        var secrets: JSONAny?
        var deleted: Bool
        var updatedAt: Int
    }

    struct Device {
        var id: String
        var name: String
        var publicKey: String
        var createdAt: Int
        var lastUsedAt: Int?
        var workspaces = 0
    }

    struct User {
        var id: String
        var email: String
        var devices: [Device] = []
        var items: [String: Item] = [:]
        var seq = 0
    }

    private let lock = NSLock()
    private var users: [String: User] = [:]
    private var sessions: [String: String] = [:]
    private var flows: [String: (challenge: String, email: String)] = [:]
    private var counter = 0

    // Settings of the scenario.
    var methods = (apple: true, google: true, github: true)
    /// The account a browser sign-in signs in as.
    var signInEmail = "ada@example.com"
    /// Devices one account may have (nil: no limit, a self-hosted server).
    var deviceLimit: Int?
    /// The server keeps secret values (it has a host key).
    var secretsStored = true
    /// A server older than the app's return address: refuses `returnTo`.
    var refusesAppReturn = false
    /// Offline: every request fails.
    var offline = false
    /// Every library write answers this status (500: down for a moment).
    var libraryFailure: Int?
    /// UI test: the first update of a collection meets another device's change first.
    var conflictOnFirstUpdate = false
    /// Every request seen: "METHOD path" (tests check what was sent).
    private(set) var log: [String] = []
    /// The last request's headers.
    private(set) var lastHeaders: [String: String] = [:]

    init() {}

    /// The app in UI tests and screenshots.
    nonisolated(unsafe) static var shared = FakeCloud()

    private func locked<T>(_ f: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return f()
    }

    private func next() -> Int {
        counter += 1
        return counter
    }

    private var now: Int { Int(Date().timeIntervalSince1970) }

    // MARK: Test setup

    @discardableResult
    func addUser(_ email: String) -> String {
        locked { userID(email) }
    }

    private func userID(_ email: String) -> String {
        if let u = users.values.first(where: { $0.email == email }) { return u.id }
        let id = "user-\(next())"
        users[id] = User(id: id, email: email)
        return id
    }

    /// A device the account already has (another phone, a browser).
    func addDevice(_ email: String, name: String, workspaces: Int = 0) {
        locked {
            let id = userID(email)
            users[id]!.devices.append(Device(id: "key\(next())", name: name, publicKey: CloudCrypto.base64url(CloudCrypto.randomBytes(32)), createdAt: now, workspaces: workspaces))
        }
    }

    func devices(_ email: String) -> [String] {
        locked { users.values.first { $0.email == email }?.devices.map(\.name) ?? [] }
    }

    /// Another device writes an item (as `PUT` does).
    func write(_ email: String, kind: String, id: String, data: JSONAny, secrets: JSONAny? = nil) {
        locked { _ = put(userID(email), kind, id, data, secrets) }
    }

    func deleteItem(_ email: String, kind: String, id: String) {
        locked {
            let uid = userID(email)
            guard var it = users[uid]!.items["\(kind)/\(id)"], !it.deleted else { return }
            users[uid]!.seq += 1
            it.version += 1
            it.seq = users[uid]!.seq
            it.deleted = true
            it.data = .null
            it.secrets = nil
            users[uid]!.items["\(kind)/\(id)"] = it
        }
    }

    func item(_ email: String, kind: String, id: String) -> Item? {
        locked { users.values.first { $0.email == email }?.items["\(kind)/\(id)"] }
    }

    func items(_ email: String) -> [Item] {
        locked { users.values.first { $0.email == email }.map { Array($0.items.values) } ?? [] }
    }

    func clearLog() { locked { log = [] } }

    // MARK: CloudTransport

    func send(_ request: CloudRequest) async throws -> CloudResponse {
        if offline { throw URLError(.notConnectedToInternet) }
        return handle(request)
    }

    // MARK: The server

    private func reply(_ status: Int, _ json: JSONAny) -> CloudResponse { CloudResponse(status: status, body: json.canonical) }

    private func fail(_ status: Int, _ code: String, _ message: String, _ extra: [String: JSONAny] = [:]) -> CloudResponse {
        var o: [String: JSONAny] = ["error": .string(code), "message": .string(message)]
        for (k, v) in extra { o[k] = v }
        return reply(status, .object(o))
    }

    func handle(_ request: CloudRequest) -> CloudResponse {
        locked { () -> CloudResponse in
            let path = request.url.path
            let query = Dictionary((URLComponents(url: request.url, resolvingAgainstBaseURL: false)?.queryItems ?? []).map { ($0.name, $0.value ?? "") }) { a, _ in a }
            log.append("\(request.method) \(path)\(query.isEmpty ? "" : "?" + query.keys.sorted().map { "\($0)=\(query[$0]!)" }.joined(separator: "&"))")
            lastHeaders = request.headers
            let lower = Dictionary(request.headers.map { ($0.key.lowercased(), $0.value) }) { a, _ in a }
            func header(_ name: String) -> String? { lower[name] }
            let body = request.body.flatMap { try? JSONDecoder().decode(JSONAny.self, from: $0) }
            if let body, body.object == nil { return fail(400, "bad_request", "invalid JSON body") }
            let parts = path.split(separator: "/").map(String.init)
            // "POST /v1/auth/:/start": the request's method and path, ":" = any one segment (returned).
            func at(_ pattern: String) -> [String]? {
                let p = pattern.split(separator: " ", maxSplits: 1)
                let want = p[1].split(separator: "/").map(String.init)
                guard String(p[0]) == request.method, want.count == parts.count else { return nil }
                var got: [String] = []
                for (w, s) in zip(want, parts) {
                    if w == ":" { got.append(s) } else if w != s { return nil }
                }
                return got
            }
            let user: String? = header("authorization").flatMap { a in
                a.hasPrefix("Bearer ") ? sessions[String(a.dropFirst(7))] : nil
            }
            if let user, let device = header("x-hitop-device"), let i = users[user]?.devices.firstIndex(where: { $0.id == device }) {
                users[user]!.devices[i].lastUsedAt = now
            }
            if false {
            } else if at("GET /v1/auth/methods") != nil {
                return reply(200, .object(["password": .object(["enabled": .bool(false)]), "emailCode": .bool(true), "passkey": .bool(true), "sso": .null,
                                           "google": .bool(methods.google), "github": .bool(methods.github), "apple": .bool(methods.apple)]))
            } else if let a = at("POST /v1/auth/:/start"), case let kind = a[0], ["google", "github", "apple"].contains(kind) {
                guard let b = body?.object, Set(b.keys).isSubset(of: ["challenge", "returnTo"]), let challenge = b["challenge"]?.string else {
                    return fail(400, "bad_request", "invalid JSON body")
                }
                let off = kind == "google" ? !methods.google : kind == "github" ? !methods.github : !methods.apple
                if off { return fail(404, "provider_off", "sign-in with \(kind) isn't configured on this server") }
                if let r = b["returnTo"]?.string, !r.isEmpty, refusesAppReturn || r != HitopCloud.returnTo {
                    return fail(400, "bad_request", "returnTo must be http://127.0.0.1:<port>/... or a page of an allowed web origin")
                }
                let code = "code-\(next())"
                flows[code] = (challenge, signInEmail)
                return reply(200, .object(["url": .string("\(Self.server)/authorize/\(kind)?code=\(code)"), "expiresAt": .number(Double(now + 600))]))
            } else if let a = at("POST /v1/auth/:/finish"), case let kind = a[0] {
                guard let b = body?.object, Set(b.keys) == ["code", "verifier"], let code = b["code"]?.string, let verifier = b["verifier"]?.string else {
                    return fail(400, "bad_request", "invalid JSON body")
                }
                guard let flow = flows.removeValue(forKey: code), CloudCrypto.challenge(verifier) == flow.challenge else {
                    return fail(401, "invalid_code", "this sign-in code is invalid, used or expired; sign in again")
                }
                _ = kind
                return session(userID(flow.email))
            } else if at("POST /v1/auth/email") != nil {
                guard let email = body?["email"]?.string else { return fail(400, "bad_request", "email") }
                flows["mail-\(email)"] = ("", email)
                return reply(202, .object([:]))
            } else if at("POST /v1/auth/email/verify") != nil {
                guard let token = body?["token"]?.string, let flow = flows.removeValue(forKey: token) else {
                    return fail(401, "invalid_code", "this sign-in link is invalid or expired")
                }
                return session(userID(flow.email))
            } else {
                _ = 0
            }
            guard let uid = user, users[uid] != nil else { return fail(401, "unauthorized", "sign in first") }
            if false {
            } else if at("POST /v1/auth/logout") != nil {
                sessions = sessions.filter { $0.value != uid || "Bearer \($0.key)" != header("authorization") }
                return reply(200, .object([:]))
            } else if at("GET /v1/me") != nil {
                let u = users[uid]!
                return reply(200, .object(["id": .string(u.id), "email": .string(u.email), "devices": .bool(true),
                                           "keys": .array(u.devices.map { .object(["id": .string($0.id), "name": .string($0.name), "publicKey": .string($0.publicKey)]) })]))
            } else if at("POST /v1/me/keys") != nil {
                guard let b = body?.object, Set(b.keys) == ["name", "publicKey"], let key = b["publicKey"]?.string, CloudCrypto.unbase64url(key)?.count == 32 else {
                    return fail(400, "bad_request", "publicKey must be a 32-byte X25519 public key")
                }
                let have = users[uid]!.devices.count
                if let max = deviceLimit, have + 1 > max {
                    return fail(402, "limit", "plan limit: the pro plan of your personal account allows \(max) devices; nothing changed. See or change the plan at https://cloud.hitop.rws8.tech/orgs/personal-\(uid)/plan",
                                ["limit": .string("devices"), "plan": .string("pro"), "org": .string("personal-\(uid)"), "used": .number(Double(have)), "max": .number(Double(max)),
                                 "upgradeUrl": .string("https://cloud.hitop.rws8.tech/orgs/personal-\(uid)/plan")])
                }
                let id = "key\(next())"
                users[uid]!.devices.append(Device(id: id, name: b["name"]?.string ?? "", publicKey: key, createdAt: now))
                return reply(201, .object(["id": .string(id)]))
            } else if let a = at("PUT /v1/me/keys/:"), case let id = a[0] {
                guard let b = body?.object, Set(b.keys) == ["name"], let name = b["name"]?.string else { return fail(400, "bad_request", "invalid JSON body") }
                guard let i = users[uid]!.devices.firstIndex(where: { $0.id == id }) else { return fail(404, "not_found", "no such device") }
                users[uid]!.devices[i].name = name
                return reply(200, .object(["id": .string(id), "name": .string(name)]))
            } else if let a = at("DELETE /v1/me/keys/:"), case let id = a[0] {
                guard let i = users[uid]!.devices.firstIndex(where: { $0.id == id }) else { return fail(404, "not_found", "no such device") }
                if header("x-hitop-device") == id { return fail(409, "this_device", "this is the device you are using: sign out here instead") }
                if users[uid]!.devices[i].workspaces > 0 && query["force"] != "true" {
                    return fail(409, "key_in_use", "this device key holds workspace keys; remove it with force=true (its workspaces then rotate their key)")
                }
                users[uid]!.devices.remove(at: i)
                return CloudResponse(status: 204, body: Data())
            } else if at("GET /v1/me/devices") != nil {
                let me = header("x-hitop-device")
                let list: [JSONAny] = users[uid]!.devices.map { d in
                    .object(["id": .string(d.id), "name": .string(d.name), "createdAt": .number(Double(d.createdAt)),
                             "lastUsedAt": d.lastUsedAt.map { .number(Double($0)) } ?? .null, "workspaces": .number(Double(d.workspaces)),
                             "onlyHere": .number(0), "thisDevice": .bool(d.id == me)])
                }
                return reply(200, .object(["devices": .array(list), "limit": deviceLimit.map { .number(Double($0)) } ?? .null]))
            } else if at("GET /v1/orgs") != nil {
                return reply(200, .object(["orgs": .array([org("personal-\(uid)", users[uid]!.email, personal: true, role: "owner"),
                                                           org("org-acme", "Acme", personal: false, role: "member")])]))
            } else if let a = at("GET /v1/orgs/:/usage"), case let o = a[0] {
                let personal = o.hasPrefix("personal-")
                let limits: JSONAny = deviceLimit == nil
                    ? .object(["seats": .null, "guestViewers": .number(0), "workspaces": .null, "storageBytes": .null, "devices": .null, "runners": .null, "invitesPerDay": .null])
                    : .object(["seats": .number(personal ? 3 : 10), "guestViewers": .number(0), "workspaces": .number(25), "storageBytes": .number(2 * 1_073_741_824),
                               "devices": .number(Double(deviceLimit!)), "runners": .number(10), "invitesPerDay": .number(50)])
                return reply(200, .object(["org": .string(o), "plan": .string(personal ? "pro" : "team"), "planInForce": .string(personal ? "pro" : "team"), "status": .string("active"),
                                           "seats": .number(1), "enforced": .bool(deviceLimit != nil), "limits": limits,
                                           "upgradeUrl": .string("https://cloud.hitop.rws8.tech/orgs/\(o)/plan"),
                                           "usage": .object(["seats": .number(1), "guests": .number(0), "workspaces": .number(2), "storageBytes": .number(48_234_496),
                                                             "devices": .number(Double(users[uid]!.devices.count)), "runners": .number(0), "invitesPerDay": .number(0), "writesToday": .number(3)])]))
            } else if at("GET /v1/library") != nil {
                return library(uid, since: query["since"].flatMap { Int($0) })
            } else if let a = at("PUT /v1/library/:/:"), case let (kind, id) = (a[0], a[1]) {
                if let f = libraryFailure { return fail(f, "unavailable", "the server is busy") }
                guard let b = body?.object, Set(b.keys).isSubset(of: ["data", "secrets", "baseVersion"]), let data = b["data"], data.object != nil else {
                    return fail(400, "bad_request", "invalid JSON body")
                }
                if let bad = checkRef(kind, id) { return bad }
                let base = b["baseVersion"]?.int ?? 0
                let key = "\(kind)/\(id)"
                if conflictOnFirstUpdate, kind == "collection", base > 0, var other = users[uid]!.items[key]?.data.object {
                    conflictOnFirstUpdate = false
                    other["name"] = .string("\(other["name"]?.string ?? "") (edited)")
                    _ = put(uid, kind, id, .object(other), nil)
                }
                let current = users[uid]!.items[key]
                let seen = current.map { $0.deleted ? 0 : $0.version } ?? 0
                if base != seen { return fail(409, "conflict", "this item changed in another browser; the server's version is attached", ["current": current.map(itemJSON) ?? .null]) }
                let it = put(uid, kind, id, data, b["secrets"])
                return reply(200, .object(["kind": .string(kind), "id": .string(id), "version": .number(Double(it.version)), "seq": .number(Double(it.seq)),
                                           "updatedAt": .number(Double(it.updatedAt)), "secretsStored": .bool(b["secrets"]?.object?.isEmpty ?? true ? true : secretsStored)]))
            } else if let a = at("DELETE /v1/library/:/:"), case let (kind, id) = (a[0], a[1]) {
                if let f = libraryFailure { return fail(f, "unavailable", "the server is busy") }
                if let bad = checkRef(kind, id) { return bad }
                let key = "\(kind)/\(id)"
                let base = Int(query["baseVersion"] ?? "0") ?? 0
                guard var it = users[uid]!.items[key], !it.deleted else {
                    return reply(200, .object(["kind": .string(kind), "id": .string(id), "deleted": .bool(true)]))
                }
                if it.version != base { return fail(409, "conflict", "this item changed in another browser; the server's version is attached", ["current": itemJSON(it)]) }
                users[uid]!.seq += 1
                it.version += 1
                it.seq = users[uid]!.seq
                it.deleted = true
                it.data = .null
                it.secrets = nil
                users[uid]!.items[key] = it
                return reply(200, .object(["kind": .string(kind), "id": .string(id), "version": .number(Double(it.version)), "seq": .number(Double(it.seq)), "deleted": .bool(true)]))
            } else {
                return fail(404, "not_found", "no such endpoint")
            }
        }
    }

    private func checkRef(_ kind: String, _ id: String) -> CloudResponse? {
        guard ["collection", "environments", "globals"].contains(kind) else { return fail(400, "bad_request", "kind is collection, environments or globals") }
        if id.isEmpty || id.count > 64 { return fail(400, "bad_request", "an item id is 1 to 64 letters, digits, - _ or .") }
        if kind != "collection" && id != "all" { return fail(400, "bad_request", "\(kind) has one item, id all") }
        return nil
    }

    private func org(_ id: String, _ name: String, personal: Bool, role: String) -> JSONAny {
        .object(["id": .string(id), "name": .string(name), "personal": .bool(personal), "role": .string(role), "plan": .string(personal ? "pro" : "team"),
                 "status": .string("active"), "seats": .number(1), "region": .string("eu"), "createdAt": .number(0), "members": .number(1), "workspaces": .number(0)])
    }

    private func session(_ uid: String) -> CloudResponse {
        let token = "session-\(next())-\(CloudCrypto.base64url(CloudCrypto.randomBytes(8)))"
        sessions[token] = uid
        let u = users[uid]!
        return reply(200, .object(["session": .string(token), "expiresAt": .number(Double(now + 86400 * 30)),
                                   "user": .object(["id": .string(u.id), "email": .string(u.email)])]))
    }

    private func put(_ uid: String, _ kind: String, _ id: String, _ data: JSONAny, _ secrets: JSONAny?) -> Item {
        let key = "\(kind)/\(id)"
        users[uid]!.seq += 1
        let version = (users[uid]!.items[key]?.version ?? 0) + 1
        let keep = secretsStored && !(secrets?.object?.isEmpty ?? true) ? secrets : nil
        let it = Item(kind: kind, id: id, version: version, seq: users[uid]!.seq, data: data, secrets: keep, deleted: false, updatedAt: now)
        users[uid]!.items[key] = it
        return it
    }

    private func itemJSON(_ it: Item) -> JSONAny {
        var o: [String: JSONAny] = ["kind": .string(it.kind), "id": .string(it.id), "version": .number(Double(it.version)), "seq": .number(Double(it.seq)),
                                    "updatedAt": .number(Double(it.updatedAt)), "deleted": .bool(it.deleted)]
        if !it.deleted {
            o["data"] = it.data
            if let s = it.secrets { o["secrets"] = s }
        }
        return .object(o)
    }

    private func library(_ uid: String, since: Int?) -> CloudResponse {
        let u = users[uid]!
        let items = u.items.values.filter { since == nil ? !$0.deleted : $0.seq > since! }.sorted { $0.seq < $1.seq }
        let cursor = u.items.values.map(\.seq).max() ?? 0
        return reply(200, .object(["items": .array(items.map(itemJSON)), "cursor": .number(Double(cursor)), "secretsStored": .bool(secretsStored), "git": .array([])]))
    }
}
