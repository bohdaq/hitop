import Foundation
import Observation

// Shared collections (I2b): hitop sync workspaces, end-to-end encrypted, as the web app's
// `sync.js` SyncClient and the CLI's `hitop sync` keep them. A shared collection is a collection
// folder (hitop.yaml, requests/**, environments/*, globals.yaml, comments/**) whose files are
// blobs on the server, sealed under the workspace key; the server never sees a file name, a
// request or a key. Every decision (which files to pull, push or show as a conflict, how
// hitop.yaml merges, which wrapped keys may be used) is the Rust every hitop client runs
// (`SyncCore`); this file is the transport and the bookkeeping, a port of sync.js item for item.
//
// What stays on this device (hitop-library/):
//   shared-sync.json   pins (device key id → public key, trust on first use), the key memory per
//                      workspace (hitop-sync-crypto `WorkspaceMemory`: strict, newest epoch, key
//                      commitments, the invite's fingerprints), links (collection → workspace,
//                      role, organization), the plan state per workspace (seq, entries, conflicts)
//   shared/<ws>.json   the workspace's files as last synced or edited here (the collection
//                      folder; environments/, globals.yaml and comments/ are kept as they came)
// The device key is I2's (Keychain `ios/cloud/<host>`); workspace keys are never stored: they are
// unwrapped again on every sync.

struct SharedLink: Codable, Hashable, Sendable {
    var workspace: String
    var role: String
    var org: String?
    var orgName: String?
    var orgPersonal = false
    /// No longer a member here (removed, or another account signed in): kept on this device,
    /// never synced, never in the personal library.
    var gone = false
}

struct SharedState: Codable, Hashable, Sendable {
    /// "<server> <userId>" the links are for.
    var account = ""
    var pins: [String: String] = [:]
    var memory: [String: JSONAny] = [:]
    /// Collection id → link.
    var links: [String: SharedLink] = [:]
    /// Workspace → hitop-core `sync::State` (JSON).
    var states: [String: JSONAny] = [:]
    /// Workspace → share id → newest version seen (shared environment values never go back).
    var shareVersions: [String: [String: Int]] = [:]
}

// A key another version didn't write takes its default; a wrong type is a damaged file.
extension SharedLink {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(workspace: try c.decode(String.self, forKey: .workspace), role: try c.decode(String.self, forKey: .role),
                  org: try c.decodeIfPresent(String.self, forKey: .org), orgName: try c.decodeIfPresent(String.self, forKey: .orgName),
                  orgPersonal: try c.decodeIfPresent(Bool.self, forKey: .orgPersonal) ?? false, gone: try c.decodeIfPresent(Bool.self, forKey: .gone) ?? false)
    }
}

extension SharedState {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.init(account: try c.decodeIfPresent(String.self, forKey: .account) ?? "",
                  pins: try c.decodeIfPresent([String: String].self, forKey: .pins) ?? [:],
                  memory: try c.decodeIfPresent([String: JSONAny].self, forKey: .memory) ?? [:],
                  links: try c.decode([String: SharedLink].self, forKey: .links),
                  states: try c.decodeIfPresent([String: JSONAny].self, forKey: .states) ?? [:],
                  shareVersions: try c.decodeIfPresent([String: [String: Int]].self, forKey: .shareVersions) ?? [:])
    }
}

/// A sync's own file is there but can't be used. Never taken as empty: without the list of shared
/// collections an end-to-end encrypted one would look like any other, and go to the library,
/// which the server can read.
enum StateFileProblem: Error, Equatable {
    /// Can't be opened now (the phone hasn't been unlocked since it started): later.
    case unreadable
    /// Holds something else.
    case damaged

    /// The file's value; nil when there is no file.
    static func read<T: Decodable>(_ type: T.Type, _ file: URL) throws -> T? {
        let data: Data
        do {
            data = try Data(contentsOf: file)
        } catch {
            if FileManager.default.fileExists(atPath: file.path) { throw StateFileProblem.unreadable }
            return nil
        }
        do { return try JSONDecoder().decode(type, from: data) } catch { throw StateFileProblem.damaged }
    }

    static func setAside(_ file: URL) throws {
        let stamp = Int(Date().timeIntervalSince1970 * 1000)
        try FileManager.default.moveItem(at: file, to: file.appendingPathExtension("damaged-\(stamp)"))
    }
}

protocol SharedSyncStore: Sendable {
    /// Throws `StateFileProblem`.
    func load() throws -> SharedState
    /// A damaged file out of the way, kept beside.
    func setAsideDamaged() throws
    func save(_ state: SharedState) throws
    func files(_ workspace: String) -> [String: String]
    func saveFiles(_ workspace: String, _ files: [String: String]) throws
    func dropFiles(_ workspace: String)
}

struct FileSharedSyncStore: SharedSyncStore {
    let root: URL

    private var stateFile: URL { root.appendingPathComponent("shared-sync.json") }
    private func filesURL(_ ws: String) -> URL { root.appendingPathComponent("shared").appendingPathComponent("\(ws).json") }

    func load() throws -> SharedState { try StateFileProblem.read(SharedState.self, stateFile) ?? SharedState() }

    func setAsideDamaged() throws { try StateFileProblem.setAside(stateFile) }

    func save(_ state: SharedState) throws {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys]
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try e.encode(state).write(to: stateFile, options: [.atomic])
    }

    func files(_ ws: String) -> [String: String] {
        guard let data = try? Data(contentsOf: filesURL(ws)) else { return [:] }
        return (try? JSONDecoder().decode([String: String].self, from: data)) ?? [:]
    }

    func saveFiles(_ ws: String, _ files: [String: String]) throws {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys]
        try FileManager.default.createDirectory(at: filesURL(ws).deletingLastPathComponent(), withIntermediateDirectories: true)
        try e.encode(files).write(to: filesURL(ws), options: [.atomic])
    }

    func dropFiles(_ ws: String) { try? FileManager.default.removeItem(at: filesURL(ws)) }
}

final class MemorySharedSyncStore: SharedSyncStore, @unchecked Sendable {
    private let lock = NSLock()
    private var state = SharedState()
    private var all: [String: [String: String]] = [:]

    init() {}

    func load() throws -> SharedState { lock.lock(); defer { lock.unlock() }; return state }
    func setAsideDamaged() throws {}
    func save(_ s: SharedState) throws { lock.lock(); defer { lock.unlock() }; state = s }
    func files(_ ws: String) -> [String: String] { lock.lock(); defer { lock.unlock() }; return all[ws] ?? [:] }
    func saveFiles(_ ws: String, _ files: [String: String]) throws { lock.lock(); defer { lock.unlock() }; all[ws] = files }
    func dropFiles(_ ws: String) { lock.lock(); defer { lock.unlock() }; all[ws] = nil }
}

/// A workspace this account is a member of (`GET /v1/workspaces`).
struct SharedWorkspace: Hashable, Sendable, Identifiable {
    var id: String
    var role: String
    var epoch: Int
    var org: String?
    var orgName: String?
    var orgPersonal: Bool
}

