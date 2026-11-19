import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import Observation
import HitopKit

// I4 WebSocket: a request with `kind: websocket` connects instead of sending (spec §3
// "WebSocket"). The URL, params, headers and auth are the request's own and go with the opening
// handshake; `${variables}` are filled in at connect and send time (hitop-core websocket.rs
// `target()`/`message()`, the web app's `wsConnect`/`wsSend`). The log is memory only.

/// What a connection opens with, everything filled in.
struct WebSocketTarget: Equatable, Sendable {
    var url: String
    var headers: [[String]]
    var subprotocols: [String]
}

enum WebSocketTargets {
    struct NotAWebSocketURL: Error, LocalizedError, Equatable {
        var url: String
        var errorDescription: String? { "'\(url)' is not a WebSocket URL: use ws:// or wss://" }
    }

    /// ws:// and wss:// as given; http(s):// read as ws(s)://, as hitop-core does.
    static func wsURL(_ url: String) throws -> String {
        let t = url.trimmingCharacters(in: .whitespaces)
        guard let r = t.range(of: "://") else { throw NotAWebSocketURL(url: url) }
        let scheme = t[..<r.lowerBound].lowercased()
        let mapped = ["ws": "ws", "wss": "wss", "http": "ws", "https": "wss"][scheme]
        guard let mapped else { throw NotAWebSocketURL(url: url) }
        return mapped + t[r.lowerBound...]
    }

    /// The request's URL, enabled headers, auth (the collection's when it has none) and
    /// subprotocols with `variables` filled in; a missing variable is an error naming it.
    static func make(_ r: SavedRequest, collectionAuth: AuthSpec?, variables: [String: String]) throws -> WebSocketTarget {
        let fill = { (s: String) throws -> String in try Interpolation.interpolate(s, variables) }
        let url = try fill(r.url.trimmingCharacters(in: .whitespaces))
        let headers = try r.headers.filter { $0.enabled && !$0.name.isEmpty }.map { (try fill($0.name), try fill($0.value)) }
        let (authedURL, authed) = try AuthApply.apply(Sender.kitAuth(r.auth ?? collectionAuth), url: url, headers: headers, fill)
        let subs = try r.websocket.subprotocols.map(fill).filter { !$0.isEmpty }
        return WebSocketTarget(url: try wsURL(authedURL), headers: authed.map { [$0.0, $0.1] }, subprotocols: subs)
    }
}

struct SocketError: Error, LocalizedError, Sendable {
    var errorDescription: String?
}

/// One line of the message log.
struct WSLogEntry: Identifiable, Hashable, Sendable {
    enum Kind: String, Sendable { case system, sent, received, error }
    var id: Int
    var time: Date
    var kind: Kind
    var text: String?
    var bytes: Data?

    /// Text as it is; binary as its size and base64 (the first 1 KiB).
    var display: String {
        if let text { return text }
        guard let bytes else { return "" }
        let shown = bytes.prefix(1024).base64EncodedString()
        return "Binary, \(ByteText.text(bytes.count)) · base64 \(shown)\(bytes.count > 1024 ? "…" : "")"
    }
}

// MARK: - Transport

/// What a connection reports, in order.
enum WebSocketEvent: Sendable {
    case opened(protocol: String?)
    case text(String)
    case binary(Data)
    case failed(String)
    case closed(code: Int?, reason: String?)
}

protocol WebSocketConnection: AnyObject, Sendable {
    func send(text: String) async throws
    func close()
}

/// Opens connections: URLSessionWebSocketTask in the app, an in-process echo in UI tests.
protocol WebSocketTransport: Sendable {
    func connect(_ target: WebSocketTarget, timeout: TimeInterval, events: AsyncStream<WebSocketEvent>.Continuation) -> WebSocketConnection
}

#if canImport(Darwin)
/// URLSessionWebSocketTask: handshake headers and subprotocols on the opening request.
struct URLSessionWebSocketTransport: WebSocketTransport {
    func connect(_ target: WebSocketTarget, timeout: TimeInterval, events: AsyncStream<WebSocketEvent>.Continuation) -> WebSocketConnection {
        let socket = URLSessionSocket(events: events)
        socket.start(target, timeout: timeout)
        return socket
    }
}

final class URLSessionSocket: NSObject, URLSessionWebSocketDelegate, WebSocketConnection, @unchecked Sendable {
    private let events: AsyncStream<WebSocketEvent>.Continuation
    private let lock = NSLock()
    private var session: URLSession?
    private var task: URLSessionWebSocketTask?
    private var opened = false
    private var ended = false

    init(events: AsyncStream<WebSocketEvent>.Continuation) {
        self.events = events
    }

