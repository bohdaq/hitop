import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HitopKit
import Observation

// Collection runs (I3): HitopKit's `Runner` on the app's URLSession client, the run's report in
// hitop's report.json shape (spec/run-report.md, reportVersion 1, written by HitopKit's
// `ReportBuilder`, so the file the app shares is the one `hitop run --reporter json` writes) and
// the run history kept on the device.

/// `report.json` read back (spec/run-report.md, reportVersion 1), for showing a run. The stored
/// and shared text is always HitopKit's own output; this is only a reader.
struct RunReport: Codable, Hashable, Sendable {
    struct Tool: Codable, Hashable, Sendable {
        var name: String
        var version: String
    }

    struct Counts: Codable, Hashable, Sendable {
        var total: Int
        var passed: Int
        var failed: Int
    }

    struct Summary: Codable, Hashable, Sendable {
        var collections: Int
        var iterations: Int
        var requests: Counts
        var assertions: Counts
    }

    struct Assertion: Codable, Hashable, Sendable {
        var name: String
        var passed: Bool
        var message: String?
    }

    struct Request: Codable, Hashable, Sendable {
        var name: String
        var method: String
        var url: String
        var passed: Bool
        var status: Int?
        var responseTimeMs: Int?
        var durationMs: Int
        var error: String?
        var failure: String?
        var assertions: [Assertion]
        var file: String?
        /// Added by HitopKit (what this platform left out); other readers ignore it.
        var notes: [String]?
    }

    struct Iteration: Codable, Hashable, Sendable {
        var iteration: Int
        var startedAt: String
        var durationMs: Int
        var data: [String: String]?
        var passed: Bool
        var requests: [Request]
    }

    struct Collection: Codable, Hashable, Sendable {
        var name: String
        var environment: String?
        var passed: Bool
        var iterations: [Iteration]
    }

    var reportVersion: Int
    var tool: Tool
    var startedAt: String
    var durationMs: Int
    var passed: Bool
    var bailed: Bool
    var summary: Summary
    var collections: [Collection]

    static func decode(_ text: String) throws -> RunReport {
        try JSONDecoder().decode(RunReport.self, from: Data(text.utf8))
    }

    /// Every request of every iteration, in order.
    var requests: [Request] { collections.flatMap(\.iterations).flatMap(\.requests) }

    /// The slowest request time below which 95 % of the answered requests came back (ms).
    var p95: Int? {
        let times = requests.compactMap(\.responseTimeMs).sorted()
        guard !times.isEmpty else { return nil }
        let rank = Int((Double(times.count) * 0.95).rounded(.up)) - 1
        return times[max(0, min(times.count - 1, rank))]
    }
}

/// One finished run, kept in the Runs tab.
struct RunRecord: Codable, Hashable, Sendable, Identifiable {
    var id: UInt64
    var collectionID: UInt64
    var collectionName: String
    var environmentName: String?
    /// Milliseconds since 1970.
    var finishedAt: UInt64
    var passed: Bool
    var bailed: Bool
    var requestsPassed: Int
    var requestsTotal: Int
    var assertionsPassed: Int
    var assertionsTotal: Int
    var durationMs: Int
    var iterations: Int
    /// The first failed request: "Cancel order: HTTP 409".
    var firstFailure: String?
    /// report.json as HitopKit wrote it (secret values already masked).
    var reportJSON: String

    var report: RunReport? { try? RunReport.decode(reportJSON) }
    var date: Date { Date(timeIntervalSince1970: Double(finishedAt) / 1000) }
    var requestsFailed: Int { requestsTotal - requestsPassed }

    init(id: UInt64 = IDs.next(), collectionID: UInt64, report: Report, finishedAt: Date = Date()) {
        self.id = id
        self.collectionID = collectionID
        let c = report.collections.first
        collectionName = c?.name ?? ""
        environmentName = c?.environment
        self.finishedAt = UInt64(finishedAt.timeIntervalSince1970 * 1000)
        passed = report.passed
        bailed = report.bailed
        requestsPassed = report.requests.passed
        requestsTotal = report.requests.total
        assertionsPassed = report.assertions.passed
        assertionsTotal = report.assertions.total
        durationMs = report.durationMs
        iterations = report.summaryIterations
        let failed = report.collections.flatMap(\.iterations).flatMap(\.requests).first { !$0.passed }
        firstFailure = failed.map { "\($0.name): \($0.failure ?? "failed")" }
        reportJSON = report.jsonText
    }
}

