import Foundation
import Observation

/// A collection run every so often while HITOP is closed. iOS decides when: never before the time
/// is up, often much later.
struct RunSchedule: Codable, Hashable, Sendable, Identifiable {
    var collectionID: UInt64
    /// nil: no environment.
    var environmentID: String?
    var everyMinutes: Int
    /// The interval counts from here: when it was chosen, then when it last started.
    var since: Date
    /// Why the last one didn't finish.
    var note: String?
    var id: UInt64 { collectionID }

    var due: Date { since.addingTimeInterval(Double(everyMinutes) * 60) }

    static let choices: [(minutes: Int, name: String)] = [(60, "hour"), (360, "6 hours"), (1440, "day")]
}

/// schedules.json (this device only, never synced).
protocol ScheduleStore: Sendable {
    func load() throws -> [RunSchedule]
    func save(_ list: [RunSchedule]) throws
}

struct FileScheduleStore: ScheduleStore {
    let file: URL

    init(root: URL) { file = root.appendingPathComponent("schedules.json") }

    func load() throws -> [RunSchedule] { try StateFileProblem.read([RunSchedule].self, file) ?? [] }

    func save(_ list: [RunSchedule]) throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONEncoder().encode(list).write(to: file, options: [.atomic])
    }
}

/// The schedules and what is due.
@MainActor
@Observable
final class Schedules {
    private(set) var list: [RunSchedule] = []
    @ObservationIgnored let store: ScheduleStore
    @ObservationIgnored let runs: RunCenter
    @ObservationIgnored private var usable = false
    /// The system's timer is set again after every change.
    @ObservationIgnored var onChange: (@MainActor () -> Void)?

    init(store: ScheduleStore, runs: RunCenter) {
        self.store = store
        self.runs = runs
        load()
    }

    /// A file that can't be opened now is read again later; a damaged one is replaced by the next change.
    private func load() {
        do {
            list = try store.load()
            usable = true
        } catch {
            usable = error as? StateFileProblem == .damaged
        }
    }

    func schedule(of collectionID: UInt64) -> RunSchedule? { list.first { $0.collectionID == collectionID } }

    func set(_ collectionID: UInt64, environmentID: String?, everyMinutes: Int?, now: Date = Date()) throws {
        if !usable { load() }
        guard usable else { throw StateFileProblem.unreadable }
        var next = list.filter { $0.collectionID != collectionID }
        if let m = everyMinutes { next.append(RunSchedule(collectionID: collectionID, environmentID: environmentID, everyMinutes: m, since: now)) }
        try store.save(next)
        list = next
        onChange?()
    }

    private func change(_ id: UInt64, _ f: (inout RunSchedule) -> Void) throws {
        guard let i = list.firstIndex(where: { $0.collectionID == id }) else { return }
        var next = list
        f(&next[i])
        try store.save(next)
        list = next
    }

    /// When the next one may run; nil: none.
    var next: Date? { list.map(\.due).min() }

    /// Runs what is due, the longest waiting first, and stops when the task is cancelled. `fits`
    /// leaves out those that won't finish in the time there is. Each counts as started before its
    /// first request, so one iOS stops isn't sent again at once.
    func runDue(now: Date = Date(), fits: (RunSchedule) -> Bool = { _ in true }) async -> [RunRecord] {
        if !usable { load() }
        guard usable, runs.model.store.unreadableFiles.isEmpty else { return [] }
        var records: [RunRecord] = []
        for s in list.filter({ $0.due <= now }).sorted(by: { $0.due < $1.due }) {
            if Task.isCancelled || runs.isRunning { break }
            guard fits(s) else { continue }
            guard runs.model.collection(s.collectionID) != nil else {
                try? set(s.collectionID, environmentID: nil, everyMinutes: nil)
                continue
            }
            if let problem = problem(s) {
                try? change(s.collectionID) { $0.note = problem; $0.since = now }
                continue
            }
            // Not written down: it would run again and again.
            do { try change(s.collectionID) { $0.since = now; $0.note = nil } } catch { break }
            if let r = await runs.start(collectionID: s.collectionID, settings: RunSettings(environmentID: s.environmentID)) {
                records.append(r)
            } else {
                try? change(s.collectionID) { $0.note = "iOS stopped the last scheduled run before it finished." }
            }
        }
        onChange?()
        return records
    }

    /// Why it can't run as it was set up: never with other values than the ones it was meant for.
    private func problem(_ s: RunSchedule) -> String? {
        let m = runs.model
        var envs = [(globalsVaultID, m.globals)]
        if let id = s.environmentID {
            guard let e = m.environments.first(where: { $0.id == id }) else { return "The environment it runs with is gone: choose the schedule again." }
            envs.append((e.id, e))
        }
        for (vault, e) in envs {
            for n in e.secrets where !readable(m, vault, n) {
                return "The Keychain couldn't be read for the last scheduled run."
            }
        }
        return nil
    }

    private func readable(_ m: AppModel, _ vault: String, _ name: String) -> Bool {
        do { _ = try m.vault.read(environment: vault, name: name); return true } catch { return false }
    }
}
