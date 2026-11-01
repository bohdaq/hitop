import Foundation

/// Runs a collection folder the way `hitop run <folder> --env <name>` does:
/// variables from globals, collection, environment, then the run's values
/// and secrets; a report with each request's file.
public enum FolderRun {
    public struct UnknownEnvironment: Error, CustomStringConvertible, LocalizedError, Sendable {
        public let name: String
        public let known: [String]
        public var description: String {
            known.isEmpty ? "environment '\(name)' not found: the folder has no environments/"
                : "environment '\(name)' not found; the folder has \(known.joined(separator: ", "))"
        }
        public var errorDescription: String? { description }
    }

    /// The globals with their secret values filled in, and those secrets.
    public static func resolveGlobals(_ folder: CollectionFolder, given: [String: String], store: SecretStore,
                                      lookupEnv: (String) -> String?) throws -> ([String: String], Secrets, [String]) {
        guard let g = folder.globals else { return ([:], Secrets(), []) }
        var values = g.variables
        for s in g.secrets { values.removeValue(forKey: s) }
        if g.secrets.isEmpty { return (values, Secrets(), []) }
        let own = given.filter { g.secrets.contains($0.key) }
        var env = g
        env.name = SecretNames.globalsEnvironment
        let (secrets, notes) = try resolveSecrets(workspace: folder.collection.workspaceID, environment: env, given: own, store: store, lookupEnv: lookupEnv)
        secrets.apply(to: &values)
        return (values, secrets, notes)
    }

    /// Runs the folder's collection (or only the requests in `only`, in
    /// collection order). Throws before sending anything when the environment
    /// is unknown or a secret has no value (the CLI's exit code 2); a failed
    /// request is in the report, not an error.
    public static func run(_ folder: CollectionFolder, environment name: String?, only: Set<UInt64>? = nil,
                           given: [String: String] = [:], store: SecretStore = platformSecretStore(),
                           lookupEnv: @escaping (String) -> String? = { ProcessInfo.processInfo.environment[$0] },
                           options: RunOptions = RunOptions(), runner: Runner = Runner(),
                           onStep: (StepResult) -> Void = { _ in }) async throws -> Report {
        var environment: Environment?
        if let name {
            guard let e = folder.environment(named: name) else {
                throw UnknownEnvironment(name: name, known: folder.environments.map(\.name))
            }
            environment = e
        }
        // As `hitop run`: every given value is an environment secret too.
        let (secrets, _) = try resolveSecrets(workspace: folder.collection.workspaceID, environment: environment, given: given, store: store, lookupEnv: lookupEnv)
        let (globals, globalSecrets, _) = try resolveGlobals(folder, given: given, store: store, lookupEnv: lookupEnv)
        var opts = options
        opts.globals = globals
        opts.globalSecrets = globalSecrets

        var collection = folder.collection
        if let only { collection.requests = collection.requests.filter { only.contains($0.id) } }

        var builder = ReportBuilder()
        builder.collection(collection.name, environment: environment?.name, folderName: folder.url.standardizedFileURL.lastPathComponent)
        builder.startIteration()
        let steps = await runner.run(collection, environment: environment, secrets: secrets, options: opts, onStep: onStep)
        builder.iteration(steps: steps)
        let bailed = opts.stopOnFail && steps.contains { !$0.passed } && steps.count < collection.requests.filter { !$0.isWebSocket && !$0.isGRPC }.count
        return builder.finish(bailed: bailed)
    }
}

/// The last run of a collection, for the widget and the Shortcuts intent:
/// counts and the first failure line, never a response or a secret value
/// (the report is masked before this is made).
public struct RunStatus: Codable, Hashable, Sendable, Identifiable {
    /// A stable key for the collection: its folder's bookmark id in the app.
    public var id: String
    public var collectionName: String
    public var environment: String?
    public var passed: Bool
    public var requestsPassed: Int
    public var requestsTotal: Int
    public var assertionsPassed: Int
    public var assertionsTotal: Int
    public var finishedAt: Date
    /// The first failed request's name and failure line.
    public var firstFailure: String?

    public init(id: String, report: Report, finishedAt: Date = Date()) {
        self.id = id
        let c = report.collections.first
        collectionName = c?.name ?? ""
        environment = c?.environment
        passed = report.passed
        requestsPassed = report.requests.passed
        requestsTotal = report.requests.total
        assertionsPassed = report.assertions.passed
        assertionsTotal = report.assertions.total
        self.finishedAt = finishedAt
        let failed = report.collections.flatMap(\.iterations).flatMap(\.requests).first { !$0.passed }
        firstFailure = failed.map { "\($0.name): \($0.failure ?? "failed")" }
    }

    /// `3/3 passed` or `2/3 passed`.
    public var summary: String { "\(requestsPassed)/\(requestsTotal) passed" }
}

/// Run statuses in one JSON file (`run-status.json`) in a directory the app
/// and its widget share (the app group container).
public struct RunStatusStore: Sendable {
    public let file: URL

    public init(directory: URL) { file = directory.appendingPathComponent("run-status.json") }

    public func all() -> [RunStatus] {
        guard let data = try? Data(contentsOf: file) else { return [] }
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return (try? d.decode([RunStatus].self, from: data)) ?? []
    }

    public func status(_ id: String) -> RunStatus? { all().first { $0.id == id } }

    /// Keeps the newest status per collection.
    public func save(_ status: RunStatus) throws {
        var list = all().filter { $0.id != status.id }
        list.append(status)
        list.sort { $0.finishedAt > $1.finishedAt }
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.prettyPrinted, .sortedKeys]
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try e.encode(list).write(to: file, options: .atomic)
    }
}