    func start(_ target: WebSocketTarget, timeout: TimeInterval) {
        guard let url = URL(string: target.url), url.host != nil else {
            end(.failed("'\(target.url)' is not a valid URL"), .closed(code: nil, reason: nil))
            return
        }
        var req = URLRequest(url: url)
        req.timeoutInterval = timeout
        for h in target.headers where h.count == 2 && MeteredClient.validHeader(h[0], h[1]) { req.setValue(h[1], forHTTPHeaderField: h[0]) }
        if !target.subprotocols.isEmpty { req.setValue(target.subprotocols.joined(separator: ", "), forHTTPHeaderField: "Sec-WebSocket-Protocol") }
        let config = URLSessionConfiguration.ephemeral
        let session = URLSession(configuration: config, delegate: self, delegateQueue: nil)
        let task = session.webSocketTask(with: req)
        lock.withLock {
            self.session = session
            self.task = task
        }
        task.resume()
        receive(task)
    }

    private func receive(_ task: URLSessionWebSocketTask) {
        task.receive { [weak self] result in
            guard let self else { return }
            switch result {
            case let .success(.string(s)):
                self.events.yield(.text(s))
                self.receive(task)
            case let .success(.data(d)):
                self.events.yield(.binary(d))
                self.receive(task)
            case .success:
                self.receive(task)
            case let .failure(e):
                let wasOpen = self.lock.withLock { self.opened }
                let code = task.closeCode == .invalid ? nil : task.closeCode.rawValue
                let reason = task.closeReason.map { String(decoding: $0, as: UTF8.self) }
                if code != nil {
                    self.end(.closed(code: code, reason: reason))
                } else {
                    self.end(.failed(wasOpen ? e.localizedDescription : "Couldn't connect: \(e.localizedDescription)"), .closed(code: nil, reason: nil))
                }
            }
        }
    }

    /// The last events of the connection (once).
    private func end(_ last: WebSocketEvent...) {
        let first = lock.withLock { () -> Bool in
            if ended { return false }
            ended = true
            return true
        }
        guard first else { return }
        for e in last { events.yield(e) }
        events.finish()
        lock.withLock { session }?.invalidateAndCancel()
    }

    func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask, didOpenWithProtocol protocol: String?) {
        lock.withLock { opened = true }
        events.yield(.opened(protocol: `protocol`))
    }

    func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask, didCloseWith closeCode: URLSessionWebSocketTask.CloseCode, reason: Data?) {
        end(.closed(code: closeCode.rawValue, reason: reason.map { String(decoding: $0, as: UTF8.self) }))
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        if let error {
            let wasOpen = lock.withLock { opened }
            end(.failed(wasOpen ? error.localizedDescription : "Couldn't connect: \(error.localizedDescription)"), .closed(code: nil, reason: nil))
        } else {
            end(.closed(code: nil, reason: nil))
        }
    }

    func send(text: String) async throws {
        guard let task = lock.withLock({ task }) else { throw SocketError(errorDescription: "not connected") }
        try await task.send(.string(text))
    }

    func close() {
        let task = lock.withLock { self.task }
        task?.cancel(with: .normalClosure, reason: nil)
        end(.closed(code: 1000, reason: nil))
    }
}
#endif

/// An in-process echo server (UI tests and screenshots, `-UITestSeed`): the CLI test server's
/// rules (tests/support/ws_echo.rs): echoes text, answers `bin` with bytes 00 ff, closes with 4001
/// "bye" on `close`, stays silent on `quiet`, picks the first subprotocol offered.
struct EchoWebSocketTransport: WebSocketTransport {
    final class Socket: WebSocketConnection, @unchecked Sendable {
        let events: AsyncStream<WebSocketEvent>.Continuation
        private let lock = NSLock()
        private var open = true
        init(events: AsyncStream<WebSocketEvent>.Continuation) { self.events = events }

        func send(text: String) async throws {
            guard lock.withLock({ open }) else { throw SocketError(errorDescription: "the connection is closed") }
            switch text {
            case "close":
                lock.withLock { open = false }
                events.yield(.closed(code: 4001, reason: "bye"))
                events.finish()
            case "quiet": break
            case "bin": events.yield(.binary(Data([0x00, 0xFF])))
            default: events.yield(.text(text))
            }
        }

        func close() {
            let was = lock.withLock { () -> Bool in defer { open = false }; return open }
            if was {
                events.yield(.closed(code: 1000, reason: nil))
                events.finish()
            }
        }
    }

