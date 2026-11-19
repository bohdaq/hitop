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

    /// Send, ⌘↩ and the iPad's Send: an HTTP request is sent, a WebSocket request connects (or
    /// disconnects), a gRPC request is called (or cancelled).
    func send() {
        if draft.isWebSocket {
            socket.status == .closed ? connect() : socket.disconnect()
            return
        }
        if draft.isGrpc {
            grpcSession.calling ? grpcSession.cancel() : callGrpc()
            return
        }
        guard let model, canSend else { return }
        task?.cancel()
        isSending = true
        sendStarted = Date()
        liveEvents = []
        streamOpen = nil
        let request = draft
        let collectionID = self.collectionID
        // An answer with Content-Type text/event-stream is read live: its events show as they
        // arrive (the Events view), until it ends or Stop.
        let control = StreamControl(onOpen: { [weak self] status, headers in
            Task { @MainActor in self?.streamOpen = StreamHead(status: status, headers: headers) }
        }, onEvents: { [weak self] events in
            Task { @MainActor in self?.take(events) }
        })
        stream = control
        task = Task { [weak self] in
            let result = await model.send(request, collectionID: collectionID, stream: control)
            guard let self, !Task.isCancelled else { return }
            self.outcome = result
            self.isSending = false
            self.stream = nil
        }
    }

    /// Waits for the Send in flight (tests).
    func waitForSend() async { await task?.value }

    func cancel() {
        task?.cancel()
        task = nil
        stream?.stop()
        stream = nil
        streamOpen = nil
        isSending = false
    }

    func clearResponse() { outcome = nil }

    // MARK: - Server-Sent Events

    struct StreamHead: Equatable {
        var status: Int
        var headers: [[String]]
    }

    /// The status and headers of an event stream being read (nil when none is).
    private(set) var streamOpen: StreamHead?
    /// Its events so far (the last 1000).
    private(set) var liveEvents: [SSEEvent] = []
    @ObservationIgnored private var stream: StreamControl?

    var isStreaming: Bool { isSending && streamOpen != nil }

    private func take(_ events: [SSEEvent]) {
        liveEvents += events
        if liveEvents.count > 1000 { liveEvents.removeFirst(liveEvents.count - 1000) }
        if zip(liveEvents, liveEvents.dropFirst()).contains(where: { $0.id > $1.id }) { liveEvents.sort { $0.id < $1.id } }
    }

    /// Ends the stream: the response is what came so far; scripts and tests run on it.
    func stopStream() { stream?.stop() }

    // MARK: - WebSocket

    /// The connection and its log (memory only, while the app runs).
    let socket = SocketSession()

    private var variables: [String: String] { model?.context(for: collectionID).merged ?? [:] }

    /// Connects with the URL, headers, auth and subprotocols, variables filled in.
    func connect() {
        guard let model else { return }
        do {
            let target = try WebSocketTargets.make(draft, collectionAuth: model.collectionOf(request: id)?.auth, variables: variables)
            socket.connect(target, transport: model.webSockets, secrets: model.secretValues(for: collectionID))
        } catch {
            socket.note(.error, "Not connected: \(error.localizedDescription)")
        }
    }

    /// Sends text (a saved message or the typed one) with variables filled in.
    func sendMessage(_ text: String) async {
        do {
            let filled = try HitopKitBridge.fill(text, variables)
            await socket.send(filled)
        } catch {
            socket.note(.error, "Not sent: \(error.localizedDescription)")
        }
    }

    // MARK: - gRPC

    /// The schema, the call and its log (memory only).
    let grpcSession = GrpcSession()

    private func grpcServer() throws -> GrpcServer {
        guard let model else { throw GrpcFailure(message: "no library") }
        return try GrpcServer.of(draft, collectionAuth: model.collectionOf(request: id)?.auth, variables: variables)
    }

    /// Reads the services and methods by server reflection.
    func loadGrpcSchema() async {
        guard let model else { return }
        guard !draft.url.trimmingCharacters(in: .whitespaces).isEmpty else {
            grpcSession.setFailure("Enter the server address first (host:port).")
            return
        }
        do {
            await grpcSession.loadSchema(try grpcServer(), client: model.grpcClient)
        } catch {
            grpcSession.setFailure(error.localizedDescription)
        }
    }

    /// Fills the message with the method's request template.
    func useGrpcTemplate() async {
        guard let model, let server = try? grpcServer(), !draft.grpc.service.isEmpty, !draft.grpc.method.isEmpty else { return }
        if let t = await grpcSession.template(server, service: draft.grpc.service, method: draft.grpc.method, client: model.grpcClient) {
            draft.grpc.message = t
        }
    }

    func callGrpc() {
        guard let model else { return }
        let g = draft.grpc
        do {
            let server = try grpcServer()
            let message = try HitopKitBridge.fill(g.message, variables)
            guard !g.service.isEmpty, !g.method.isEmpty else {
                grpcSession.setFailure("No gRPC method: pick the service and method (load them from the server, or type package.Service and Method).")
                return
            }
            grpcSession.call(server, service: g.service, method: g.method, message: message, client: model.grpcClient)
        } catch {
            grpcSession.setFailure("Not called: \(error.localizedDescription)")
        }
    }

    // MARK: - GraphQL schema

    private(set) var schema: GraphQLSchema?
    private(set) var schemaLoading = false
    private(set) var schemaError: String?

    /// Introspection through the normal send path (URL, headers, auth, variables; no scripts, no
    /// history).
    func fetchSchema() async {
        guard let model else { return }
        guard !draft.url.trimmingCharacters(in: .whitespaces).isEmpty else {
            schemaError = "Enter a URL first."
            return
        }
        schemaLoading = true
        schemaError = nil
        defer { schemaLoading = false }
        let result = await model.introspect(GraphQLSchema.introspectionRequest(for: draft), collectionID: collectionID)
        switch result {
        case let .success(o):
            if !(200..<300).contains(o.status) && !o.text.contains("__schema") {
                schemaError = "The server answered \(o.status)\(o.text.isEmpty ? "" : ": \(o.text.prefix(160))")"
                return
            }
            do { schema = try GraphQLSchema.parse(o.text) } catch { schemaError = error.localizedDescription }
        case let .failure(f):
            schemaError = f.explained.headline
        }
    }
}

/// HitopKit's `${name}` filling (one pass; a name without a value is an error naming it).
enum HitopKitBridge {
    static func fill(_ text: String, _ variables: [String: String]) throws -> String {
        try Interpolation.interpolate(text, variables)
    }
}
