import Foundation

struct Collection: Codable, Identifiable {
    var id: UInt64
    var name: String
    var requests: [HTTPRequest]
    var variables: [String: String]

    init(id: UInt64 = IDGenerator.next(), name: String, requests: [HTTPRequest] = [], variables: [String: String] = [:]) {
        self.id = id
        self.name = name
        self.requests = requests
        self.variables = variables
    }
}

struct HTTPRequest: Codable, Identifiable {
    var id: UInt64
    var name: String
    var url: String
    var method: String
    var headers: [Header]
    var body: String
    var preRequestScript: String
    var postRequestScript: String

    init(
        id: UInt64 = IDGenerator.next(),
        name: String = "New Request",
        url: String = "",
        method: String = "GET",
        headers: [Header] = [],
        body: String = "",
        preRequestScript: String = "",
        postRequestScript: String = ""
    ) {
        self.id = id
        self.name = name
        self.url = url
        self.method = method
        self.headers = headers
        self.body = body
        self.preRequestScript = preRequestScript
        self.postRequestScript = postRequestScript
    }
}

struct Header: Codable, Identifiable {
    var id: UUID = UUID()
    var name: String
    var value: String

    init(name: String = "", value: String = "") {
        self.name = name
        self.value = value
    }

    enum CodingKeys: String, CodingKey {
        case name, value
    }
}

enum HTTPMethod: String, CaseIterable {
    case get = "GET"
    case post = "POST"
    case put = "PUT"
    case patch = "PATCH"
    case delete = "DELETE"
    case head = "HEAD"
    case options = "OPTIONS"
}
