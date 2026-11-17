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
        guard let data = try? Data(contentsOf: file) else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }

    private func write<T: Encodable>(_ value: T, _ file: URL) throws {
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
}
