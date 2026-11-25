import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HitopKit

/// The phases of one exchange, from URLSessionTaskMetrics (ms). Each is nil when the metrics
/// don't have it (a reused connection has no DNS, connect or TLS).
struct TimingPhases: Hashable, Sendable, Codable {
    var dns: Double?
    var connect: Double?
    var tls: Double?
    var waiting: Double?
    var download: Double?

    var total: Double { [dns, connect, tls, waiting, download].compactMap { $0 }.reduce(0, +) }
}

struct CookieRow: Hashable, Sendable, Identifiable {
    var id: String { "\(domain)|\(path)|\(name)" }
    var name: String
    var value: String
    var domain: String
    var path: String
    var expires: Date?
    var secure: Bool
    var httpOnly: Bool
}

/// What one Send gives back.
struct SendOutcome: Sendable {
    var status: Int
    /// In order, names as received.
    var headers: [[String]]
    var body: Data
    /// The body as text (secret values masked).
    var text: String
    var durationMs: Int
    var timing: TimingPhases?
    var tests: [TestOutcome]
    var cookies: [CookieRow]
    /// Notes from the runner (a script not run...).
    var notes: [String]
    var sentURL: String
    /// Server-Sent Events: the response was an event stream (read live from the editor's Send, or
    /// from the whole body); nil otherwise.
    var events: [SSEEvent]? = nil
    /// The stream was stopped with Stop (the body is what came until then).
    var stopped = false
    var contentType: String { headers.first { $0.first?.lowercased() == "content-type" }?.last ?? "" }
    var size: Int { body.count }
    var isImage: Bool { contentType.lowercased().hasPrefix("image/") }
}

/// A failed Send: nothing came back.
struct SendFailure: Error, Sendable, Equatable {
    var message: String
    var explained: ExplainedError { ExplainedError.explain(message) }
}

/// One Send's result plus the script-set values to keep.
struct SendResult: Sendable {
    var outcome: Result<SendOutcome, SendFailure>
    var variableUpdates: [String: String?]
    var globalUpdates: [String: String?]
}

/// Sends a request through HitopKit's runner (variables, auth, GraphQL and form bodies, send
/// rules, pre- and post-request scripts in JavaScriptCore with pm-shim.js) on URLSession.
struct Sender: Sendable {
    /// The session configuration each Send uses (a fresh ephemeral one by default: its own
    /// cookies). UI tests put `StubURLProtocol` into it.
    var configuration: @Sendable () -> URLSessionConfiguration
    var timeout: TimeInterval = 60
    /// Where a person signs in for OAuth 2.0 (the editor's Send only).
    var browser: OAuthBrowser?
    /// The files picked for multipart rows.
    var uploads: Uploads?

    init(configuration: @escaping @Sendable () -> URLSessionConfiguration = Sender.defaultConfiguration) {
        self.configuration = configuration
    }

    static let defaultConfiguration: @Sendable () -> URLSessionConfiguration = {
        let config = URLSessionConfiguration.ephemeral
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.urlCache = nil
        config.httpShouldSetCookies = true
        config.httpCookieAcceptPolicy = .always
        return config
    }

    static func kitAuth(_ a: AuthSpec?) -> Auth? {
        switch a {
        case nil: nil
        case .none?: Auth.none
        case let .bearer(t)?: .bearer(token: t)
        case let .basic(u, p)?: .basic(username: u, password: p)
        case let .apiKey(k, v, q)?: .apiKey(key: k, value: v, location: q ? .query : .header)
        // OAuth 2, AWS Signature, Digest: `ExtraAuth`.
        case .other?: Auth.none
        }
    }

