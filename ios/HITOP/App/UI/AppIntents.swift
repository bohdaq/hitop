import AppIntents
import Foundation

// The app's own App Intents (Siri, Shortcuts, Spotlight, the Action button). Only the app has
// them (the widget extension compiles `Shared/WidgetIntents.swift` alone), so the system always
// performs them where the collections, runner and Keychain are.

/// Runs a collection with its tests (a Live Activity shows it while it goes).
struct RunCollectionIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Run collection"
    static let description = IntentDescription("Runs a HITOP collection with its tests and tells you how it went.")

    @Parameter(title: "Collection")
    var collection: CollectionEntity

    @Parameter(title: "Environment", description: "The one HITOP uses now when left empty.")
    var environment: EnvironmentEntity?

    init() {}

    init(collection: CollectionEntity) { self.collection = collection }

    static var parameterSummary: some ParameterSummary {
        Summary("Run \(\.$collection) in \(\.$environment)")
    }

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let text = try await IntentBridge.runCollection(collection.id, environmentID: environment?.id)
        return .result(dialog: "\(text)")
    }
}

/// Picks the environment HITOP uses.
struct SwitchEnvironmentIntent: AppIntent {
    static let title: LocalizedStringResource = "Switch environment"
    static let description = IntentDescription("Picks the environment HITOP sends and runs with.")

    @Parameter(title: "Environment")
    var environment: EnvironmentEntity

    init() {}

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let text = try await IntentBridge.switchEnvironment(environment.id)
        return .result(dialog: "\(text)")
    }
}

/// How the last run went (of one collection, or of any).
struct LastRunIntent: AppIntent {
    static let title: LocalizedStringResource = "Last run result"
    static let description = IntentDescription("Tells you how the last HITOP collection run went.")

    @Parameter(title: "Collection", description: "Any collection when left empty.")
    var collection: CollectionEntity?

    init() {}

    func perform() async throws -> some IntentResult & ProvidesDialog & ReturnsValue<Bool> {
        let s = SharedContainer.current()
        let run = collection.map { c in s.collections.first { $0.id == c.id }?.lastRun } ?? s.lastRun
        guard let run else {
            return .result(value: false, dialog: "\(collection.map { "\($0.name) hasn't run yet." } ?? "No collection has run yet.")")
        }
        let ago = WidgetStyle.ago(run.finishedAt, now: Date())
        var text = "\(run.collectionName): \(run.summary), \(ago)"
        if let f = run.firstFailure { text += ". \(f)" }
        return .result(value: run.passed, dialog: "\(text)")
    }
}

struct HITOPShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: RunCollectionIntent(), phrases: [
            "Run \(\.$collection) in \(.applicationName)",
            "Run \(\.$collection) with \(.applicationName)",
            "Run a collection in \(.applicationName)",
        ], shortTitle: "Run collection", systemImageName: "play.circle")
        AppShortcut(intent: SendRequestIntent(), phrases: [
            "Send \(\.$request) with \(.applicationName)",
            "Send a request with \(.applicationName)",
        ], shortTitle: "Send request", systemImageName: "paperplane")
        AppShortcut(intent: SwitchEnvironmentIntent(), phrases: [
            "Switch \(.applicationName) to \(\.$environment)",
            "Switch environment in \(.applicationName)",
        ], shortTitle: "Switch environment", systemImageName: "globe")
        AppShortcut(intent: LastRunIntent(), phrases: [
            "Last run in \(.applicationName)",
            "How did my \(.applicationName) run go",
        ], shortTitle: "Last run result", systemImageName: "checkmark.circle")
    }

    static let shortcutTileColor: ShortcutTileColor = .purple
}
