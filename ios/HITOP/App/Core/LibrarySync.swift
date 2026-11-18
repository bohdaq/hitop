import Foundation
import Observation

/// The personal library (HITOP Cloud C8, `GET /v1/library`, `PUT`/`DELETE /v1/library/:kind/:id`):
/// signed in, this device's collections, environments and globals are kept on the server, so every
/// device and browser the person signs in to has them. The web app's `library.js`, item for item:
///
/// - items: `collection/<id>` (one per collection, the web app's JSON), `environments/all`
///   (`{environments, activeEnvironmentId}`, secret values left empty) and `globals/all`
///   (`{variables, secrets}`). Secret values travel apart in `secrets` (`{envId: {name: value}}`,
///   globals `{name: value}`): the server seals them under its host key, or doesn't keep them
///   (`secretsStored: false`, then they stay in this device's Keychain). They are sent because a
///   PUT replaces the item's secrets: an item without them would wipe the values another device
///   saved. Here they live in the Keychain only (`SecretVault`), never in the library files.
/// - every write names the version it last saw (`baseVersion`); another device's change in
///   between answers 409 with the server's item: the server's is kept, and this device's
///   collection stays as a copy named "… (this iPhone)".
/// - deletes leave tombstones; a pull removes a collection only if this device had it from the
///   library.
/// - offline first: what differs from what the library last took (`known` fingerprints, kept in
///   library-sync.json) is the queue, so it survives a relaunch; a transport error or a 5xx stops
///   the round and retries later, a refused item (402 full, 413 too large) stays on this device.
/// - the first link uploads this device's collections, except an untouched sample or an empty
///   My requests; linking another account later uploads none of them (they stay on this device).
@MainActor
@Observable
final class LibrarySync {
    struct Known: Codable, Hashable, Sendable {
        var version: Int
        var hash: String
    }

    /// library-sync.json.
    struct State: Codable, Hashable, Sendable {
        /// "<server> <userId>" this state is for.
        var account = ""
        var cursor: Int?
        var known: [String: Known] = [:]
        /// Collections kept on this device only (here before the first link, or refused).
        var localOnly: [String] = []
        /// Items the server refused (4xx), by the fingerprint refused: tried again once changed.
        var refused: [String: String] = [:]
        var lastSync: Date?
    }

    struct Status: Hashable, Sendable {
        var on = false
        var saving = false
        var pulling = false
        var error: String?
        var secretsStored: Bool?
        var lastSync: Date?
    }

    enum Badge: Hashable, Sendable {
        /// Signed out, or kept on this device only.
        case local
        /// Changed here, not in HITOP Cloud yet.
        case waiting
        case synced
        /// The server refused it (full, too large): it stays on this device.
        case refused
    }

    private(set) var status = Status()
    /// Per collection, for the cards ("synced 2 min ago", "on this iPhone").
    private(set) var badges: [UInt64: Badge] = [:]
    /// What the person should know (a conflict copy, a refused item): the newest first.
    private(set) var notices: [String] = []

    @ObservationIgnored let cloud: HitopCloud
    @ObservationIgnored let model: AppModel
    @ObservationIgnored let stateStore: LibrarySyncStateStore
    /// "this iPhone" / "this iPad": the conflict copy's name.
    @ObservationIgnored let deviceLabel: String
    @ObservationIgnored var pushDelay: Duration = .seconds(1)
    @ObservationIgnored var retryDelay: Duration = .seconds(15)
    @ObservationIgnored private(set) var state: State
    @ObservationIgnored private var pushTask: Task<Void, Never>?
    @ObservationIgnored private var pushing = false
    @ObservationIgnored private var pushAgain = false
    @ObservationIgnored private var pullTask: Task<Void, Never>?

    init(cloud: HitopCloud, model: AppModel, stateStore: LibrarySyncStateStore, deviceLabel: String = "this iPhone") {
        self.cloud = cloud
        self.model = model
        self.stateStore = stateStore
        self.deviceLabel = deviceLabel
        state = stateStore.load()
        status.lastSync = state.lastSync
        model.onLibraryChange = { [weak self] in self?.changed() }
        cloud.onAccountChange = { [weak self] in
            guard let self else { return }
            Task { await self.sync() }
        }
        refreshBadges()
    }

    private func save() {
        try? stateStore.save(state)
    }

    // MARK: - Account

