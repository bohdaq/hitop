import Foundation

/// A new request's automatic name, as the web app makes it (`webapp/src/new-request.js`
/// `requestAutoName`): "GET /users", "GET api.example.com" (host only), "Untitled" with no URL.
enum AutoName {
    static let untitled = "Untitled"
    static let myRequests = "My requests"

    /// The part of a URL the name shows: its path, or the host when there is no path; query and
    /// fragment left out. "${baseUrl}/users" → "/users".
    static func urlWhere(_ text: String) -> String {
        var url = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let r = url.range(of: "^[a-zA-Z][a-zA-Z0-9+.-]*://", options: .regularExpression) { url.removeSubrange(r) }
        if let i = url.firstIndex(where: { $0 == "?" || $0 == "#" }) { url = String(url[..<i]) }
        if url.isEmpty { return "" }
        let host: String
        if let r = url.range(of: "^(\\$\\{[^}]*\\}|\\{\\{[^}]*\\}\\})", options: .regularExpression) {
            host = String(url[r])
        } else {
            host = String(url.split(separator: "/", omittingEmptySubsequences: false).first ?? "")
        }
        let path = String(url.dropFirst(host.count))
        let out = !path.isEmpty && path != "/" ? path : host
        return out.count > 48 ? String(out.prefix(47)) + "…" : out
    }

    static func name(method: String, url: String) -> String {
        let w = urlWhere(url)
        return w.isEmpty ? untitled : "\(method.isEmpty ? "GET" : method.uppercased()) \(w)"
    }
}

/// A `${name}` reference in a text, for chips.
enum Segment: Hashable, Sendable {
    case text(String)
    case variable(String)
}

enum VariableText {
    /// The text cut into plain runs and `${name}` references.
    static func segments(_ s: String) -> [Segment] {
        var out: [Segment] = []
        var rest = s[...]
        while let start = rest.range(of: "${") {
            guard let end = rest[start.upperBound...].firstIndex(of: "}") else { break }
            if start.lowerBound > rest.startIndex { out.append(.text(String(rest[..<start.lowerBound]))) }
            out.append(.variable(String(rest[start.upperBound..<end])))
            rest = rest[rest.index(after: end)...]
        }
        if !rest.isEmpty { out.append(.text(String(rest))) }
        return out
    }

    static func names(_ s: String) -> [String] {
        segments(s).compactMap { if case let .variable(n) = $0 { return n } else { return nil } }
    }
}

/// An engine error as a headline and a next step (the web app's `explainError`,
/// `webapp/src/errors.js`), with URLSession's own texts added.
struct ExplainedError: Equatable, Sendable {
    var headline: String
    var next: String
    var raw: String
    var known: Bool

    private struct Rule {
        let pattern: String
        let headline: String
        let next: String
    }

    private static let rules: [Rule] = [
        Rule(pattern: "variable '([^']*)' is not defined",
             headline: "A variable has no value",
             next: "Give ${$1} a value in the active environment, the collection or the globals, or pick another environment."),
        Rule(pattern: "isn't a URL",
             headline: "That isn't a URL",
             next: "Enter a full URL, such as https://api.example.com/users."),
        Rule(pattern: "secret manager|\\$\\{secret:",
             headline: "Secret manager references don't work on iPhone",
             next: "Put the value in an environment secret instead; it stays in the Keychain."),
        Rule(pattern: "received corrupt message|wrong version number|invalid http version parsed",
             headline: "The server didn't answer over HTTPS",
             next: "If it only speaks plain HTTP, try the address with http:// instead of https://."),
        Rule(pattern: "failed to lookup address|dns error|name or service not known|nodename nor servname|no such host|hostname could not be found|could not resolve host|couldn.t resolve",
             headline: "Host not found",
             next: "Check the host name for typos, and the value of any variable in it (an environment may be missing). Is the host reachable from here?"),
        Rule(pattern: "connection refused|ECONNREFUSED|could not connect to the server",
             headline: "Connection refused",
             next: "Nothing answered on that port: is the server running, and is the port right?"),
        Rule(pattern: "certificate|self[- ]signed|SSL error|secure connection|tls handshake|handshake failure",
             headline: "The TLS (HTTPS) handshake failed",
             next: "The server's certificate isn't trusted or doesn't match the host name. Check the address, or try http:// if the server doesn't speak HTTPS."),
        Rule(pattern: "timed out|deadline has elapsed|ETIMEDOUT",
             headline: "No answer in time",
             next: "The server didn't answer. It may be down, slow or behind a firewall that drops the connection."),
        Rule(pattern: "connection reset|ECONNRESET|broken pipe|network connection was lost|unexpected eof",
             headline: "The server closed the connection",
             next: "It hung up before answering. Check the scheme (http or https) and the port, then try again."),
        Rule(pattern: "network is unreachable|no route to host|appears to be offline|not connected to the internet",
             headline: "Network unreachable",
             next: "This device can't reach that address. Check the network connection or VPN."),
    ]

    static func explain(_ raw: String) -> ExplainedError {
        let text = raw.replacingOccurrences(of: "^Error:\\s*", with: "", options: .regularExpression).trimmingCharacters(in: .whitespacesAndNewlines)
        for rule in rules {
            guard let re = try? NSRegularExpression(pattern: rule.pattern, options: [.caseInsensitive]),
                  let m = re.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)) else { continue }
            var next = rule.next
            if m.numberOfRanges > 1, let r = Range(m.range(at: 1), in: text) {
                next = next.replacingOccurrences(of: "$1", with: String(text[r]))
            }
            return ExplainedError(headline: rule.headline, next: next, raw: text, known: true)
        }
        return ExplainedError(headline: "The request failed", next: "", raw: text, known: false)
    }
}

/// Status texts for the status pill.
enum HTTPStatusText {
    static func text(_ code: Int) -> String {
        switch code {
        case 200: "OK"
        case 201: "Created"
        case 202: "Accepted"
        case 204: "No Content"
        case 301: "Moved Permanently"
        case 302: "Found"
        case 304: "Not Modified"
        case 400: "Bad Request"
        case 401: "Unauthorized"
        case 403: "Forbidden"
        case 404: "Not Found"
        case 405: "Method Not Allowed"
        case 409: "Conflict"
        case 422: "Unprocessable Content"
        case 429: "Too Many Requests"
        case 500: "Internal Server Error"
        case 502: "Bad Gateway"
        case 503: "Service Unavailable"
        case 504: "Gateway Timeout"
        default: ""
        }
    }
}

enum ByteText {
    static func text(_ n: Int) -> String {
        if n < 1024 { return "\(n) B" }
        if n < 1024 * 1024 { return String(format: "%.1f KB", Double(n) / 1024) }
        return String(format: "%.1f MB", Double(n) / 1024 / 1024)
    }
}
