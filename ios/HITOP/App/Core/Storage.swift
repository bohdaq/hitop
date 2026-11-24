import Foundation

/// What the app keeps on the device that isn't a secret value: one file per collection (so the
/// I2 sync can push and pull them one by one, each with its `updatedAt`), environments, globals,
/// history and small state. Secret values go to a `SecretVault` (the Keychain).
///
/// Layout under `root` (Application Support/hitop-library): `collections/<id>.json`
/// `{formatVersion, updatedAt, collection}`, `environments.json`, `globals.json`,
/// `history.json`, `state.json`. The old app's files (Application Support/hitop) are only read,
/// never written (`LegacyImport`).
protocol LibraryStore: Sendable {
    func loadCollections() -> [SavedCollection]
    func saveCollection(_ collection: SavedCollection) throws
    func deleteCollection(id: UInt64) throws
    func loadEnvironments() -> [SavedEnvironment]
    func saveEnvironments(_ environments: [SavedEnvironment]) throws
    func loadGlobals() -> SavedEnvironment
    func saveGlobals(_ globals: SavedEnvironment) throws
    func loadHistory() -> [HistoryEntry]
    func saveHistory(_ history: [HistoryEntry]) throws
    func loadState() -> AppState
    func saveState(_ state: AppState) throws
    /// Unsaved edits of open requests (they outlive the app's process).
    func loadDrafts() -> [SavedRequest]
    func saveDrafts(_ drafts: [SavedRequest]) throws
    /// Files that are there but couldn't be used, by name under the library folder.
    var damagedFiles: Set<String> { get }
    /// Files that couldn't be opened at all (the phone is locked): nothing is written over them.
    var unreadableFiles: Set<String> { get }
    /// Moves the damaged files aside (`<name>.damaged-<time>`).
    func setAsideDamaged()
}

extension LibraryStore {
    var damagedFiles: Set<String> { [] }
    var unreadableFiles: Set<String> { [] }
    func setAsideDamaged() {}
}

struct UnreadableFile: LocalizedError {
    let name: String
    var errorDescription: String? { "\(name) can't be read right now, so it isn't replaced." }
}

/// Small local state: not synced.
struct AppState: Codable, Hashable, Sendable {
    var activeEnvironmentID: String?
    var pinnedRequests: [UInt64] = []
    var pinnedCollections: [UInt64] = []
    /// The old app's data was imported (or there was none).
    var legacyImported = false
    /// Collection order (ids); collections not in it come after, by name.
    var collectionOrder: [UInt64] = []
}

/// Where the collection files carry their sync bookkeeping.
struct CollectionFile: Codable, Sendable {
    var formatVersion: Int
    /// Milliseconds since 1970 of the last save on this device.
    var updatedAt: UInt64
    var collection: SavedCollection
}

final class FileLibraryStore: LibraryStore, @unchecked Sendable {
    let root: URL
    private let fm = FileManager.default
    static let historyLimit = 100
    private let lock = NSLock()
    private var damaged: Set<String> = []
    private var unreadable: Set<String> = []

    var damagedFiles: Set<String> { lock.lock(); defer { lock.unlock() }; return damaged }
    var unreadableFiles: Set<String> { lock.lock(); defer { lock.unlock() }; return unreadable }

    private func name(_ file: URL) -> String {
        file.deletingLastPathComponent().lastPathComponent == "collections" ? "collections/\(file.lastPathComponent)" : file.lastPathComponent
    }

    func setAsideDamaged() {
        for n in damagedFiles { setAside(n) }
    }

    private func setAside(_ n: String) {
        let stamp = Int(Date().timeIntervalSince1970 * 1000)
        guard (try? fm.moveItem(at: url(n), to: url("\(n).damaged-\(stamp)"))) != nil else { return }
        lock.lock()
        damaged.remove(n)
        lock.unlock()
    }

    init(root: URL) {
        self.root = root
        try? fm.createDirectory(at: root.appendingPathComponent("collections"), withIntermediateDirectories: true)
    }