    /// Linked to the signed-in account (a new account starts a new state, with the first-link rules).
    @discardableResult
    func checkAccount() -> Bool {
        let key = cloud.accountKey
        guard !key.isEmpty else {
            if status.on { status.on = false }
            pushTask?.cancel()
            refreshBadges()
            return false
        }
        if state.account != key {
            state = Self.firstLink(account: key, previous: state, collections: model.collections)
            save()
        }
        status.on = true
        return true
    }

    /// A new account's state. The first account this device links uploads what is here, except
    /// what nobody filled in (an empty My requests, the untouched sample); a different account
    /// later uploads nothing (those collections stay on this device).
    static func firstLink(account: String, previous: State, collections: [SavedCollection]) -> State {
        let anotherBefore = !previous.account.isEmpty
        let localOnly = collections.filter { anotherBefore || LibraryRules.untouched($0) }.map { String($0.id) }
        return State(account: account, cursor: nil, known: [:], localOnly: localOnly)
    }

    // MARK: - Items

    struct Item: Hashable, Sendable {
        var kind: String
        var id: String
        var data: JSONAny
        var secrets: [String: JSONAny]

        var key: String { "\(kind)/\(id)" }

        /// A short, stable fingerprint of the item as pushed (data + secrets).
        var hash: String {
            let bytes = JSONAny.object(["data": data, "secrets": .object(secrets)]).canonical
            var h: UInt64 = 0xcbf2_9ce4_8422_2325
            for b in bytes {
                h ^= UInt64(b)
                h = h &* 0x100_0000_01b3
            }
            return "\(String(h, radix: 16))-\(bytes.count)"
        }

        var isEmptySettings: Bool {
            kind != "collection" && (data["environments"]?.array?.isEmpty ?? true) && (data["variables"]?.object?.isEmpty ?? true)
        }
    }

    private func isTeam(_ e: SavedEnvironment) -> Bool {
        if let t = e.extras["team"], t != .null, t != .bool(false), t != .string("") { return true }
        return false
    }

    func collectionItem(_ c: SavedCollection) -> Item {
        Item(kind: "collection", id: String(c.id), data: (try? JSONAny.from(c)) ?? .null, secrets: [:])
    }

    /// An environment as the library keeps it: secret values empty, the values apart.
    private func split(_ e: SavedEnvironment, vaultID: String) -> (JSONAny, [String: JSONAny]) {
        var json = ((try? JSONAny.from(e)) ?? .null).object ?? [:]
        var variables = json["variables"]?.object ?? [:]
        var secrets: [String: JSONAny] = [:]
        for n in e.secrets {
            variables[n] = .string("")
            if let v = model.vault.value(environment: vaultID, name: n), !v.isEmpty { secrets[n] = .string(v) }
        }
        json["variables"] = .object(variables)
        return (.object(json), secrets)
    }

    var environmentsItem: Item {
        let envs = model.environments.filter { !isTeam($0) }
        var list: [JSONAny] = []
        var secrets: [String: JSONAny] = [:]
        for e in envs {
            let (data, s) = split(e, vaultID: e.id)
            list.append(data)
            if !s.isEmpty { secrets[e.id] = .object(s) }
        }
        let activeEnv = envs.first { $0.id == model.state.activeEnvironmentID }
        let active: JSONAny = activeEnv.map { e in e.numericID ? .number(Double(e.id) ?? 0) : .string(e.id) } ?? .null
        return Item(kind: "environments", id: "all", data: .object(["environments": .array(list), "activeEnvironmentId": active]), secrets: secrets)
    }

    var globalsItem: Item {
        let g = model.globals
        var variables = g.variables.mapValues(JSONAny.string)
        var secrets: [String: JSONAny] = [:]
        for n in g.secrets {
            variables[n] = .string("")
            if let v = model.vault.value(environment: globalsVaultID, name: n), !v.isEmpty { secrets[n] = .string(v) }
        }
        return Item(kind: "globals", id: "all", data: .object(["variables": .object(variables), "secrets": .array(g.secrets.map(JSONAny.string))]), secrets: secrets)
    }

    /// What the library should hold now, by key.
    func desired() -> [String: Item] {
        var out: [String: Item] = [:]
        let localOnly = Set(state.localOnly)
        for c in model.collections where !localOnly.contains(String(c.id)) {
            let it = collectionItem(c)
            out[it.key] = it
        }
        for it in [environmentsItem, globalsItem] { out[it.key] = it }
        return out
    }