/// A member device key as the Share sheet shows it.
struct MemberKey: Hashable, Sendable, Identifiable {
    enum Trust: String, Sendable { case trusted, changed, new }
    var id: String
    var name: String
    var publicKey: String
    var hasKey: Bool
    var legacy: Bool
    var fingerprint: String
    var trust: Trust
}

struct WorkspaceMember: Hashable, Sendable, Identifiable {
    var userId: String
    var email: String
    var role: String
    var runner: Bool
    var keys: [MemberKey]
    var id: String { userId }
}

struct WorkspaceInvite: Hashable, Sendable, Identifiable {
    var id: String
    var email: String
    var role: String
    /// Accepted: the account and its device keys, to check and add.
    var acceptedBy: WorkspaceMember?
}

struct WorkspaceInfo: Hashable, Sendable {
    var workspace: String
    var epoch: Int
    var role: String
    var members: [WorkspaceMember]
    var invites: [WorkspaceInvite]
}

/// A file changed here and by someone else.
struct SharedConflict: Hashable, Sendable, Identifiable {
    var path: String
    var name: String
    var base: String?
    var mine: String?
    var theirs: String?
    var id: String { path }
    /// Keep both: only a request file both sides still have.
    var canKeepBoth: Bool { path.hasPrefix("requests/") && mine != nil && theirs != nil }
}

struct SharedResult: Hashable, Sendable {
    var pulled: [String] = []
    var removed: [String] = []
    var pushed: [String] = []
    var merged: [String] = []
    var conflicts: [String] = []
    var collectionID: UInt64
}

enum SharedError: Error, LocalizedError, Equatable {
    case notShared
    case notSignedIn
    case message(String)

    var errorDescription: String? {
        switch self {
        case .notShared: "This collection isn't shared."
        case .notSignedIn: "Sign in to HITOP Cloud first."
        case let .message(m): m
        }
    }
}

/// An invite code: `<token>.<fingerprint of the inviter's device key>` (hitop 1.2), or a bare token.
struct InviteCode: Hashable, Sendable {
    var token: String
    /// Hex digits only; nil for a bare token.
    var fingerprint: String?

    /// The code, or a whole invite link (the code after `#`).
    static func parse(_ text: String) throws -> InviteCode {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let code = String(t.split(separator: "#").last ?? Substring(t))
        let parts = code.split(separator: ".", maxSplits: 1).map(String.init)
        guard let token = parts.first, !token.isEmpty else { throw SharedError.message("Paste the invite code you were sent") }
        guard parts.count > 1 else { return InviteCode(token: token, fingerprint: nil) }
        let fp = SyncCore.normalize(fingerprint: parts[1])
        guard fp.count == 32 else { throw SharedError.message("This invite code is damaged (the part after the “.” must be a key fingerprint)") }
        return InviteCode(token: token, fingerprint: fp)
    }
}

/// Something about shared collections the person should know: a headline and secondary text.
struct SharedNotice: Hashable, Sendable {
    var text: String
    var detail: String?

    init(_ text: String, detail: String? = nil) {
        self.text = text
        self.detail = detail
    }
}

@MainActor
@Observable
final class SharedSync {
    struct Status: Hashable, Sendable {
        var syncing = false
        var error: String?
        var lastSync: Date?
    }

    /// Per shared collection.
    private(set) var statuses: [UInt64: Status] = [:]
    /// Per shared collection: its open conflicts.
    private(set) var conflicts: [UInt64: [SharedConflict]] = [:]
    /// The workspaces of the signed-in account, as last listed.
    private(set) var workspaces: [SharedWorkspace] = []
    /// What the person should know (warnings from accepting keys, a removal), newest first.
    private(set) var notices: [SharedNotice] = []
    /// A workspace key came from a device this one hasn't trusted: the Account screen asks to
    /// compare its fingerprint (`trustSender`).
    var pendingTrust: SyncCore.KeyRefusal?

    @ObservationIgnored let cloud: HitopCloud
    @ObservationIgnored let model: AppModel
    @ObservationIgnored let store: SharedSyncStore
    @ObservationIgnored var pushDelay: Duration = .seconds(1)
    @ObservationIgnored private(set) var state: SharedState
    @ObservationIgnored private var pending: [UInt64: Task<Void, Never>] = [:]
    @ObservationIgnored private var running: Set<UInt64> = []
    @ObservationIgnored private var again: Set<UInt64> = []

    init(cloud: HitopCloud, model: AppModel, store: SharedSyncStore) {
        self.cloud = cloud
        self.model = model
        self.store = store
        state = SharedState()
        loadState()
        model.onSharedChange = { [weak self] id in self?.schedule(id) }
        model.onCollectionDeleted = { [weak self] id in self?.forget(id) }
        cloud.onAccountDeleted = { [weak self] a in self?.accountDeleted(a) }
        model.reloadSharing = { [weak self] in self?.loadState() }
        model.startSharingOver = { [weak self] in
            guard let self, model.sharingProblem == .damaged else { return }
            try store.setAsideDamaged()
            loadState()
            notices.insert(SharedNotice("The list of shared collections on this device was damaged", detail: "Their copies here are kept on this device only. Open them again under Shared with you."), at: 0)
        }
        applyRoles()
    }

    private func loadState() {
        do {
            state = try store.load()
            model.sharingProblem = nil
        } catch {
            state = SharedState()
            model.sharingProblem = error as? StateFileProblem ?? .unreadable
        }
        applyRoles()
        conflicts = [:]
        for (id, l) in state.links { if let cid = UInt64(id) { conflicts[cid] = conflictList(cid, l) } }
    }

    private func save() {
        guard model.sharingProblem == nil else { return }
        try? store.save(state)
    }

    // MARK: Links and roles

    func link(_ id: UInt64) -> SharedLink? { state.links[String(id)] }

    /// The collection's workspace, while it syncs.
    private func live(_ id: UInt64) throws -> SharedLink {
        guard let l = link(id), !l.gone else { throw SharedError.notShared }
        return l
    }

    var linkedWorkspaces: Set<String> { Set(state.links.values.map(\.workspace)) }

    /// Workspaces this account is in that aren't open on this device ("Shared with you").
    var joinable: [SharedWorkspace] { workspaces.filter { !linkedWorkspaces.contains($0.id) } }

    /// "Shared with Acme", "Shared", "No longer shared".
    func badge(_ id: UInt64) -> String? {
        guard let l = link(id) else { return nil }
        if l.gone { return "No longer shared" }
        if let n = l.orgName, !l.orgPersonal, !n.isEmpty { return "Shared with \(n)" }
        return "Shared"
    }

    func applyRoles() {
        var roles: [UInt64: String] = [:]
        var orgs: [UInt64: String?] = [:]
        let teams = Set(cloud.settings.orgs.filter { !$0.personal }.map(\.id))
        for (id, l) in state.links {
            guard let cid = UInt64(id) else { continue }
            roles[cid] = l.gone ? "removed" : l.role
            orgs[cid] = .some(l.org.flatMap { teams.contains($0) ? $0 : nil })
        }
        if model.sharedRoles != roles { model.sharedRoles = roles }
        if model.sharedOrgs != orgs { model.sharedOrgs = orgs }
    }