    /// Application Support/hitop-library (the old app used Application Support/hitop).
    static func defaultRoot() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return base.appendingPathComponent("hitop-library", isDirectory: true)
    }

    private func url(_ name: String) -> URL { root.appendingPathComponent(name) }

    private func read<T: Decodable>(_ type: T.Type, _ file: URL) -> T? {
        guard fm.fileExists(atPath: file.path) else { return nil }
        let n = name(file)
        let data = try? Data(contentsOf: file)
        let value = data.flatMap { try? JSONDecoder().decode(T.self, from: $0) }
        lock.lock()
        defer { lock.unlock() }
        unreadable.remove(n)
        damaged.remove(n)
        if data == nil { unreadable.insert(n) } else if value == nil { damaged.insert(n) }
        return value
    }

    private func write<T: Encodable>(_ value: T, _ file: URL) throws {
        let n = name(file)
        if unreadableFiles.contains(n) { throw UnreadableFile(name: n) }
        if damagedFiles.contains(n) { setAside(n) }
        let enc = JSONEncoder()
        enc.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try fm.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try enc.encode(value).write(to: file, options: [.atomic])
    }

    func loadCollections() -> [SavedCollection] {
        let dir = url("collections")
        let files = (try? fm.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? []
        return files.filter { $0.pathExtension == "json" }.compactMap { read(CollectionFile.self, $0)?.collection }
    }

    func saveCollection(_ collection: SavedCollection) throws {
        let file = CollectionFile(formatVersion: 1, updatedAt: UInt64(Date().timeIntervalSince1970 * 1000), collection: collection)
        try write(file, url("collections/\(collection.id).json"))
    }

    func deleteCollection(id: UInt64) throws {
        let file = url("collections/\(id).json")
        if fm.fileExists(atPath: file.path) { try fm.removeItem(at: file) }
    }

    private struct EnvironmentsFile: Codable { var environments: [SavedEnvironment] }

    func loadEnvironments() -> [SavedEnvironment] { read(EnvironmentsFile.self, url("environments.json"))?.environments ?? [] }
    func saveEnvironments(_ environments: [SavedEnvironment]) throws { try write(EnvironmentsFile(environments: environments), url("environments.json")) }

    func loadGlobals() -> SavedEnvironment {
        read(SavedEnvironment.self, url("globals.json")) ?? SavedEnvironment(id: "globals", name: "Globals")
    }

    func saveGlobals(_ globals: SavedEnvironment) throws { try write(globals, url("globals.json")) }

    func loadHistory() -> [HistoryEntry] { read([HistoryEntry].self, url("history.json")) ?? [] }
    func saveHistory(_ history: [HistoryEntry]) throws { try write(Array(history.prefix(Self.historyLimit)), url("history.json")) }

    func loadState() -> AppState { read(AppState.self, url("state.json")) ?? AppState() }
    func saveState(_ state: AppState) throws { try write(state, url("state.json")) }

    func loadDrafts() -> [SavedRequest] { read([SavedRequest].self, url("drafts.json")) ?? [] }
    func saveDrafts(_ drafts: [SavedRequest]) throws {
        if drafts.isEmpty && !fm.fileExists(atPath: url("drafts.json").path) { return }
        try write(drafts, url("drafts.json"))
    }
}

/// Everything in memory, for tests and previews.
final class MemoryLibraryStore: LibraryStore, @unchecked Sendable {
    private let lock = NSLock()
    private var collections: [UInt64: SavedCollection] = [:]
    private var environments: [SavedEnvironment] = []
    private var globals = SavedEnvironment(id: "globals", name: "Globals")
    private var history: [HistoryEntry] = []
    private var state = AppState()

    init() {}

    private func locked<T>(_ f: () -> T) -> T { lock.lock(); defer { lock.unlock() }; return f() }

    func loadCollections() -> [SavedCollection] { locked { Array(collections.values) } }
    func saveCollection(_ collection: SavedCollection) throws { locked { collections[collection.id] = collection } }
    func deleteCollection(id: UInt64) throws { _ = locked { collections.removeValue(forKey: id) } }
    func loadEnvironments() -> [SavedEnvironment] { locked { environments } }
    func saveEnvironments(_ e: [SavedEnvironment]) throws { locked { environments = e } }
    func loadGlobals() -> SavedEnvironment { locked { globals } }
    func saveGlobals(_ g: SavedEnvironment) throws { locked { globals = g } }
    func loadHistory() -> [HistoryEntry] { locked { history } }
    func saveHistory(_ h: [HistoryEntry]) throws { locked { history = Array(h.prefix(FileLibraryStore.historyLimit)) } }
    func loadState() -> AppState { locked { state } }
    func saveState(_ s: AppState) throws { locked { state = s } }
    private var drafts: [SavedRequest] = []
    func loadDrafts() -> [SavedRequest] { locked { drafts } }
    func saveDrafts(_ d: [SavedRequest]) throws { locked { drafts = d } }
}
