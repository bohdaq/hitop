import Foundation

// What the app hands its widgets, Live Activity, Control Center control and App Intents entity
// queries: a small JSON file in the app group container (`widget-snapshot.json`). Names, counts,
// statuses and ids only: never a response body, a variable value or a secret.
//
// This file is self-contained (Foundation only, no HitopKit, no app model): it is also compiled
// into the widget extension (project.yml lists it there by path).

enum SharedContainer {
    /// Registered for the app and the extension (owner: developer portal before I6).
    static let appGroup = "group.hitop.rws8.tech.HiTop"
    static let snapshotFile = "widget-snapshot.json"

    /// The app group's folder; nil when the build has no app group (unsigned CI builds, Linux).
    static func directory() -> URL? {
        #if canImport(Darwin)
        return FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
        #else
        return nil
        #endif
    }

    /// Where the snapshot is kept: the app group's folder, else this process's own Application
    /// Support/hitop-shared (an unsigned build: the app's intents still find it, the extension
    /// shows its empty state).
    static func folder() -> URL? {
        if let d = directory() { return d }
        return FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?.appendingPathComponent("hitop-shared", isDirectory: true)
    }

    /// The snapshot where `folder()` keeps it (empty when there is none yet).
    static func current() -> WidgetSnapshot { read(from: folder()) ?? .empty }

    /// `override`: a folder to use instead (tests).
    static func read(from override: URL? = nil) -> WidgetSnapshot? {
        guard let dir = override ?? directory(), let data = try? Data(contentsOf: dir.appendingPathComponent(snapshotFile)) else { return nil }
        return try? WidgetSnapshot.decoder.decode(WidgetSnapshot.self, from: data)
    }

    static func write(_ snapshot: WidgetSnapshot, to override: URL? = nil) throws {
        guard let dir = override ?? directory() else { return }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try WidgetSnapshot.encoder.encode(snapshot).write(to: dir.appendingPathComponent(snapshotFile), options: [.atomic])
    }
}

struct WidgetSnapshot: Codable, Hashable, Sendable {
    struct Environment: Codable, Hashable, Sendable, Identifiable {
        var id: String
        var name: String
    }

    struct Run: Codable, Hashable, Sendable {
        var collectionID: String
        var collectionName: String
        var environment: String?
        var passed: Bool
        var requestsPassed: Int
        var requestsTotal: Int
        var assertionsPassed: Int
        var assertionsTotal: Int
        var durationMs: Int
        var finishedAt: Date
        /// "Cancel order: HTTP 409".
        var firstFailure: String?

        var requestsFailed: Int { requestsTotal - requestsPassed }
        /// "11 of 12 passed".
        var summary: String { "\(requestsPassed) of \(requestsTotal) passed" }
    }

    struct Collection: Codable, Hashable, Sendable, Identifiable {
        var id: String
        var name: String
        var requestCount: Int
        var lastRun: Run?
    }

    struct Request: Codable, Hashable, Sendable, Identifiable {
        var id: String
        var name: String
        /// GET, POST..., WS, GQL as the app shows it.
        var method: String
        var collection: String
        /// The last send of it (history), if any.
        var lastStatus: Int?
        var lastFailed: Bool = false
        var lastAt: Date?
    }

    var updatedAt: Date
    var activeEnvironment: Environment?
    var environments: [Environment]
    var collections: [Collection]
    /// Pinned requests, in pin order (the medium widget shows the first three).
    var pinned: [Request]
    /// Every request (for the Send request intent's entity query), at most `requestLimit`.
    var requests: [Request]
    var lastRun: Run?

    static let requestLimit = 500

    var failing: [Collection] { collections.filter { $0.lastRun.map { !$0.passed } ?? false } }

    static let empty = WidgetSnapshot(updatedAt: Date(timeIntervalSince1970: 0), activeEnvironment: nil, environments: [], collections: [],
                                      pinned: [], requests: [], lastRun: nil)

    /// The widget gallery's and previews' sample (the drafts' numbers).
    static let sample = WidgetSnapshot(
        updatedAt: Date(timeIntervalSince1970: 1_794_912_120),
        activeEnvironment: Environment(id: "env-staging", name: "Staging"),
        environments: [Environment(id: "env-staging", name: "Staging"), Environment(id: "env-production", name: "Production")],
        collections: [
            Collection(id: "1", name: "Smoke tests", requestCount: 13, lastRun: Run(collectionID: "1", collectionName: "Smoke tests", environment: "Staging", passed: true,
                                                                                  requestsPassed: 13, requestsTotal: 13, assertionsPassed: 32, assertionsTotal: 32, durationMs: 2400,
                                                                                  finishedAt: Date(timeIntervalSince1970: 1_794_911_940), firstFailure: nil)),
            Collection(id: "2", name: "Payments API", requestCount: 9, lastRun: Run(collectionID: "2", collectionName: "Payments API", environment: "Staging", passed: false,
                                                                                   requestsPassed: 7, requestsTotal: 9, assertionsPassed: 18, assertionsTotal: 21, durationMs: 1900,
                                                                                   finishedAt: Date(timeIntervalSince1970: 1_794_911_000), firstFailure: "Refund: HTTP 500")),
        ],
        pinned: [Request(id: "11", name: "Health check", method: "GET", collection: "Smoke tests", lastStatus: 200),
                 Request(id: "12", name: "Log in", method: "POST", collection: "Smoke tests", lastStatus: 200),
                 Request(id: "13", name: "List orders", method: "GET", collection: "Smoke tests")],
        requests: [], lastRun: nil)

    static var encoder: JSONEncoder {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .millisecondsSince1970
        e.outputFormatting = [.sortedKeys]
        return e
    }

    static var decoder: JSONDecoder {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .millisecondsSince1970
        return d
    }
}
