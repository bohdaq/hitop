import Foundation

// The app's data, in the JSON shape the web app keeps (`hitop_collections`, `hitop_environments`,
// `hitop_globals`; CLAUDE.md "Data model"), so the I2 sync can exchange it as is. Every type keeps
// the keys it doesn't model (`extras`), so a collection edited here keeps the web app's examples,
// OpenAPI records, WebSocket/gRPC fields and settings. Foundation only: this folder (Core/) also
// builds and tests on Linux.

/// Any JSON value, for keys the app doesn't model.
enum JSONAny: Codable, Hashable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONAny])
    case object([String: JSONAny])

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let b = try? c.decode(Bool.self) { self = .bool(b) }
        else if let n = try? c.decode(Double.self) { self = .number(n) }
        else if let s = try? c.decode(String.self) { self = .string(s) }
        else if let a = try? c.decode([JSONAny].self) { self = .array(a) }
        else { self = .object(try c.decode([String: JSONAny].self)) }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case let .bool(b): try c.encode(b)
        case let .number(n):
            if n.rounded() == n, abs(n) < 9.0e15 { try c.encode(Int64(n)) } else { try c.encode(n) }
        case let .string(s): try c.encode(s)
        case let .array(a): try c.encode(a)
        case let .object(o): try c.encode(o)
        }
    }
}

struct AnyKey: CodingKey, Hashable {
    var stringValue: String
    var intValue: Int? { nil }
    init(_ s: String) { stringValue = s }
    init?(stringValue: String) { self.stringValue = stringValue }
    init?(intValue: Int) { nil }
}

extension KeyedDecodingContainer where K == AnyKey {
    func value<T: Decodable>(_ key: String, _ fallback: T) -> T {
        (try? decodeIfPresent(T.self, forKey: AnyKey(key))) ?? fallback
    }

    func optional<T: Decodable>(_ key: String) -> T? {
        (try? decodeIfPresent(T.self, forKey: AnyKey(key))) ?? nil
    }

    /// Every key not in `known`.
    func extras(_ known: Set<String>) -> [String: JSONAny] {
        var out: [String: JSONAny] = [:]
        for k in allKeys where !known.contains(k.stringValue) {
            if let v = try? decode(JSONAny.self, forKey: k) { out[k.stringValue] = v }
        }
        return out
    }
}

extension KeyedEncodingContainer where K == AnyKey {
    mutating func put<T: Encodable>(_ key: String, _ value: T) throws {
        try encode(value, forKey: AnyKey(key))
    }

    mutating func putExtras(_ extras: [String: JSONAny], skipping known: Set<String>) throws {
        for (k, v) in extras where !known.contains(k) { try encode(v, forKey: AnyKey(k)) }
    }
}

/// Ids: milliseconds since 1970 plus a counter, as the web app and the old app make them; below
/// 2^53 so they survive JavaScript.
enum IDs {
    nonisolated(unsafe) private static var last: UInt64 = 0
    private static let lock = NSLock()

    static func next() -> UInt64 {
        lock.lock()
        defer { lock.unlock() }
        let now = UInt64(Date().timeIntervalSince1970 * 1000)
        last = max(now, last + 1)
        return last
    }
}

/// A header row (also a parameter row): `{name, value, enabled}`.
struct KeyValue: Codable, Hashable, Sendable, Identifiable {
    var id = UUID()
    var name: String
    var value: String
    var enabled: Bool

    init(_ name: String = "", _ value: String = "", enabled: Bool = true) {
        self.name = name
        self.value = value
        self.enabled = enabled
    }

    static func == (a: KeyValue, b: KeyValue) -> Bool { a.name == b.name && a.value == b.value && a.enabled == b.enabled }
    func hash(into h: inout Hasher) { h.combine(name); h.combine(value); h.combine(enabled) }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        name = c.value("name", c.value("key", ""))
        value = c.value("value", "")
        enabled = c.value("enabled", true)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: AnyKey.self)
        try c.put("name", name)
        try c.put("value", value)
        try c.put("enabled", enabled)
    }
}

/// A disabled query parameter (`disabledParams`: `{key, value}`); enabled ones live in the URL.
struct ParamRow: Codable, Hashable, Sendable {
    var key: String
    var value: String
}