    /// The signed-in account's links; another account's are kept, not synced (`gone`). This
    /// device's own key is pinned (it authenticates the wraps it makes and opens its own).
    @discardableResult
    func checkAccount() -> Bool {
        if model.sharingProblem == .unreadable { loadState() }
        guard model.sharingProblem == nil, let a = cloud.account, let secret = cloud.credentials.load(a.server)?.secret else { return false }
        let key = cloud.accountKey
        if state.account != key {
            if !state.account.isEmpty {
                for k in state.links.keys { state.links[k]?.gone = true }
            }
            state.account = key
        }
        if state.pins[a.keyId] == nil, let pk = try? SyncCore.publicKey(secret: secret) { state.pins[a.keyId] = pk }
        save()
        applyRoles()
        return true
    }

    /// The account was deleted (`HitopCloud.deleteAccount`): its shared collections stay on this
    /// device as they were last synced, never synced again (`gone`), and its own device key is no
    /// longer pinned (the key itself is gone from the Keychain).
    func accountDeleted(_ a: CloudAccount) {
        for k in state.links.keys { state.links[k]?.gone = true }
        state.pins[a.keyId] = nil
        state.account = ""
        save()
        applyRoles()
    }

    private func me() throws -> (account: CloudAccount, secret: String) {
        guard checkAccount(), let a = cloud.account, let s = cloud.credentials.load(a.server)?.secret else { throw SharedError.notSignedIn }
        return (a, s)
    }

    /// This device's key fingerprint ("62bd a406 …"), for the person to read out to the others.
    var fingerprint: String? {
        guard let a = cloud.account ?? cloud.settings.device, let s = cloud.credentials.load(a.server)?.secret,
              let pk = try? SyncCore.publicKey(secret: s) else { return nil }
        return try? SyncCore.fingerprint(pk)
    }

    private func api(_ method: String, _ path: String, _ body: JSONAny? = nil, allow: Set<Int> = []) async throws -> CloudResponse {
        try await cloud.call(method, path, body: body, allow: allow)
    }

    private func json(_ method: String, _ path: String, _ body: JSONAny? = nil) async throws -> JSONAny {
        try await api(method, path, body).json ?? .null
    }

    // MARK: Trust on first use

    func trustOf(_ keyId: String, _ publicKey: String) -> MemberKey.Trust {
        guard let p = state.pins[keyId] else { return .new }
        return p == publicKey ? .trusted : .changed
    }

    /// Trusts device keys after the person compared their fingerprints. A changed key never
    /// replaces a trusted one.
    func trust(_ keys: [MemberKey]) {
        for k in keys where state.pins[k.id] == nil { state.pins[k.id] = k.publicKey }
        save()
    }

    /// Trusts the device that handed this one a workspace key (`pendingTrust`), after the person
    /// compared its fingerprint: the server's key for it must still have that fingerprint.
    func trustSender(_ refusal: SyncCore.KeyRefusal) async throws {
        guard let sender = refusal.senderKeyId, let fp = refusal.fingerprint else { throw SharedError.message("That device isn't listed by the server: not trusting it") }
        let info = try await json("GET", "/v1/workspaces/\(refusal.workspace)")
        let keys = (info["members"]?.array ?? []).flatMap { $0["keys"]?.array ?? [] }
        guard let k = keys.first(where: { $0["id"]?.string == sender }), let pk = k["publicKey"]?.string,
              SyncCore.normalize(fingerprint: try SyncCore.fingerprint(pk)) == SyncCore.normalize(fingerprint: fp) else {
            throw SharedError.message("That device's key isn't the one you compared any more: not trusting it")
        }
        if trustOf(sender, pk) == .changed { throw SharedError.message("That device's key changed since you trusted it: not trusting the new one") }
        state.pins[sender] = pk
        save()
        if pendingTrust == refusal { pendingTrust = nil }
    }

    private func checkTrusted(_ keys: [MemberKey], _ who: String) throws {
        let bad = keys.filter { $0.trust != .trusted }
        if bad.contains(where: { $0.trust == .changed }) { throw SharedError.message("A device key of \(who) changed since you trusted it: not sharing the collection's key with it") }
        if !bad.isEmpty { throw SharedError.message("Check and trust the device keys of \(who) first (compare the fingerprints with them)") }
    }

    private func memberKeys(_ list: JSONAny?) -> [MemberKey] {
        (list?.array ?? []).map { k in
            let pk = k["publicKey"]?.string ?? ""
            return MemberKey(id: k["id"]?.string ?? "", name: k["name"]?.string ?? "", publicKey: pk, hasKey: k["hasKey"]?.bool ?? true,
                             legacy: !(k["legacyEpochs"]?.array ?? []).isEmpty, fingerprint: (try? SyncCore.fingerprint(pk)) ?? "",
                             trust: trustOf(k["id"]?.string ?? "", pk))
        }
    }

    private func member(_ m: JSONAny) -> WorkspaceMember {
        WorkspaceMember(userId: m["userId"]?.string ?? "", email: m["email"]?.string ?? "", role: m["role"]?.string ?? "viewer",
                        runner: m["runner"]?.bool ?? false, keys: memberKeys(m["keys"]))
    }

    // MARK: Keys

    struct Keys {
        var epoch: Int
        var keys: [Int: String]
        var legacy: Bool
    }

    /// The keys this device may use for `ws` (hitop-sync-crypto `accept_wraps`), or a
    /// `SyncCore.KeyRefusal`.
    func workspaceKeys(_ ws: String) async throws -> Keys {
        let (a, secret) = try me()
        let r = try await json("GET", "/v1/workspaces/\(ws)/wraps?keyId=\(a.keyId)")
        let wraps = (r["wraps"]?.array ?? []).compactMap { w -> (epoch: Int, wrapped: String)? in
            guard let e = w["epoch"]?.int, let x = w["wrapped"]?.string else { return nil }
            return (e, x)
        }
        let headers = wraps.map { (try? SyncCore.wrapHeader($0.wrapped)) ?? SyncCore.WrapHeader(legacy: true) }
        // The member list only when a sender isn't trusted: its fingerprint, or the inviter's.
        var listed: [String: String] = [:]
        var names: [String: String] = [:]
        if headers.contains(where: { !$0.legacy && state.pins[$0.sender ?? ""] == nil }) {
            let info = try await json("GET", "/v1/workspaces/\(ws)")
            for m in info["members"]?.array ?? [] {
                for k in m["keys"]?.array ?? [] {
                    if let id = k["id"]?.string, let pk = k["publicKey"]?.string { listed[id] = pk }
                    if let id = k["id"]?.string {
                        let name = k["name"]?.string ?? ""
                        names[id] = name.isEmpty ? (m["email"]?.string).map { "a device of \($0)" } : name
                    }
                }
            }
        }
        do {
            let ring = try SyncCore.acceptWraps(secret: secret, keyId: a.keyId, workspace: ws, currentEpoch: r["epoch"]?.int ?? 0, wraps: wraps,
                                                pinned: state.pins, listed: listed, memory: state.memory[ws] ?? .object([:]))
            state.memory[ws] = ring.memory
            for (id, pk) in ring.newPins { state.pins[id] = pk }
            save()
            for d in ring.details {
                let n = Self.describe(d, names: names)
                if !notices.contains(n) { notices.insert(n, at: 0) }
            }
            return Keys(epoch: ring.current, keys: ring.keys, legacy: headers.contains { $0.legacy })
        } catch let e as SyncCore.KeyRefusal {
            if e.kind == "untrustedSender", e.fingerprint != nil { pendingTrust = e }
            throw e
        }
    }

