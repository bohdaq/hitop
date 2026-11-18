import ActivityKit
import AppIntents
import Foundation
import WidgetKit

/// The app's side of the system parts: the run's Live Activity (started, updated, ended from
/// `RunCenter`'s events), the widget snapshot in the app group (written after runs, sends and when
/// the app leaves the foreground; then the widgets and controls reload) and the App Shortcuts'
/// parameters.
@MainActor
final class SystemBridge {
    let model: AppModel
    let runs: RunCenter
    /// UI tests: no Live Activity (the simulator would show it over the screenshots).
    var liveActivities = true
    private var activity: Activity<RunActivityAttributes>?
    private var lastUpdate = Date.distantPast

    static let liveActivityKey = "liveActivity"

    init(model: AppModel, runs: RunCenter) {
        self.model = model
        self.runs = runs
        runs.onEvent = { [weak self] e in self?.handle(e) }
    }

    private var wantsLiveActivity: Bool {
        liveActivities && (UserDefaults.standard.object(forKey: Self.liveActivityKey) as? Bool ?? true)
            && ActivityAuthorizationInfo().areActivitiesEnabled
    }

    private static func state(_ run: ActiveRun, finished: Bool) -> RunActivityAttributes.ContentState {
        RunActivityAttributes.ContentState(done: run.progress.done, total: run.progress.total, passed: run.progress.passed,
                                           failed: run.progress.failed, current: run.progress.current,
                                           elapsedMs: run.progress.elapsedMs, finished: finished)
    }

    private func handle(_ e: RunEvent) {
        switch e {
        case let .started(run):
            guard wantsLiveActivity else { return }
            let attributes = RunActivityAttributes(collectionID: String(run.collectionID), collectionName: run.collectionName, environment: run.environmentName)
            // A refused Live Activity (turned off, too many, background) never stops the run.
            activity = try? Activity.request(attributes: attributes, content: ActivityContent(state: Self.state(run, finished: false), staleDate: nil))
        case let .progress(run):
            guard let activity, Date().timeIntervalSince(lastUpdate) > 0.25 || run.progress.done == run.progress.total else { return }
            lastUpdate = Date()
            let content = ActivityContent(state: Self.state(run, finished: false), staleDate: nil)
            Task { await activity.update(content) }
        case let .finished(run, record):
            if let activity {
                var final = Self.state(run, finished: true)
                final.passed = record.requestsPassed
                final.failed = record.requestsFailed
                final.done = record.requestsTotal
                final.total = record.requestsTotal
                final.elapsedMs = record.durationMs
                let content = ActivityContent(state: final, staleDate: nil)
                Task { await activity.end(content, dismissalPolicy: .after(Date().addingTimeInterval(15 * 60))) }
                self.activity = nil
            }
            refreshWidgets()
        }
    }

    /// Writes the snapshot and reloads widgets, controls and App Shortcuts' parameters.
    func refreshWidgets() {
        let snapshot = WidgetSnapshot.make(model: model, runs: runs.runs)
        do { try SharedContainer.write(snapshot, to: SharedContainer.folder()) } catch {}
        WidgetCenter.shared.reloadAllTimelines()
        ControlCenter.shared.reloadAllControls()
        HITOPShortcuts.updateAppShortcutParameters()
    }
}
