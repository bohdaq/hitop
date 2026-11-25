import Foundation
import Observation

/// Which HITOP Cloud organization's collections show (the web app's `visibleCollections`): a
/// collection's `org`, none = the personal one.
struct OrgScope: Hashable, Sendable {
    /// The organization shown; nil = personal.
    var current: String?
    /// The personal organization's id (a collection may name it in `org`).
    var personal: String?

    /// `org`: the collection's organization when it isn't its own `org` (a shared collection's
    /// workspace's).
    func contains(_ c: SavedCollection, org: String?? = nil) -> Bool {
        let o = org ?? c.org
        if current == nil || current == personal { return o == nil || o == personal }
        return o == current
    }

    /// The `org` a collection made now gets.
    var newCollectionOrg: String? { current == personal ? nil : current }
}

/// The app's state and every change to it. Views read it and call its methods; it saves through
/// a `LibraryStore` and a `SecretVault`. Foundation + Observation only (tested on Linux too).
@MainActor
@Observable
final class AppModel {
    private(set) var collections: [SavedCollection] = []
    private(set) var environments: [SavedEnvironment] = []
    private(set) var globals = SavedEnvironment(id: "globals", name: "Globals")
    private(set) var history: [HistoryEntry] = []
    private(set) var state = AppState()
    /// Values scripts set (`pm.collectionVariables.set`...), per collection, until the app quits
    /// (as the web app's `scriptVariables`).
    private(set) var scriptValues: [UInt64: [String: String]] = [:]
    private(set) var scriptGlobals: [String: String] = [:]
    /// The last save error, for a banner.
    var saveError: String?

    /// HITOP Cloud organizations: which one the Collections tab shows. nil = every collection
    /// (signed out, or a server without organizations).
    var orgScope: OrgScope?

    /// Shared collections (I2b, `SharedSync`): collection id → this account's role in its workspace
    /// (owner, editor or viewer). A viewer's collection is read-only here: every change is refused.
    var sharedRoles: [UInt64: String] = [:]
    /// A shared collection's organization (its workspace's, when it is one of the account's team
    /// organizations; nil = shown with the personal ones).
    var sharedOrgs: [UInt64: String?] = [:]
    /// Called after a change to a shared collection made here (SharedSync pushes a moment later).
    @ObservationIgnored var onSharedChange: (@MainActor (UInt64) -> Void)?
    /// Called when a collection is deleted here (a shared one stops syncing on this device).
    @ObservationIgnored var onCollectionDeleted: (@MainActor (UInt64) -> Void)?

    /// Which collections are shared isn't known (the list can't be used): nothing syncs until it is.
    var sharingProblem: StateFileProblem?
    /// A team environment was changed or deleted here (`TeamEnvironments` sends it).
    @ObservationIgnored var onTeamChange: (@MainActor (String) -> Void)?
    @ObservationIgnored var onTeamGone: (@MainActor (String) -> Void)?
    /// A collection that stays on this device only (LibrarySync).
    @ObservationIgnored var keepHere: (@MainActor (UInt64) -> Void)?
    /// Secret values shared end to end with a collection, by environment name: in memory only. They
    /// fill a secret only where this device has no value of its own (as the web app).
    @ObservationIgnored var sharedValues: [UInt64: [String: [String: String]]] = [:]
    @ObservationIgnored var reloadSharing: (@MainActor () -> Void)?
    /// Sets a damaged list aside (LibrarySync, once no collection can reach the library by it).
    @ObservationIgnored var startSharingOver: (@MainActor () throws -> Void)?

    func isShared(_ collectionID: UInt64?) -> Bool { collectionID.map { sharedRoles[$0] != nil } ?? false }

    /// A collection this account may only read (a viewer of its shared workspace).
    func isReadOnly(_ collectionID: UInt64?) -> Bool { collectionID.map { sharedRoles[$0] == "viewer" } ?? false }

    @ObservationIgnored let store: LibraryStore
    @ObservationIgnored let vault: SecretVault
    @ObservationIgnored let sender: Sender
    /// Called after every change the personal library syncs (collections, environments, globals,
    /// the active environment): LibrarySync pushes a moment later.
    @ObservationIgnored var onLibraryChange: (@MainActor () -> Void)?
    /// Changes coming from HITOP Cloud don't call `onLibraryChange`.
    @ObservationIgnored private var applyingCloud = false