    /// Authenticated wraps of `key` (epoch `epoch`) made by this device for `keys`.
    private func wraps(for keys: [MemberKey], _ ws: String, epoch: Int, key: String, head: Int? = nil) throws -> JSONAny {
        let (a, secret) = try me()
        return .array(try keys.map { k in
            .object(["keyId": .string(k.id), "wrapped": .string(try SyncCore.sealWrap(secret: secret, sender: a.keyId, recipient: k.id, recipientPublicKey: k.publicKey,
                                                                                    workspace: ws, epoch: epoch, head: head ?? epoch, key: key))])
        })
    }

    /// As an owner: replaces unauthenticated (hitop 1.1) wraps with authenticated ones for every
    /// trusted device (sync.js `upgradeWraps`).
    private func upgradeWraps(_ ws: String, _ k: Keys) async throws {
        let info = try await json("GET", "/v1/workspaces/\(ws)")
        guard info["role"]?.string == "owner" else { return }
        var byEpoch: [Int: [MemberKey]] = [:]
        for m in info["members"]?.array ?? [] where !(m["runner"]?.bool ?? false) {
            for raw in m["keys"]?.array ?? [] {
                let epochs = (raw["legacyEpochs"]?.array ?? []).compactMap(\.int).filter { k.keys[$0] != nil }
                guard !epochs.isEmpty, let key = memberKeys(.array([raw])).first, key.trust == .trusted else { continue }
                for e in epochs { byEpoch[e, default: []].append(key) }
            }
        }
        for (e, list) in byEpoch.sorted(by: { $0.key < $1.key }) {
            _ = try await api("POST", "/v1/workspaces/\(ws)/wraps", .object(["epoch": .number(Double(e)), "wraps": try wraps(for: list, ws, epoch: e, key: k.keys[e]!, head: k.epoch)]))
        }
    }

    // MARK: Files

    static let emptyState: JSONAny = .object(["seq": .number(0), "entries": .object([:]), "conflicts": .object([:]), "losers": .object([:])])

    private static func isCollectionFile(_ p: String) -> Bool { p == "hitop.yaml" || p.hasPrefix("requests/") }

    /// The workspace's files with this device's edits: the stored folder, with hitop.yaml and
    /// requests/ as the collection now is here. environments/, globals.yaml and comments/ stay
    /// as they came (this app doesn't edit them; leaving them out would delete them for everyone).
    ///
    /// Only what changed here is rewritten: the stored files are read into the app's model and
    /// written back (`base`), and a file is replaced only where the collection as it is now writes
    /// it differently. A file another client wrote (by hand, without ids, as `.yml`) keeps its
    /// bytes until someone edits that request here.
    func localFiles(_ id: UInt64) throws -> [String: String] {
        let l = try live(id)
        var files = store.files(l.workspace)
        guard l.role != "viewer", let c = model.collection(id) else { return files }
        let own = try SyncCore.toFiles(c).filter { Self.isCollectionFile($0.key) }
        let base: [String: String] = files["hitop.yaml"] == nil ? [:]
            : try SyncCore.toFiles(SyncCore.readFiles(files, name: c.name)).filter { Self.isCollectionFile($0.key) }
        /// The stored file for a written path (an existing `.yml` keeps its extension).
        func stored(_ p: String) -> String {
            guard p.hasSuffix(".yaml") else { return p }
            let yml = String(p.dropLast(5)) + ".yml"
            return files[p] == nil && files[yml] != nil ? yml : p
        }
        for (p, text) in own where base[p] != text { files[stored(p)] = text }
        for p in base.keys where own[p] == nil { files[stored(p)] = nil }
        return files
    }

    /// Writes the workspace's files here and shows the collection they hold. Returns its id (a new
    /// one when hitop.yaml's id changed).
    @discardableResult
    func applyFiles(_ id: UInt64, _ files: [String: String]) throws -> UInt64 {
        let l = try live(id)
        try store.saveFiles(l.workspace, files)
        let c = try SyncCore.readFiles(files, name: model.collection(id)?.name ?? "Shared collection")
        if c.id != id {
            state.links[String(c.id)] = l
            state.links[String(id)] = nil
            conflicts[id] = nil
            statuses[c.id] = statuses[id]
            statuses[id] = nil
            save()
            applyRoles()
            model.applyRemoteRemoval(collection: id)
        }
        model.applyRemote(collection: c)
        return c.id
    }

    // MARK: Transport

    /// The blobs changed after `since`, opened with `keys`.
    private func changes(_ ws: String, since: Int, keys: [Int: String]) async throws -> (remote: [JSONAny], seq: Int) {
        var out: [JSONAny] = []
        var cursor = since
        var seq = since
        while true {
            let r = try await json("GET", "/v1/workspaces/\(ws)/changes?since=\(cursor)")
            seq = r["seq"]?.int ?? seq
            let list = r["changes"]?.array ?? []
            for c in list {
                guard let id = c["id"]?.string else { continue }
                cursor = max(cursor, c["seq"]?.int ?? cursor)
                if c["deleted"]?.bool ?? false {
                    out.append(.object(["blob": .string(id), "version": c["version"] ?? .number(0), "vv": c["vv"] ?? .object([:]), "deleted": .bool(true)]))
                    continue
                }
                let b = try await json("GET", "/v1/workspaces/\(ws)/blobs/\(id)")
                let envelope = b["content"]?.string ?? ""
                let epoch = try SyncCore.blobEpoch(envelope)
                guard let key = keys[epoch] else { throw SharedError.message("A file is under key epoch \(epoch), which this device doesn't have") }
                let version = b["version"]?.int ?? 0
                let opened = try SyncCore.open(key: key, workspace: ws, blob: id, version: version, envelope: envelope)
                out.append(.object(["blob": .string(id), "version": .number(Double(version)), "vv": b["vv"] ?? .object([:]), "deleted": .bool(false),
                                    "path": .string(opened.path), "text": .string(opened.text)]))
            }
            if !(r["more"]?.bool ?? false) || list.isEmpty { break }
        }
        return (out, seq)
    }

    /// One write; the refusal's code (conflict, stale_epoch) or nil.
    private func put(_ ws: String, _ push: JSONAny, epoch: Int, key: String) async throws -> String? {
        guard let blob = push["blob"]?.string, let version = push["version"]?.int else { return "conflict" }
        var body: [String: JSONAny] = ["version": .number(Double(version)), "vv": push["vv"] ?? .object([:]), "epoch": .number(Double(epoch))]
        if let text = push["text"]?.string {
            body["content"] = .string(try SyncCore.seal(key: key, workspace: ws, blob: blob, version: version, epoch: epoch, path: push["path"]?.string ?? "", text: text))
        } else {
            body["deleted"] = .bool(true)
        }
        let r = try await api("PUT", "/v1/workspaces/\(ws)/blobs/\(blob)", .object(body), allow: [409])
        return r.status == 409 ? (r.json?["error"]?.string ?? "conflict") : nil
    }