    func connect(_ target: WebSocketTarget, timeout: TimeInterval, events: AsyncStream<WebSocketEvent>.Continuation) -> WebSocketConnection {
        let socket = Socket(events: events)
        if target.url.contains("unreachable") {
            events.yield(.failed("Couldn't connect: Could not connect to the server."))
            events.yield(.closed(code: nil, reason: nil))
            events.finish()
            return socket
        }
        events.yield(.opened(protocol: target.subprotocols.first))
        if target.url.contains("/prices") { events.yield(.text(#"{"type":"hello","feed":"prices","symbols":2}"#)) }
        return socket
    }
}

/// No WebSocket support on this platform (Linux has no URLSessionWebSocketTask the app can count on).
struct UnavailableWebSocketTransport: WebSocketTransport {
    func connect(_ target: WebSocketTarget, timeout: TimeInterval, events: AsyncStream<WebSocketEvent>.Continuation) -> WebSocketConnection {
        events.yield(.failed("WebSocket connections aren't available here."))
        events.yield(.closed(code: nil, reason: nil))
        events.finish()
        return EchoWebSocketTransport.Socket(events: events)
    }
}

enum DefaultTransports {
    static var webSocket: WebSocketTransport {
        #if canImport(Darwin)
        URLSessionWebSocketTransport()
        #else
        UnavailableWebSocketTransport()
        #endif
    }
}

// MARK: - Session

/// One request's connection and its log (the web app's `store.sockets` entry): status, the
/// subprotocol the server picked, the timestamped log (500 lines), never saved.
@MainActor
@Observable
final class SocketSession {
    enum Status: String, Sendable { case closed, connecting, open }

    static let logLimit = 500

    private(set) var status: Status = .closed
    private(set) var negotiated: String?
    private(set) var log: [WSLogEntry] = []
    /// The message being typed.
    var draft = ""

    @ObservationIgnored private var connection: WebSocketConnection?
    @ObservationIgnored private var token = 0
    @ObservationIgnored private var nextID = 0
    @ObservationIgnored private var mask: @Sendable (String) -> String = { $0 }

    init() {}

    func note(_ kind: WSLogEntry.Kind, _ text: String? = nil, bytes: Data? = nil) {
        log.append(WSLogEntry(id: nextID, time: Date(), kind: kind, text: text.map(mask), bytes: bytes))
        nextID += 1
        if log.count > Self.logLimit { log.removeFirst(log.count - Self.logLimit) }
    }

    /// Opens `target`; `secrets` are masked in the log (a key in the URL, a token sent).
    func connect(_ target: WebSocketTarget, transport: WebSocketTransport, secrets: [String] = [], timeout: TimeInterval = 30) {
        guard status == .closed else { return }
        let values = secrets.filter { !$0.isEmpty }.sorted { $0.count > $1.count }
        mask = { text in values.reduce(text) { $0.replacingOccurrences(of: $1, with: SecretNames.mask) } }
        token += 1
        let mine = token
        status = .connecting
        negotiated = nil
        note(.system, "Connecting to \(target.url)\(target.subprotocols.isEmpty ? "" : " (subprotocols \(target.subprotocols.joined(separator: ", ")))")")
        let (stream, continuation) = AsyncStream<WebSocketEvent>.makeStream()
        connection = transport.connect(target, timeout: timeout, events: continuation)
        Task { [weak self] in
            for await e in stream {
                guard let self, self.token == mine else { return }
                self.apply(e)
            }
            if let self, self.token == mine, self.status != .closed {
                self.status = .closed
                self.connection = nil
            }
        }
    }

    private func apply(_ e: WebSocketEvent) {
        switch e {
        case let .opened(p):
            status = .open
            negotiated = p.flatMap { $0.isEmpty ? nil : $0 }
            note(.system, "Connected\(negotiated.map { ", subprotocol \($0)" } ?? "")")
        case let .text(t): note(.received, t)
        case let .binary(d): note(.received, bytes: d)
        case let .failed(m): note(.error, m)
        case let .closed(code, reason):
            let was = status
            status = .closed
            connection = nil
            if was == .open || code != nil {
                note(.system, "Closed\(code.map { " \($0)" } ?? "")\(reason.flatMap { $0.isEmpty ? nil : " \($0)" } ?? "")")
            }
        }
    }

    /// Sends `text` (variables already filled in); logged as sent. False when not connected.
    @discardableResult
    func send(_ text: String) async -> Bool {
        guard status == .open, let connection else {
            note(.error, "Not sent: connect first.")
            return false
        }
        do {
            try await connection.send(text: text)
            note(.sent, text)
            return true
        } catch {
            note(.error, "Not sent: \(error.localizedDescription)")
            return false
        }
    }

    func disconnect() {
        switch status {
        case .closed: return
        case .connecting:
            // Cancel while connecting: closed now; that connection's late events are ignored.
            token += 1
            connection?.close()
            connection = nil
            status = .closed
            note(.system, "Cancelled")
        case .open:
            connection?.close()
        }
    }

    func clear() { log = [] }
}
