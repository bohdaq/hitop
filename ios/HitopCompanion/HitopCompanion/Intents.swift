import AppIntents
import HitopKit
import WidgetKit

/// A saved collection folder, as Shortcuts shows it.
struct CollectionEntity: AppEntity {
    static var typeDisplayRepresentation: TypeDisplayRepresentation = "HITOP collection"
    static var defaultQuery: CollectionQuery = CollectionQuery()

    var id: String
    var name: String

    var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }

    init(_ saved: SavedFolder) {
        id = saved.id
        name = saved.name
    }
}

struct CollectionQuery: EntityQuery {
    func entities(for identifiers: [String]) async throws -> [CollectionEntity] {
        FolderLibrary.all().filter { identifiers.contains($0.id) }.map(CollectionEntity.init)
    }

    func suggestedEntities() async throws -> [CollectionEntity] {
        FolderLibrary.all().map(CollectionEntity.init)
    }
}

struct IntentFailure: Error, CustomLocalizedStringResourceConvertible {
    let message: String
    var localizedStringResource: LocalizedStringResource { "\(message)" }
}

/// "Run HITOP collection": runs it with its chosen environment and secrets
/// from the keychain; the result is the summary line, and the run's status
/// shows in the widget.
struct RunCollectionIntent: AppIntent {
    static var title: LocalizedStringResource = "Run HITOP collection"
    static var description: IntentDescription = IntentDescription("Runs every request of a hitop collection folder and returns how many passed.")

    @Parameter(title: "Collection") var collection: CollectionEntity

    func perform() async throws -> some IntentResult & ReturnsValue<Bool> & ProvidesDialog {
        guard let saved = FolderLibrary.folder(collection.id) else { throw IntentFailure(message: "That collection is no longer in hitop.") }
        let report: Report
        do { report = try await CompanionRunner.run(saved) } catch { throw IntentFailure(message: error.localizedDescription) }
        WidgetCenter.shared.reloadAllTimelines()
        let status = RunStatus(id: saved.id, report: report)
        let line = status.passed ? "\(saved.name): \(status.summary)" : "\(saved.name): \(status.summary). \(status.firstFailure ?? "")"
        return .result(value: report.passed, dialog: "\(line)")
    }
}

/// "Get last hitop status": the last run's result without running anything.
struct LastStatusIntent: AppIntent {
    static var title: LocalizedStringResource = "Get last HITOP run status"
    static var description: IntentDescription = IntentDescription("Whether the last run of a hitop collection passed, without running it again.")

    @Parameter(title: "Collection") var collection: CollectionEntity

    func perform() async throws -> some IntentResult & ReturnsValue<Bool> & ProvidesDialog {
        guard let status = AppGroup.statusStore.status(collection.id) else {
            throw IntentFailure(message: "\(collection.name) hasn't been run yet.")
        }
        let when = status.finishedAt.formatted(date: .abbreviated, time: .shortened)
        return .result(value: status.passed, dialog: "\(status.collectionName): \(status.summary) (\(when))")
    }
}

struct HitopShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: RunCollectionIntent(), phrases: ["Run a collection in \(.applicationName)"],
                    shortTitle: "Run collection", systemImageName: "play.fill")
        AppShortcut(intent: LastStatusIntent(), phrases: ["Last run in \(.applicationName)"],
                    shortTitle: "Last run status", systemImageName: "checkmark.circle")
    }
}
