import Foundation
import HitopKit
import Observation

// I5: Apple Intelligence (the on-device Foundation Models framework) — everything except the
// framework itself, which lives in the app target (App/Platform/AppleIntelligence.swift) behind
// `LanguageAssistant`. Here: what the model is allowed to see (secrets masked, bodies cut),
// the prompts, the checks it suggests rendered into a pm.test script, a free-text request shape
// mapped onto a request, and a deterministic fake for unit and UI tests. Nothing in this file
// sends anything anywhere.

// MARK: - What the model sees

/// The text that goes to the model: secret values masked (the same `••••` HitopKit's masker
/// writes), credentials masked by header, query and JSON key name, bodies cut to fit the small
/// on-device context window.
enum ModelInput {
    /// Characters of a response body the model gets (the window is about 4k tokens, shared with
    /// the instructions, the guided-generation schema and the answer).
    static let bodyLimit = 2_400
    /// Characters of a request body.
    static let requestBodyLimit = 600
    /// Characters of one header value.
    static let headerLimit = 160
    /// Characters of one JSON string value.
    static let stringLimit = 160
    /// Items of an array kept (the rest counted).
    static let arrayItems = 3
    static let mask = SecretNames.mask

    private static let sensitiveHeaderNames: Set<String> = [
        "authorization", "proxy-authorization", "cookie", "set-cookie", "x-api-key", "api-key", "apikey", "x-auth-token",
        "x-access-token", "x-csrf-token", "x-xsrf-token", "x-amz-security-token", "x-goog-api-key", "ocp-apim-subscription-key",
    ]
    private static let sensitiveWords = ["token", "secret", "password", "passwd", "apikey", "api_key", "api-key", "auth", "cookie",
                                         "session", "credential", "private", "signature", "sig", "jwt", "bearer", "otp", "pin"]

    /// Whether a header's value is a credential (by its name).
    static func isSensitiveHeader(_ name: String) -> Bool {
        let n = name.lowercased()
        return sensitiveHeaderNames.contains(n) || sensitiveWords.contains { n.contains($0) && $0 != "pin" && $0 != "sig" && $0 != "otp" }
    }

    /// Whether a JSON key or query parameter holds a credential.
    static func isSensitiveKey(_ name: String) -> Bool {
        let n = name.lowercased().replacingOccurrences(of: "-", with: "_")
        if ["key", "sig", "pin", "otp", "pwd", "code_verifier"].contains(n) { return true }
        // "auth" alone would hide an article's author.
        if n == "auth" || n.hasPrefix("auth_") || n.hasSuffix("_auth") || n.contains("authorization") { return true }
        return sensitiveWords.filter { $0.count > 3 && $0 != "auth" }.contains { n.contains($0) }
    }

    /// Secret values (environment and global secrets) masked.
    static func maskSecrets(_ text: String, _ secrets: [String]) -> String {
        Masker(secrets).mask(text)
    }

    /// A URL with secret values and credential query values masked.
    static func url(_ url: String, secrets: [String]) -> String {
        let masked = maskSecrets(url, secrets)
        guard let q = masked.firstIndex(of: "?") else { return masked }
        let head = masked[..<q]
        var tail = String(masked[masked.index(after: q)...])
        var fragment = ""
        if let h = tail.firstIndex(of: "#") {
            fragment = String(tail[h...])
            tail = String(tail[..<h])
        }
        let parts = tail.split(separator: "&", omittingEmptySubsequences: false).map { part -> String in
            guard let eq = part.firstIndex(of: "=") else { return String(part) }
            let name = String(part[..<eq])
            return isSensitiveKey(name.removingPercentEncoding ?? name) ? "\(name)=\(mask)" : String(part)
        }
        return "\(head)?\(parts.joined(separator: "&"))\(fragment)"
    }

    /// Headers as `Name: value` lines, credentials masked, long values cut.
    static func headers(_ headers: [[String]], secrets: [String]) -> String {
        headers.compactMap { h -> String? in
            guard let name = h.first, !name.isEmpty else { return nil }
            let value = h.count > 1 ? h[1] : ""
            return "\(name): \(headerValue(name: name, value: value, secrets: secrets))"
        }.joined(separator: "\n")
    }

    static func headerValue(name: String, value: String, secrets: [String]) -> String {
        if isSensitiveHeader(name) {
            // The scheme says something ("Bearer"), the credential nothing.
            if let space = value.firstIndex(of: " "), name.lowercased().contains("authorization") { return "\(value[..<space]) \(mask)" }
            return mask
        }
        return cut(maskSecrets(value, secrets), headerLimit)
    }

    static func cut(_ s: String, _ limit: Int) -> String {
        s.count > limit ? String(s.prefix(limit)) + "…" : s
    }

