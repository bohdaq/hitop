import Foundation
import Observation
import HitopKit

/// One open request: the saved copy, the edited copy (`draft`), Send and its result.
@MainActor
@Observable
final class RequestEditor: Identifiable {
    let id: UInt64
    private(set) var saved: SavedRequest
    var draft: SavedRequest {
        didSet { if autoName { followURL() } }
    }
    /// The name follows the URL ("GET /users") until the request is renamed (web app FV1).
    private(set) var autoName: Bool
    private(set) var isSending = false
    private(set) var outcome: Result<SendOutcome, SendFailure>?
    /// When the last Send started, for "sending…" timers.
    private(set) var sendStarted: Date?

    @ObservationIgnored private weak var model: AppModel?
    @ObservationIgnored private var task: Task<Void, Never>?

    init(request: SavedRequest, model: AppModel) {
        id = request.id
        saved = request
        draft = request
        self.model = model
        autoName = request.name == AutoName.untitled || request.name == AutoName.name(method: request.method, url: request.url) && request.url.isEmpty
        if autoName { followURL() }
    }

    var collectionID: UInt64? { model?.collectionOf(request: id)?.id }
    var collectionName: String { model?.collectionOf(request: id)?.name ?? "" }

    /// "shop-api › orders".
    var breadcrumb: String { [collectionName, saved.folder.replacingOccurrences(of: "/", with: " › ")].filter { !$0.isEmpty }.joined(separator: " › ") }

    var isDirty: Bool { draft != saved }

    private func followURL() {
        let n = AutoName.name(method: draft.method, url: draft.url)
        if draft.name != n { draft.name = n }
    }

    func rename(_ name: String) {
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !n.isEmpty else { return }
        autoName = false
        draft.name = n
    }

    func save() {
        autoName = false
        model?.save(draft)
        let stored = model?.request(id) ?? draft
        saved = stored
        draft = stored
    }

    /// The saved request changed elsewhere (renamed, moved): keep the other edits.
    func adopt(_ r: SavedRequest) {
        let wasClean = !isDirty
        if r.name != AutoName.untitled { autoName = false }
        saved = r
        if wasClean { draft = r } else { draft.name = r.name; draft.file = r.file }
    }

    func revert() {
        draft = saved
    }

    // MARK: - Params (the URL's query, plus disabled rows)

    struct ParamItem: Hashable, Identifiable {
        var id: Int
        var key: String
        var value: String
        var enabled: Bool
    }

    var params: [ParamItem] {
        let on = Query.parse(draft.url).enumerated().map { ParamItem(id: $0.offset, key: $0.element.0, value: $0.element.1, enabled: true) }
        let off = draft.disabledParams.enumerated().map { ParamItem(id: 10_000 + $0.offset, key: $0.element.key, value: $0.element.value, enabled: false) }
        return on + off
    }

    func setParams(_ rows: [ParamItem]) {
        draft.url = Query.with(draft.url, rows.filter(\.enabled).map { ($0.key, $0.value) })
        draft.disabledParams = rows.filter { !$0.enabled }.map { ParamRow(key: $0.key, value: $0.value) }
    }

    // MARK: - Body

    var jsonCheck: (valid: Bool, message: String?) { JSONOutline.validate(draft.body) }

    @discardableResult
    func formatBody() -> Bool {
        guard let f = JSONOutline.format(draft.body) else { return false }
        draft.body = f
        return true
    }

    // MARK: - Sending

    var canSend: Bool { !draft.url.trimmingCharacters(in: .whitespaces).isEmpty && !isSending }

    func send() {
        guard let model, canSend else { return }
        task?.cancel()
        isSending = true
        sendStarted = Date()
        let request = draft
        let collectionID = self.collectionID
        task = Task { [weak self] in
            let result = await model.send(request, collectionID: collectionID)
            guard let self, !Task.isCancelled else { return }
            self.outcome = result
            self.isSending = false
        }
    }

    /// Waits for the Send in flight (tests).
    func waitForSend() async { await task?.value }

    func cancel() {
        task?.cancel()
        task = nil
        isSending = false
    }

    func clearResponse() { outcome = nil }
}