    // MARK: - Applying the server's items

    /// The server's items over this device's (pull, conflicts).
    func apply(_ items: [JSONAny]) {
        for it in items {
            guard let kind = it["kind"]?.string, let id = it["id"]?.string, let version = it["version"]?.int else { continue }
            let key = "\(kind)/\(id)"
            if let k = state.known[key], k.version >= version { continue } // our own write, or older
            let deleted = it["deleted"]?.bool ?? false
            switch kind {
            case "collection":
                guard let cid = UInt64(id) ?? (it["data"]?["id"]?.int).map({ UInt64($0) }) else { continue }
                if deleted {
                    // Only a collection this device holds from the library goes.
                    if state.known[key] != nil, model.collection(cid) != nil { model.applyRemoteRemoval(collection: cid) }
                    state.known.removeValue(forKey: key)
                    continue
                }
                guard let data = it["data"], var c = try? data.decode(SavedCollection.self) else { continue }
                c.id = cid
                model.applyRemote(collection: c)
                state.localOnly.removeAll { $0 == id }
                state.refused.removeValue(forKey: key)
                state.known[key] = Known(version: version, hash: collectionItem(c).hash)
            case "environments" where !deleted:
                applyEnvironments(it)
                state.known[key] = Known(version: version, hash: environmentsItem.hash)
            case "globals" where !deleted:
                applyGlobals(it)
                state.known[key] = Known(version: version, hash: globalsItem.hash)
            default:
                continue
            }
        }
        save()
    }

    private func applyEnvironments(_ it: JSONAny) {
        let data = it["data"] ?? .null
        let secrets = it["secrets"] ?? .null
        let list: [SavedEnvironment] = (data["environments"]?.array ?? []).compactMap { j in
            guard var e = try? j.decode(SavedEnvironment.self) else { return nil }
            // Secret values: the server's, else the ones this device already has.
            for n in e.secrets {
                let value = secrets[e.id]?[n]?.string ?? model.vault.value(environment: e.id, name: n) ?? e.variables[n] ?? ""
                model.vault.setValue(value, environment: e.id, name: n)
                e.variables.removeValue(forKey: n)
            }
            return e
        }
        // Team environments (an organization's, I2b) aren't the library's.
        let kept = model.environments.filter(isTeam)
        let activeJSON = data["activeEnvironmentId"]
        let activeID = activeJSON?.string ?? activeJSON?.int.map(String.init)
        let active = list.contains(where: { $0.id == activeID }) ? activeID : model.state.activeEnvironmentID
        model.applyRemote(environments: list + kept, active: active)
    }

    private func applyGlobals(_ it: JSONAny) {
        let data = it["data"] ?? .null
        let names = (data["secrets"]?.array ?? []).compactMap(\.string)
        var variables: [String: String] = [:]
        for (k, v) in data["variables"]?.object ?? [:] { variables[k] = v.string ?? "" }
        for n in names {
            let value = it["secrets"]?[n]?.string ?? model.vault.value(environment: globalsVaultID, name: n) ?? variables[n] ?? ""
            model.vault.setValue(value, environment: globalsVaultID, name: n)
            variables.removeValue(forKey: n)
        }
        var g = model.globals
        g.variables = variables
        g.secrets = names
        model.applyRemote(globals: g)
    }

    // MARK: - Pull

    /// Other devices' changes (on launch, coming to the foreground, pull to refresh).
    func pull() async {
        guard checkAccount() else { return }
        if let pullTask { return await pullTask.value }
        let task = Task { await self.pullNow() }
        pullTask = task
        await task.value
        pullTask = nil
    }

    private func pullNow() async {
        status.pulling = true
        defer { status.pulling = false }
        let first = state.cursor == nil
        let path = state.cursor.map { "/v1/library?since=\($0)" } ?? "/v1/library"
        do {
            let r = try await cloud.call("GET", path, allow: Set(400...599))
            if r.status == 401 { return cloud.sessionEnded() }
            guard r.status == 200, let j = r.json else {
                status.error = "Couldn't read HITOP Cloud: \(HitopCloud.error(r).localizedDescription)"
                return
            }
            apply(j["items"]?.array ?? [])
            if first { dropEmptyMyRequests() }
            state.cursor = j["cursor"]?.int ?? state.cursor
            status.secretsStored = j["secretsStored"]?.bool
            synced()
        } catch {
            status.error = "Working on this device until HITOP Cloud answers: \(error.localizedDescription)"
        }
        refreshBadges()
    }

