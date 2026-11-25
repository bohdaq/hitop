import Foundation

/// An in-memory hitop sync server: the unit tests' HTTP layer and, behind `StubURLProtocol`
/// (host `cloud.hitop.test`), the UI tests' HITOP Cloud. It answers what the app uses with the
/// shapes and rules of sync-server (`auth.rs` sessions, `oidc.rs` start/finish with the
/// verifier, `devices.rs`, `limits.rs` 402 `devices`, `library.rs` versions, cursor, 409 with
/// `current`, tombstones, `deny_unknown_fields` bodies, `orgs.rs`) and, for shared collections
/// (I2b), `workspaces.rs`/`teams.rs`/`blobs.rs`: wraps for exactly every device of every member,
/// a format 2 wrap naming one of the caller's own keys, roles (viewer 403, non-member 404),
/// removal only with the next epoch's wraps, `stale_epoch`, version + version vector 409s,
/// invites, and `teamenvs.rs`. The end-to-end test checks the same client against the real server.
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
    fileprivate var spaces: [String: FakeWorkspace] = [:]
    fileprivate var invites: [FakeInvite] = []
    /// Team environments of org-acme (`GET /v1/orgs/org-acme/environments` items).
    var teamEnvironments: [JSONAny] = []
    /// Invites get this token (UI tests type the code), else a random one.
    var fixedInviteToken: String?
    /// Creating a workspace answers the plan limit (402, with the plan link).
    var sharingLimit = false
    /// Blob writes from this address are answered but held back until someone else writes to the
    /// same workspace: the UI tests' "changed by someone else meanwhile".
    var holdWritesFrom: String?
    fileprivate var held: [(ws: String, id: String, blob: FakeBlob)] = []
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
    /// `DELETE /v1/me` answers 409 `transfer_ownership_first` with these (account.rs: the only
    /// owner of something other people use).
    var deleteBlockers: [JSONAny]?
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

    /// The account with this address exists (a deleted one doesn't).
    func hasAccount(_ email: String) -> Bool {
        locked { users.values.contains { $0.email == email } }
    }

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
            } else if let a = at("POST /v1/auth/:/start"), case let kind = a[0], ["google", "github", "apple", "sso"].contains(kind) {
                guard let b = body?.object, Set(b.keys).isSubset(of: kind == "sso" ? ["challenge", "returnTo", "email"] : ["challenge", "returnTo"]), let challenge = b["challenge"]?.string else {
                    return fail(400, "bad_request", "invalid JSON body")
                }
                if kind == "sso", !(b["email"]?.string ?? "").hasSuffix("@acme.test") { return fail(404, "org_sso_off", "no organization signs in with single sign-on for this address") }
                let off = kind == "sso" ? false : kind == "google" ? !methods.google : kind == "github" ? !methods.github : !methods.apple
                if off { return fail(404, "provider_off", "sign-in with \(kind) isn't configured on this server") }
                if let r = b["returnTo"]?.string, !r.isEmpty, refusesAppReturn || r != HitopCloud.returnTo {
                    return fail(400, "bad_request", "returnTo must be http://127.0.0.1:<port>/... or a page of an allowed web origin")
                }
                let code = "code-\(next())"
                flows[code] = (challenge, kind == "sso" ? b["email"]!.string! : signInEmail)
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
            } else if at("DELETE /v1/me") != nil {
            // account.rs: `confirm` = the address; the only owner of shared things is refused.
            guard let b = body?.object, Set(b.keys) == ["confirm"], let confirm = b["confirm"]?.string else {
                return fail(400, "bad_request", "send {\"confirm\": \"<the account's email address>\"}")
            }
            guard confirm.trimmingCharacters(in: .whitespaces).lowercased() == users[uid]!.email.lowercased() else {
                return fail(400, "confirm_mismatch", "confirm must be this account's email address")
            }
            if let blockers = deleteBlockers, !blockers.isEmpty {
                // account.rs `refuse_blocked`: organizations by name, shared collections counted.
                var what = blockers.filter { $0["kind"]?.string == "org" }.map { "the organization \($0["name"]?.string ?? "")" }
                let shared = blockers.filter { $0["kind"]?.string == "workspace" }.count
                if shared > 0 { what.append(shared == 1 ? "1 shared collection" : "\(shared) shared collections") }
                return fail(409, "transfer_ownership_first", "You are the only owner of \(what.joined(separator: " and ")) that other people use: make another member an owner, or remove the other members, then delete the account.", ["blockers": .array(blockers)])
            }
            let n = users[uid]!.devices.count
            users.removeValue(forKey: uid)
            sessions = sessions.filter { $0.value != uid }
            for (ws, var w) in spaces where w.role(uid) != nil {
                w.members.removeAll { $0.0 == uid }
                spaces[ws] = w
            }
            return reply(200, .object(["deleted": .bool(true), "workspacesDeleted": .number(0), "workspacesLeft": .number(0), "workspacesMoved": .number(0),
                                       "orgsDeleted": .number(1), "orgsLeft": .number(0), "subscriptionsCancelled": .number(0),
                                       "appleRevoked": .number(0), "appleNotRevoked": .number(0), "devices": .number(Double(n))]))
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
            } else if let r = workspaceRoute(uid, request.method, parts, query, body) {
                return r
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

// MARK: - Shared workspaces (sync-server workspaces.rs, teams.rs, blobs.rs, teamenvs.rs)

fileprivate struct FakeBlob {
    var version: Int
    var vv: [String: Int]
    var epoch: Int
    var deleted: Bool
    var content: String
    var seq: Int
}

fileprivate struct FakeWorkspace {
    var id: String
    var epoch = 1
    var seq = 0
    var org: String
    /// user id → role, in the order added.
    var members: [(String, String)] = []
    /// epoch → key id → wrapped (base64url).
    var wraps: [Int: [String: String]] = [:]
    var blobs: [String: FakeBlob] = [:]

    func role(_ uid: String) -> String? { members.first { $0.0 == uid }?.1 }
}

fileprivate struct FakeInvite {
    var id: String
    var workspace: String
    var token: String
    var email: String
    var role: String
    var acceptedBy: String?
}

extension FakeCloud {
    // Test setup and checks.

    /// Workspace ids, in no order.
    func workspaceIDs() -> [String] { locked { Array(spaces.keys) } }

    /// The role of `email` in `ws` (nil: not a member).
    func role(_ ws: String, _ email: String) -> String? {
        locked { users.values.first { $0.email == email }.flatMap { spaces[ws]?.role($0.id) } }
    }

    func epoch(_ ws: String) -> Int { locked { spaces[ws]?.epoch ?? 0 } }

    /// The stored envelopes of `ws` (tests check the server never sees plaintext).
    func envelopes(_ ws: String) -> [String] { locked { spaces[ws]?.blobs.values.map(\.content) ?? [] } }

    /// A malicious server: stores `wrapped` as `keyId`'s wrap of `epoch` (no checks).
    func replaceWrap(_ ws: String, keyId: String, epoch: Int, wrapped: String) {
        locked { spaces[ws]?.wraps[epoch, default: [:]][keyId] = wrapped }
    }

    /// The device key ids of `email`.
    func deviceKeys(_ email: String) -> [String] { locked { users.values.first { $0.email == email }?.devices.map(\.id) ?? [] } }

    func devicePublicKey(_ email: String) -> String? { locked { users.values.first { $0.email == email }?.devices.first?.publicKey } }

    fileprivate func deviceIDs(_ uid: String) -> [String] { users[uid]?.devices.map(\.id) ?? [] }

    /// The sender key id a format 2 wrap names (nil: format 1), or "" for a malformed one.
    fileprivate func wrapSender(_ b64: String) -> String? {
        guard let b = CloudCrypto.unbase64url(b64) else { return "" }
        if b.count == 80 { return nil }
        guard b.count >= 88, b[0] == 2 else { return "" }
        let len = Int(b[5]) << 8 | Int(b[6])
        guard len > 0, len <= 128, b.count == 7 + len + 80 else { return "" }
        return String(bytes: b[7..<(7 + len)], encoding: .utf8) ?? ""
    }

    /// `exact_wraps`: one well-formed wrap for each device of `members`, made by `uid`.
    fileprivate func exactWraps(_ uid: String, _ members: [String], _ wraps: JSONAny?) -> Result<[String: String], CloudResponse> {
        var given: [String: String] = [:]
        for w in wraps?.array ?? [] {
            guard let o = w.object, Set(o.keys) == ["keyId", "wrapped"], let id = o["keyId"]?.string, let x = o["wrapped"]?.string else {
                return .failure(fail(400, "bad_request", "invalid JSON body"))
            }
            if let bad = checkWrap(uid, id, x) { return .failure(bad) }
            if given[id] != nil { return .failure(fail(400, "bad_request", "two wrapped keys for \(id)")) }
            given[id] = x
        }
        let expected = Set(members.flatMap(deviceIDs))
        guard Set(given.keys) == expected else {
            return .failure(fail(400, "wraps_mismatch", "the workspace key must be wrapped for every device key of every member, and nothing else",
                                 ["missing": .array(expected.subtracting(given.keys).sorted().map(JSONAny.string)),
                                  "unexpected": .array(Set(given.keys).subtracting(expected).sorted().map(JSONAny.string))]))
        }
        return .success(given)
    }

    fileprivate func checkWrap(_ uid: String, _ keyId: String, _ wrapped: String) -> CloudResponse? {
        switch wrapSender(wrapped) {
        case nil: return nil
        case ""?: return fail(400, "bad_request", "wrapped key for \(keyId): not a format 1 or format 2 wrap")
        case let sender?:
            return deviceIDs(uid).contains(sender) ? nil : fail(400, "bad_request", "the wrapped key for \(keyId) names \(sender) as its sender, which is not one of your device keys")
        }
    }

    fileprivate func memberJSON(_ ws: FakeWorkspace, _ uid: String, _ role: String) -> JSONAny {
        let u = users[uid]!
        let keys: [JSONAny] = u.devices.map { d in
            .object(["id": .string(d.id), "name": .string(d.name), "publicKey": .string(d.publicKey),
                     "hasKey": .bool(ws.wraps[ws.epoch]?[d.id] != nil), "legacyEpochs": .array([])])
        }
        return .object(["userId": .string(uid), "email": .string(u.email), "role": .string(role), "runner": .bool(false), "keys": .array(keys)])
    }

    fileprivate func orgJSON(_ id: String) -> JSONAny {
        if id == "org-acme" { return .object(["id": .string(id), "name": .string("Acme"), "personal": .bool(false)]) }
        let owner = users[String(id.dropFirst("personal-".count))]?.email ?? id
        return .object(["id": .string(id), "name": .string(owner), "personal": .bool(true)])
    }

    fileprivate func blobJSON(_ id: String, _ b: FakeBlob, content: Bool) -> JSONAny {
        var o: [String: JSONAny] = ["id": .string(id), "version": .number(Double(b.version)), "vv": .object(b.vv.mapValues { .number(Double($0)) }),
                                    "epoch": .number(Double(b.epoch)), "deleted": .bool(b.deleted), "seq": .number(Double(b.seq))]
        if content { o["content"] = .string(b.content) } else { o["size"] = .number(Double(CloudCrypto.unbase64url(b.content)?.count ?? 0)) }
        return .object(o)
    }

    /// The workspace routes (called with the lock held); nil: not one of them.
    fileprivate func workspaceRoute(_ uid: String, _ method: String, _ parts: [String], _ query: [String: String], _ body: JSONAny?) -> CloudResponse? {
        func at(_ pattern: String) -> [String]? {
            let p = pattern.split(separator: " ", maxSplits: 1)
            let want = p[1].split(separator: "/").map(String.init)
            guard String(p[0]) == method, want.count == parts.count else { return nil }
            var got: [String] = []
            for (w, s) in zip(want, parts) {
                if w == ":" { got.append(s) } else if w != s { return nil }
            }
            return got
        }
        let b = body?.object ?? [:]
        func keys(_ allowed: Set<String>, required: Set<String> = []) -> CloudResponse? {
            Set(b.keys).isSubset(of: allowed) && required.isSubset(of: Set(b.keys)) ? nil : fail(400, "bad_request", "invalid JSON body")
        }
        /// The workspace as `uid` sees it, at least `role`.
        func space(_ id: String, _ need: String = "viewer") -> Result<FakeWorkspace, CloudResponse> {
            guard let ws = spaces[id], let r = ws.role(uid) else { return .failure(fail(404, "not_found", "no such workspace")) }
            let rank = ["viewer": 0, "editor": 1, "owner": 2]
            if rank[r]! < rank[need]! { return .failure(fail(403, "forbidden", "this needs the \(need) role")) }
            return .success(ws)
        }
        if at("GET /v1/workspaces") != nil {
            let list: [JSONAny] = spaces.values.filter { $0.role(uid) != nil }.sorted { $0.id < $1.id }.map { ws in
                .object(["id": .string(ws.id), "role": .string(ws.role(uid)!), "epoch": .number(Double(ws.epoch)), "seq": .number(Double(ws.seq)), "org": orgJSON(ws.org)])
            }
            return reply(200, .object(["workspaces": .array(list)]))
        }
        if at("POST /v1/workspaces") != nil {
            if let bad = keys(["id", "wraps", "org"], required: ["id", "wraps"]) { return bad }
            guard let id = b["id"]?.string, id.count == 32, id.allSatisfy({ $0.isHexDigit && !$0.isUppercase }) else {
                return fail(400, "bad_request", "id must be 32 lower-case hex digits (random, from the client)")
            }
            if spaces[id] != nil { return fail(409, "exists", "a workspace with this id exists") }
            if sharingLimit {
                return fail(402, "limit", "plan limit: the free plan of your personal account allows 0 shared collections; nothing changed. See or change the plan at https://cloud.hitop.rws8.tech/orgs/personal-\(uid)/plan",
                            ["limit": .string("workspaces"), "upgradeUrl": .string("https://cloud.hitop.rws8.tech/orgs/personal-\(uid)/plan")])
            }
            let org = b["org"]?.string ?? "personal-\(uid)"
            guard org == "personal-\(uid)" || org == "org-acme" else { return fail(403, "forbidden", "not a member of that organization") }
            switch exactWraps(uid, [uid], b["wraps"]) {
            case let .failure(r): return r
            case let .success(w):
                spaces[id] = FakeWorkspace(id: id, org: org, members: [(uid, "owner")], wraps: [1: w])
                return reply(201, .object(["id": .string(id), "epoch": .number(1)]))
            }
        }
        if let a = at("GET /v1/workspaces/:") {
            switch space(a[0]) {
            case let .failure(r): return r
            case let .success(ws):
                return reply(200, .object(["id": .string(ws.id), "epoch": .number(Double(ws.epoch)), "seq": .number(Double(ws.seq)), "bytes": .number(0),
                                           "role": .string(ws.role(uid)!), "members": .array(ws.members.map { memberJSON(ws, $0.0, $0.1) }), "rotationRequested": .null]))
            }
        }
        if let a = at("GET /v1/workspaces/:/wraps") {
            switch space(a[0]) {
            case let .failure(r): return r
            case let .success(ws):
                guard let key = query["keyId"], deviceIDs(uid).contains(key) else { return fail(404, "not_found", "keyId is not one of your device keys") }
                let list: [JSONAny] = ws.wraps.keys.sorted().compactMap { e in ws.wraps[e]?[key].map { .object(["epoch": .number(Double(e)), "wrapped": .string($0)]) } }
                return reply(200, .object(["epoch": .number(Double(ws.epoch)), "wraps": .array(list)]))
            }
        }
        if let a = at("POST /v1/workspaces/:/wraps") {
            if let bad = keys(["epoch", "wraps"], required: ["epoch", "wraps"]) { return bad }
            switch space(a[0]) {
            case let .failure(r): return r
            case var .success(ws):
                guard let e = b["epoch"]?.int, e >= 1, e <= ws.epoch else { return fail(400, "bad_request", "no such epoch") }
                var stored = 0
                for w in b["wraps"]?.array ?? [] {
                    guard let id = w["keyId"]?.string, let x = w["wrapped"]?.string else { return fail(400, "bad_request", "invalid JSON body") }
                    if let bad = checkWrap(uid, id, x) { return bad }
                    guard let owner = ws.members.first(where: { deviceIDs($0.0).contains(id) }) else { return fail(400, "bad_request", "\(id) is not a device key of a member") }
                    if ws.role(uid) == "viewer" && owner.0 != uid { return fail(403, "forbidden", "viewers can only add keys for their own devices") }
                    if ws.wraps[e]?[id] == nil {
                        ws.wraps[e, default: [:]][id] = x
                        stored += 1
                    }
                }
                spaces[ws.id] = ws
                return reply(200, .object(["epoch": .number(Double(e)), "added": .number(Double(b["wraps"]?.array?.count ?? 0)), "stored": .number(Double(stored))]))
            }
        }
        if let a = at("POST /v1/workspaces/:/members") {
            if let bad = keys(["email", "role", "wraps"], required: ["email", "role", "wraps"]) { return bad }
            switch space(a[0], "owner") {
            case let .failure(r): return r
            case var .success(ws):
                guard let email = b["email"]?.string?.lowercased(), let role = b["role"]?.string, ["owner", "editor", "viewer"].contains(role) else { return fail(400, "bad_request", "role must be owner, editor or viewer") }
                guard let new = users.values.first(where: { $0.email == email })?.id else { return fail(404, "not_found", "no account with this address (they sign in once first)") }
                if ws.role(new) != nil { return fail(409, "already_member", "already a member") }
                switch exactWraps(uid, [new], b["wraps"]) {
                case let .failure(r): return r
                case let .success(w):
                    if w.isEmpty { return fail(400, "bad_request", "that account has no device key yet") }
                    ws.members.append((new, role))
                    for (k, v) in w { ws.wraps[ws.epoch, default: [:]][k] = v }
                    spaces[ws.id] = ws
                    invites.removeAll { $0.workspace == ws.id && $0.acceptedBy == new }
                    return reply(201, .object(["userId": .string(new), "role": .string(role), "epoch": .number(Double(ws.epoch))]))
                }
            }
        }
        if let a = at("DELETE /v1/workspaces/:") {
            switch space(a[0], "owner") {
            case let .failure(r): return r
            case .success:
                spaces[a[0]] = nil
                return reply(200, .object(["deleted": .string(a[0])]))
            }
        }
        if let a = at("DELETE /v1/workspaces/:/members/:") {
            guard keys(["epoch", "wraps"], required: ["epoch", "wraps"]) == nil else {
                return fail(400, "bad_request", "removing a member needs the rotated key: {\"epoch\": current + 1, \"wraps\": [...]}")
            }
            switch space(a[0], "owner") {
            case let .failure(r): return r
            case var .success(ws):
                let gone = a[1]
                guard let role = ws.role(gone) else { return fail(404, "not_found", "not a member") }
                if role == "owner" && ws.members.filter({ $0.1 == "owner" }).count <= 1 { return fail(400, "bad_request", "a workspace keeps at least one owner") }
                guard b["epoch"]?.int == ws.epoch + 1 else { return fail(409, "stale_epoch", "the next epoch is \(ws.epoch + 1)", ["epoch": .number(Double(ws.epoch))]) }
                ws.members.removeAll { $0.0 == gone }
                let theirs = Set(deviceIDs(gone))
                for e in ws.wraps.keys { ws.wraps[e] = ws.wraps[e]?.filter { !theirs.contains($0.key) } }
                switch exactWraps(uid, ws.members.map(\.0), b["wraps"]) {
                case let .failure(r): return r
                case let .success(w):
                    ws.epoch += 1
                    ws.wraps[ws.epoch] = w
                    spaces[ws.id] = ws
                    return reply(200, .object(["removed": .string(gone), "epoch": .number(Double(ws.epoch))]))
                }
            }
        }
        if let a = at("PUT /v1/workspaces/:/members/:") {
            if let bad = keys(["role"], required: ["role"]) { return bad }
            switch space(a[0], "owner") {
            case let .failure(r): return r
            case var .success(ws):
                guard let role = b["role"]?.string, let i = ws.members.firstIndex(where: { $0.0 == a[1] }) else { return fail(404, "not_found", "not a member") }
                ws.members[i].1 = role
                spaces[ws.id] = ws
                return reply(200, .object(["userId": .string(a[1]), "role": .string(role)]))
            }
        }
        if let a = at("POST /v1/workspaces/:/invites") {
            if let bad = keys(["email", "role"], required: ["role"]) { return bad }
            switch space(a[0], "owner") {
            case let .failure(r): return r
            case let .success(ws):
                let token = fixedInviteToken ?? CloudCrypto.base64url(CloudCrypto.randomBytes(24))
                let email = b["email"]?.string?.lowercased() ?? ""
                let inv = FakeInvite(id: "inv\(next())", workspace: ws.id, token: token, email: email, role: b["role"]?.string ?? "editor")
                invites.append(inv)
                return reply(201, .object(["id": .string(inv.id), "token": .string(token), "link": .string("\(Self.server)/invite#\(token)"),
                                           "mailed": .bool(!email.isEmpty), "expiresAt": .number(Double(now + 7 * 86400))]))
            }
        }
        if let a = at("GET /v1/workspaces/:/invites") {
            switch space(a[0], "owner") {
            case let .failure(r): return r
            case let .success(ws):
                let list: [JSONAny] = invites.filter { $0.workspace == ws.id }.map { i in
                    let acc: JSONAny = i.acceptedBy.flatMap { u in users[u] }.map { u in
                        .object(["userId": .string(u.id), "email": .string(u.email),
                                 "keys": .array(u.devices.map { .object(["id": .string($0.id), "name": .string($0.name), "publicKey": .string($0.publicKey)]) })])
                    } ?? .null
                    return .object(["id": .string(i.id), "email": .string(i.email), "role": .string(i.role), "createdAt": .number(0), "expiresAt": .number(0), "accepted": acc])
                }
                return reply(200, .object(["invites": .array(list)]))
            }
        }
        if at("POST /v1/invites/accept") != nil {
            if let bad = keys(["token"], required: ["token"]) { return bad }
            let token = String((b["token"]?.string ?? "").split(separator: ".").first ?? "")
            guard let i = invites.firstIndex(where: { $0.token == token }) else { return fail(404, "invalid_invite", "this invite is invalid or expired") }
            let inv = invites[i]
            if !inv.email.isEmpty && inv.email != users[uid]!.email { return fail(403, "forbidden", "this invite is for another address; sign in with the address it was sent to") }
            if let by = inv.acceptedBy, by != uid { return fail(409, "already_accepted", "someone else accepted this invite") }
            if spaces[inv.workspace]?.role(uid) != nil { return fail(409, "already_member", "already a member") }
            if users[uid]!.devices.isEmpty { return fail(400, "bad_request", "register a device key first (POST /v1/me/keys)") }
            invites[i].acceptedBy = uid
            return reply(200, .object(["workspaceId": .string(inv.workspace), "role": .string(inv.role)]))
        }
        if let a = at("GET /v1/workspaces/:/changes") {
            switch space(a[0]) {
            case let .failure(r): return r
            case let .success(ws):
                let since = Int(query["since"] ?? "0") ?? 0
                let list = ws.blobs.filter { $0.value.seq > since }.sorted { $0.value.seq < $1.value.seq }.map { blobJSON($0.key, $0.value, content: false) }
                return reply(200, .object(["seq": .number(Double(ws.seq)), "epoch": .number(Double(ws.epoch)), "changes": .array(list), "more": .bool(false)]))
            }
        }
        if let a = at("GET /v1/workspaces/:/blobs/:") {
            switch space(a[0]) {
            case let .failure(r): return r
            case let .success(ws):
                guard let blob = ws.blobs[a[1]] else { return fail(404, "not_found", "no such blob") }
                return reply(200, blobJSON(a[1], blob, content: true))
            }
        }
        if let a = at("PUT /v1/workspaces/:/blobs/:") {
            if let bad = keys(["version", "vv", "epoch", "content", "deleted"], required: ["version", "vv", "epoch"]) { return bad }
            let deleted = b["deleted"]?.bool ?? false
            let content = b["content"]?.string
            if deleted == (content != nil) { return fail(400, "bad_request", "content is required (or \"deleted\": true)") }
            guard let version = b["version"]?.int, let epoch = b["epoch"]?.int, let vvo = b["vv"]?.object, !vvo.isEmpty else { return fail(400, "bad_request", "invalid JSON body") }
            let vv = vvo.compactMapValues(\.int)
            if let c = content {
                guard let bytes = CloudCrypto.unbase64url(c), bytes.count >= 45, bytes[0] == 1,
                      Int(bytes[1]) << 24 | Int(bytes[2]) << 16 | Int(bytes[3]) << 8 | Int(bytes[4]) == epoch else {
                    return fail(400, "bad_request", "content is not a format 1 blob envelope for this epoch")
                }
            }
            if holdWritesFrom != users[uid]?.email {
                for h in held where h.ws == a[0] {
                    guard var w = spaces[h.ws] else { continue }
                    w.seq += 1
                    var blob = h.blob
                    blob.seq = w.seq
                    w.blobs[h.id] = blob
                    spaces[h.ws] = w
                }
                held.removeAll { $0.ws == a[0] }
            }
            switch space(a[0], "editor") {
            case let .failure(r): return r
            case var .success(ws):
                if epoch != ws.epoch { return fail(409, "stale_epoch", "write with the current key epoch \(ws.epoch)", ["epoch": .number(Double(ws.epoch))]) }
                let cur = ws.blobs[a[1]]
                let expected = (cur?.version ?? 0) + 1
                let base = cur?.vv ?? [:]
                let descends = base.allSatisfy { (vv[$0.key] ?? 0) >= $0.value } && vv.contains { $0.value > (base[$0.key] ?? 0) }
                if version != expected || !descends {
                    return fail(409, "conflict", version != expected ? "expected version \(expected)" : "the version vector doesn't descend from the stored one",
                                ["current": cur.map { blobJSON(a[1], $0, content: false) } ?? .null])
                }
                if holdWritesFrom != nil && holdWritesFrom == users[uid]?.email {
                    held.append((ws.id, a[1], FakeBlob(version: version, vv: vv, epoch: epoch, deleted: deleted, content: content ?? "", seq: 0)))
                    return reply(200, .object(["version": .number(Double(version)), "seq": .number(Double(ws.seq + 1))]))
                }
                ws.seq += 1
                ws.blobs[a[1]] = FakeBlob(version: version, vv: vv, epoch: epoch, deleted: deleted, content: content ?? "", seq: ws.seq)
                spaces[ws.id] = ws
                return reply(200, .object(["version": .number(Double(version)), "seq": .number(Double(ws.seq))]))
            }
        }
        if let a = at("GET /v1/orgs/:/environments") {
            if a[0].hasPrefix("personal-") { return fail(400, "bad_request", "a personal organization has no team environments: your own are in your library") }
            return reply(200, .object(["environments": .array(a[0] == "org-acme" ? teamEnvironments : [])]))
        }
        if let a = at("PUT /v1/orgs/:/environments/:") ?? at("DELETE /v1/orgs/:/environments/:") {
            if a[0] != "org-acme" { return fail(404, "not_found", "no such organization") }
            let i = teamEnvironments.firstIndex { $0["id"]?.string == a[1] }
            let cur = i.map { teamEnvironments[$0] }
            let seen = cur?["version"]?.int ?? 0
            if at("DELETE /v1/orgs/:/environments/:") != nil {
                guard let i else { return reply(200, .object(["deleted": .bool(true)])) }
                if query["baseVersion"].flatMap({ Int($0) }) != seen { return fail(409, "conflict", "changed", ["current": cur!]) }
                teamEnvironments.remove(at: i)
                return reply(200, .object(["deleted": .bool(true)]))
            }
            guard let data = body?["data"] else { return fail(400, "bad_request", "data") }
            for n in data["secrets"]?.array ?? [] {
                if let n = n.string, let v = data["variables"]?[n]?.string, !v.isEmpty { return fail(400, "bad_request", "secret values stay on your devices: send secret variables empty") }
            }
            if body?["baseVersion"]?.int != seen { return fail(409, "conflict", "changed", ["current": cur ?? .null]) }
            let item: JSONAny = .object(["id": .string(a[1]), "version": .number(Double(seen + 1)), "updatedBy": .string(users[uid]?.email ?? ""), "data": data])
            if let i { teamEnvironments[i] = item } else { teamEnvironments.append(item) }
            return reply(200, .object(["id": .string(a[1]), "version": .number(Double(seen + 1))]))
        }
        return nil
    }
}

/// FakeCloud answers a refused request as a `Result` failure.
extension CloudResponse: Error {}
