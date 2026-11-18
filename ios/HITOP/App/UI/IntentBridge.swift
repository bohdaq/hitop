import Foundation

/// What the App Intents do, in the app's process (Siri, Shortcuts, Spotlight, the widgets' Send
/// button, the Control Center control). `HITOPApp` sets `services` at launch, also when the
/// system launches the app in the background for an intent.
@MainActor
enum IntentBridge {
    struct Services {
        var model: AppModel
        var runs: RunCenter
        var system: SystemBridge
        var nav: Navigator
    }

    static var services: Services?

    private static func app() throws -> Services {
        guard let s = services else { throw IntentFailure("HITOP isn't ready yet: open it once and try again.") }
        return s
    }

    /// "11 of 12 passed in 2.4 s · Cancel order: HTTP 409".
    static func describe(_ r: RunRecord) -> String {
        var text = "\(r.collectionName): \(r.requestsPassed) of \(r.requestsTotal) passed in \(RunActivityAttributes.seconds(r.durationMs))"
        if let f = r.firstFailure { text += ". \(f)" }
        return text
    }

    static func runCollection(_ id: String, environmentID: String?) async throws -> String {
        let s = try app()
        guard let cid = UInt64(id), s.model.collection(cid) != nil else { throw IntentFailure("That collection isn't on this device any more.") }
        if s.runs.isRunning { throw IntentFailure("A run is going already: wait for it to finish.") }
        var settings = s.runs.defaultSettings(for: cid)
        if let environmentID { settings.environmentID = environmentID }
        guard let record = await s.runs.start(collectionID: cid, settings: settings) else { throw IntentFailure("The run didn't start.") }
        return describe(record)
    }

    static func runLast() async throws -> String {
        let s = try app()
        guard let last = s.runs.runs.first(where: { s.model.collection($0.collectionID) != nil }) else {
            throw IntentFailure("Run a collection in HITOP first: this runs the one you ran last.")
        }
        let env = s.model.environments.first { $0.name == last.environmentName }?.id
        return try await runCollection(String(last.collectionID), environmentID: env)
    }

    static func send(requestID: String) async throws -> String {
        let s = try app()
        guard let id = UInt64(requestID), let r = s.model.request(id), let c = s.model.collectionOf(request: id) else {
            throw IntentFailure("That request isn't on this device any more.")
        }
        let outcome = await s.model.send(r, collectionID: c.id)
        s.system.refreshWidgets()
        switch outcome {
        case let .success(o):
            let failed = o.tests.filter { !$0.pass }.count
            var text = "\(r.name): \(o.status) in \(RunActivityAttributes.seconds(o.durationMs))"
            if !o.tests.isEmpty { text += failed == 0 ? ", \(o.tests.count) tests passed" : ", \(failed) of \(o.tests.count) tests failed" }
            return text
        case let .failure(f):
            return "\(r.name) failed: \(f.explained.headline)"
        }
    }

    static func switchEnvironment(_ id: String?) throws -> String {
        let s = try app()
        if let id, !s.model.environments.contains(where: { $0.id == id }) { throw IntentFailure("That environment isn't on this device any more.") }
        s.model.setActiveEnvironment(id)
        s.system.refreshWidgets()
        return "HITOP now uses \(s.model.activeEnvironment?.name ?? "no environment")."
    }
}