    /// A JSON value with credential keys masked, long strings cut and long arrays shortened.
    static func shortened(_ v: JSONValue, depth: Int = 0) -> JSONValue {
        switch v {
        case let .string(s): return .string(cut(s, stringLimit))
        case let .array(items):
            var out = items.prefix(arrayItems).map { shortened($0, depth: depth + 1) }
            if items.count > arrayItems { out.append(.string("… \(items.count - arrayItems) more items")) }
            return .array(out)
        case let .object(pairs):
            return .object(pairs.map { k, val in
                if isSensitiveKey(k), !isContainer(val) { return (k, .string(mask)) }
                return (k, shortened(val, depth: depth + 1))
            })
        default: return v
        }
    }

    private static func isContainer(_ v: JSONValue) -> Bool {
        if case .object = v { return true }
        if case .array = v { return true }
        return false
    }

    struct Body: Equatable, Sendable {
        var text: String
        /// The JSON's shape ("object {id: string, total: object {…}}"), nil when not JSON.
        var shape: String?
        /// The body was longer than what the model gets.
        var cut: Bool
    }

    /// A body as the model gets it.
    static func body(_ text: String, secrets: [String], limit: Int = bodyLimit) -> Body {
        let masked = maskSecrets(text, secrets)
        if let json = try? JSONValue.parse(masked) {
            let short = shortened(json)
            let pretty = short.pretty
            let shortenedAny = short != json
            if pretty.count <= limit {
                return Body(text: pretty, shape: shape(json), cut: shortenedAny)
            }
            let compact = short.compact
            return Body(text: cut(compact, limit), shape: shape(json), cut: true)
        }
        return Body(text: cut(masked, limit), shape: nil, cut: masked.count > limit)
    }

    /// The structure of a JSON value, two levels deep: what the model needs to reason about a body
    /// it only sees part of.
    static func shape(_ v: JSONValue, depth: Int = 0) -> String {
        switch v {
        case .null: return "null"
        case .bool: return "boolean"
        case .number: return "number"
        case .string: return "string"
        case let .array(items):
            guard let first = items.first else { return "array (empty)" }
            return "array of \(items.count) \(depth >= 2 ? JSONPath.typeName(first) : shape(first, depth: depth + 1))"
        case let .object(pairs):
            if depth >= 3 { return "object (\(pairs.count) keys)" }
            let inner = pairs.prefix(14).map { "\($0.0): \(shape($0.1, depth: depth + 1))" }.joined(separator: ", ")
            return "object {\(inner)\(pairs.count > 14 ? ", … \(pairs.count - 14) more" : "")}"
        }
    }

    /// The request, for the prompt: method, URL as sent (masked), headers (masked), auth by kind
    /// only (never its values), body cut.
    static func request(_ r: SavedRequest, sentURL: String?, secrets: [String]) -> String {
        var lines = ["\(r.method) \(url(sentURL ?? r.url, secrets: secrets))"]
        let headers = r.headers.filter { $0.enabled && !$0.name.isEmpty }.map { [$0.name, $0.value] }
        if !headers.isEmpty { lines.append(Self.headers(headers, secrets: secrets)) }
        if let auth = r.auth, authKind(auth) != "none" { lines.append("Auth: \(authKind(auth)) (value hidden)") }
        if r.bodyKind != .none, !r.body.isEmpty {
            let b = body(r.body, secrets: secrets, limit: requestBodyLimit)
            lines.append("Body:\n\(b.text)")
        }
        return lines.joined(separator: "\n")
    }

    static func authKind(_ a: AuthSpec) -> String {
        switch a {
        case .none: "none"
        case .bearer: "bearer token"
        case .basic: "basic"
        case .apiKey: "API key"
        case let .other(type, _): type
        }
    }
}

// MARK: - The response as facts

/// What a response is, for prompts and for checking suggested tests against it.
struct ResponseFacts: Sendable, Equatable {
    var status: Int
    var headers: [[String]]
    var text: String
    var durationMs: Int
    var timing: TimingPhases?
    var tests: [TestOutcome]
    var sentURL: String

    var json: JSONValue? { try? JSONValue.parse(text) }

    func header(_ name: String) -> String? {
        headers.first { $0.first?.lowercased() == name.lowercased() }.flatMap { $0.count > 1 ? $0[1] : nil }
    }

    var contentType: String { header("content-type") ?? "" }
}

extension ResponseFacts {
    init(_ o: SendOutcome) {
        self.init(status: o.status, headers: o.headers, text: o.text, durationMs: o.durationMs, timing: o.timing, tests: o.tests, sentURL: o.sentURL)
    }
}

// MARK: - JSON paths ("links.pay", "items[0].sku")

enum JSONPath {
    enum Step: Equatable { case key(String), index(Int) }