    private func set(_ o: JSONAny, _ key: String, _ value: JSONAny) -> JSONAny {
        var m = o.object ?? [:]
        m[key] = value
        return .object(m)
    }

    // MARK: Sync

    /// Pulls and pushes one shared collection (a viewer only pulls).
    @discardableResult
    func syncNow(_ id: UInt64) async throws -> SharedResult {
        let l = try live(id)
        let ws = l.workspace
        let (a, _) = try me()
        var cid = id
        statuses[cid, default: Status()].syncing = true
        defer { statuses[cid, default: Status()].syncing = false }
        do {
            var k = try await workspaceKeys(ws)
            if k.legacy { try? await upgradeWraps(ws, k) }
            for _ in 0..<5 {
                let st = state.states[ws] ?? Self.emptyState
                let (remote, seq) = try await changes(ws, since: st["seq"]?.int ?? 0, keys: k.keys)
                let local = try localFiles(cid)
                let plan = try SyncCore.call("plan", ["local": .object(local.mapValues(JSONAny.string)), "state": st, "remote": .array(remote), "me": .string(a.keyId)])
                var next = set(plan["state"] ?? Self.emptyState, "seq", .number(Double(seq)))
                let write = (plan["write"]?.object ?? [:]).compactMapValues(\.string)
                let remove = (plan["remove"]?.array ?? []).compactMap(\.string)
                if !write.isEmpty || !remove.isEmpty {
                    var files = local.merging(write) { _, b in b }
                    for p in remove { files[p] = nil }
                    let before = cid
                    cid = try applyFiles(cid, files)
                    if before != cid { statuses[cid, default: Status()].syncing = true }
                } else {
                    try store.saveFiles(ws, local)
                }
                state.states[ws] = next
                save()
                var result = SharedResult(pulled: Array(write.keys).sorted(), removed: remove, merged: (plan["merged"]?.array ?? []).compactMap(\.string),
                                          conflicts: (plan["conflicts"]?.array ?? []).compactMap(\.string), collectionID: cid)
                conflicts[cid] = conflictList(cid, try live(cid))
                if l.role == "viewer" {
                    synced(cid)
                    return result
                }
                var retry = false
                for p in plan["push"]?.array ?? [] {
                    if let refused = try await put(ws, p, epoch: k.epoch, key: k.keys[k.epoch] ?? "") {
                        if refused == "stale_epoch" { k = try await workspaceKeys(ws) }
                        retry = true
                        break
                    }
                    next = try SyncCore.call("pushed", ["state": next, "push": p])
                    state.states[ws] = next
                    save()
                    if let path = p["path"]?.string, !path.isEmpty { result.pushed.append(path) }
                }
                if !retry {
                    synced(cid)
                    try? await refreshShares(ws)
                    return result
                }
            }
            throw SharedError.message("The shared collection kept changing while syncing; try again")
        } catch {
            if (error as? CloudError)?.status == 401 { cloud.sessionEnded() }
            statuses[cid, default: Status()].error = StoreText.withoutPlanLink(error.localizedDescription)
            throw error
        }
    }

    private func synced(_ id: UInt64) {
        statuses[id, default: Status()].error = nil
        statuses[id, default: Status()].lastSync = Date()
    }

    /// A change made here: synced a moment later (one round at a time per collection).
    func schedule(_ id: UInt64) {
        guard cloud.signedIn, let l = link(id), !l.gone, l.role != "viewer" else { return }
        pending[id]?.cancel()
        let wait = pushDelay
        pending[id] = Task { [weak self] in
            try? await Task.sleep(for: wait)
            guard !Task.isCancelled else { return }
            await self?.run(id)
        }
    }

    private func run(_ id: UInt64) async {
        if running.contains(id) {
            again.insert(id)
            return
        }
        running.insert(id)
        _ = try? await syncNow(id)
        running.remove(id)
        if again.remove(id) != nil { await run(id) }
    }

    /// Lists the account's workspaces (roles, organizations, removals), syncs every shared
    /// collection and hands keys to members' new devices (launch, foreground, pull to refresh).
    func syncAll() async {
        guard checkAccount() else { return }
        do {
            try await refreshWorkspaces()
        } catch {
            return
        }
        for (id, l) in state.links.sorted(by: { $0.key < $1.key }) where !l.gone {
            guard let cid = UInt64(id) else { continue }
            if (try? await syncNow(cid)) != nil, l.role != "viewer" {
                _ = try? await shareKeysWithNewDevices(cid)
            }
        }
        await loadTeamEnvironments()
    }

    /// Waits for scheduled pushes (tests, Sync now).
    func flush() async {
        let ids = Array(pending.keys)
        for id in ids {
            pending[id]?.cancel()
            pending[id] = nil
            await run(id)
        }
    }

    func refreshWorkspaces() async throws {
        let r = try await json("GET", "/v1/workspaces")
        workspaces = (r["workspaces"]?.array ?? []).compactMap { w in
            guard let id = w["id"]?.string else { return nil }
            let o = w["org"]
            return SharedWorkspace(id: id, role: w["role"]?.string ?? "viewer", epoch: w["epoch"]?.int ?? 1, org: o?["id"]?.string,
                                   orgName: o?["name"]?.string, orgPersonal: o?["personal"]?.bool ?? false)
        }
        let byID = Dictionary(workspaces.map { ($0.id, $0) }) { a, _ in a }
        for (id, l) in state.links {
            if let w = byID[l.workspace] {
                // A member again (re-added, or this account signed in again): it syncs again.
                state.links[id]?.gone = false
                state.links[id]?.role = w.role
                state.links[id]?.org = w.org
                state.links[id]?.orgName = w.orgName
                state.links[id]?.orgPersonal = w.orgPersonal
            } else if !l.gone {
                state.links[id]?.gone = true
                let name = UInt64(id).flatMap { model.collection($0)?.name } ?? "A shared collection"
                notices.insert(SharedNotice("You're no longer a member of “\(name)”: its copy stays on this device and no longer syncs."), at: 0)
            }
        }
        save()
        applyRoles()
    }

    // MARK: Conflicts

    private func conflictList(_ id: UInt64, _ l: SharedLink) -> [SharedConflict] {
        let st = state.states[l.workspace] ?? Self.emptyState
        let open = st["conflicts"]?.object ?? [:]
        guard !open.isEmpty else { return [] }
        let local = (try? localFiles(id)) ?? store.files(l.workspace)
        return open.keys.sorted().map { path in
            let c = open[path] ?? .null
            let mine = local[path]
            let text = mine ?? c["theirs"]?.string ?? ""
            let name = text.split(separator: "\n").first(where: { $0.hasPrefix("name:") }).map { String($0.dropFirst(5)).trimmingCharacters(in: CharacterSet(charactersIn: " '\"")) }
            return SharedConflict(path: path, name: name ?? path, base: c["base"]?.string, mine: mine, theirs: c["theirs"]?.string)
        }
    }