    private func changed() {
        if !applyingCloud { onLibraryChange?() }
    }

    /// WebSocket connections (URLSessionWebSocketTask; an in-process echo in UI tests).
    @ObservationIgnored let webSockets: WebSocketTransport
    /// gRPC calls (hitop-core through SyncFFI; a canned Greeter in UI tests).
    @ObservationIgnored let grpcClient: GrpcClient

    init(store: LibraryStore, vault: SecretVault, sender: Sender = Sender(), webSockets: WebSocketTransport = DefaultTransports.webSocket,
         grpcClient: GrpcClient = FFIGrpcClient()) {
        self.store = store
        self.vault = vault
        self.sender = sender
        self.webSockets = webSockets
        self.grpcClient = grpcClient
    }

    // MARK: - Loading

    /// Reads the library; on first launch imports the old app's data from `legacyDirectory`.
    func load(legacyDirectory: URL? = nil) {
        collections = store.loadCollections()
        environments = store.loadEnvironments()
        globals = store.loadGlobals()
        history = store.loadHistory()
        state = store.loadState()
        keptDrafts = Dictionary(store.loadDrafts().map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        if !state.legacyImported && !store.unreadableFiles.contains("state.json") {
            if let dir = legacyDirectory { importLegacy(LegacyImport.read(from: dir)) }
            state.legacyImported = true
            persistState()
        }
        if collections.isEmpty && store.unreadableFiles.isEmpty {
            let mine = SavedCollection(name: AutoName.myRequests)
            collections = [mine]
            persist(mine)
        }
        sortCollections()
        pruneUploads()
    }

    func importLegacy(_ legacy: LegacyImport.Result) {
        let known = Set(collections.map(\.id))
        for c in legacy.collections where !known.contains(c.id) {
            collections.append(c)
            persist(c)
        }
        if !legacy.history.isEmpty {
            let ids = Set(history.map(\.id))
            history += legacy.history.filter { !ids.contains($0.id) }
            history.sort { $0.timestamp > $1.timestamp }
            persistHistory()
        }
    }

    /// Replaces everything with the shop-api sample (UI tests, screenshots).
    func seed() {
        for c in collections { try? store.deleteCollection(id: c.id) }
        collections = Seed.collections()
        collections.forEach(persist)
        environments = Seed.environments()
        for (env, name, value) in Seed.secrets { vault.setValue(value, environment: env, name: name) }
        globals = SavedEnvironment(id: "globals", name: "Globals", variables: ["userAgent": "HITOP"])
        history = Seed.history()
        state = AppState(activeEnvironmentID: "env-staging", pinnedRequests: [1104], legacyImported: true,
                         collectionOrder: collections.map(\.id))
        persistEnvironments()
        persistGlobals()
        persistHistory()
        persistState()
    }

    private func sortCollections() {
        let order = state.collectionOrder
        collections.sort { a, b in
            let ia = order.firstIndex(of: a.id) ?? Int.max
            let ib = order.firstIndex(of: b.id) ?? Int.max
            if ia != ib { return ia < ib }
            if a.name == AutoName.myRequests { return false }
            if b.name == AutoName.myRequests { return true }
            return a.name.localizedCaseInsensitiveCompare(b.name) == .orderedAscending
        }
    }

    // MARK: - Saving

    private func persist(_ c: SavedCollection) {
        do { try store.saveCollection(c) } catch { saveError = "Couldn't save \(c.name): \(error.localizedDescription)" }
        changed()
        if !applyingCloud, sharedRoles[c.id] != nil { onSharedChange?(c.id) }
    }

    private func persistEnvironments() {
        do { try store.saveEnvironments(environments) } catch { saveError = "Couldn't save the environments: \(error.localizedDescription)" }
        changed()
    }

    private func persistGlobals() {
        do { try store.saveGlobals(globals) } catch { saveError = "Couldn't save the globals: \(error.localizedDescription)" }
        changed()
    }

    // MARK: - HITOP Cloud (LibrarySync)

    /// A change from HITOP Cloud: saved here without pushing it back.
    private func fromCloud(_ change: () -> Void) {
        applyingCloud = true
        defer { applyingCloud = false }
        change()
    }

    /// Another device's collection (new or changed).
    func applyRemote(collection c: SavedCollection) {
        fromCloud {
            if let i = collections.firstIndex(where: { $0.id == c.id }) { collections[i] = c } else { collections.append(c) }
            persist(c)
            sortCollections()
        }
        for (id, e) in editors where c.requests.contains(where: { $0.id == id }) && !e.isDirty { e.adopt(request(id)!) }
    }

    /// A collection another device deleted (or one replaced by a conflict).
    func applyRemoteRemoval(collection id: UInt64) {
        fromCloud {
            collections.removeAll { $0.id == id }
            do { try store.deleteCollection(id: id) } catch { saveError = error.localizedDescription }
        }
    }

    func applyRemote(environments list: [SavedEnvironment], active: String?) {
        fromCloud {
            environments = list
            persistEnvironments()
            if state.activeEnvironmentID != active {
                state.activeEnvironmentID = active
                persistState()
            }
        }
    }

    func applyRemote(globals g: SavedEnvironment) {
        fromCloud {
            globals = g
            persistGlobals()
        }
    }

    /// The team environments of organization `org` (`TeamEnvironments`), in place of the ones
    /// this device had for it.
    func applyTeam(environments list: [SavedEnvironment], org: String) {
        let isOrg = { (e: SavedEnvironment) in e.extras["team"] == .string(org) }
        let before = environments.filter(isOrg)
        guard before != list else { return }
        fromCloud {
            environments = environments.filter { !isOrg($0) } + list
            persistEnvironments()
            if let a = state.activeEnvironmentID, before.contains(where: { $0.id == a }), !list.contains(where: { $0.id == a }) {
                state.activeEnvironmentID = nil
                persistState()
            }
        }
    }

    /// Takes out the team environments of every organization but `org` (another one is shown now,
    /// or none: signed out, personal).
    func dropTeamEnvironments(except org: String?) {
        let other = { (e: SavedEnvironment) in e.extras["team"].map { $0 != .null && $0 != org.map(JSONAny.string) } ?? false }
        guard environments.contains(where: other) else { return }
        fromCloud {
            if let a = state.activeEnvironmentID, environments.contains(where: { $0.id == a && other($0) }) {
                state.activeEnvironmentID = nil
                persistState()
            }
            environments.removeAll(where: other)
            persistEnvironments()
        }
    }

    /// A conflict copy ("shop-api (this iPhone)"): a new collection, synced like any other.
    func addCollectionCopy(_ c: SavedCollection) {
        collections.append(c)
        persist(c)
        sortCollections()
    }

    /// The collections of the organization the Collections tab shows (every one when signed out).
    var visibleCollections: [SavedCollection] {
        guard let scope = orgScope else { return collections }
        return collections.filter { c in sharedOrgs[c.id].map { scope.contains(c, org: .some($0)) } ?? scope.contains(c) }
    }

    private func persistHistory() {
        do { try store.saveHistory(history) } catch { saveError = "Couldn't save the history: \(error.localizedDescription)" }
    }

    private func persistState() {
        do { try store.saveState(state) } catch { saveError = "Couldn't save: \(error.localizedDescription)" }
    }

    private func update(collection id: UInt64, _ change: (inout SavedCollection) -> Void) {
        guard !isReadOnly(id), let i = collections.firstIndex(where: { $0.id == id }) else { return }
        change(&collections[i])
        persist(collections[i])
    }

    // MARK: - Undo

    /// What can be taken back now ("Deleted Create order").
    struct UndoOffer: Identifiable {
        let id: UInt64
        let label: String
        fileprivate let before: [SavedCollection]
        fileprivate let after: [SavedCollection]
        fileprivate let pins: AppState
        fileprivate let shared: Set<UInt64>
    }

    private(set) var undoOffer: UndoOffer?

    /// Makes a change to collections that can be taken back.
    func undoable(_ label: String, _ change: () -> Void) {
        let (before, pins, shared) = (collections, state, Set(sharedRoles.keys))
        change()
        guard collections != before else { return }
        undoOffer = UndoOffer(id: IDs.next(), label: label, before: before, after: collections, pins: pins, shared: shared)
    }

    func dismissUndo(_ id: UInt64) {
        if undoOffer?.id == id { undoOffer = nil }
    }

    /// Takes the last change back, keeping what was done since: requests come back where they were; a
    /// deleted collection comes back under a new id, since HITOP Cloud may have its deletion. A
    /// shared one that left this device isn't brought back (its members still have it).
    func undo() {
        guard let u = undoOffer else { return }
        undoOffer = nil
        for old in u.before {
            let then = u.after.first { $0.id == old.id }
            if then == nil {
                guard collection(old.id) == nil, !u.shared.contains(old.id) else { continue }
                var c = old
                c.id = IDs.next()
                // Requests that are somewhere else by now stay there.
                c.requests.removeAll { request($0.id) != nil }
                addCollectionCopy(c)
                if u.pins.pinnedCollections.contains(old.id) { togglePin(collection: c.id) }
            } else if let then, then != old {
                let changed = old.requests.filter { o in then.requests.first { $0.id == o.id } != o }
                let ids = Set(changed.map(\.id))
                let added = Set(then.requests.map(\.id)).subtracting(old.requests.map(\.id))
                update(collection: old.id) { now in
                    var list = now.requests.filter { !added.contains($0.id) }.map { r in changed.first { $0.id == r.id } ?? r }
                    for (i, r) in old.requests.enumerated() where ids.contains(r.id) && !list.contains(where: { $0.id == r.id }) {
                        let prev = old.requests[..<i].last { p in list.contains { $0.id == p.id } }
                        let at = prev.flatMap { p in list.firstIndex { $0.id == p.id } }.map { $0 + 1 } ?? 0
                        list.insert(r, at: at)
                    }
                    let missing = old.folders.filter { !now.folders.contains($0) && !then.folders.contains($0) }
                    now.folders = now.folders == then.folders ? old.folders : now.folders + missing
                    now.requests = list
                }
                ids.forEach(syncEditor)
            }
        }
        let lost = u.pins.pinnedRequests.filter { !state.pinnedRequests.contains($0) && request($0) != nil }
        if !lost.isEmpty {
            state.pinnedRequests += lost
            persistState()
        }
    }

    // MARK: - Lookups

    func collection(_ id: UInt64?) -> SavedCollection? { collections.first { $0.id == id } }

    func collectionOf(request id: UInt64) -> SavedCollection? { collections.first { $0.requests.contains { $0.id == id } } }

    func request(_ id: UInt64) -> SavedRequest? {
        for c in collections { if let r = c.requests.first(where: { $0.id == id }) { return r } }
        return nil
    }

    var activeEnvironment: SavedEnvironment? { environments.first { $0.id == state.activeEnvironmentID } }

    /// "My requests" (of the organization shown), made when missing.
    func myRequests() -> SavedCollection {
        if let c = visibleCollections.first(where: { ($0.name == AutoName.myRequests || $0.name == "Default") && !isShared($0.id) }) { return c }
        var c = SavedCollection(name: AutoName.myRequests)
        c.org = orgScope?.newCollectionOrg
        collections.append(c)
        persist(c)
        return c
    }

    // MARK: - Collections

    @discardableResult
    func addCollection(name: String) -> SavedCollection {
        var c = SavedCollection(name: name.isEmpty ? "New collection" : name)
        c.org = orgScope?.newCollectionOrg
        collections.append(c)
        persist(c)
        sortCollections()
        return c
    }

    func renameCollection(_ id: UInt64, to name: String) {
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !n.isEmpty else { return }
        update(collection: id) { $0.name = n }
    }

    func deleteCollection(_ id: UInt64) {
        onCollectionDeleted?(id)
        sharedRoles[id] = nil
        sharedOrgs[id] = nil
        collections.removeAll { $0.id == id }
        do { try store.deleteCollection(id: id) } catch { saveError = error.localizedDescription }
        state.pinnedCollections.removeAll { $0 == id }
        state.collectionOrder.removeAll { $0 == id }
        persistState()
        if collections.isEmpty { _ = myRequests() }
        changed()
    }

    func isPinned(collection id: UInt64) -> Bool { state.pinnedCollections.contains(id) }
    func isPinned(request id: UInt64) -> Bool { state.pinnedRequests.contains(id) }

    func togglePin(collection id: UInt64) {
        if let i = state.pinnedCollections.firstIndex(of: id) { state.pinnedCollections.remove(at: i) } else { state.pinnedCollections.append(id) }
        persistState()
    }

    func togglePin(request id: UInt64) {
        if let i = state.pinnedRequests.firstIndex(of: id) { state.pinnedRequests.remove(at: i) } else { state.pinnedRequests.append(id) }
        persistState()
    }

    // MARK: - Folders

    func addFolder(_ name: String, in collectionID: UInt64, parent: String = "") {
        let clean = name.replacingOccurrences(of: "/", with: "-").trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        let path = parent.isEmpty ? clean : "\(parent)/\(clean)"
        update(collection: collectionID) { c in
            if !c.allFolders.contains(path) { c.folders = c.allFolders + [path] }
        }
    }

    /// Deletes a folder, its subfolders and their requests.
    func deleteFolder(_ path: String, in collectionID: UInt64) {
        update(collection: collectionID) { c in
            let inside = { (f: String) in f == path || f.hasPrefix(path + "/") }
            c.requests.removeAll { inside($0.folder) }
            c.folders = c.allFolders.filter { !inside($0) }
        }
    }

    func renameFolder(_ path: String, to name: String, in collectionID: UInt64) {
        let clean = name.replacingOccurrences(of: "/", with: "-").trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        let parent = path.contains("/") ? String(path[..<path.lastIndex(of: "/")!]) : ""
        let newPath = parent.isEmpty ? clean : "\(parent)/\(clean)"
        func moved(_ p: String) -> String {
            if p == path { return newPath }
            if p.hasPrefix(path + "/") { return newPath + p.dropFirst(path.count) }
            return p
        }
        update(collection: collectionID) { c in
            c.folders = c.allFolders.map(moved)
            for i in c.requests.indices where !c.requests[i].folder.isEmpty {
                let r = c.requests[i]
                let fileName = String(r.file!.split(separator: "/").last ?? "")
                c.requests[i].file = "\(moved(r.folder))/\(fileName)"
            }
        }
    }

    // MARK: - Requests

    static func fileName(_ name: String) -> String {
        let s = name.lowercased().map { $0.isLetter || $0.isNumber ? String($0) : "-" }.joined()
        let parts = s.split(separator: "-").joined(separator: "-")
        return parts.isEmpty ? "request" : String(parts.prefix(60))
    }

    /// A new "Untitled" request, added at once (FV1), in `collectionID` (else My requests).
    @discardableResult
    func newRequest(in collectionID: UInt64? = nil, folder: String = "", from template: SavedRequest? = nil) -> SavedRequest {
        // A viewer's shared collection takes nothing: the request goes to My requests.
        let target = collection(collectionID).flatMap { isReadOnly($0.id) ? nil : $0 } ?? myRequests()
        var r = template ?? SavedRequest()
        r.id = IDs.next()
        if template == nil { r.name = AutoName.untitled }
        r.file = folder.isEmpty ? nil : "\(folder)/\(Self.fileName(r.name))"
        update(collection: target.id) { $0.requests.append(r) }
        return r
    }

    /// Saves an edited request in place (keeps its folder).
    func save(_ request: SavedRequest) {
        if let c = collectionOf(request: request.id), isReadOnly(c.id) { return }
        guard let c = collectionOf(request: request.id) else {
            update(collection: myRequests().id) { $0.requests.append(request) }
            return
        }
        update(collection: c.id) { c in
            guard let i = c.requests.firstIndex(where: { $0.id == request.id }) else { return }
            var r = request
            let folder = c.requests[i].folder
            // The file follows the name only when the name changed: a file named by hand (another
            // client, a shared collection) keeps its name through edits.
            if c.requests[i].name == r.name {
                r.file = c.requests[i].file
            } else {
                r.file = folder.isEmpty ? (c.requests[i].file == nil ? nil : Self.fileName(r.name)) : "\(folder)/\(Self.fileName(r.name))"
            }
            c.requests[i] = r
        }
    }

    func renameRequest(_ id: UInt64, to name: String) {
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !n.isEmpty, var r = request(id) else { return }
        r.name = n
        save(r)
        syncEditor(id)
    }

    func deleteRequest(_ id: UInt64) {
        guard let c = collectionOf(request: id) else { return }
        update(collection: c.id) { $0.requests.removeAll { $0.id == id } }
        editors[id] = nil
        if state.pinnedRequests.contains(id) {
            state.pinnedRequests.removeAll { $0 == id }
            persistState()
        }
    }

    @discardableResult
    func duplicateRequest(_ id: UInt64) -> SavedRequest? {
        guard let r = request(id), let c = collectionOf(request: id) else { return nil }
        var copy = r
        copy.id = IDs.next()
        copy.name = "\(r.name) copy"
        update(collection: c.id) { c in
            let i = (c.requests.firstIndex { $0.id == id } ?? c.requests.count - 1) + 1
            copy.file = r.folder.isEmpty ? r.file.map { _ in Self.fileName(copy.name) } : "\(r.folder)/\(Self.fileName(copy.name))"
            c.requests.insert(copy, at: i)
        }
        return copy
    }

    /// A request kept next to another's version of it (a shared collection's "Keep both").
    func addRequestCopy(_ r: SavedRequest, to collectionID: UInt64) {
        update(collection: collectionID) { $0.requests.append(r) }
    }

    /// Moves a request to another collection and/or folder (at the end).
    func moveRequest(_ id: UInt64, to collectionID: UInt64, folder: String = "") {
        guard var r = request(id), let from = collectionOf(request: id), collection(collectionID) != nil,
              !isReadOnly(from.id), !isReadOnly(collectionID) else { return }
        update(collection: from.id) { $0.requests.removeAll { $0.id == id } }
        r.file = folder.isEmpty ? nil : "\(folder)/\(Self.fileName(r.name))"
        update(collection: collectionID) { c in
            c.requests.append(r)
            if !folder.isEmpty, !c.allFolders.contains(folder) { c.folders = c.allFolders + [folder] }
        }
        syncEditor(id)
    }

    /// Every place a request can go: (collection, folder) pairs.
    var moveTargets: [(collection: SavedCollection, folder: String)] {
        collections.filter { !isReadOnly($0.id) }.flatMap { c in [(c, "")] + c.allFolders.map { (c, $0) } }
    }

    // MARK: - Environments, globals, secrets

    func setActiveEnvironment(_ id: String?) {
        state.activeEnvironmentID = id
        persistState()
        changed()
    }

    @discardableResult
    func addEnvironment(name: String) -> SavedEnvironment {
        let e = SavedEnvironment(name: name.isEmpty ? "New environment" : name)
        environments.append(e)
        persistEnvironments()
        return e
    }

    func updateEnvironment(_ e: SavedEnvironment) {
        if e.id == globals.id {
            globals = e
            persistGlobals()
            return
        }
        guard let i = environments.firstIndex(where: { $0.id == e.id }) else { return }
        let before = environments[i]
        environments[i] = e
        persistEnvironments()
        if e.extras["team"] != nil && TeamEnvironments.data(of: e, over: nil) != TeamEnvironments.data(of: before, over: nil) { onTeamChange?(e.id) }
    }

    /// The team's version, or none, in place of the one here.
    func replaceTeam(_ id: String, with e: SavedEnvironment?) {
        guard let i = environments.firstIndex(where: { $0.id == id }) else { return }
        fromCloud {
            if let e { environments[i] = e } else { environments.remove(at: i) }
            if e == nil && state.activeEnvironmentID == id {
                state.activeEnvironmentID = nil
                persistState()
            }
            persistEnvironments()
        }
        // One that is this person's own again goes to their library.
        if let e, e.extras["team"] == nil { onLibraryChange?() }
    }

    /// Shares an environment with an organization (nil: stops; it stays this person's own). Secret
    /// values stay on this device.
    func share(environment id: String, with org: String?) {
        guard let i = environments.firstIndex(where: { $0.id == id }), environments[i].extras["team"]?.string != org else { return }
        let stopped = environments[i].extras["team"] != nil
        environments[i].extras["team"] = org.map(JSONAny.string)
        environments[i].extras["org"] = org.map(JSONAny.string)
        environments[i].extras["updatedBy"] = nil
        if org != nil { environments[i].numericID = false }
        persistEnvironments()
        if org != nil { onTeamChange?(id) } else if stopped { onTeamGone?(id) }
    }

    func deleteEnvironment(_ id: String) {
        guard let e = environments.first(where: { $0.id == id }) else { return }
        for name in e.secrets { vault.setValue(nil, environment: id, name: name) }
        environments.removeAll { $0.id == id }
        if e.extras["team"] != nil { onTeamGone?(id) }
        if state.activeEnvironmentID == id { state.activeEnvironmentID = nil }
        persistEnvironments()
        persistState()
    }

    func vaultID(_ env: SavedEnvironment) -> String { env.id == globals.id ? globalsVaultID : env.id }

    func secretValue(_ env: SavedEnvironment, _ name: String) -> String { vault.value(environment: vaultID(env), name: name) ?? "" }

    /// Sets a variable; a secret's value goes to the vault only.
    func setVariable(_ env: SavedEnvironment, name: String, value: String, secret: Bool, renamedFrom old: String? = nil) {
        var e = env
        let n = name.trimmingCharacters(in: .whitespaces)
        if let old, old != n {
            e.variables.removeValue(forKey: old)
            if e.secrets.contains(old) {
                vault.setValue(nil, environment: vaultID(e), name: old)
                e.secrets.removeAll { $0 == old }
            }
        }
        guard !n.isEmpty else { updateEnvironment(e); return }
        if secret {
            e.variables.removeValue(forKey: n)
            if !e.secrets.contains(n) { e.secrets.append(n) }
            vault.setValue(value, environment: vaultID(e), name: n)
        } else {
            if e.secrets.contains(n) {
                e.secrets.removeAll { $0 == n }
                vault.setValue(nil, environment: vaultID(e), name: n)
            }
            e.variables[n] = value
        }
        updateEnvironment(e)
    }

    func removeVariable(_ env: SavedEnvironment, name: String) {
        var e = env
        e.variables.removeValue(forKey: name)
        if e.secrets.contains(name) {
            e.secrets.removeAll { $0 == name }
            vault.setValue(nil, environment: vaultID(e), name: name)
        }
        updateEnvironment(e)
    }

    /// The variables a request in `collectionID` sees.
    func context(for collectionID: UInt64?) -> VariableContext { context(for: collectionID, environment: activeEnvironment) }

    /// The variables a request in `collectionID` sees with environment `environment`.
    func context(for collectionID: UInt64?, environment: SavedEnvironment?) -> VariableContext {
        var ctx = VariableContext()
        ctx.globals = globals.variables.merging(scriptGlobals) { _, s in s }
        for n in globals.secrets { ctx.globalSecrets[n] = vault.value(environment: globalsVaultID, name: n) ?? "" }
        let c = collection(collectionID)
        ctx.collection = c?.variables ?? [:]
        if let env = environment {
            ctx.environmentName = env.name
            ctx.environment = env.variables
            for n in env.secrets {
                let own = vault.value(environment: env.id, name: n) ?? ""
                ctx.environmentSecrets[n] = own.isEmpty ? collectionID.flatMap { sharedValues[$0]?[env.name]?[n] } ?? "" : own
                ctx.environment.removeValue(forKey: n)
            }
            ctx.secretNames = Set(env.secrets)
        }
        ctx.secretNames.formUnion(globals.secrets)
        ctx.scriptSet = collectionID.flatMap { scriptValues[$0] } ?? [:]
        return ctx
    }

    // MARK: - Sending and history

    /// Sends `request` (as edited) and records it in the history.
    func send(_ request: SavedRequest, collectionID: UInt64?, stream: StreamControl? = nil) async -> Result<SendOutcome, SendFailure> {
        let ctx = context(for: collectionID)
        let col = collection(collectionID)
        let result = await sender.send(request, collection: col, context: ctx, stream: stream)
        if let id = collectionID {
            var values = scriptValues[id] ?? [:]
            for (k, v) in result.variableUpdates { values[k] = v }
            scriptValues[id] = values
        }
        for (k, v) in result.globalUpdates { scriptGlobals[k] = v }
        record(request, collectionName: col?.name, result.outcome)
        return result.outcome
    }

    /// A GraphQL introspection request (I4): sent with the request's variables and auth, but kept
    /// out of the history and of the script-set values (it runs no scripts).
    func introspect(_ request: SavedRequest, collectionID: UInt64?) async -> Result<SendOutcome, SendFailure> {
        await sender.send(request, collection: collection(collectionID), context: context(for: collectionID)).outcome
    }

    /// Secret values a request in `collectionID` can use (masked in WebSocket logs).
    func secretValues(for collectionID: UInt64?) -> [String] {
        let ctx = context(for: collectionID)
        return Array(ctx.environmentSecrets.values) + Array(ctx.globalSecrets.values)
    }

    func record(_ request: SavedRequest, collectionName: String?, _ outcome: Result<SendOutcome, SendFailure>) {
        var entry = HistoryEntry(id: IDs.next(), timestamp: UInt64(Date().timeIntervalSince1970 * 1000), collectionName: collectionName, request: request)
        switch outcome {
        case let .success(o):
            let cut = o.text.count > ResponseSnapshot.maxBody
            entry.response = ResponseSnapshot(status: o.status, headers: o.headers, body: o.isImage ? "" : String(o.text.prefix(ResponseSnapshot.maxBody)),
                                              truncated: cut, durationMs: o.durationMs, size: o.size, tests: o.tests)
        case let .failure(f):
            entry.error = f.message
        }
        history.insert(entry, at: 0)
        if history.count > FileLibraryStore.historyLimit { history.removeLast(history.count - FileLibraryStore.historyLimit) }
        persistHistory()
    }

    func deleteHistory(_ id: UInt64) {
        history.removeAll { $0.id == id }
        persistHistory()
    }

    func clearHistory() {
        history = []
        persistHistory()
    }

    /// A history item opened as a new request in My requests (as the web app does).
    @discardableResult
    func openAsNewRequest(_ entry: HistoryEntry) -> SavedRequest {
        var template = entry.request
        template.file = nil
        if template.name.isEmpty || template.name == AutoName.untitled { template.name = AutoName.name(method: template.method, url: template.url) }
        return newRequest(in: myRequests().id, from: template)
    }

    // MARK: - Open editors

    @ObservationIgnored private var editors: [UInt64: RequestEditor] = [:]

    /// The editor of a request, kept while the app runs so an unsaved draft survives navigation.
    func editor(for id: UInt64) -> RequestEditor? {
        if let e = editors[id] {
            if request(id) == nil { editors[id] = nil; return nil }
            return e
        }
        guard let r = request(id) else { return nil }
        let e = RequestEditor(request: r, model: self)
        if let kept = keptDrafts.removeValue(forKey: id), kept != r { e.draft = kept }
        editors[id] = e
        return e
    }

    /// Unsaved edits from before the app's process ended, until their request is opened.
    @ObservationIgnored private var keptDrafts: [UInt64: SavedRequest] = [:]

    /// Writes unsaved edits (the app leaves the screen: iOS may end its process).
    func saveDrafts() {
        var all = keptDrafts.filter { request($0.key) != nil }
        for (id, e) in editors where e.isDirty && request(id) != nil { all[id] = e.draft }
        try? store.saveDrafts(all.values.sorted { $0.id < $1.id })
    }

    /// After a rename or move from a list: an open editor takes the new name and place.
    private func syncEditor(_ id: UInt64) {
        guard let e = editors[id] else { return }
        if let r = request(id) { e.adopt(r) } else { editors[id] = nil }
    }

    // MARK: - Search

    struct SearchHit: Identifiable, Hashable {
        enum Kind: Hashable { case request, collection, history }
        var id: String
        var kind: Kind
        var title: String
        var subtitle: String
        var method: String?
        var requestID: UInt64?
        var collectionID: UInt64?
        var historyID: UInt64?
    }

    func search(_ query: String) -> [SearchHit] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return [] }
        var hits: [SearchHit] = []
        for c in collections {
            if c.name.lowercased().contains(q) {
                hits.append(SearchHit(id: "c\(c.id)", kind: .collection, title: c.name, subtitle: "\(c.requests.count) requests", collectionID: c.id))
            }
            for r in c.requests where r.name.lowercased().contains(q) || r.url.lowercased().contains(q) {
                hits.append(SearchHit(id: "r\(r.id)", kind: .request, title: r.name, subtitle: [c.name, r.folder].filter { !$0.isEmpty }.joined(separator: " › "),
                                      method: r.method, requestID: r.id, collectionID: c.id))
            }
        }
        for h in history.prefix(50) where h.request.url.lowercased().contains(q) || h.request.name.lowercased().contains(q) {
            hits.append(SearchHit(id: "h\(h.id)", kind: .history, title: h.request.name, subtitle: h.request.url, method: h.request.method, historyID: h.id))
        }
        return hits
    }
}