    /// "a.b[0].c" → steps; nil when malformed. An empty path is the body itself.
    static func parse(_ path: String) -> [Step]? {
        var steps: [Step] = []
        var p = Substring(path.trimmingCharacters(in: .whitespaces))
        if p.hasPrefix("$.") { p = p.dropFirst(2) } else if p == "$" { p = "" }
        if p.hasPrefix("body.") { p = p.dropFirst(5) }
        var key = ""
        var i = p.startIndex
        while i < p.endIndex {
            let c = p[i]
            if c == "." {
                if !key.isEmpty { steps.append(.key(key)); key = "" } else if steps.isEmpty { return nil }
            } else if c == "[" {
                if !key.isEmpty { steps.append(.key(key)); key = "" }
                guard let close = p[i...].firstIndex(of: "]") else { return nil }
                let inner = p[p.index(after: i)..<close].trimmingCharacters(in: .whitespaces)
                if let n = Int(inner), n >= 0 {
                    steps.append(.index(n))
                } else if inner.count >= 2, let q = inner.first, q == "\"" || q == "'", inner.last == q {
                    steps.append(.key(String(inner.dropFirst().dropLast())))
                } else {
                    return nil
                }
                i = close
            } else {
                key.append(c)
            }
            i = p.index(after: i)
        }
        if !key.isEmpty { steps.append(.key(key)) }
        return steps
    }

    static func value(_ root: JSONValue, _ path: String) -> JSONValue? {
        guard let steps = parse(path) else { return nil }
        var v = root
        for s in steps {
            switch s {
            case let .key(k):
                guard let pairs = v.objectValue, let next = pairs.first(where: { $0.0 == k })?.1 else { return nil }
                v = next
            case let .index(n):
                guard let items = v.arrayValue, n < items.count else { return nil }
                v = items[n]
            }
        }
        return v
    }

    /// The JavaScript that reads `path` from `body` (`body.links.pay`, `body["x-id"]`, `body.items[0]`).
    static func js(_ path: String, root: String = "body") -> String? {
        guard let steps = parse(path) else { return nil }
        var out = root
        for s in steps {
            switch s {
            case let .key(k):
                let ident = k.first.map { $0.isLetter || $0 == "_" || $0 == "$" } == true && k.allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" || $0 == "$" }
                    && k.unicodeScalars.allSatisfy(\.isASCII)
                out += ident ? ".\(k)" : "[\(JSONValue.quote(k))]"
            case let .index(n): out += "[\(n)]"
            }
        }
        return out
    }

    /// pm-shim's `typeOf` names.
    static func typeName(_ v: JSONValue) -> String {
        switch v {
        case .null: "null"
        case .bool: "boolean"
        case .number: "number"
        case .string: "string"
        case .array: "array"
        case .object: "object"
        }
    }
}

// MARK: - Suggested tests

/// One check the model suggests (the shape guided generation fills in), rendered by `TestScript`
/// into a `pm.test` — the same script API as the web app, the CLI and HitopKit.
struct SuggestedCheck: Equatable, Sendable, Codable, Identifiable {
    enum Kind: String, Sendable, Codable, CaseIterable {
        /// `expected` = the status code.
        case status
        /// `path` = header name, `expected` = text the value contains ("" = it is there).
        case header
        /// `path` = JSON path, `expected` = the value as JSON ("pending_payment" or 6480 or true).
        case equals
        /// `path` = JSON path: it is there and not null.
        case exists
        /// `path` = JSON path, `expected` = string, number, boolean, array, object or null.
        case type
        /// `expected` = milliseconds the response comes under.
        case responseTime
        /// `expected` = text the body contains.
        case bodyContains
    }

    var kind: Kind
    var path: String
    var expected: String
    /// The test's name ("order waits for payment"); a plain one is made when empty.
    var name: String

    var id: String { "\(kind.rawValue)|\(path)|\(expected)" }

    init(_ kind: Kind, path: String = "", expected: String = "", name: String = "") {
        self.kind = kind
        self.path = path
        self.expected = expected
        self.name = name
    }
}

struct TestSuggestion: Equatable, Sendable {
    var checks: [SuggestedCheck]
    var script: String
    /// Checks the model offered that the response contradicts (left out).
    var dropped: Int
}

enum TestScript {
    static let maxChecks = 6

    /// The checks that hold for `facts` (a suggestion that the response it came from fails is
    /// wrong), at most `maxChecks`, duplicates out, in a stable order (status first).
    static func validated(_ checks: [SuggestedCheck], facts: ResponseFacts) -> (kept: [SuggestedCheck], dropped: Int) {
        var seen = Set<String>()
        var kept: [(Int, SuggestedCheck)] = []
        var dropped = 0
        for (i, c) in checks.enumerated() {
            guard let n = normalized(c), seen.insert(n.id).inserted else { dropped += 1; continue }
            if holds(n, facts) { kept.append((i, n)) } else { dropped += 1 }
        }
        kept.sort { (order($0.1.kind), $0.0) < (order($1.1.kind), $1.0) }
        return (kept.prefix(maxChecks).map(\.1), dropped)
    }

