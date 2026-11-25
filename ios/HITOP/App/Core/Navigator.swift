import Foundation
import Observation

enum AppTab: Hashable { case collections, history, environments, runs, search }

/// What the Import sheet starts with.
struct ImportTarget: Identifiable, Hashable, Sendable {
    var id = UUID()
    var file: URL?
    var text: String?
}

/// The request whose response is being explained.
struct ExplainTarget: Identifiable, Hashable, Sendable {
    var id: UInt64
}

enum Route: Hashable {
    case request(UInt64)
    case history(UInt64)
    case environment(String)
}

/// Navigation state shared by the iPhone tabs and the iPad columns.
@MainActor
@Observable
final class Navigator {
    var tab: AppTab = .collections
    var collectionsPath: [Route] = []
    var historyPath: [Route] = []
    var environmentsPath: [Route] = []
    var runsPath: [RunRoute] = []
    var searchText = ""
    var showAbout = false
    /// The Account screen (HITOP Cloud), from the organization pill.
    var showAccount = false
    /// iPad: what the middle column shows.
    var padPage: AppTab = .collections
    var padRequest: UInt64?
    var padSearch = false

    /// The run sheet of a collection (Collections, Runs, an intent or a widget's link).
    var runSheet: RunTarget?

    /// Set by RootView: the three-column layout is showing.
    var isPad = false

    /// I5: "New request from text or curl" (Collections +, the iPad sidebar, ⇧⌘N).
    var showRequestFromText = false
    /// What it starts with (shared text).
    var requestText = ""
    /// The Import sheet: picked from a menu (nil file), or a file another app handed over.
    var importing: ImportTarget?
    /// I5: the "Explain response" sheet of an open request (the response's button; ⇧⌘E on iPad).
    var explaining: ExplainTarget?

    /// The request the editor shows (iPad middle column, or the top of the iPhone Collections stack).
    var currentRequest: UInt64? {
        if isPad { return padPage == .collections ? padRequest : nil }
        if tab == .collections, case let .request(id)? = collectionsPath.last { return id }
        return nil
    }

    func showSearch() {
        if isPad { padSearch = true } else { tab = .search }
    }

    // MARK: Keyboard shortcuts (the scene's "Request" menu: ⌘↩, ⌘S, ⌘N, ⌘K)

    /// ⌘↩: sends the open request.
    func sendCurrent(_ model: AppModel) {
        guard let id = currentRequest else { return }
        model.editor(for: id)?.send()
    }

    /// ⌘S: saves the open request.
    func saveCurrent(_ model: AppModel) {
        guard let id = currentRequest else { return }
        model.editor(for: id)?.save()
    }

    /// ⌘N: a new request, opened in the editor.
    func newRequest(_ model: AppModel) {
        open(request: model.newRequest().id)
    }

    /// ⇧⌘N: a request from plain text or a curl command.
    func newRequestFromText() {
        showRequestFromText = true
    }

    /// ⇧⌘E: explains the open request's response (iPad: the response column is on screen; on
    /// iPhone the response sheet's own button does it).
    func explainCurrent(_ model: AppModel) {
        guard isPad, let id = currentRequest, case .success? = model.editor(for: id)?.outcome else { return }
        explaining = ExplainTarget(id: id)
    }

    /// Opens the run sheet of a collection (`showLast`: on its last run).
    func run(collection id: UInt64, showLast: Bool = false) {
        runSheet = RunTarget(id: id, showLast: showLast)
    }

    /// What another app shared: a file or a document to import, a curl command, or an address, which
    /// becomes a request (nothing is sent). Anything else starts as a request's text.
    func receive(_ item: SharedInbox.Item, _ model: AppModel) {
        switch item {
        case let .file(url): importing = ImportTarget(file: url)
        case let .text(text):
            let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if !t.contains(where: \.isWhitespace), let u = URL(string: t), u.scheme == "https" || u.scheme == "http", u.host != nil {
                open(request: model.newRequest(from: SavedRequest(name: AutoName.name(method: "GET", url: t), url: t, method: "GET")).id)
            } else if t.hasPrefix("{") || t.hasPrefix("[") || t.hasPrefix("openapi:") || t.hasPrefix("swagger:") {
                importing = ImportTarget(text: t)
            } else {
                requestText = t
                showRequestFromText = true
            }
        }
    }

    /// hitop://runs (the failing widget) and hitop://run/<collection id> (the last-run widget, the
    /// Live Activity).
    func open(url: URL) {
        // A file opened with HITOP (Files, Mail, another app's share sheet).
        if url.isFileURL {
            importing = ImportTarget(file: url)
            return
        }
        guard url.scheme == "hitop" else { return }
        switch url.host {
        case "runs":
            tab = .runs
            padPage = .runs
        case "run":
            tab = .runs
            padPage = .runs
            if let id = UInt64(url.lastPathComponent) { run(collection: id, showLast: true) }
        default: break
        }
    }

    /// Opens a request in the editor (Collections tab on iPhone, middle column on iPad).
    func open(request id: UInt64) {
        tab = .collections
        collectionsPath = [.request(id)]
        padPage = .collections
        padRequest = id
    }
}
