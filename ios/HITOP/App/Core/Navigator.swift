import Foundation
import Observation

enum AppTab: Hashable { case collections, history, environments, runs, search }

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
    var searchText = ""
    var showAbout = false
    /// iPad: what the middle column shows.
    var padPage: AppTab = .collections
    var padRequest: UInt64?
    var padSearch = false

    /// Set by RootView: the three-column layout is showing.
    var isPad = false

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

    /// Opens a request in the editor (Collections tab on iPhone, middle column on iPad).
    func open(request id: UInt64) {
        tab = .collections
        collectionsPath = [.request(id)]
        padPage = .collections
        padRequest = id
    }
}