    private static func order(_ k: SuggestedCheck.Kind) -> Int { SuggestedCheck.Kind.allCases.firstIndex(of: k) ?? 0 }

    private static func normalized(_ c: SuggestedCheck) -> SuggestedCheck? {
        var c = c
        c.path = c.path.trimmingCharacters(in: .whitespacesAndNewlines)
        c.expected = c.expected.trimmingCharacters(in: .whitespacesAndNewlines)
        c.name = c.name.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "\n", with: " ")
        switch c.kind {
        case .status: guard Int(c.expected) != nil else { return nil }
        case .responseTime: guard let ms = Int(c.expected.filter(\.isNumber)), ms > 0 else { return nil }
            c.expected = String(ms)
        case .header: guard !c.path.isEmpty, !c.path.contains(" ") else { return nil }
        case .equals, .exists, .type: guard JSONPath.parse(c.path) != nil, !c.path.isEmpty else { return nil }
        case .bodyContains: guard !c.expected.isEmpty else { return nil }
        }
        if c.kind == .type { c.expected = c.expected.lowercased() }
        return c
    }

    static func holds(_ c: SuggestedCheck, _ f: ResponseFacts) -> Bool {
        switch c.kind {
        case .status: return Int(c.expected) == f.status
        case .header:
            guard let v = f.header(c.path) else { return false }
            return c.expected.isEmpty || v.lowercased().contains(c.expected.lowercased())
        case .responseTime:
            // Leaves room: the next send is rarely as fast, and a test that fails at random helps nobody.
            return Int(c.expected).map { $0 >= max(f.durationMs * 2, f.durationMs + 200) } ?? false
        case .bodyContains: return f.text.contains(c.expected)
        case .exists:
            guard let json = f.json, let v = JSONPath.value(json, c.path) else { return false }
            return v != .null
        case .type:
            guard let json = f.json, let v = JSONPath.value(json, c.path) else { return false }
            return JSONPath.typeName(v) == c.expected
        case .equals:
            guard let json = f.json, let v = JSONPath.value(json, c.path) else { return false }
            return v == expectedValue(c.expected, like: v)
        }
    }

    /// `expected` as a JSON value: JSON when it parses ("6480", "true", "\"x\""), else a string.
    static func expectedValue(_ s: String, like actual: JSONValue? = nil) -> JSONValue {
        if let v = try? JSONValue.parse(s) {
            // "6480" meant as the string "6480" when the body has a string there.
            if case .string = actual, case .number = v { return .string(s) }
            if case .string = actual, case .bool = v { return .string(s) }
            return v
        }
        return .string(s)
    }

    /// The pm.test script for `checks` (they read `pm.response.json()` once, as `body`).
    static func render(_ checks: [SuggestedCheck], facts: ResponseFacts? = nil) -> String {
        var lines: [String] = []
        if checks.contains(where: { [.equals, .exists, .type].contains($0.kind) }) {
            lines.append("const body = pm.response.json();")
        }
        for c in checks {
            let name = JSONValue.quote(c.name.isEmpty ? defaultName(c) : c.name)
            switch c.kind {
            case .status:
                lines.append("pm.test(\(name), () => pm.response.to.have.status(\(Int(c.expected) ?? 200)));")
            case .header:
                let get = "pm.response.headers.get(\(JSONValue.quote(c.path)))"
                lines.append(c.expected.isEmpty ? "pm.test(\(name), () => pm.response.to.have.header(\(JSONValue.quote(c.path))));"
                    : "pm.test(\(name), () => pm.expect(\(get)).to.include(\(JSONValue.quote(c.expected))));")
            case .responseTime:
                lines.append("pm.test(\(name), () => pm.expect(pm.response.responseTime).to.be.below(\(Int(c.expected) ?? 1000)));")
            case .bodyContains:
                lines.append("pm.test(\(name), () => pm.expect(pm.response.text()).to.include(\(JSONValue.quote(c.expected))));")
            case .exists:
                lines.append("pm.test(\(name), () => pm.expect(\(JSONPath.js(c.path) ?? "body")).to.exist);")
            case .type:
                lines.append("pm.test(\(name), () => pm.expect(\(JSONPath.js(c.path) ?? "body")).to.be.a(\(JSONValue.quote(c.expected))));")
            case .equals:
                let actual = facts?.json.flatMap { JSONPath.value($0, c.path) }
                lines.append("pm.test(\(name), () => pm.expect(\(JSONPath.js(c.path) ?? "body")).to.eql(\(expectedValue(c.expected, like: actual).compact)));")
            }
        }
        return lines.joined(separator: "\n")
    }

    static func defaultName(_ c: SuggestedCheck) -> String {
        switch c.kind {
        case .status: "Status is \(c.expected)"
        case .header: c.expected.isEmpty ? "Has header \(c.path)" : "\(c.path) includes \(c.expected)"
        case .responseTime: "Responds under \(c.expected) ms"
        case .bodyContains: "Body contains \(ModelInput.cut(c.expected, 40))"
        case .exists: "\(c.path) is present"
        case .type: "\(c.path) is \(c.expected == "array" || c.expected == "object" ? "an" : "a") \(c.expected)"
        case .equals: "\(c.path) is \(ModelInput.cut(c.expected, 40))"
        }
    }

    /// A suggestion without the model (the fake assistant, and what the model's answer is checked
    /// against): the status, the content type, and up to three top-level fields.
    static func heuristic(_ f: ResponseFacts) -> [SuggestedCheck] {
        var out = [SuggestedCheck(.status, expected: String(f.status))]
        if let ct = f.header("content-type"), let main = ct.split(separator: ";").first.map(String.init), !main.isEmpty {
            out.append(SuggestedCheck(.header, path: "Content-Type", expected: main.trimmingCharacters(in: .whitespaces)))
        }
        if let json = f.json, let pairs = json.objectValue {
            var picked = 0
            for (k, v) in pairs where picked < 3 && !ModelInput.isSensitiveKey(k) {
                guard JSONPath.js(k) != nil else { continue }
                switch v {
                case let .string(s) where (k.lowercased().contains("status") || k.lowercased().contains("state")) && s.count < 60:
                    out.append(SuggestedCheck(.equals, path: k, expected: JSONValue.quote(s)))
                case .null: continue
                default: out.append(SuggestedCheck(.type, path: k, expected: JSONPath.typeName(v)))
                }
                picked += 1
            }
        }
        return out
    }

    /// The suggestion for `facts` from `checks`: validated and rendered.
    static func suggestion(_ checks: [SuggestedCheck], facts: ResponseFacts) -> TestSuggestion {
        let v = validated(checks, facts: facts)
        return TestSuggestion(checks: v.kept, script: render(v.kept, facts: facts), dropped: v.dropped)
    }

    /// `script` added to a request's tests (after what is there, on a new line).
    static func appending(_ script: String, to existing: String) -> String {
        let trimmed = existing.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? script : existing.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression) + "\n\n" + script
    }
}