/// A form body row (`formFields`).
struct FormRow: Codable, Hashable, Sendable, Identifiable {
    var id = UUID()
    var key: String
    var value: String
    var type: String
    var enabled: Bool

    init(_ key: String = "", _ value: String = "", type: String = "text", enabled: Bool = true) {
        self.key = key
        self.value = value
        self.type = type
        self.enabled = enabled
    }

    static func == (a: FormRow, b: FormRow) -> Bool { a.key == b.key && a.value == b.value && a.type == b.type && a.enabled == b.enabled }
    func hash(into h: inout Hasher) { h.combine(key); h.combine(value); h.combine(type); h.combine(enabled) }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        key = c.value("key", "")
        value = c.value("value", "")
        type = c.value("type", "text")
        enabled = c.value("enabled", true)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: AnyKey.self)
        try c.put("key", key)
        try c.put("value", value)
        try c.put("type", type)
        try c.put("enabled", enabled)
    }
}

/// An auth preset (spec section 4): `{type: none|bearer|basic|apikey, ...}`.
enum AuthSpec: Codable, Hashable, Sendable {
    case none
    case bearer(token: String)
    case basic(username: String, password: String)
    case apiKey(key: String, value: String, inQuery: Bool)

    var kind: String {
        switch self {
        case .none: "none"
        case .bearer: "bearer"
        case .basic: "basic"
        case .apiKey: "apikey"
        }
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        switch c.value("type", "none") {
        case "bearer": self = .bearer(token: c.value("token", ""))
        case "basic": self = .basic(username: c.value("username", ""), password: c.value("password", ""))
        case "apikey": self = .apiKey(key: c.value("key", ""), value: c.value("value", ""), inQuery: c.value("in", "header") == "query")
        default: self = .none
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: AnyKey.self)
        try c.put("type", kind)
        switch self {
        case .none: break
        case let .bearer(t): try c.put("token", t)
        case let .basic(u, p):
            try c.put("username", u)
            try c.put("password", p)
        case let .apiKey(k, v, q):
            try c.put("key", k)
            try c.put("value", v)
            try c.put("in", q ? "query" : "header")
        }
    }
}

/// How the editor shows a body; saved as `bodyMode` (+ `bodyType`).
enum BodyKind: String, CaseIterable, Sendable, Identifiable {
    case none, json, text, form, multipart, graphql
    var id: String { rawValue }

    var label: String {
        switch self {
        case .none: "None"
        case .json: "JSON"
        case .text: "Raw"
        case .form: "Form"
        case .multipart: "Multipart"
        case .graphql: "GraphQL"
        }
    }
}

struct SavedRequest: Codable, Hashable, Sendable, Identifiable {
    var id: UInt64
    var name: String
    var url: String
    var method: String
    var headers: [KeyValue]
    var body: String
    var preRequestScript: String
    var postRequestScript: String
    /// nil: the collection's auth applies; `.none` turns it off.
    var auth: AuthSpec?
    /// `raw`, `urlencoded` or `multipart`.
    var bodyMode: String
    /// `graphql`, `json`, `text`... when set; nil = inferred.
    var bodyType: String?
    var formFields: [FormRow]
    var disabledParams: [ParamRow]
    var graphqlVariables: String
    var graphqlOperationName: String
    /// The folder path + file name in a collection folder (`orders/create-order`), nil at the top.
    var file: String?
    /// Keys the app doesn't model, kept as they came.
    var extras: [String: JSONAny]

    init(id: UInt64 = IDs.next(), name: String = "Untitled", url: String = "", method: String = "GET", headers: [KeyValue] = [],
         body: String = "", preRequestScript: String = "", postRequestScript: String = "", auth: AuthSpec? = nil,
         bodyMode: String = "raw", bodyType: String? = nil, formFields: [FormRow] = [], disabledParams: [ParamRow] = [],
         graphqlVariables: String = "", graphqlOperationName: String = "", file: String? = nil, extras: [String: JSONAny] = [:]) {
        self.id = id
        self.name = name
        self.url = url
        self.method = method
        self.headers = headers
        self.body = body
        self.preRequestScript = preRequestScript
        self.postRequestScript = postRequestScript
        self.auth = auth
        self.bodyMode = bodyMode
        self.bodyType = bodyType
        self.formFields = formFields
        self.disabledParams = disabledParams
        self.graphqlVariables = graphqlVariables
        self.graphqlOperationName = graphqlOperationName
        self.file = file
        self.extras = extras
    }

