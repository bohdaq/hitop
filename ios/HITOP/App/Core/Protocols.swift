import Foundation

// I4: a request's kind (HTTP, WebSocket, gRPC) and the WebSocket and gRPC fields, in the saved
// shape of the web app and the CLI (spec/collection-format.md §3 "WebSocket", "gRPC"; hitop-core
// model.rs `WebSocket`, `WsMessage`, `Grpc`). They live in the request's `extras` as JSON, so a
// key this app doesn't know (in `websocket`, a message row or `grpc`) is kept as it came.

/// A saved WebSocket message: `{name, content}` (+ unknown keys).
struct WSSavedMessage: Hashable, Sendable, Identifiable {
    var id = UUID()
    var name: String
    var content: String
    var extra: [String: JSONAny] = [:]

    init(name: String = "", content: String = "", extra: [String: JSONAny] = [:]) {
        self.name = name
        self.content = content
        self.extra = extra
    }

    static func == (a: Self, b: Self) -> Bool { a.name == b.name && a.content == b.content && a.extra == b.extra }
    func hash(into h: inout Hasher) { h.combine(name); h.combine(content) }
}

/// `websocket`: `{subprotocols, messages}`.
struct WebSocketFields: Hashable, Sendable {
    var subprotocols: [String] = []
    var messages: [WSSavedMessage] = []
    var extra: [String: JSONAny] = [:]

    init(subprotocols: [String] = [], messages: [WSSavedMessage] = [], extra: [String: JSONAny] = [:]) {
        self.subprotocols = subprotocols
        self.messages = messages
        self.extra = extra
    }

    init(json: JSONAny?) {
        guard case let .object(o)? = json else { return }
        subprotocols = o["subprotocols"]?.array?.compactMap(\.string) ?? []
        messages = o["messages"]?.array?.compactMap { m -> WSSavedMessage? in
            guard case var .object(row) = m else { return nil }
            let name = row.removeValue(forKey: "name")?.string ?? ""
            let content = row.removeValue(forKey: "content")?.string ?? ""
            return WSSavedMessage(name: name, content: content, extra: row)
        } ?? []
        extra = o.filter { $0.key != "subprotocols" && $0.key != "messages" }
    }

    /// As saved; nil when empty (the web app drops an empty `websocket`).
    var json: JSONAny? {
        var o = extra
        let subs = subprotocols.filter { !$0.isEmpty }
        if !subs.isEmpty { o["subprotocols"] = .array(subs.map(JSONAny.string)) }
        if !messages.isEmpty {
            o["messages"] = .array(messages.map { m in
                var row = m.extra
                row["name"] = .string(m.name)
                row["content"] = .string(m.content)
                return .object(row)
            })
        }
        return o.isEmpty ? nil : .object(o)
    }
}

/// `grpc`: `{service, method, message, protos, importPaths, plaintext}`.
struct GrpcFields: Hashable, Sendable {
    var service = ""
    var method = ""
    var message = ""
    var protos: [String] = []
    var importPaths: [String] = []
    var plaintext = false
    var extra: [String: JSONAny] = [:]

    init(service: String = "", method: String = "", message: String = "", plaintext: Bool = false) {
        self.service = service
        self.method = method
        self.message = message
        self.plaintext = plaintext
    }

    private static let known: Set<String> = ["service", "method", "message", "protos", "importPaths", "plaintext"]

    init(json: JSONAny?) {
        guard case let .object(o)? = json else { return }
        service = o["service"]?.string ?? ""
        method = o["method"]?.string ?? ""
        message = o["message"]?.string ?? ""
        protos = o["protos"]?.array?.compactMap(\.string) ?? []
        importPaths = o["importPaths"]?.array?.compactMap(\.string) ?? []
        plaintext = o["plaintext"]?.bool ?? false
        extra = o.filter { !Self.known.contains($0.key) }
    }

    /// As saved (empty fields left out, as hitop-core and the web app write it); nil when empty.
    var json: JSONAny? {
        var o = extra
        if !service.isEmpty { o["service"] = .string(service) }
        if !method.isEmpty { o["method"] = .string(method) }
        if !message.isEmpty { o["message"] = .string(message) }
        if !protos.isEmpty { o["protos"] = .array(protos.map(JSONAny.string)) }
        if !importPaths.isEmpty { o["importPaths"] = .array(importPaths.map(JSONAny.string)) }
        if plaintext { o["plaintext"] = .bool(true) }
        return o.isEmpty ? nil : .object(o)
    }
}

extension SavedRequest {
    static let websocketKind = "websocket"
    static let grpcKind = "grpc"

    /// `websocket`, `grpc`, or nil for HTTP (the key is left out, as the web app saves it).
    var kind: String? {
        get {
            if case let .string(k)? = extras["kind"] { return k }
            return nil
        }
        set {
            if let newValue, newValue == Self.websocketKind || newValue == Self.grpcKind { extras["kind"] = .string(newValue) } else { extras["kind"] = nil }
        }
    }

    var isWebSocket: Bool { kind == Self.websocketKind }
    var isGrpc: Bool { kind == Self.grpcKind }

    /// The subprotocols and saved messages (kept when the request switches to HTTP, as the web app keeps them).
    var websocket: WebSocketFields {
        get { WebSocketFields(json: extras["websocket"]) }
        set { extras["websocket"] = newValue.json }
    }

    var grpc: GrpcFields {
        get { GrpcFields(json: extras["grpc"]) }
        set { extras["grpc"] = newValue.json }
    }

    /// A request that asks for an event stream (`Accept: text/event-stream`, enabled): the web app's
    /// `asksForEvents`.
    var asksForEvents: Bool { SSE.asksForEvents(headers) }

    /// What the method menu shows and picks (the web app's `methodSelect`).
    enum Mode: Hashable, Sendable {
        case http(String)
        case sse
        case websocket
        case grpc
    }

    var mode: Mode {
        if isWebSocket { return .websocket }
        if isGrpc { return .grpc }
        if method == "GET" && asksForEvents { return .sse }
        return .http(method)
    }

    /// The web app's rules: WebSocket/gRPC set `kind` (the rest is kept); SSE = GET + `Accept:
    /// text/event-stream` (any other Accept row replaced); SSE → GET takes that header out again
    /// (another method keeps it: POST streams exist).
    mutating func setMode(_ m: Mode) {
        let wasSSE = mode == .sse
        switch m {
        case .websocket: kind = Self.websocketKind
        case .grpc: kind = Self.grpcKind
        case .sse:
            kind = nil
            method = "GET"
            headers = headers.filter { !($0.name.isEmpty && $0.value.isEmpty) && $0.name.lowercased() != "accept" } + [KeyValue("Accept", "text/event-stream")]
        case let .http(verb):
            kind = nil
            method = verb
            if wasSSE && verb == "GET" {
                headers.removeAll { $0.name.lowercased() == "accept" && $0.value.lowercased().contains("text/event-stream") }
            }
        }
    }
}