// MARK: - A request from plain text

/// What the model fills in for "POST a JSON order to …" (guided generation), mapped onto a request
/// here. A curl command never comes this way: `CurlImport` reads those without the model.
struct RequestDraft: Equatable, Sendable, Codable {
    struct Header: Equatable, Sendable, Codable {
        var name: String
        var value: String
    }

    var name: String
    var method: String
    var url: String
    var headers: [Header]
    /// "none", "json", "text" or "form".
    var bodyKind: String
    var body: String

    init(name: String = "", method: String = "GET", url: String = "", headers: [Header] = [], bodyKind: String = "none", body: String = "") {
        self.name = name
        self.method = method
        self.url = url
        self.headers = headers
        self.bodyKind = bodyKind
        self.body = body
    }

    static let methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]

    /// The request it makes (unknown methods become GET, a URL with spaces loses them, a JSON
    /// body that parses is formatted, header rows without a valid name are dropped).
    var savedRequest: SavedRequest {
        let m = method.uppercased().trimmingCharacters(in: .whitespaces)
        var r = SavedRequest(name: AutoName.untitled, url: url.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: " ", with: ""),
                             method: Self.methods.contains(m) ? m : "GET")
        let allowed = CharacterSet(charactersIn: "!#$%&'*+-.^_`|~").union(.alphanumerics)
        r.headers = headers.compactMap { h in
            let n = h.name.trimmingCharacters(in: .whitespaces)
            guard !n.isEmpty, n.unicodeScalars.allSatisfy({ allowed.contains($0) && $0.isASCII }) else { return nil }
            // Credentials are never made up: an Authorization header keeps only a variable.
            return KeyValue(n, h.value.trimmingCharacters(in: .whitespaces))
        }
        let text = body.trimmingCharacters(in: .whitespacesAndNewlines)
        switch bodyKind.lowercased() {
        case "json" where !text.isEmpty:
            r.body = JSONOutline.format(text) ?? text
            r.bodyType = "json"
        case "text" where !text.isEmpty, "raw" where !text.isEmpty:
            r.body = text
            r.bodyType = "text"
        case "form" where !text.isEmpty:
            r.bodyMode = "urlencoded"
            r.formFields = text.split(separator: "&").map { pair in
                let kv = pair.split(separator: "=", maxSplits: 1).map { String($0).removingPercentEncoding ?? String($0) }
                return FormRow(kv.first ?? "", kv.count > 1 ? kv[1] : "")
            }
        default:
            break
        }
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if !n.isEmpty { r.name = String(n.prefix(80)) }
        return r
    }
}

// MARK: - Prompts