/// Where the run history lives: `hitop-library/runs.json`, newest first, the last `limit` runs.
protocol RunStore: Sendable {
    func load() -> [RunRecord]
    func save(_ runs: [RunRecord]) throws
}

final class FileRunStore: RunStore, @unchecked Sendable {
    let file: URL
    static let limit = 30

    init(root: URL) { file = root.appendingPathComponent("runs.json") }

    func load() -> [RunRecord] {
        guard let data = try? Data(contentsOf: file) else { return [] }
        return (try? JSONDecoder().decode([RunRecord].self, from: data)) ?? []
    }

    func save(_ runs: [RunRecord]) throws {
        let enc = JSONEncoder()
        enc.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try enc.encode(Array(runs.prefix(Self.limit))).write(to: file, options: [.atomic])
    }
}

final class MemoryRunStore: RunStore, @unchecked Sendable {
    private let lock = NSLock()
    private var runs: [RunRecord] = []

    init() {}

    func load() -> [RunRecord] { lock.withLock { runs } }
    func save(_ r: [RunRecord]) throws { lock.withLock { runs = Array(r.prefix(FileRunStore.limit)) } }
}

/// How a run goes: the environment, stop at the first failure (`--bail`), how many times.
struct RunSettings: Hashable, Sendable {
    /// nil: no environment.
    var environmentID: String?
    var bail = false
    var iterations = 1
}

/// The Runs tab's and the run sheet's pages.
enum RunRoute: Hashable, Sendable { case report(UInt64) }

/// The run sheet of a collection.
struct RunTarget: Identifiable, Hashable, Sendable {
    var id: UInt64
    /// Open on the collection's last run instead of the run options.
    var showLast = false
}

/// A run while it goes.
struct RunProgress: Hashable, Sendable {
    var total: Int
    var done = 0
    var passed = 0
    var failed = 0
    /// The last request done.
    var current: String?
    var elapsedMs = 0

    var fraction: Double { total == 0 ? 0 : Double(done) / Double(total) }
}

/// One request done, as the run screen lists it while the run goes.
struct RunStepRow: Hashable, Sendable, Identifiable {
    var id: Int
    var name: String
    var method: String
    var passed: Bool
    var status: Int?
    var durationMs: Int
}

extension Sender {
    /// The requests a run sends (WebSocket and gRPC are left out, as `hitop run` leaves them out).
    static func runnable(_ c: SavedCollection) -> [SavedRequest] {
        c.requests.filter { $0.kind != "websocket" && $0.kind != "grpc" }
    }

    /// Runs `collection` `iterations` times the way `hitop run --env <e> [--bail] [--iterations n]`
    /// does: one report (HitopKit's `ReportBuilder`), steps masked by the runner, `onStep` after
    /// each request (from the runner's task).
    func run(_ collection: SavedCollection, context: VariableContext, bail: Bool = false, iterations: Int = 1,
             onStep: @escaping @Sendable (StepResult) -> Void = { _ in }) async -> Report {
        let kit = HitopKit.Collection(id: collection.id, name: collection.name, requests: collection.requests.map(Self.kitRequest),
                                      variables: context.collection, auth: Self.kitAuth(collection.auth))
        let env = context.environmentName.map { HitopKit.Environment(id: "app", name: $0, variables: context.environment) }
        var secrets = Secrets()
        for (k, v) in context.environmentSecrets where !v.isEmpty { secrets.insert(k, v, origin: .keychain) }
        var globals = context.globals
        var globalSecrets = Secrets()
        for (k, v) in context.globalSecrets where !v.isEmpty {
            globals[k] = v
            globalSecrets.insert(k, v, origin: .keychain)
        }
        var options = RunOptions(stopOnFail: bail, timeout: timeout, globals: globals, globalSecrets: globalSecrets)
        ExtraAuth(collection.requests, collectionAuth: collection.auth).install(&options)
        let config = configuration
        let runner = Runner(engine: defaultScriptEngine, makeClient: { MeteredClient(configuration: config()) })
        var builder = ReportBuilder()
        builder.collection(collection.name, environment: context.environmentName)
        let runnable = Self.runnable(collection).count
        var bailed = false
        let rounds = max(1, iterations)
        for round in 0..<rounds {
            builder.startIteration()
            let steps = await runner.run(kit, environment: env, secrets: secrets, options: options) { onStep($0) }
            builder.iteration(steps: steps)
            if bail && steps.contains(where: { !$0.passed }) {
                bailed = steps.count < runnable || round < rounds - 1
                break
            }
        }
        return builder.finish(bailed: bailed)
    }
}

