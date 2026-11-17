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
        }
    }

    static func kitRequest(_ r: SavedRequest) -> HitopKit.Request {
        let form: String? = r.bodyMode == "urlencoded" || r.bodyMode == "multipart" ? r.bodyMode : nil
        let gql = r.bodyType == "graphql" ? GraphQL(variables: r.graphqlVariables, operationName: r.graphqlOperationName) : nil
        return HitopKit.Request(id: r.id, name: r.name, url: r.url, method: r.method,
                       headers: r.headers.map { HitopKit.Header($0.name, $0.value, enabled: $0.enabled) },
                       body: r.body, preRequestScript: r.preRequestScript, postRequestScript: r.postRequestScript,
                       auth: kitAuth(r.auth), bodyMode: form,
                       formFields: r.formFields.map { FormField(key: $0.key, value: $0.value, type: $0.type, enabled: $0.enabled) },
                       disabledParams: r.disabledParams.map { Param(key: $0.key, value: $0.value) },
                       bodyType: r.bodyType == "graphql" ? "graphql" : nil, graphql: gql, kind: r.kind,
                       path: r.file ?? "request")
    }

    func send(_ request: SavedRequest, collection: SavedCollection?, context: VariableContext) async -> SendResult {
        if let kind = request.kind, kind == "websocket" || kind == "grpc" {
            let what = kind == "grpc" ? "gRPC" : "WebSocket"
            return SendResult(outcome: .failure(SendFailure(message: "\(what) requests open in a later version of HITOP for iPhone and iPad. Send them from the web app, the desktop app or the CLI for now.")),
                              variableUpdates: [:], globalUpdates: [:])
        }
        let client = MeteredClient(configuration: configuration())
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
        let options = RunOptions(timeout: timeout, overrides: context.scriptSet, globals: globals, globalSecrets: globalSecrets)
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
                                  tests: tests, cookies: raw?.cookies ?? [], notes: step.notes, sentURL: step.url)
        return SendResult(outcome: .success(outcome), variableUpdates: step.variableUpdates, globalUpdates: step.globalUpdates)
    }
}

/// What the client saw of the last exchange, beyond HitopKit's `HTTPResponse`.
struct RawExchange: Sendable {
    var headers: [(String, String)]
    var body: Data
    var timing: TimingPhases?
    var cookies: [CookieRow]
}

/// A HitopKit `HTTPClient` on a given URLSession configuration that also keeps the raw body,
/// headers in order, cookies and timing.
final class MeteredClient: HTTPClient, @unchecked Sendable {
    let session: URLSession
    private let lock = NSLock()
    private var _last: RawExchange?

    var last: RawExchange? { lock.withLock { _last } }

    init(configuration: URLSessionConfiguration) {
        session = URLSession(configuration: configuration)
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
        if let timeout { req.timeoutInterval = timeout }
        let start = Date()
        let data: Data
        let response: URLResponse
        var timing: TimingPhases?
        do {
            #if canImport(Darwin)
            let metrics = MetricsDelegate()
            (data, response) = try await session.data(for: req, delegate: metrics)
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
        let exchange = RawExchange(headers: ordered, body: data, timing: timing, cookies: cookies)
        lock.withLock { _last = exchange }
        return HTTPResponse(status: http.statusCode, headers: lower, body: String(decoding: data, as: UTF8.self), durationMs: ms)
    }
}

#if canImport(Darwin)
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