/// A prompt: the session's instructions and the turn's text.
struct ModelPrompt: Equatable, Sendable {
    var instructions: String
    var text: String
}

enum Prompts {
    static func explain(request: SavedRequest, facts: ResponseFacts, secrets: [String]) -> ModelPrompt {
        let instructions = """
        You explain HTTP API responses to developers in plain language. Answer in 3 to 6 short sentences, no headings, \
        no code. Say what the status means for this request, what the body contains and what it tells about the result, \
        anything notable in the headers (caching, rate limits, content type), and, for an error, the likely cause and \
        what to try next. Values shown as •••• are hidden on purpose: never guess them. Never invent fields that are not shown.
        """
        var parts: [String] = []
        parts.append("REQUEST\n\(ModelInput.request(request, sentURL: facts.sentURL, secrets: secrets))")
        let reason = HTTPStatusText.text(facts.status)
        var timing = "\(facts.durationMs) ms"
        if let t = facts.timing {
            let phases = [("DNS", t.dns), ("connect", t.connect), ("TLS", t.tls), ("waiting for the server", t.waiting), ("download", t.download)]
                .compactMap { name, v in v.map { "\(name) \(Int($0.rounded())) ms" } }
            if !phases.isEmpty { timing += " (\(phases.joined(separator: ", ")))" }
        }
        parts.append("RESPONSE\nStatus: \(facts.status)\(reason.isEmpty ? "" : " \(reason)")\nTime: \(timing)")
        if !facts.headers.isEmpty { parts.append("HEADERS\n\(ModelInput.headers(facts.headers, secrets: secrets))") }
        let body = ModelInput.body(facts.text, secrets: secrets)
        if let shape = body.shape { parts.append("BODY SHAPE\n\(shape)") }
        parts.append(body.text.isEmpty ? "BODY\n(empty)" : "BODY\(body.cut ? " (shortened)" : "")\n\(body.text)")
        if !facts.tests.isEmpty {
            let lines = facts.tests.prefix(10).map { "\($0.pass ? "passed" : "FAILED"): \($0.name)\($0.pass || $0.detail.isEmpty ? "" : " (\($0.detail))")" }
            parts.append("TESTS\n\(lines.joined(separator: "\n"))")
        }
        return ModelPrompt(instructions: instructions, text: parts.joined(separator: "\n\n") + "\n\nExplain this response.")
    }

    static func suggestTest(request: SavedRequest, facts: ResponseFacts, secrets: [String]) -> ModelPrompt {
        let instructions = """
        You write API tests. Suggest 3 to 5 checks that this exact response passes and that would catch a regression: \
        the status, an important header, and the body fields that matter for this request (values that identify the \
        result such as a status field, types of ids and amounts). Use JSON paths like links.pay or items[0].sku. \
        Never check values shown as ••••, timestamps, or random ids by exact value: check their type instead.
        """
        var parts: [String] = []
        parts.append("REQUEST\n\(ModelInput.request(request, sentURL: facts.sentURL, secrets: secrets))")
        parts.append("RESPONSE\nStatus: \(facts.status)\nTime: \(facts.durationMs) ms")
        if !facts.headers.isEmpty { parts.append("HEADERS\n\(ModelInput.headers(facts.headers, secrets: secrets))") }
        let body = ModelInput.body(facts.text, secrets: secrets, limit: 1_800)
        if let shape = body.shape { parts.append("BODY SHAPE\n\(shape)") }
        parts.append("BODY\n\(body.text.isEmpty ? "(empty)" : body.text)")
        return ModelPrompt(instructions: instructions, text: parts.joined(separator: "\n\n"))
    }

    static func request(from text: String, variables: [String]) -> ModelPrompt {
        let instructions = """
        You turn a plain-language description into one HTTP request for an API client. Use only what the description \
        says: the method, the full URL, headers and a body. When the description names no host and a variable such as \
        baseUrl exists, start the URL with ${baseUrl}. For a JSON body write valid JSON with example values that fit \
        the description, and add a Content-Type: application/json header. Never invent tokens, passwords or keys: for \
        credentials use a variable such as ${token}. Name the request in a few words.
        """
        var t = "DESCRIPTION\n\(ModelInput.cut(text.trimmingCharacters(in: .whitespacesAndNewlines), 1_200))"
        let names = variables.filter { !$0.isEmpty }.sorted().prefix(30)
        if !names.isEmpty { t += "\n\nVARIABLES (use as ${name})\n\(names.joined(separator: ", "))" }
        return ModelPrompt(instructions: instructions, text: t)
    }
}

// MARK: - The assistant

enum AssistantAvailability: Equatable, Sendable {
    case available
    /// Why not, in a few words for the screen (no settings jargon, no purchase wording).
    case unavailable(String)

    var isAvailable: Bool { self == .available }
    var reason: String? {
        if case let .unavailable(r) = self { return r }
        return nil
    }