    static func kitRequest(_ r: SavedRequest) -> HitopKit.Request {
        let form: String? = r.bodyMode == "urlencoded" || r.bodyMode == "multipart" ? r.bodyMode : nil
        let gql = r.bodyType == "graphql" ? GraphQL(variables: r.graphqlVariables, operationName: r.graphqlOperationName) : nil
        return HitopKit.Request(id: r.id, name: r.name, url: r.url, method: r.method,
                       headers: r.headers.map { HitopKit.Header($0.name, $0.value, enabled: $0.enabled) },
                       body: r.body, preRequestScript: r.preRequestScript, postRequestScript: r.postRequestScript,
                       auth: kitAuth(r.auth), bodyMode: form,
                       formFields: r.formFields.map { FormField(key: $0.key, value: $0.value, type: $0.type, enabled: $0.enabled, fileName: $0.fileName, fileToken: $0.fileToken) },
                       disabledParams: r.disabledParams.map { Param(key: $0.key, value: $0.value) },
                       bodyType: r.bodyType == "graphql" ? "graphql" : nil, graphql: gql, kind: r.kind,
                       path: r.file ?? "request")
    }

    /// Sends an HTTP request. `stream` (the editor's Send only): an answer with Content-Type
    /// text/event-stream is read live, its events handed over as they arrive, until it ends or
    /// Stop. WebSocket and gRPC requests aren't sent here (the editor connects or calls them;
    /// intents and runs leave them out).
    func send(_ request: SavedRequest, collection: SavedCollection?, context: VariableContext, stream: StreamControl? = nil) async -> SendResult {
        if let kind = request.kind, kind == "websocket" || kind == "grpc" {
            let what = kind == "grpc" ? "A gRPC request is called" : "A WebSocket request is connected"
            return SendResult(outcome: .failure(SendFailure(message: "\(what) from its request screen in HITOP, not sent from here.")),
                              variableUpdates: [:], globalUpdates: [:])
        }
        let client = MeteredClient(configuration: configuration(), stream: stream)
        let kit = Self.kitRequest(request)
        // Collection, environment, script-set values (as run values), secrets: the CLI's order.
        let kitCollection = HitopKit.Collection(id: collection?.id ?? 0, name: collection?.name ?? "", requests: [kit],
                                       variables: context.collection, auth: Self.kitAuth(collection?.auth))
        let env = HitopKit.Environment(id: "app", name: context.environmentName ?? "", variables: context.environment)
        var secrets = Secrets()
        for (k, v) in context.environmentSecrets where !v.isEmpty { secrets.insert(k, v, origin: .keychain) }
        var globals = context.globals
        var globalSecrets = Secrets()
        for (k, v) in context.globalSecrets where !v.isEmpty {
            globals[k] = v
            globalSecrets.insert(k, v, origin: .keychain)
        }
        var options = RunOptions(timeout: timeout, overrides: context.scriptSet, globals: globals, globalSecrets: globalSecrets)
        ExtraAuth([request], collectionAuth: collection?.auth, browser: stream == nil ? nil : browser).install(&options)
        options.file = Uploads.source(uploads)
        let runner = Runner(engine: defaultScriptEngine, makeClient: { client })
        let steps = await runner.run(kitCollection, environment: env, secrets: secrets, options: options)
        guard let step = steps.first else {
            return SendResult(outcome: .failure(SendFailure(message: "Nothing was sent.")), variableUpdates: [:], globalUpdates: [:])
        }
        let tests = step.tests.map { TestOutcome(name: $0.name, pass: $0.pass, detail: $0.detail) }
        guard let response = step.response, step.error == nil else {
            return SendResult(outcome: .failure(SendFailure(message: step.error ?? "No response.")), variableUpdates: step.variableUpdates, globalUpdates: step.globalUpdates)
        }
        let raw = client.last
        let masker = secrets.masker(with: globalSecrets)
        let outcome = SendOutcome(status: response.status,
                                  headers: raw?.headers.map { [$0.0, masker.mask($0.1)] } ?? response.headers.map { [$0.key, $0.value] },
                                  body: raw?.body ?? Data(response.body.utf8),
                                  text: response.body, durationMs: response.durationMs, timing: raw?.timing,
                                  tests: tests, cookies: raw?.cookies ?? [], notes: step.notes, sentURL: step.url,
                                  events: raw?.events, stopped: raw?.stopped ?? false)
        return SendResult(outcome: .success(outcome), variableUpdates: step.variableUpdates, globalUpdates: step.globalUpdates)
    }
}

