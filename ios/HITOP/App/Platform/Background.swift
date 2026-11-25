import BackgroundTasks
import SwiftUI
import UIKit
import UserNotifications

/// Work while HITOP is closed: HITOP Cloud's sync (an app refresh, seconds long) and scheduled runs
/// (a processing task, minutes long; a run known to be short also in a refresh). iOS decides when.
@MainActor
final class Background: NSObject, UNUserNotificationCenterDelegate {
    static let sync = "hitop.rws8.tech.HiTop.sync"
    static let runs = "hitop.rws8.tech.HiTop.runs"
    /// A refresh has about 30 seconds.
    static let shortRunMs = 8000

    private let model: AppModel
    private let cloud: HitopCloud
    private let library: LibrarySync
    private let shared: SharedSync
    private let schedules: Schedules
    private let system: SystemBridge
    private let nav: Navigator
    private var registered = false
    private var leaving = UIBackgroundTaskIdentifier.invalid

    init(model: AppModel, cloud: HitopCloud, sync: LibrarySync, shared: SharedSync, schedules: Schedules, system: SystemBridge, nav: Navigator) {
        self.model = model
        self.cloud = cloud
        library = sync
        self.shared = shared
        self.schedules = schedules
        self.system = system
        self.nav = nav
    }

    /// Once, before the app has finished launching.
    func register() {
        registered = true
        UNUserNotificationCenter.current().delegate = self
        schedules.onChange = { [weak self] in self?.plan() }
        for id in [Self.sync, Self.runs] {
            BGTaskScheduler.shared.register(forTaskWithIdentifier: id, using: .main) { @Sendable [weak self] task in
                // Called on the main queue, as asked.
                nonisolated(unsafe) let t = task
                MainActor.assumeIsolated { self?.begin(t, id) }
            }
        }
    }

    private func begin(_ task: BGTask, _ id: String) {
        // The next one first: iOS may end this one at any time.
        plan()
        let work = Task { @MainActor in
            if id == Self.sync {
                await syncNow()
                await runDue { s in self.schedules.runs.lastRun(of: s.collectionID).map { $0.durationMs < Self.shortRunMs } ?? false }
            } else {
                await runDue()
            }
        }
        task.expirationHandler = { @Sendable in work.cancel() }
        nonisolated(unsafe) let t = task
        Task { @MainActor in
            await work.value
            t.setTaskCompleted(success: !work.isCancelled)
        }
    }

    private var syncs: Bool { cloud.signedIn && (model.sender.settings?.shown.backgroundSync ?? true) }

    private func syncNow() async {
        guard syncs else { return }
        await library.sync()
        if Task.isCancelled { return }
        await shared.syncAll()
        system.refreshWidgets()
    }

    private func runDue(_ fits: @escaping (RunSchedule) -> Bool = { _ in true }) async {
        for r in await schedules.runDue(fits: fits) where !r.passed { await notify(r) }
    }

    /// Asks iOS for the next times.
    func plan() {
        guard registered else { return }
        let scheduler = BGTaskScheduler.shared
        scheduler.cancel(taskRequestWithIdentifier: Self.sync)
        scheduler.cancel(taskRequestWithIdentifier: Self.runs)
        let due = schedules.next
        if syncs || due != nil {
            let r = BGAppRefreshTaskRequest(identifier: Self.sync)
            let sync = syncs ? Date(timeIntervalSinceNow: library.dirty ? 15 * 60 : 6 * 3600) : .distantFuture
            r.earliestBeginDate = min(sync, due ?? .distantFuture)
            try? scheduler.submit(r)
        }
        if let due {
            let r = BGProcessingTaskRequest(identifier: Self.runs)
            r.requiresNetworkConnectivity = true
            r.earliestBeginDate = due
            try? scheduler.submit(r)
        }
    }

    /// The app left the screen: what waits for HITOP Cloud goes now if it can, the rest later.
    func left() {
        guard registered else { return }
        plan()
        guard cloud.signedIn, library.dirty else { return }
        let push = Task { @MainActor in
            await library.flush()
            await shared.flush()
            plan()
            endLeaving()
        }
        guard leaving == .invalid else { return }
        leaving = UIApplication.shared.beginBackgroundTask { [weak self] in
            push.cancel()
            self?.endLeaving()
        }
    }

    private func endLeaving() {
        guard leaving != .invalid else { return }
        UIApplication.shared.endBackgroundTask(leaving)
        leaving = .invalid
    }

    // MARK: Notifications

    /// Whether failed runs may be shown; asks the first time.
    static func askToNotify() async -> Bool {
        if ProcessInfo.processInfo.arguments.contains("-UITestSeed") { return true }
        return (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])) ?? false
    }

    private func notify(_ r: RunRecord) async {
        let c = UNMutableNotificationContent()
        c.title = "\(r.collectionName): \(r.requestsPassed) of \(r.requestsTotal) passed"
        c.body = r.firstFailure ?? "The scheduled run failed."
        c.userInfo = ["collection": String(r.collectionID)]
        // One per collection: the newest failure replaces the one before.
        try? await UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "run-\(r.collectionID)", content: c, trigger: nil))
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let id = (response.notification.request.content.userInfo["collection"] as? String).flatMap(UInt64.init)
        await MainActor.run {
            guard let id, let url = URL(string: "hitop://run/\(id)") else { return }
            nav.open(url: url)
        }
    }
}

private struct BackgroundWorkKey: EnvironmentKey {
    static let defaultValue: Background? = nil
}

extension EnvironmentValues {
    var backgroundWork: Background? {
        get { self[BackgroundWorkKey.self] }
        set { self[BackgroundWorkKey.self] = newValue }
    }
}