    enum Choice: String, Sendable { case mine, theirs, both }

    /// Resolves a conflict: keep mine (pushed next), take theirs, or keep both (theirs, and mine as
    /// a new request "… (mine)"), then syncs.
    func resolve(_ id: UInt64, path: String, _ choice: Choice) async throws {
        let l = try live(id)
        let ws = l.workspace
        let local = try localFiles(id)
        let mineText = local[path]
        var keptCopy: SavedRequest?
        if choice == .both, let mineText, path.hasPrefix("requests/") {
            // Mine as hitop-core reads it, then a copy under a new name and file.
            var probe: [String: String] = [path: mineText]
            probe["hitop.yaml"] = local["hitop.yaml"] ?? "format: 1\nname: x\n"
            if var r = try SyncCore.readFiles(probe, name: "x").requests.first {
                let stem = String(path.dropFirst("requests/".count)).replacingOccurrences(of: ".yaml", with: "").replacingOccurrences(of: ".yml", with: "")
                r.id = IDs.next()
                r.name = "\(r.name) (mine)"
                r.file = "\(stem)-mine"
                keptCopy = r
            }
        }
        let r = try SyncCore.call("resolve", ["state": state.states[ws] ?? Self.emptyState, "path": .string(path), "keepMine": .bool(choice == .mine)])
        state.states[ws] = r["state"]
        save()
        var cid = id
        let action = r["resolution"]?["action"]?.string ?? "keepMine"
        if action != "keepMine" {
            var files = local
            if action == "write" { files[path] = r["resolution"]?["text"]?.string } else { files[path] = nil }
            cid = try applyFiles(id, files)
        }
        if let copy = keptCopy { model.addRequestCopy(copy, to: cid) }
        conflicts[cid] = conflictList(cid, try live(cid))
        try await syncNow(cid)
    }

    // MARK: Workspaces

    /// Shares a collection: a new workspace in the collection's organization, epoch 1's key
    /// wrapped for every device of this account (each one trusted here first).
    @discardableResult
    func share(_ id: UInt64) async throws -> SharedResult {
        _ = try me()
        guard let c = model.collection(id), link(id) == nil else { throw SharedError.message("This collection is shared already") }
        let mine = try await json("GET", "/v1/me")
        let keys = memberKeys(mine["keys"])
        try checkTrusted(keys, "your other devices")
        let ws = try SyncCore.randomId()
        let key = try SyncCore.newKey()
        let org = c.org ?? cloud.orgForNew
        var body: [String: JSONAny] = ["id": .string(ws), "wraps": try wraps(for: keys, ws, epoch: 1, key: key)]
        if let org { body["org"] = .string(org) }
        _ = try await api("POST", "/v1/workspaces", .object(body))
        // Made here: only authenticated keys from now on.
        state.memory[ws] = .object(["strict": .bool(true)])
        let o = cloud.settings.orgs.first { $0.id == org }
        state.links[String(id)] = SharedLink(workspace: ws, role: "owner", org: org, orgName: o?.name, orgPersonal: o?.personal ?? (org == nil))
        state.states[ws] = Self.emptyState
        save()
        try store.saveFiles(ws, try SyncCore.toFiles(c))
        applyRoles()
        let r = try await syncNow(id)
        try? await refreshWorkspaces()
        return r
    }

    /// The device keys of this account that aren't trusted here yet (Share needs them all).
    func ownUntrustedDevices() async throws -> [MemberKey] {
        let mine = try await json("GET", "/v1/me")
        return memberKeys(mine["keys"]).filter { $0.trust != .trusted }
    }

    /// Opens a workspace this account is a member of as a collection here.
    @discardableResult
    func join(_ ws: String) async throws -> UInt64 {
        let (a, _) = try me()
        let k = try await workspaceKeys(ws)
        let (remote, seq) = try await changes(ws, since: 0, keys: k.keys)
        let plan = try SyncCore.call("plan", ["local": .object([:]), "state": Self.emptyState, "remote": .array(remote), "me": .string(a.keyId)])
        let files = (plan["write"]?.object ?? [:]).compactMapValues(\.string)
        guard files["hitop.yaml"] != nil else { throw SharedError.message("This shared collection has nothing in it yet") }
        let c = try SyncCore.readFiles(files, name: "Shared collection")
        let w = workspaces.first { $0.id == ws }
        state.links[String(c.id)] = SharedLink(workspace: ws, role: w?.role ?? "viewer", org: w?.org, orgName: w?.orgName, orgPersonal: w?.orgPersonal ?? true)
        state.states[ws] = set(plan["state"] ?? Self.emptyState, "seq", .number(Double(seq)))
        save()
        try store.saveFiles(ws, files)
        // The link first: the collection is never the personal library's, not even for a moment.
        applyRoles()
        model.applyRemote(collection: c)
        synced(c.id)
        conflicts[c.id] = []
        return c.id
    }

    /// Stops syncing a collection deleted here (the workspace and its files on the server stay).
    func forget(_ id: UInt64) {
        guard let l = state.links.removeValue(forKey: String(id)) else { return }
        if !state.links.values.contains(where: { $0.workspace == l.workspace }) {
            state.states[l.workspace] = nil
            store.dropFiles(l.workspace)
        }
        pending[id]?.cancel()
        statuses[id] = nil
        conflicts[id] = nil
        save()
    }

    // MARK: Members

    func members(_ id: UInt64) async throws -> WorkspaceInfo {
        let ws = try live(id).workspace
        let info = try await json("GET", "/v1/workspaces/\(ws)")
        let role = info["role"]?.string ?? "viewer"
        var invites: [WorkspaceInvite] = []
        if role == "owner" {
            let r = try await json("GET", "/v1/workspaces/\(ws)/invites")
            invites = (r["invites"]?.array ?? []).map { i in
                let acc = i["accepted"].flatMap { $0.isNull ? nil : $0 }
                return WorkspaceInvite(id: i["id"]?.string ?? "", email: i["email"]?.string ?? "", role: i["role"]?.string ?? "editor",
                                       acceptedBy: acc.map { a in WorkspaceMember(userId: a["userId"]?.string ?? "", email: a["email"]?.string ?? "", role: i["role"]?.string ?? "editor", runner: false, keys: memberKeys(a["keys"])) })
            }
        }
        return WorkspaceInfo(workspace: ws, epoch: info["epoch"]?.int ?? 1, role: role, members: (info["members"]?.array ?? []).map(member), invites: invites)
    }