    private static let known: Set<String> = ["id", "name", "url", "method", "headers", "body", "preRequestScript", "postRequestScript",
                                             "auth", "bodyMode", "bodyType", "formFields", "disabledParams", "graphql", "file"]

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        id = c.value("id", UInt64(0))
        if id == 0, let s: String = c.optional("id") { id = derivedID(s) }
        if id == 0 { id = IDs.next() }
        name = c.value("name", "Untitled")
        url = c.value("url", "")
        method = c.value("method", "GET").uppercased()
        headers = c.value("headers", [KeyValue]()).filter { !($0.name.isEmpty && $0.value.isEmpty) }
        body = c.value("body", "")
        preRequestScript = c.value("preRequestScript", "")
        postRequestScript = c.value("postRequestScript", "")
        auth = c.optional("auth")
        bodyMode = c.value("bodyMode", "raw")
        bodyType = c.optional("bodyType")
        formFields = c.value("formFields", [FormRow]())
        disabledParams = c.value("disabledParams", [ParamRow]())
        let g: [String: String] = c.value("graphql", [:])
        graphqlVariables = g["variables"] ?? ""
        graphqlOperationName = g["operationName"] ?? ""
        file = c.optional("file")
        extras = c.extras(Self.known)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: AnyKey.self)
        try c.put("id", id)
        try c.put("name", name)
        try c.put("url", url)
        try c.put("method", method)
        try c.put("headers", headers)
        try c.put("body", body)
        try c.put("preRequestScript", preRequestScript)
        try c.put("postRequestScript", postRequestScript)
        if let auth { try c.put("auth", auth) }
        try c.put("bodyMode", bodyMode)
        if let bodyType { try c.put("bodyType", bodyType) }
        if !formFields.isEmpty { try c.put("formFields", formFields) }
        if !disabledParams.isEmpty { try c.put("disabledParams", disabledParams) }
        var g: [String: String] = [:]
        if !graphqlVariables.isEmpty { g["variables"] = graphqlVariables }
        if !graphqlOperationName.isEmpty { g["operationName"] = graphqlOperationName }
        if !g.isEmpty { try c.put("graphql", g) }
        if let file, !file.isEmpty { try c.put("file", file) }
        try c.putExtras(extras, skipping: Self.known)
    }

    /// The folder the request is in ("" at the top): its `file` without the last part.
    var folder: String {
        guard let file, let slash = file.lastIndex(of: "/") else { return "" }
        return String(file[..<slash])
    }

    /// WebSocket and gRPC requests come from the web app and the CLI; this version shows them but
    /// sends only HTTP (WebSocket, SSE and gRPC are I4).
    var kind: String? {
        if case let .string(k)? = extras["kind"] { return k }
        return nil
    }

    var bodyKind: BodyKind {
        get {
            switch bodyMode {
            case "urlencoded": return .form
            case "multipart": return .multipart
            default: break
            }
            if bodyType == "graphql" { return .graphql }
            if body.isEmpty && bodyType == nil { return .none }
            if bodyType == "text" || bodyType == "xml" { return .text }
            if bodyType == "json" { return .json }
            let t = body.drop(while: { $0.isWhitespace })
            return t.hasPrefix("{") || t.hasPrefix("[") || body.isEmpty ? .json : .text
        }
        set {
            switch newValue {
            case .none:
                bodyMode = "raw"
                bodyType = nil
                body = ""
            case .json:
                bodyMode = "raw"
                bodyType = "json"
            case .text:
                bodyMode = "raw"
                bodyType = "text"
            case .form:
                bodyMode = "urlencoded"
            case .multipart:
                bodyMode = "multipart"
            case .graphql:
                bodyMode = "raw"
                bodyType = "graphql"
            }
        }
    }
}

/// hitop-core's id for an old string id (FNV-1a 64, below 2^53), as `model.rs` `de_id`.
func derivedID(_ s: String) -> UInt64 {
    var h: UInt64 = 0xcbf2_9ce4_8422_2325
    for b in s.utf8 {
        h ^= UInt64(b)
        h = h &* 0x100_0000_01b3
    }
    return h & ((1 << 53) - 1)
}