/// What the client saw of the last exchange, beyond HitopKit's `HTTPResponse`.
struct RawExchange: Sendable {
    var headers: [(String, String)]
    var body: Data
    var timing: TimingPhases?
    var cookies: [CookieRow]
    /// The events of an event stream (read live or from the whole body).
    var events: [SSEEvent]?
    var stopped = false
}

/// A HitopKit `HTTPClient` on a given URLSession configuration that also keeps the raw body,
/// headers in order, cookies and timing.
final class MeteredClient: HTTPClient, @unchecked Sendable {
    let session: URLSession
    private let lock = NSLock()
    private var _last: RawExchange?

    var last: RawExchange? { lock.withLock { _last } }

    /// The editor's hold on a live event stream (nil: runs, intents).
    let stream: StreamControl?

    init(configuration: URLSessionConfiguration, stream: StreamControl? = nil) {
        session = URLSession(configuration: configuration)
        self.stream = stream
    }

    deinit { session.finishTasksAndInvalidate() }

    static func validHeader(_ name: String, _ value: String) -> Bool {
        let tchar = Set("!#$%&'*+-.^_`|~0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".unicodeScalars)
        return !name.isEmpty && name.unicodeScalars.allSatisfy { tchar.contains($0) } && !value.contains("\r") && !value.contains("\n") && !value.contains("\0")
    }

    func send(_ r: OutgoingRequest, timeout: TimeInterval?) async throws -> HTTPResponse {
        guard let url = URL(string: r.url), let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https", url.host != nil else {
            throw HTTPError(description: "builder error: invalid URL '\(r.url)'")
        }
        var req = URLRequest(url: url)
        let method = r.method.uppercased()
        req.httpMethod = !method.isEmpty && Self.validHeader(method, "") ? method : "GET"
        for (k, v) in r.headers where Self.validHeader(k, v) { req.setValue(v, forHTTPHeaderField: k) }
        if !r.body.isEmpty { req.httpBody = r.body }
        let asksForEvents = r.headers.contains { $0.0.lowercased() == "accept" && SSE.isEventStream(contentType: $0.1) }
        if let timeout { req.timeoutInterval = asksForEvents ? max(timeout, 3600) : timeout }
        let start = Date()
        let data: Data
        let response: URLResponse
        var timing: TimingPhases?
        var events: [SSEEvent]?
        var stopped = false
        do {
            #if canImport(Darwin)
            let metrics = MetricsDelegate()
            if stream != nil || asksForEvents {
                // Read as it arrives: an event stream may never end. Without the editor's Stop (a
                // run, an intent) it ends after the time limit.
                (data, response, events, stopped) = try await streamed(req, metrics: metrics, limit: stream == nil ? (timeout ?? 60) : nil)
            } else {
                (data, response) = try await session.data(for: req, delegate: metrics)
            }
            timing = metrics.phases
            #else
            (data, response) = try await session.data(for: req)
            #endif
        } catch {
            throw HTTPError(description: "error sending request for url (\(r.url)): \(error.localizedDescription)")
        }
        let ms = Int(Date().timeIntervalSince(start) * 1000)
        guard let http = response as? HTTPURLResponse else { throw HTTPError(description: "no HTTP response from \(r.url)") }
        var ordered: [(String, String)] = []
        var lower: [String: String] = [:]
        for (k, v) in http.allHeaderFields {
            let name = String(describing: k)
            let value = String(describing: v)
            ordered.append((name, value))
            lower[name.lowercased()] = value
        }
        ordered.sort { $0.0.lowercased() < $1.0.lowercased() }
        if timing == nil || timing?.total == 0, let stub = StubURLProtocol.timing(for: url) { timing = stub }
        var fields: [String: String] = [:]
        for (k, v) in ordered { fields[k] = v }
        let cookies = HTTPCookie.cookies(withResponseHeaderFields: fields, for: url).map {
            CookieRow(name: $0.name, value: $0.value, domain: $0.domain, path: $0.path, expires: $0.expiresDate, secure: $0.isSecure, httpOnly: $0.isHTTPOnly)
        }
        // UX2 (8): an event stream that came back whole: its events from the body.
        if events == nil, SSE.isEventStream(contentType: lower["content-type"] ?? "") { events = SSE.events(inBody: data) }
        let exchange = RawExchange(headers: ordered, body: data, timing: timing, cookies: cookies, events: events, stopped: stopped)
        lock.withLock { _last = exchange }
        return HTTPResponse(status: http.statusCode, headers: lower, body: String(decoding: data, as: UTF8.self), durationMs: ms)
    }
}