    /// The first link: an empty My requests kept on this device goes when the library brought one.
    private func dropEmptyMyRequests() {
        let fromLibrary = model.collections.filter { state.known["collection/\($0.id)"] != nil && LibraryRules.isMyRequests($0) }
        guard !fromLibrary.isEmpty else { return }
        for c in model.collections where state.localOnly.contains(String(c.id)) && LibraryRules.isEmptyMyRequests(c) {
            model.applyRemoteRemoval(collection: c.id)
            state.localOnly.removeAll { $0 == String(c.id) }
        }
        save()
    }

    private func synced() {
        state.lastSync = Date()
        status.lastSync = state.lastSync
        status.error = nil
        save()
    }

    // MARK: - Push

    private func changed() {
        refreshBadges()
        schedulePush()
    }

    func schedulePush(after delay: Duration? = nil) {
        guard cloud.signedIn else { return }
        pushTask?.cancel()
        let wait = delay ?? pushDelay
        pushTask = Task { [weak self] in
            try? await Task.sleep(for: wait)
            guard !Task.isCancelled else { return }
            await self?.push()
        }
    }

    /// Pull, then push what waits: the whole round (launch, foreground, pull to refresh).
    func sync() async {
        guard checkAccount() else { return }
        await pull()
        await push()
    }

    /// Pushes what waits until nothing does (tests, Sync now).
    func flush() async {
        pushTask?.cancel()
        for _ in 0..<3 {
            await push()
            if !dirty { return }
        }
    }

    /// Something here differs from what the library last took.
    var dirty: Bool {
        guard status.on else { return false }
        let want = desired()
        for (key, item) in want where state.known[key]?.hash != item.hash && state.refused[key] != item.hash {
            if state.known[key] != nil || !(item.isEmptySettings) { return true }
        }
        return state.known.keys.contains { $0.hasPrefix("collection/") && want[$0] == nil }
    }

    /// Why a round stopped (offline, server error, signed out).
    private enum Stop: Error { case again(String), signedOut }

    func push() async {
        guard checkAccount() else { return }
        if pushing {
            pushAgain = true
            return
        }
        pushing = true
        status.saving = true
        var itemErrors: [String] = []
        defer {
            pushing = false
            status.saving = false
            refreshBadges()
        }
        if let pullTask { await pullTask.value }
        do {
            let want = desired()
            // Environments and globals first, then collections (any order works).
            for key in want.keys.sorted(by: { ($0.hasPrefix("collection/") ? 1 : 0, $0) < ($1.hasPrefix("collection/") ? 1 : 0, $1) }) {
                guard let item = want[key] else { continue }
                let h = item.hash
                let known = state.known[key]
                if known?.hash == h || state.refused[key] == h { continue }
                if known == nil && item.isEmptySettings { continue }
                let body = JSONAny.object(["data": item.data, "secrets": .object(item.secrets), "baseVersion": .number(Double(known?.version ?? 0))])
                let r = try await send("PUT", "/v1/library/\(item.kind)/\(item.id)", body)
                switch r.status {
                case 200:
                    state.known[key] = Known(version: r.json?["version"]?.int ?? (known?.version ?? 0) + 1, hash: h)
                    state.refused.removeValue(forKey: key)
                    if let stored = r.json?["secretsStored"]?.bool { status.secretsStored = stored }
                case 409:
                    conflict(item, current: r.json?["current"] ?? .null)
                default:
                    // 402 (storage full), 413 (over 5 MB), another refusal: it stays here.
                    state.refused[key] = h
                    let name = item.kind == "collection" ? "“\(item.data["name"]?.string ?? "A collection")”" : item.kind == "globals" ? "The globals" : "The environments"
                    let why = r.status == 402 ? "HITOP Cloud storage is full" : r.status == 413 ? "it is over 5 MB" : HitopCloud.error(r).localizedDescription
                    itemErrors.append("\(name) wasn't saved to HITOP Cloud (\(why)). It stays on this \(deviceWord).")
                }
                save()
            }
            // Collections gone from here go from the library (a delete, or kept local now).
            for key in state.known.keys.sorted() where key.hasPrefix("collection/") && want[key] == nil {
                guard let k = state.known[key] else { continue }
                let id = String(key.dropFirst("collection/".count))
                let r = try await send("DELETE", "/v1/library/collection/\(id)?baseVersion=\(k.version)", nil)
                if r.status == 200 {
                    state.known.removeValue(forKey: key)
                } else if r.status == 409 {
                    conflict(Item(kind: "collection", id: id, data: .null, secrets: [:]), current: r.json?["current"] ?? .null)
                } else {
                    state.known.removeValue(forKey: key)
                }
                save()
            }
            synced()
            status.error = itemErrors.first
            for e in itemErrors where !notices.contains(e) { notices.insert(e, at: 0) }
        } catch Stop.signedOut {
            cloud.sessionEnded()
        } catch Stop.again(let why) {
            status.error = why
            schedulePush(after: retryDelay)
        } catch {
            status.error = error.localizedDescription
        }
        if pushAgain {
            pushAgain = false
            schedulePush(after: .zero)
        }
    }