    static let notEligible = AssistantAvailability.unavailable("Apple Intelligence isn't available on this device.")
    static let turnedOff = AssistantAvailability.unavailable("Turn on Apple Intelligence in Settings to use this.")
    static let notReady = AssistantAvailability.unavailable("Apple Intelligence is getting ready. Try again in a little while.")
    static let unsupported = AssistantAvailability.unavailable("Apple Intelligence isn't available here.")
}

/// A model answer that failed, worded for the screen.
struct AssistantFailure: Error, Equatable, Sendable, LocalizedError {
    var message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }

    static let tooLong = AssistantFailure("This response is too large for the on-device model. Try a smaller one.")
    static let refused = AssistantFailure("Apple Intelligence can't help with this one.")
    static let language = AssistantFailure("Apple Intelligence doesn't support this language yet.")
    static let busy = AssistantFailure("Apple Intelligence is busy. Try again in a moment.")
    static let other = AssistantFailure("Apple Intelligence couldn't answer. Try again.")
}

/// The on-device model, behind a protocol: the app uses Apple's Foundation Models framework
/// (App/Platform/AppleIntelligence.swift), unit and UI tests a deterministic fake.
@MainActor
protocol LanguageAssistant: AnyObject {
    var availability: AssistantAvailability { get }
    /// Streams the explanation: `onText` gets the whole text so far each time it grows.
    func explain(_ prompt: ModelPrompt, onText: @escaping @MainActor (String) -> Void) async throws -> String
    func suggestChecks(_ prompt: ModelPrompt) async throws -> [SuggestedCheck]
    func requestDraft(_ prompt: ModelPrompt) async throws -> RequestDraft
}

/// The fake (`-UITestSeed`, unit tests): answers at once from the prompt, deterministically.
@MainActor
final class FakeAssistant: LanguageAssistant {
    var availability: AssistantAvailability
    /// Every prompt it was given (unit tests check what the model would have seen).
    private(set) var prompts: [ModelPrompt] = []
    /// What `suggestChecks` answers (default: the heuristic for the response in the prompt, plus
    /// one wrong check that validation must drop).
    var checks: [SuggestedCheck]?
    var failure: AssistantFailure?
    /// App Store screenshots (`-UITestStoreScreens`): an explanation and checks written the way the
    /// on-device model answers for the seeded Create order response, instead of the test wording.
    var storeScreens = false

    init(availability: AssistantAvailability = .available, storeScreens: Bool = false) {
        self.availability = availability
        self.storeScreens = storeScreens
    }

    /// The store screenshots' explanation of shop-api's Create order answer (Stub.swift).
    static let storeExplanation = "201 Created: the server made a new order, ord_8KQ2mZ41, and its Location header points to it. "
        + "The order holds two items, 2 × TEE-0193 and 1 × CAP-0007, for customer cus_42, with a total amount of 6480 in EUR. "
        + "It was created at 10:42 UTC and isn't complete yet: its status field says what comes next, "
        + "and links lists where to go from here."

    func explain(_ prompt: ModelPrompt, onText: @escaping @MainActor (String) -> Void) async throws -> String {
        prompts.append(prompt)
        if let failure { throw failure }
        let status = Self.line(prompt.text, after: "Status: ") ?? "?"
        let shape = Self.line(prompt.text.replacingOccurrences(of: "BODY SHAPE\n", with: "SHAPE: "), after: "SHAPE: ").map { " The body is an \($0.hasPrefix("object") ? "object" : "answer") with these fields: \(ModelInput.cut($0, 120))." } ?? ""
        let text = storeScreens && status.hasPrefix("201") ? Self.storeExplanation
            : "The server answered \(status).\(shape) Nothing here was sent anywhere: this is the test stand-in for the on-device model."
        var so = ""
        for word in text.split(separator: " ") {
            so += (so.isEmpty ? "" : " ") + word
            onText(so)
            await Task.yield()
        }
        return so
    }

    func suggestChecks(_ prompt: ModelPrompt) async throws -> [SuggestedCheck] {
        prompts.append(prompt)
        if let failure { throw failure }
        if let checks { return checks }
        if storeScreens && Self.line(prompt.text, after: "Status: ")?.hasPrefix("201") == true {
            return [SuggestedCheck(.status, expected: "201", name: "Order is created"),
                    SuggestedCheck(.header, path: "Location", expected: "/v2/orders/", name: "Location points to the order"),
                    SuggestedCheck(.type, path: "items", expected: "array", name: "Items is a list"),
                    SuggestedCheck(.equals, path: "total.currency", expected: "EUR", name: "Total is in EUR")]
        }
        let status = Self.line(prompt.text, after: "Status: ").flatMap { Int($0.prefix { $0.isNumber }) } ?? 200
        return [SuggestedCheck(.status, expected: String(status), name: "Status is \(status)"),
                SuggestedCheck(.status, expected: "599", name: "a wrong guess")]
    }