    /// An invite (mailed when `email` is given): the code to send yourself, `<token>.<this
    /// device's fingerprint>`, so the invitee's app accepts the key only from this device.
    func invite(_ id: UInt64, email: String, role: String) async throws -> (code: String, mailed: Bool) {
        let ws = try live(id).workspace
        var body: [String: JSONAny] = ["role": .string(role)]
        let e = email.trimmingCharacters(in: .whitespacesAndNewlines)
        if !e.isEmpty { body["email"] = .string(e) }
        let r = try await json("POST", "/v1/workspaces/\(ws)/invites", .object(body))
        guard let token = r["token"]?.string, let fp = fingerprint else { throw SharedError.message("The server's invite has no code") }
        return ("\(token).\(SyncCore.normalize(fingerprint: fp))", r["mailed"]?.bool ?? false)
    }

    /// Accepts an invite code. From now on this device takes only authenticated keys for the
    /// workspace, and trusts the device the code names (once the owner adds this account).
    func accept(_ code: InviteCode) async throws -> (workspace: String, role: String) {
        _ = try me()
        let r = try await json("POST", "/v1/invites/accept", .object(["token": .string(code.token)]))
        guard let ws = r["workspaceId"]?.string else { throw SharedError.message("The server's answer has no workspace") }
        var m = (state.memory[ws] ?? .object([:])).object ?? [:]
        var expected = (m["expectedSenders"]?.array ?? []).compactMap(\.string)
        if let fp = code.fingerprint, !expected.contains(fp) { expected.append(fp) }
        m["strict"] = .bool(true)
        m["expectedSenders"] = .array(expected.map(JSONAny.string))
        state.memory[ws] = .object(m)
        save()
        return (ws, r["role"]?.string ?? "viewer")
    }

    /// Adds an account that accepted an invite, after its device keys were trusted here.
    func addMember(_ id: UInt64, email: String, role: String, keys: [MemberKey]) async throws {
        let ws = try live(id).workspace
        try checkTrusted(keys, email)
        let k = try await workspaceKeys(ws)
        guard let current = k.keys[k.epoch] else { throw SharedError.message("This device has no key for the collection") }
        _ = try await api("POST", "/v1/workspaces/\(ws)/members", .object(["email": .string(email), "role": .string(role),
                                                                         "wraps": try wraps(for: keys, ws, epoch: k.epoch, key: current)]))
        for (e, key) in k.keys.sorted(by: { $0.key < $1.key }) where e != k.epoch {
            _ = try await api("POST", "/v1/workspaces/\(ws)/wraps", .object(["epoch": .number(Double(e)), "wraps": try wraps(for: keys, ws, epoch: e, key: key, head: k.epoch)]))
        }
        do { _ = try await resealShares(ws) } catch { notices.insert(SharedNotice("Shared environment values not handed to them: \(error.localizedDescription)"), at: 0) }
    }

    /// Hands the workspace key to members' new devices whose keys are trusted here.
    @discardableResult
    func shareKeysWithNewDevices(_ id: UInt64) async throws -> Int {
        let info = try await members(id)
        let waiting = info.members.flatMap { $0.keys.filter { !$0.hasKey && $0.trust == .trusted } }
        guard !waiting.isEmpty, info.role != "viewer" else { return 0 }
        let k = try await workspaceKeys(info.workspace)
        for (e, key) in k.keys.sorted(by: { $0.key < $1.key }) {
            _ = try await api("POST", "/v1/workspaces/\(info.workspace)/wraps", .object(["epoch": .number(Double(e)), "wraps": try wraps(for: waiting, info.workspace, epoch: e, key: key, head: k.epoch)]))
        }
        _ = try? await resealShares(info.workspace)
        return waiting.count
    }

    /// Removes a member: a new key (next epoch) for every remaining device, every file sealed
    /// again under it, shared environment values re-keyed. Returns the new epoch.
    @discardableResult
    func removeMember(_ id: UInt64, userId: String) async throws -> Int {
        let info = try await members(id)
        let ws = info.workspace
        let remaining = info.members.filter { $0.userId != userId }.flatMap(\.keys)
        try checkTrusted(remaining, "the remaining members")
        let (a, _) = try me()
        let next = info.epoch + 1
        let key = try SyncCore.newKey()
        _ = try await api("DELETE", "/v1/workspaces/\(ws)/members/\(userId)", .object(["epoch": .number(Double(next)), "wraps": try wraps(for: remaining, ws, epoch: next, key: key)]))
        // Their devices aren't trusted here any more: keys they hand out from now on are refused.
        for k in info.members.first(where: { $0.userId == userId })?.keys ?? [] { state.pins[k.id] = nil }
        save()
        let k = try await workspaceKeys(ws)
        let (remote, _) = try await changes(ws, since: 0, keys: k.keys)
        for b in remote where !(b["deleted"]?.bool ?? false) {
            var vv = b["vv"]?.object ?? [:]
            vv[a.keyId] = .number(Double((vv[a.keyId]?.int ?? 0) + 1))
            let push: JSONAny = .object(["path": b["path"] ?? .string(""), "blob": b["blob"] ?? .string(""), "version": .number(Double((b["version"]?.int ?? 0) + 1)),
                                         "vv": .object(vv), "text": b["text"] ?? .null])
            _ = try await put(ws, push, epoch: next, key: k.keys[next] ?? key)
        }
        do { try await rekeyShares(ws) } catch { notices.insert(SharedNotice("Shared environment values not re-keyed: \(error.localizedDescription)"), at: 0) }
        _ = try? await syncNow(id)
        return next
    }

    func setRole(_ id: UInt64, userId: String, role: String) async throws {
        let ws = try live(id).workspace
        _ = try await api("PUT", "/v1/workspaces/\(ws)/members/\(userId)", .object(["role": .string(role)]))
    }

    // MARK: Shared environment values (HITOP Cloud C4)

    struct OpenedShare {
        var id: String
        var version: Int
        var content: JSONAny
        var key: String
        var wrappedFor: [String]
    }

    /// The shares of environment values this device can open (sync.js `openShares`).
    private func openShares(_ ws: String) async throws -> (opened: [OpenedShare], closed: Int) {
        let r = try await api("GET", "/v1/workspaces/\(ws)/env-shares", allow: [404])
        guard r.status == 200, let j = r.json else { return ([], 0) }
        let (a, secret) = try me()
        let seen = state.shareVersions[ws] ?? [:]
        var opened: [OpenedShare] = []
        var closed = 0
        for sh in j["shares"]?.array ?? [] {
            guard let sid = sh["id"]?.string, let version = sh["version"]?.int else { continue }
            let wraps = sh["wraps"]?.array ?? []
            guard let mine = wraps.first(where: { $0["keyId"]?.string == a.keyId })?["wrapped"]?.string else {
                closed += 1
                continue
            }
            let sender = try SyncCore.string("shareWrapSender", ["wrapped": .string(mine)])
            guard let senderKey = state.pins[sender] else {
                closed += 1
                continue
            }
            if version < (seen[sid] ?? 0) { throw SharedError.message("The server hands out version \(version) of a shared environment this device saw at version \(seen[sid]!): refusing an older version") }
            let o = try SyncCore.call("shareOpen", ["secret": .string(secret), "keyId": .string(a.keyId), "senderPublicKey": .string(senderKey), "workspace": .string(ws),
                                                    "shareId": .string(sid), "version": .number(Double(version)), "wrapped": .string(mine), "sealed": sh["sealed"] ?? .string("")])
            let content = (try? JSONDecoder().decode(JSONAny.self, from: Data((o["content"]?.string ?? "{}").utf8))) ?? .object([:])
            opened.append(OpenedShare(id: sid, version: version, content: content, key: o["key"]?.string ?? "", wrappedFor: wraps.compactMap { $0["keyId"]?.string }))
        }
        for o in opened { state.shareVersions[ws, default: [:]][o.id] = o.version }
        save()
        return (opened, closed)
    }

