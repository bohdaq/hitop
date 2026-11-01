import Foundation

/// One request of a run.
public struct StepResult: Sendable {
    public var requestName: String
    public var method: String
    /// As sent, secrets masked.
    public var url: String
    public var response: HTTPResponse?
    /// Why the request wasn't sent or answered.
    public var error: String?
    /// The post-request script's `assert.*` results.
    public var tests: [TestResult]
    /// Scripts, request and response.
    public var durationMs: Int
    /// What the run left out (a script without an engine to run it).
    public var notes: [String]
    /// The request's path under `requests/` and the file's extension.
    public var path: String
    public var fileExtension: String

    /// Sent, answered below 400, and every assertion passed.
    public var passed: Bool {
        guard let r = response, error == nil else { return false }
        return r.status < 400 && tests.allSatisfy(\.pass)
    }

    /// Secret values replaced everywhere: URL, error, response headers and
    /// body, assertion names and details.
    mutating func mask(_ m: Masker) {
        guard !m.isEmpty else { return }
        url = m.mask(url)
        error = error.map(m.mask)
        if var r = response {
            r.body = m.mask(r.body)
            r.headers = r.headers.mapValues(m.mask)
            response = r
        }
        tests = tests.map { TestResult(name: m.mask($0.name), pass: $0.pass, detail: m.mask($0.detail)) }
    }
}

public struct RunOptions: Sendable {
    /// Stop after the first request that fails (`--bail`).
    public var stopOnFail = false
    /// Time limit for each request.
    public var timeout: TimeInterval?
    /// Values over the collection's and the environment's (a data row,
    /// `--var`); secrets still win over them.
    public var overrides: [String: String] = [:]
    /// The globals scope, its secrets already filled in.
    public var globals: [String: String] = [:]
    /// The globals' secrets, for masking.
    public var globalSecrets = Secrets()

    public init(stopOnFail: Bool = false, timeout: TimeInterval? = nil, overrides: [String: String] = [:], globals: [String: String] = [:], globalSecrets: Secrets = Secrets()) {
        self.stopOnFail = stopOnFail
        self.timeout = timeout
        self.overrides = overrides
        self.globals = globals
        self.globalSecrets = globalSecrets
    }
}

/// Runs a collection's requests in order, passing script context and
/// script-set variables from one to the next (hitop-core `runner.rs`).
/// WebSocket and gRPC requests are left out, as `hitop run` leaves them out.
public struct Runner: Sendable {
    public var engine: ScriptEngine?
    /// A new HTTP client for every run (its own cookies).
    public var makeClient: @Sendable () -> HTTPClient

    public init(engine: ScriptEngine? = defaultScriptEngine, makeClient: @escaping @Sendable () -> HTTPClient = { URLSessionHTTPClient() }) {
        self.engine = engine
        self.makeClient = makeClient
    }

    public func run(_ collection: Collection, environment: Environment?, secrets: Secrets = Secrets(), options: RunOptions = RunOptions(),
                    onStep: (StepResult) -> Void = { _ in }) async -> [StepResult] {
        let client = makeClient()
        var context: [String: JSONValue] = [:]
        var upper = Prepare.variables(collection: collection.variables, environment: environment)
        for (k, v) in options.overrides { upper[k] = v }
        secrets.apply(to: &upper)
        var scopes = Scopes(globals: options.globals, upper: upper)
        let masker = secrets.masker(with: options.globalSecrets)
        var results: [StepResult] = []

        for request in collection.requests where !request.isWebSocket && !request.isGRPC {
            let started = Date()
            var step = StepResult(requestName: request.name, method: request.method, url: request.url, response: nil, error: nil,
                                  tests: [], durationMs: 0, notes: [], path: request.path, fileExtension: request.fileExtension)
            // A request's own auth (even `none`) wins over the collection's.
            let auth = request.auth ?? collection.auth
            do {
                let (pre, skipped) = try Prepare.prepare(request, auth: auth, scopes: scopes, context: context, engine: engine)
                if skipped { step.notes.append("pre-request script not run (no JavaScript engine here)") }
                step.url = pre.url
                for (k, v) in pre.contextUpdates { context[k] = v }
                scopes.apply(pre.variables, pre.globals)
                let variables = scopes.merged
                let (headers, body) = try FormBody.outgoing(request, headers: pre.headers, body: pre.body, variables)
                for part in [pre.url] + headers.flatMap({ [$0.0, $0.1] }) + [String(decoding: body, as: UTF8.self)] {
                    if let ref = SecretManagerReference.find(part) { throw SecretManagerReference(reference: ref) }
                }
                do {
                    let response = try await client.send(OutgoingRequest(method: request.method, url: pre.url, headers: headers, body: body), timeout: options.timeout)
                    step.response = response
                    let post = request.postRequestScript.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !post.isEmpty, let engine {
                        let sent = SentRequest(url: pre.url, method: request.method, headers: headers, body: String(decoding: body, as: UTF8.self))
                        do {
                            let out = try Scripts.runPostRequest(request.postRequestScript, PostRequestInput(
                                responseBody: response.body, responseHeaders: response.headers, statusCode: response.status,
                                durationMs: response.durationMs, variables: variables, context: context, request: sent, globals: scopes.globals), engine: engine)
                            for (k, v) in out.contextUpdates { context[k] = v }
                            scopes.apply(out.variables, out.globals)
                            step.tests = out.tests
                        } catch {
                            step.tests = [TestResult(name: "post-request script", pass: false, detail: "\(error)")]
                        }
                    } else if !post.isEmpty {
                        step.notes.append("post-request script not run (no JavaScript engine here)")
                    }
                } catch {
                    step.error = "\(error)"
                }
            } catch let missing as Interpolation.MissingVariable where missing.name.hasPrefix("secret:") {
                step.url = request.url
                step.error = "\(SecretManagerReference(reference: missing.name))"
            } catch {
                step.url = request.url
                step.error = "\(error)"
            }
            step.durationMs = Int(Date().timeIntervalSince(started) * 1000)
            step.mask(masker)
            onStep(step)
            results.append(step)
            if !step.passed && options.stopOnFail { break }
        }
        return results
    }
}