    func requestDraft(_ prompt: ModelPrompt) async throws -> RequestDraft {
        prompts.append(prompt)
        if let failure { throw failure }
        let text = prompt.text.components(separatedBy: "\n\nVARIABLES").first ?? prompt.text
        let words = text.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        let method = words.first { RequestDraft.methods.contains($0.uppercased()) }?.uppercased() ?? "GET"
        var url = words.first { $0.hasPrefix("http://") || $0.hasPrefix("https://") || $0.hasPrefix("${") } ?? ""
        while let last = url.last, ".,;".contains(last) { url.removeLast() }
        if url.isEmpty, prompt.text.contains("baseUrl") { url = "${baseUrl}/" }
        let json = text.lowercased().contains("json")
        return RequestDraft(name: "", method: method, url: url, headers: json ? [.init(name: "Content-Type", value: "application/json")] : [],
                            bodyKind: json ? "json" : "none", body: json ? "{\"example\": true}" : "")
    }

    static func line(_ text: String, after prefix: String) -> String? {
        text.split(separator: "\n").first { $0.hasPrefix(prefix) }.map { String($0.dropFirst(prefix.count)) }
    }
}

/// One "Explain response" sheet: the streamed explanation, then a suggested test.
@MainActor
@Observable
final class ExplainSession {
    private(set) var text = ""
    private(set) var isExplaining = false
    private(set) var failure: String?
    private(set) var suggestion: TestSuggestion?
    private(set) var isSuggesting = false
    private(set) var suggestFailure: String?

    @ObservationIgnored private let assistant: LanguageAssistant
    @ObservationIgnored let request: SavedRequest
    @ObservationIgnored let facts: ResponseFacts
    @ObservationIgnored private let secrets: [String]

    init(assistant: LanguageAssistant, request: SavedRequest, facts: ResponseFacts, secrets: [String]) {
        self.assistant = assistant
        self.request = request
        self.facts = facts
        self.secrets = secrets
    }

    var availability: AssistantAvailability { assistant.availability }

    func explain() async {
        guard assistant.availability.isAvailable else {
            failure = assistant.availability.reason
            return
        }
        isExplaining = true
        failure = nil
        text = ""
        defer { isExplaining = false }
        do {
            let final = try await assistant.explain(Prompts.explain(request: request, facts: facts, secrets: secrets)) { [weak self] t in
                self?.text = t
            }
            text = final
        } catch {
            failure = (error as? AssistantFailure)?.message ?? AssistantFailure.other.message
        }
    }

    func suggest() async {
        guard assistant.availability.isAvailable else {
            suggestFailure = assistant.availability.reason
            return
        }
        isSuggesting = true
        suggestFailure = nil
        defer { isSuggesting = false }
        do {
            let checks = try await assistant.suggestChecks(Prompts.suggestTest(request: request, facts: facts, secrets: secrets))
            let s = TestScript.suggestion(checks, facts: facts)
            if s.checks.isEmpty {
                suggestFailure = "No check that this response passes came back. Try again."
            } else {
                suggestion = s
            }
        } catch {
            suggestFailure = (error as? AssistantFailure)?.message ?? AssistantFailure.other.message
        }
    }
}

/// The assistant the screens use (Apple's model in the app, `FakeAssistant` under `-UITestSeed`).
@MainActor
@Observable
final class Intelligence {
    @ObservationIgnored let assistant: LanguageAssistant

    init(assistant: LanguageAssistant) {
        self.assistant = assistant
    }

    var availability: AssistantAvailability { assistant.availability }
}

/// "New request from text or curl": a curl command is read without the model; anything else
/// goes to the model when it is available.
@MainActor
enum RequestFromText {
    enum Source: Equatable { case curl, model }

    static func source(_ text: String) -> Source { CurlImport.isCurl(text) ? .curl : .model }

    /// The request for `text` and notes to show (curl's left-out options).
    static func make(_ text: String, assistant: LanguageAssistant?, variables: [String]) async throws -> (request: SavedRequest, notes: [String]) {
        if CurlImport.isCurl(text) {
            let c = try CurlImport.parse(text)
            var notes = c.warnings
            if c.formFields.contains(where: { $0.type == "file" }) { notes.append("File fields are kept as rows: pick the files in the web or desktop app.") }
            return (c.savedRequest, notes)
        }
        guard let assistant else { throw AssistantFailure(AssistantAvailability.unsupported.reason ?? "") }
        guard assistant.availability.isAvailable else { throw AssistantFailure(assistant.availability.reason ?? "") }
        let draft = try await assistant.requestDraft(Prompts.request(from: text, variables: variables))
        let r = draft.savedRequest
        guard !r.url.isEmpty else { throw AssistantFailure("The description has no address to send to. Add the URL or host.") }
        return (r, [])
    }
}
