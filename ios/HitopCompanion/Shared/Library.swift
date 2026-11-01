import Foundation
import HitopKit

/// What the app, its App Intents and its widget share through the app group:
/// the folders the user opened (security-scoped bookmarks) and the last run
/// status of each. Compiled into the app and the widget extension.
enum AppGroup {
    /// Set the same id in both targets' entitlements (project.yml).
    static let id = "group.hitop.rws8.tech.companion"

    static var defaults: UserDefaults { UserDefaults(suiteName: id) ?? .standard }

    static var container: URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: id)
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    }

    static var statusStore: RunStatusStore { RunStatusStore(directory: container) }
}

/// A collection folder the user picked in Files (iCloud Drive, On My
/// iPhone, a provider). The app keeps a bookmark, never a copy.
struct SavedFolder: Codable, Identifiable, Hashable, Sendable {
    var id: String
    /// The collection's name when it was last read.
    var name: String
    var bookmark: Data
    /// Shown in the widget.
    var pinned: Bool
    /// The environment to run with (`--env`); nil for none.
    var environment: String?
}

/// The saved folders, in UserDefaults of the app group.
enum FolderLibrary {
    private static let key = "folders"

    static func all() -> [SavedFolder] {
        guard let data = AppGroup.defaults.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([SavedFolder].self, from: data)) ?? []
    }

    static func save(_ folders: [SavedFolder]) {
        if let data = try? JSONEncoder().encode(folders) { AppGroup.defaults.set(data, forKey: key) }
    }

    static func folder(_ id: String) -> SavedFolder? { all().first { $0.id == id } }

    static func update(_ folder: SavedFolder) {
        var list = all()
        if let i = list.firstIndex(where: { $0.id == folder.id }) { list[i] = folder } else { list.append(folder) }
        save(list)
    }

    static func remove(_ id: String) {
        save(all().filter { $0.id != id })
    }

    /// A bookmark for a URL the document picker gave (call inside its
    /// security-scoped access).
    static func bookmark(for url: URL) throws -> Data {
        try url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
    }

    /// Reads the folder behind a bookmark: resolves it, opens its security
    /// scope, coordinates the read (iCloud Drive may still be downloading),
    /// and refreshes a stale bookmark.
    static func read(_ saved: SavedFolder) throws -> CollectionFolder {
        var stale = false
        let url = try URL(resolvingBookmarkData: saved.bookmark, options: [], relativeTo: nil, bookmarkDataIsStale: &stale)
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        if stale, let fresh = try? bookmark(for: url) {
            var s = saved
            s.bookmark = fresh
            update(s)
        }
        downloadPlaceholders(in: url)
        var result: Result<CollectionFolder, Error> = .failure(FormatError("could not read \(url.lastPathComponent)"))
        var coordinationError: NSError?
        NSFileCoordinator().coordinate(readingItemAt: url, options: [], error: &coordinationError) { readURL in
            result = Result { try CollectionFolder.read(readURL) }
        }
        if let coordinationError { throw coordinationError }
        var folder = try result.get()
        folder.url = url
        return folder
    }

    /// iCloud Drive keeps files that aren't on the device as `.<name>.icloud`
    /// placeholders (hidden, so the reader skips them). Asks for each to be
    /// downloaded and waits a little for them.
    static func downloadPlaceholders(in dir: URL, wait: TimeInterval = 15) {
        let fm = FileManager.default
        func placeholders() -> [URL] {
            guard let e = fm.enumerator(at: dir, includingPropertiesForKeys: nil) else { return [] }
            return e.compactMap { $0 as? URL }.filter { $0.lastPathComponent.hasPrefix(".") && $0.pathExtension == "icloud" }
        }
        var pending = placeholders()
        guard !pending.isEmpty else { return }
        for p in pending {
            let name = String(p.deletingPathExtension().lastPathComponent.dropFirst())
            try? fm.startDownloadingUbiquitousItem(at: p.deletingLastPathComponent().appendingPathComponent(name))
        }
        let deadline = Date().addingTimeInterval(wait)
        while !pending.isEmpty, Date() < deadline {
            Thread.sleep(forTimeInterval: 0.25)
            pending = placeholders()
        }
    }
}

/// Runs a saved folder (all of it, or some requests), keeps the status for
/// the widget and the Shortcuts intent, and returns the report. Shared by
/// the app and its intents.
enum CompanionRunner {
    static let secretStore: SecretStore = KeychainSecretStore()

    static func run(_ saved: SavedFolder, only: Set<UInt64>? = nil) async throws -> Report {
        let folder = try FolderLibrary.read(saved)
        let report = try await FolderRun.run(folder, environment: saved.environment, only: only, store: secretStore, lookupEnv: { _ in nil })
        // A whole run is what the widget shows; a single request is not.
        if only == nil {
            try? AppGroup.statusStore.save(RunStatus(id: saved.id, report: report))
        }
        return report
    }
}