#if canImport(Darwin)
extension MeteredClient {
    /// The body read as it arrives. An event stream (Content-Type text/event-stream) goes to the
    /// editor's `stream` event by event and ends at its end, at Stop, or after `limit` seconds.
    func streamed(_ req: URLRequest, metrics: MetricsDelegate, limit: TimeInterval?) async throws -> (Data, URLResponse, [SSEEvent]?, Bool) {
        let (bytes, response) = try await session.bytes(for: req, delegate: metrics)
        let type = (response as? HTTPURLResponse)?.value(forHTTPHeaderField: "Content-Type") ?? ""
        let isStream = SSE.isEventStream(contentType: type)
        var data = Data()
        var chunk = Data()
        var reader = SSE.Reader()
        var events: [SSEEvent] = []
        var stopped = false
        let deadline = limit.map { Date().addingTimeInterval($0) }
        if isStream, let stream, let http = response as? HTTPURLResponse {
            let headers = http.allHeaderFields.map { [String(describing: $0.key), String(describing: $0.value)] }
            stream.onOpen(http.statusCode, headers)
        }
        func flush() {
            guard !chunk.isEmpty else { return }
            if let stream { stream.feed(chunk) } else { events += reader.feed(chunk) }
            chunk.removeAll(keepingCapacity: true)
        }
        // Stop and the time limit are also checked between bytes by a watcher: a quiet stream
        // sends none.
        let dataTask = bytes.task
        let watcher = Task { [stream] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(150))
                if stream?.isStopped == true || deadline.map({ Date() > $0 }) == true {
                    dataTask.cancel()
                    return
                }
            }
        }
        defer { watcher.cancel() }
        do {
            for try await byte in bytes {
                data.append(byte)
                guard isStream else { continue }
                chunk.append(byte)
                if byte == 0x0A { flush() }
                if stream?.isStopped == true || deadline.map({ Date() > $0 }) == true {
                    stopped = true
                    dataTask.cancel()
                    break
                }
            }
        } catch {
            guard stream?.isStopped == true || deadline.map({ Date() > $0 }) == true else { throw error }
            stopped = true
        }
        if isStream {
            flush()
            if let stream { return (data, response, stream.events, stopped) }
            if !stopped { events += reader.finish() }
            return (data, response, events, stopped)
        }
        return (data, response, nil, false)
    }
}

/// Collects the task's metrics (DNS, connect, TLS, waiting, download).
final class MetricsDelegate: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private var _phases: TimingPhases?
    var phases: TimingPhases? { lock.lock(); defer { lock.unlock() }; return _phases }

    func urlSession(_ session: URLSession, task: URLSessionTask, didFinishCollecting metrics: URLSessionTaskMetrics) {
        guard let t = metrics.transactionMetrics.last else { return }
        func ms(_ a: Date?, _ b: Date?) -> Double? {
            guard let a, let b, b >= a else { return nil }
            return b.timeIntervalSince(a) * 1000
        }
        let tlsStart = t.secureConnectionStartDate
        var p = TimingPhases()
        p.dns = ms(t.domainLookupStartDate, t.domainLookupEndDate)
        p.connect = ms(t.connectStartDate, tlsStart ?? t.connectEndDate)
        p.tls = ms(tlsStart, t.secureConnectionEndDate)
        p.waiting = ms(t.requestStartDate, t.responseStartDate)
        p.download = ms(t.responseStartDate, t.responseEndDate)
        lock.lock()
        _phases = p
        lock.unlock()
    }
}
#endif
