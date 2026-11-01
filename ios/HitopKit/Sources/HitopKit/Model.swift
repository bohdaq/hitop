import Foundation

/// The JSON model hitop's other tools use (`collections.json`, the web
/// app's storage), cut down to what the companion reads and runs. Field
/// names are the JSON keys (spec section 6). Keys only the editors need
/// (unknown keys, saved examples, OpenAPI hashes) are not kept: the
/// companion never writes a collection.

public struct Header: Codable, Hashable, Sendable {
    public var name: String
    public var value: String
    public var enabled: Bool

    public init(_ name: String, _ value: String, enabled: Bool = true) {
        self.name = name
        self.value = value
        self.enabled = enabled
    }
}

/// A disabled query parameter (enabled ones live in the URL).
public struct Param: Codable, Hashable, Sendable {
    public var key: String
    public var value: String
}

public struct FormField: Codable, Hashable, Sendable {
    public var key: String
    public var value: String
    /// `text` or `file`.
    public var type: String
    public var enabled: Bool
    /// A file row's file name (the row's `fileName` key).
    public var fileName: String?

    public init(key: String, value: String, type: String = "text", enabled: Bool = true, fileName: String? = nil) {
        self.key = key
        self.value = value
        self.type = type
        self.enabled = enabled
        self.fileName = fileName
    }
}

public enum ApiKeyLocation: String, Codable, Sendable {
    case header, query
}

/// An auth preset (spec section 4). Values may hold `${variables}`.
public enum Auth: Hashable, Sendable {
    case none
    case bearer(token: String)
    case basic(username: String, password: String)
    case apiKey(key: String, value: String, location: ApiKeyLocation)

    /// Every string the preset holds, for finding references in it.
    var strings: [String] {
        switch self {
        case .none: return []
        case let .bearer(t): return [t]
        case let .basic(u, p): return [u, p]
        case let .apiKey(k, v, _): return [k, v]
        }
    }
}

extension Auth: Codable {
    private enum Keys: String, CodingKey { case type, token, username, password, key, value, `in` }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let s = { (k: Keys) in try c.decodeIfPresent(String.self, forKey: k) ?? "" }
        switch try c.decode(String.self, forKey: .type) {
        case "none": self = .none
        case "bearer": self = .bearer(token: try s(.token))
        case "basic": self = .basic(username: try s(.username), password: try s(.password))
        case "apikey": self = .apiKey(key: try s(.key), value: try s(.value), location: try c.decodeIfPresent(ApiKeyLocation.self, forKey: .in) ?? .header)
        case let other: throw DecodingError.dataCorruptedError(forKey: .type, in: c, debugDescription: "unknown auth type '\(other)'")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case .none: try c.encode("none", forKey: .type)
        case let .bearer(t):
            try c.encode("bearer", forKey: .type)
            try c.encode(t, forKey: .token)
        case let .basic(u, p):
            try c.encode("basic", forKey: .type)
            try c.encode(u, forKey: .username)
            try c.encode(p, forKey: .password)
        case let .apiKey(k, v, l):
            try c.encode("apikey", forKey: .type)
            try c.encode(k, forKey: .key)
            try c.encode(v, forKey: .value)
            try c.encode(l, forKey: .in)
        }
    }
}

/// A GraphQL request's variables (JSON text) and operation name; the query
/// is the request's `body`.
public struct GraphQL: Codable, Hashable, Sendable {
    public var variables: String
    public var operationName: String

    public init(variables: String = "", operationName: String = "") {
        self.variables = variables
        self.operationName = operationName
    }

    var isEmpty: Bool { variables.isEmpty && operationName.isEmpty }
}

public struct Request: Codable, Hashable, Sendable, Identifiable {
    public var id: UInt64
    public var name: String
    /// With the enabled query parameters appended.
    public var url: String
    public var method: String
    public var headers: [Header]
    /// The text body, or the GraphQL query.
    public var body: String
    public var preRequestScript: String
    public var postRequestScript: String
    /// None means the collection's auth applies; `.none` turns it off.
    public var auth: Auth?
    /// `urlencoded` or `multipart`; nil for a text body.
    public var bodyMode: String?
    public var formFields: [FormField]
    public var disabledParams: [Param]
    /// The text body's type when it differs from the inferred one (`graphql` for GraphQL).
    public var bodyType: String?
    public var graphql: GraphQL?
    /// `websocket` or `grpc`; nil for HTTP.
    public var kind: String?
    /// The request's path under `requests/`, without the extension
    /// (`orders/create-order`). Always set for a request read from a folder
    /// (the JSON model keeps `file` only when it isn't the default one).
    public var path: String
    /// `yaml` or `yml`: the file's extension on disk.
    public var fileExtension: String

    public init(id: UInt64 = 0, name: String, url: String, method: String = "GET", headers: [Header] = [], body: String = "",
                preRequestScript: String = "", postRequestScript: String = "", auth: Auth? = nil, bodyMode: String? = nil,
                formFields: [FormField] = [], disabledParams: [Param] = [], bodyType: String? = nil, graphql: GraphQL? = nil,
                kind: String? = nil, path: String = "", fileExtension: String = "yaml") {
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
        self.formFields = formFields
        self.disabledParams = disabledParams
        self.bodyType = bodyType
        self.graphql = graphql
        self.kind = kind
        self.path = path
        self.fileExtension = fileExtension
    }

    public var isWebSocket: Bool { kind == "websocket" }
    public var isGRPC: Bool { kind == "grpc" }

    /// Sent as GraphQL: `bodyType` is `graphql` and there is no form body.
    public var isGraphQL: Bool { bodyType == "graphql" && bodyMode == nil }

    /// The request's file, relative to the collection folder.
    public var file: String { "requests/\(path).\(fileExtension)" }
}

public struct Collection: Codable, Hashable, Sendable, Identifiable {
    public var id: UInt64
    public var name: String
    public var requests: [Request]
    public var variables: [String: String]
    /// The key of this collection's secrets in the keychain (spec section 5).
    public var workspace: String?
    public var auth: Auth?

    public init(id: UInt64 = 0, name: String, requests: [Request] = [], variables: [String: String] = [:], workspace: String? = nil, auth: Auth? = nil) {
        self.id = id
        self.name = name
        self.requests = requests
        self.variables = variables
        self.workspace = workspace
        self.auth = auth
    }

    /// The keychain workspace id: `workspace`, else the id in decimal.
    public var workspaceID: String {
        if let w = workspace, !w.isEmpty { return w }
        return String(id)
    }
}

public struct Environment: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    /// Values of the variables that aren't secret.
    public var variables: [String: String]
    /// Names of the secret variables (no value in the file).
    public var secrets: [String]

    public init(id: String, name: String, variables: [String: String] = [:], secrets: [String] = []) {
        self.id = id
        self.name = name
        self.variables = variables
        self.secrets = secrets
    }
}

/// The globals scope's id and name (`globals.yaml`).
public let globalsID = "globals"

/// Why a collection folder couldn't be read.
public struct FormatError: Error, CustomStringConvertible, LocalizedError, Sendable {
    public let description: String
    public init(_ description: String) { self.description = description }
    public var errorDescription: String? { description }
}
