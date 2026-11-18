import AppIntents
import Foundation

// Compiled into the app and the widget extension (project.yml): the entities every intent uses and
// the two intents the extension's views start (the pinned-requests widget's Send button, the
// Control Center control). Entity queries read the snapshot the app writes (`SharedContainer`),
// so they answer in either process. The intents' work is done by `IntentBridge`: the app's runs
// it, the extension's only says to open HITOP. Both intents are `LiveActivityIntent`s, which the
// system performs in the app's process (launched in the background if needed), so the app's
// Keychain, collections and runner are there and the extension needs no secrets.

struct CollectionEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Collection"
    static let defaultQuery = CollectionQuery()

    var id: String
    var name: String
    var requestCount: Int

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)", subtitle: "\(requestCount) requests", image: .init(systemName: "folder"))
    }

    init(id: String, name: String, requestCount: Int) {
        self.id = id
        self.name = name
        self.requestCount = requestCount
    }

    init(_ c: WidgetSnapshot.Collection) { self.init(id: c.id, name: c.name, requestCount: c.requestCount) }
}

struct CollectionQuery: EntityStringQuery {
    init() {}

    func entities(for identifiers: [String]) async throws -> [CollectionEntity] {
        SharedContainer.current().collections.filter { identifiers.contains($0.id) }.map(CollectionEntity.init)
    }

    func suggestedEntities() async throws -> [CollectionEntity] { SharedContainer.current().collections.map(CollectionEntity.init) }

    func entities(matching string: String) async throws -> [CollectionEntity] {
        SharedContainer.current().collections.filter { $0.name.localizedCaseInsensitiveContains(string) }.map(CollectionEntity.init)
    }
}

struct RequestEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Request"
    static let defaultQuery = RequestQuery()

    var id: String
    var name: String
    var method: String
    var collection: String

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)", subtitle: "\(method) · \(collection)")
    }

    init(id: String, name: String, method: String, collection: String) {
        self.id = id
        self.name = name
        self.method = method
        self.collection = collection
    }

    init(_ r: WidgetSnapshot.Request) { self.init(id: r.id, name: r.name, method: r.method, collection: r.collection) }
}

struct RequestQuery: EntityStringQuery {
    init() {}

    private func all() -> [WidgetSnapshot.Request] {
        let s = SharedContainer.current()
        let pinned = Set(s.pinned.map(\.id))
        return s.pinned + s.requests.filter { !pinned.contains($0.id) }
    }

    func entities(for identifiers: [String]) async throws -> [RequestEntity] {
        all().filter { identifiers.contains($0.id) }.map(RequestEntity.init)
    }

    /// Pinned requests first.
    func suggestedEntities() async throws -> [RequestEntity] { Array(all().prefix(50)).map(RequestEntity.init) }

    func entities(matching string: String) async throws -> [RequestEntity] {
        all().filter { $0.name.localizedCaseInsensitiveContains(string) || $0.collection.localizedCaseInsensitiveContains(string) }.map(RequestEntity.init)
    }
}

struct EnvironmentEntity: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Environment"
    static let defaultQuery = EnvironmentQuery()

    var id: String
    var name: String

    var displayRepresentation: DisplayRepresentation { DisplayRepresentation(title: "\(name)") }

    init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

struct EnvironmentQuery: EntityStringQuery {
    init() {}

    private func all() -> [EnvironmentEntity] { SharedContainer.current().environments.map { EnvironmentEntity(id: $0.id, name: $0.name) } }

    func entities(for identifiers: [String]) async throws -> [EnvironmentEntity] { all().filter { identifiers.contains($0.id) } }
    func suggestedEntities() async throws -> [EnvironmentEntity] { all() }
    func entities(matching string: String) async throws -> [EnvironmentEntity] { all().filter { $0.name.localizedCaseInsensitiveContains(string) } }
}

/// Sends a saved request and says its status (the pinned-requests widget's Send button, Siri,
/// Shortcuts). The response stays in HITOP's History.
struct SendRequestIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Send request"
    static let description = IntentDescription("Sends a saved HITOP request with the current environment and tells you its status.")

    @Parameter(title: "Request")
    var request: RequestEntity

    init() {}

    init(request: RequestEntity) { self.request = request }

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let text = try await IntentBridge.send(requestID: request.id)
        return .result(dialog: "\(text)")
    }
}

/// Runs again the collection run last (the Control Center control, the Action button).
struct RunLastCollectionIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Run last collection"
    static let description = IntentDescription("Runs the HITOP collection you ran last, with the environment it ran with.")

    init() {}

    func perform() async throws -> some IntentResult & ProvidesDialog {
        let text = try await IntentBridge.runLast()
        return .result(dialog: "\(text)")
    }
}

/// What an intent couldn't do, in words.
struct IntentFailure: Error, CustomLocalizedStringResourceConvertible {
    var message: String

    init(_ message: String) { self.message = message }

    var localizedStringResource: LocalizedStringResource { "\(message)" }
}