    private func shareRecipients(_ info: JSONAny, _ c: JSONAny) -> [MemberKey] {
        (info["members"]?.array ?? []).filter { m in
            let role = m["role"]?.string ?? ""
            if m["runner"]?.bool ?? false {
                return c["allRunners"]?.bool ?? false || (c["runners"]?.array ?? []).contains(m["userId"] ?? .null)
            }
            return role == "owner" || role == "editor" || (role == "viewer" && (c["viewers"]?.bool ?? false))
        }.flatMap { memberKeys($0["keys"]) }
    }

    private func shareWraps(_ keys: [MemberKey], _ ws: String, id: String, version: Int, key: String) throws -> JSONAny {
        let (a, secret) = try me()
        return .array(try keys.map { k in
            .object(["keyId": .string(k.id), "wrapped": .string(try SyncCore.string("shareWrap", [
                "secret": .string(secret), "senderKeyId": .string(a.keyId), "recipientKeyId": .string(k.id), "recipientPublicKey": .string(k.publicKey),
                "workspace": .string(ws), "shareId": .string(id), "version": .number(Double(version)), "key": .string(key),
            ]))])
        })
    }

    /// Hands every share this device can open to the trusted devices meant to have it.
    private func resealShares(_ ws: String) async throws -> Int {
        let (opened, _) = try await openShares(ws)
        guard !opened.isEmpty else { return 0 }
        let info = try await json("GET", "/v1/workspaces/\(ws)")
        guard info["role"]?.string != "viewer" else { return 0 }
        let me = try self.me().account.keyId
        var n = 0
        for o in opened {
            let missing = shareRecipients(info, o.content).filter { !o.wrappedFor.contains($0.id) && ($0.id == me || $0.trust == .trusted) }
            guard !missing.isEmpty else { continue }
            let r = try await json("POST", "/v1/workspaces/\(ws)/env-shares/\(o.id)/wraps", .object(["version": .number(Double(o.version)), "wraps": try shareWraps(missing, ws, id: o.id, version: o.version, key: o.key)]))
            n += r["stored"]?.int ?? 0
        }
        return n
    }

    /// After a removal: a new key (next version) for every share this device can open.
    private func rekeyShares(_ ws: String) async throws {
        let (opened, closed) = try await openShares(ws)
        guard !opened.isEmpty else {
            if closed > 0 { notices.insert(SharedNotice("\(closed) shared environment value set(s) couldn't be re-keyed from this device: ask whoever shared them to open the collection."), at: 0) }
            return
        }
        let info = try await json("GET", "/v1/workspaces/\(ws)")
        let me = try self.me().account.keyId
        for o in opened {
            let to = shareRecipients(info, o.content).filter { $0.id == me || $0.trust == .trusted }
            let version = o.version + 1
            let key = try SyncCore.newKey()
            let sealed = try SyncCore.string("shareSeal", ["key": .string(key), "workspace": .string(ws), "shareId": .string(o.id), "version": .number(Double(version)),
                                                           "content": .string(String(decoding: o.content.canonical, as: UTF8.self))])
            _ = try await api("PUT", "/v1/workspaces/\(ws)/env-shares/\(o.id)", .object(["version": .number(Double(version)), "sealed": .string(sealed),
                                                                                       "wraps": try shareWraps(to, ws, id: o.id, version: version, key: key)]))
        }
    }

    /// After a sync: as a sharer, hand shared environment values to new devices.
    private func refreshShares(_ ws: String) async throws {
        let (opened, _) = try await openShares(ws)
        if !opened.isEmpty { _ = try await resealShares(ws) }
    }

    // MARK: Team environments (sync-server teamenvs.rs)

    /// The current organization's team environments (`GET /v1/orgs/:org/environments`, secret
    /// values never there: each member keeps theirs in this device's Keychain), shown in the
    /// environment pill. Read here; edited in the web app.
    func loadTeamEnvironments() async {
        // Only the current organization's team environments are in the pill.
        let current = cloud.signedIn ? cloud.currentOrg.flatMap { $0.personal ? nil : $0.id } : nil
        model.dropTeamEnvironments(except: current)
        guard cloud.signedIn, let org = cloud.currentOrg, !org.personal else { return }
        guard let r = try? await api("GET", "/v1/orgs/\(org.id)/environments", allow: [400, 403, 404]), r.status == 200, let j = r.json else { return }
        let list: [SavedEnvironment] = (j["environments"]?.array ?? []).compactMap { e in
            guard let id = e["id"]?.string, var env = try? (e["data"] ?? .object([:])).decode(SavedEnvironment.self) else { return nil }
            env.id = id
            env.numericID = false
            for n in env.secrets { env.variables[n] = nil }
            env.extras["team"] = .string(org.id)
            env.extras["org"] = .string(org.id)
            if let by = e["updatedBy"]?.string { env.extras["updatedBy"] = .string(by) }
            return env
        }
        model.applyTeam(environments: list, org: org.id)
    }

    func dismissNotice(_ notice: SharedNotice) { notices.removeAll { $0 == notice } }

    /// A key notice in words for people: the device's name (never its internal key id), the
    /// fingerprint as secondary text.
    static func describe(_ n: SyncCore.KeyNotice, names: [String: String]) -> SharedNotice {
        let device = n.senderKeyId.flatMap { names[$0] }.flatMap { $0.isEmpty ? nil : $0 } ?? "a device"
        let Device = device.prefix(1).uppercased() + device.dropFirst()
        let fp = n.fingerprint.map { "Fingerprint \($0)" }
        switch n.kind {
        case "senderPinned":
            return SharedNotice("Trusted \(device) — the device your invite code named", detail: fp)
        case "senderKeyChanged":
            return SharedNotice("Security warning: \(device) has a different key on the server now. Keys from it are refused; if its owner replaced it, they add the new device.", detail: fp)
        case "knownKeyFromUntrusted":
            return SharedNotice("\(Device) sent a key you already had. Compare its fingerprint and trust it, or its next key will be refused.", detail: fp)
        case "untrustedIgnored":
            return SharedNotice("An older key from \(device), which you haven't trusted, wasn't used.", detail: fp)
        case "legacyAccepted":
            return SharedNotice("This collection's key came from an older HITOP that didn't sign it. It's in use; the owner's next sync replaces it with a signed one.")
        case "legacyIgnored":
            return SharedNotice("An older key that wasn't signed wasn't used: files still under it can't be read until the owner syncs.")
        default:
            var text = n.message
            if let id = n.senderKeyId, !id.isEmpty { text = text.replacingOccurrences(of: "device \(id)", with: device) }
            return SharedNotice(text, detail: fp)
        }
    }
}