struct SavedCollection: Codable, Hashable, Sendable, Identifiable {
    var id: UInt64
    var name: String
    var requests: [SavedRequest]
    var variables: [String: String]
    var auth: AuthSpec?
    /// Folder paths in order, empty ones too (web app DD1, hitop.yaml `folders`).
    var folders: [String]
    var extras: [String: JSONAny]

    init(id: UInt64 = IDs.next(), name: String, requests: [SavedRequest] = [], variables: [String: String] = [:], auth: AuthSpec? = nil,
         folders: [String] = [], extras: [String: JSONAny] = [:]) {
        self.id = id
        self.name = name
        self.requests = requests
        self.variables = variables
        self.auth = auth
        self.folders = folders
        self.extras = extras
    }

    private static let known: Set<String> = ["id", "name", "requests", "variables", "auth", "folders"]

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        id = c.value("id", UInt64(0))
        if id == 0, let s: String = c.optional("id") { id = derivedID(s) }
        if id == 0 { id = IDs.next() }
        name = c.value("name", "Collection")
        requests = c.value("requests", [SavedRequest]())
        variables = c.value("variables", [String: String]())
        auth = c.optional("auth")
        folders = c.value("folders", [String]())
        extras = c.extras(Self.known)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: AnyKey.self)
        try c.put("id", id)
        try c.put("name", name)
        try c.put("requests", requests)
        try c.put("variables", variables)
        if let auth { try c.put("auth", auth) }
        if !folders.isEmpty { try c.put("folders", folders) }
        try c.putExtras(extras, skipping: Self.known)
    }

    /// Every folder: the stored ones, then any a request is in, parents before children.
    var allFolders: [String] {
        var out: [String] = []
        func add(_ path: String) {
            guard !path.isEmpty, !out.contains(path) else { return }
            if let slash = path.lastIndex(of: "/") { add(String(path[..<slash])) }
            out.append(path)
        }
        for f in folders { add(f) }
        for r in requests { add(r.folder) }
        return out
    }
}

struct SavedEnvironment: Codable, Hashable, Sendable, Identifiable {
    var id: String
    var name: String
    /// Values of the variables that aren't secret (secret values live in the Keychain).
    var variables: [String: String]
    /// Names of the secret variables.
    var secrets: [String]
    var extras: [String: JSONAny]

    init(id: String = String(IDs.next()), name: String, variables: [String: String] = [:], secrets: [String] = [], extras: [String: JSONAny] = [:]) {
        self.id = id
        self.name = name
        self.variables = variables
        self.secrets = secrets
        self.extras = extras
    }

    private static let known: Set<String> = ["id", "name", "variables", "secrets"]

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: AnyKey.self)
        if let s: String = c.optional("id") { id = s } else if let n: UInt64 = c.optional("id") { id = String(n) } else { id = String(IDs.next()) }
        name = c.value("name", "Environment")
        variables = c.value("variables", [String: String]())
        secrets = c.value("secrets", [String]())
        extras = c.extras(Self.known)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: AnyKey.self)
        try c.put("id", id)
        try c.put("name", name)
        try c.put("variables", variables)
        if !secrets.isEmpty { try c.put("secrets", secrets) }
        try c.putExtras(extras, skipping: Self.known)
    }
}

/// One assertion result.
struct TestOutcome: Codable, Hashable, Sendable {
    var name: String
    var pass: Bool
    var detail: String
}

/// A response as history keeps it (body cut at 100 kB like the web app).
struct ResponseSnapshot: Codable, Hashable, Sendable {
    var status: Int
    /// In order, names as received.
    var headers: [[String]]
    var body: String
    var truncated: Bool
    var durationMs: Int
    var size: Int
    var tests: [TestOutcome]

    static let maxBody = 100_000
}

struct HistoryEntry: Codable, Hashable, Sendable, Identifiable {
    var id: UInt64
    /// Milliseconds since 1970.
    var timestamp: UInt64
    var collectionName: String?
    var request: SavedRequest
    var response: ResponseSnapshot?
    var error: String?

    var date: Date { Date(timeIntervalSince1970: TimeInterval(timestamp) / 1000) }
}