    private var deviceWord: String { deviceLabel.replacingOccurrences(of: "this ", with: "") }

    /// One request of a push: a transport error, a 5xx or a 429 stops the round (tried again
    /// later), a 401 signs out; anything else is the item's answer.
    private func send(_ method: String, _ path: String, _ body: JSONAny?) async throws -> CloudResponse {
        let r: CloudResponse
        do {
            r = try await cloud.call(method, path, body: body, allow: Set(400...599))
        } catch CloudError.notSignedIn {
            throw Stop.signedOut
        } catch {
            throw Stop.again("Saved on this \(deviceWord); HITOP Cloud can't be reached, trying again.")
        }
        if r.status == 401 { throw Stop.signedOut }
        if r.status >= 500 || r.status == 429 { throw Stop.again("Saved on this \(deviceWord); HITOP Cloud answered \(r.status), trying again.") }
        return r
    }

    /// Another device changed `item` first: the server's stays, this device's collection is kept
    /// as a copy named "… (this iPhone)".
    func conflict(_ item: Item, current: JSONAny) {
        let key = item.key
        if current.isNull {
            // Gone from the server altogether: next round writes it as new.
            state.known.removeValue(forKey: key)
            pushAgain = true
            return
        }
        let version = current["version"]?.int ?? 0
        let theirsDeleted = current["deleted"]?.bool ?? false
        guard item.kind == "collection" else {
            state.known.removeValue(forKey: key)
            apply([current])
            notices.insert(item.kind == "globals" ? "Globals changed on another device: showing theirs." : "Environments changed on another device: showing theirs.", at: 0)
            return
        }
        guard let cid = UInt64(item.id) else { return }
        let mine = item.data.isNull ? nil : model.collection(cid)
        model.applyRemoteRemoval(collection: cid)
        if !theirsDeleted, let data = current["data"], var theirs = try? data.decode(SavedCollection.self) {
            theirs.id = cid
            model.applyRemote(collection: theirs)
            state.known[key] = Known(version: version, hash: collectionItem(theirs).hash)
        } else {
            state.known.removeValue(forKey: key)
        }
        if var copy = mine {
            let name = copy.name
            copy.id = IDs.next()
            copy.name = "\(name) (\(deviceLabel))"
            model.addCollectionCopy(copy)
            let note = "“\(name)” changed on another device. Kept theirs and saved yours as “\(copy.name)”."
            notices.insert(note, at: 0)
        }
        save()
    }

    // MARK: - Local only

    func isLocalOnly(_ id: UInt64) -> Bool { state.localOnly.contains(String(id)) }

    /// A collection kept on this device only goes to HITOP Cloud now.
    func upload(_ id: UInt64) async {
        state.localOnly.removeAll { $0 == String(id) }
        save()
        refreshBadges()
        await flush()
    }

    func dismissNotice(_ text: String) {
        notices.removeAll { $0 == text }
    }

    // MARK: - Badges

    /// "just now", "2 min ago", "3 h ago", "4 days ago" (the cards' "synced 2 min ago").
    nonisolated static func ago(_ date: Date, now: Date = Date()) -> String {
        let s = Int(now.timeIntervalSince(date))
        if s < 60 { return "just now" }
        if s < 3600 { return "\(s / 60) min ago" }
        if s < 86400 { return "\(s / 3600) h ago" }
        let d = s / 86400
        return d == 1 ? "yesterday" : "\(d) days ago"
    }