/// What a run tells the system parts (Live Activity, widgets) as it goes.
enum RunEvent: Sendable {
    case started(ActiveRun)
    case progress(ActiveRun)
    case finished(ActiveRun, RunRecord)
}

/// The run going on now.
struct ActiveRun: Hashable, Sendable {
    var collectionID: UInt64
    var collectionName: String
    var environmentName: String?
    var settings: RunSettings
    var progress: RunProgress
    var steps: [RunStepRow] = []
    var startedAt = Date()
}

/// Runs collections and keeps their history (the Runs tab, the run screen, widgets, intents).
@MainActor
@Observable
final class RunCenter {
    private(set) var runs: [RunRecord] = []
    private(set) var active: ActiveRun?
    /// The last run's record (also when it is no longer the newest of its collection).
    private(set) var lastFinished: RunRecord?
    var saveError: String?
    /// ActivityKit and WidgetKit (UI layer) listen here.
    @ObservationIgnored var onEvent: (@MainActor (RunEvent) -> Void)?

    @ObservationIgnored let model: AppModel
    @ObservationIgnored let store: RunStore

    init(model: AppModel, store: RunStore) {
        self.model = model
        self.store = store
        runs = store.load().sorted { $0.finishedAt > $1.finishedAt }
    }

    var isRunning: Bool { active != nil }

    func lastRun(of collectionID: UInt64) -> RunRecord? { runs.first { $0.collectionID == collectionID } }

    func run(_ id: UInt64) -> RunRecord? { runs.first { $0.id == id } }

    /// Collections whose last run failed, newest first.
    var failing: [RunRecord] {
        var seen = Set<UInt64>()
        return runs.filter { r in
            defer { seen.insert(r.collectionID) }
            return !seen.contains(r.collectionID) && !r.passed
        }
    }

    /// Default settings for a collection: the active environment, or the one it last ran with.
    func defaultSettings(for collectionID: UInt64) -> RunSettings {
        RunSettings(environmentID: model.state.activeEnvironmentID)
    }

    /// Runs a collection (one at a time; a second start while one runs returns nil). Viewers of a
    /// shared collection can run it: nothing of the collection is changed.
    @discardableResult
    func start(collectionID: UInt64, settings: RunSettings) async -> RunRecord? {
        guard active == nil, let collection = model.collection(collectionID) else { return nil }
        let context = model.runContext(for: collectionID, environmentID: settings.environmentID)
        let total = Sender.runnable(collection).count * max(1, settings.iterations)
        var run = ActiveRun(collectionID: collectionID, collectionName: collection.name, environmentName: context.environmentName,
                            settings: settings, progress: RunProgress(total: total))
        active = run
        onEvent?(.started(run))

        let (stream, continuation) = AsyncStream.makeStream(of: StepResult.self)
        let sender = model.sender
        let task = Task.detached {
            let report = await sender.run(collection, context: context, bail: settings.bail, iterations: settings.iterations) { continuation.yield($0) }
            continuation.finish()
            return report
        }
        for await step in stream {
            run.progress.done += 1
            if step.passed { run.progress.passed += 1 } else { run.progress.failed += 1 }
            run.progress.current = step.requestName
            run.progress.elapsedMs = Int(Date().timeIntervalSince(run.startedAt) * 1000)
            run.steps.append(RunStepRow(id: run.steps.count, name: step.requestName, method: step.method, passed: step.passed,
                                        status: step.error == nil ? step.response?.status : nil, durationMs: step.durationMs))
            active = run
            onEvent?(.progress(run))
        }
        let report = await task.value
        let record = RunRecord(collectionID: collectionID, report: report)
        run.progress.elapsedMs = report.durationMs
        runs.insert(record, at: 0)
        if runs.count > FileRunStore.limit { runs.removeLast(runs.count - FileRunStore.limit) }
        persist()
        lastFinished = record
        active = nil
        onEvent?(.finished(run, record))
        return record
    }

    func delete(_ id: UInt64) {
        runs.removeAll { $0.id == id }
        persist()
    }

    func clear() {
        runs = []
        persist()
    }

    private func persist() {
        do { try store.save(runs) } catch { saveError = "Couldn't save the run: \(error.localizedDescription)" }
    }
}

extension AppModel {
    /// The variables a run of `collectionID` sees with environment `environmentID` (nil: none).
    /// Script-set values of earlier sends don't carry into a run (a run starts afresh, as `hitop run`).
    func runContext(for collectionID: UInt64, environmentID: String?) -> VariableContext {
        var ctx = context(for: collectionID, environment: environments.first { $0.id == environmentID })
        ctx.scriptSet = [:]
        return ctx
    }
}