    func refreshBadges() {
        var out: [UInt64: Badge] = [:]
        let on = cloud.signedIn && state.account == cloud.accountKey
        for c in model.collections {
            let key = "collection/\(c.id)"
            if !on || state.localOnly.contains(String(c.id)) {
                out[c.id] = .local
            } else {
                let h = collectionItem(c).hash
                if state.known[key]?.hash == h {
                    out[c.id] = .synced
                } else if state.refused[key] == h {
                    out[c.id] = .refused
                } else {
                    out[c.id] = .waiting
                }
            }
        }
        if out != badges { badges = out }
    }
}

/// The first-link rules (web app `new-request.js`): what nobody filled in isn't uploaded.
enum LibraryRules {
    static func isMyRequests(_ c: SavedCollection) -> Bool { c.name == AutoName.myRequests || c.name == "Default" }

    static func isBlank(_ r: SavedRequest) -> Bool {
        let t = { (s: String) in s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        return t(r.url) && t(r.body) && t(r.preRequestScript) && t(r.postRequestScript) && r.kind == nil
    }

    /// An empty My requests (or an older "Default"): no request with anything in it, no
    /// variables, no folders.
    static func isEmptyMyRequests(_ c: SavedCollection) -> Bool {
        isMyRequests(c) && c.requests.allSatisfy(isBlank) && c.variables.isEmpty && c.folders.isEmpty
    }

    /// The web app's sample collection as it was added (`isPristineSample`).
    static let sampleName = "Sample: JSONPlaceholder"

    static func isPristineSample(_ c: SavedCollection) -> Bool {
        guard c.name == sampleName else { return false }
        return c.requests.allSatisfy { r in
            guard let s = sampleRequests[r.name] else { return false }
            return s == [r.method, r.url, r.body, r.preRequestScript, r.postRequestScript]
        }
    }

    static func untouched(_ c: SavedCollection) -> Bool { isEmptyMyRequests(c) || isPristineSample(c) }

    /// The web app's sample requests (`new-request.js` `sampleRequests()`): method, URL, body, scripts.
    static let sampleRequests: [String: [String]] = [
        "List posts": ["GET", "${baseUrl}/posts?_limit=3", "", "",
                       "pm.test(\"status is 200\", () => pm.response.to.have.status(200));\npm.test(\"three posts\", () => pm.expect(pm.response.json().length).to.eql(3));\n// The next request reads it as ${postId}.\npm.collectionVariables.set(\"postId\", pm.response.json()[0].id);"],
        "Get the first post": ["GET", "${baseUrl}/posts/${postId}", "", "",
                               "pm.test(\"same post\", () => pm.expect(pm.response.json().id).to.eql(Number(pm.collectionVariables.get(\"postId\"))));"],
        "Create a post": ["POST", "${baseUrl}/posts", "{\n  \"title\": \"Hello from HITOP\",\n  \"body\": \"A sample post\",\n  \"userId\": 1\n}", "",
                          "pm.test(\"created\", () => pm.response.to.have.status(201));"],
    ]
}

protocol LibrarySyncStateStore: Sendable {
    func load() -> LibrarySync.State
    func save(_ state: LibrarySync.State) throws
}

struct FileLibrarySyncStateStore: LibrarySyncStateStore {
    let file: URL

    init(root: URL) { file = root.appendingPathComponent("library-sync.json") }

    func load() -> LibrarySync.State {
        guard let data = try? Data(contentsOf: file) else { return LibrarySync.State() }
        return (try? JSONDecoder().decode(LibrarySync.State.self, from: data)) ?? LibrarySync.State()
    }

    func save(_ state: LibrarySync.State) throws {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys]
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try e.encode(state).write(to: file, options: [.atomic])
    }
}

final class MemoryLibrarySyncStateStore: LibrarySyncStateStore, @unchecked Sendable {
    private let lock = NSLock()
    private var value = LibrarySync.State()

    init() {}

    func load() -> LibrarySync.State { lock.lock(); defer { lock.unlock() }; return value }
    func save(_ state: LibrarySync.State) throws { lock.lock(); defer { lock.unlock() }; value = state }
}
