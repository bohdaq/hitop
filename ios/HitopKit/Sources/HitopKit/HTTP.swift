import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// A request ready to go out.
public struct OutgoingRequest: Sendable {
    public var method: String
    public var url: String
    public var headers: [(String, String)]
    public var body: Data
}

public struct HTTPResponse: Sendable {
    public var status: Int
    /// Names in lower case; repeated headers joined with `, `.
    public var headers: [String: String]
    public var body: String
    /// Until the response arrived (the whole body, see `URLSessionHTTPClient`).
    public var durationMs: Int
}

public struct HTTPError: Error, CustomStringConvertible, LocalizedError, Sendable {
    public let description: String
    public var errorDescription: String? { description }
}

/// Sends requests. One client per run, so cookies a response sets go out
/// with the later requests of the same run and nowhere else.
public protocol HTTPClient: Sendable {
    func send(_ request: OutgoingRequest, timeout: TimeInterval?) async throws -> HTTPResponse
}

/// URLSession with an ephemeral configuration: its own cookie storage, no
/// cache, nothing written to disk.
public final class URLSessionHTTPClient: HTTPClient, @unchecked Sendable {
    let session: URLSession

    public init() {
        let config = URLSessionConfiguration.ephemeral
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.urlCache = nil
        config.httpShouldSetCookies = true
        config.httpCookieAcceptPolicy = .always
        session = URLSession(configuration: config)
    }

    deinit { session.invalidateAndCancel() }

    /// Header names are HTTP tokens; values can't hold line breaks. Others
    /// are left out, as the Rust client leaves them out.
    static func validHeader(_ name: String, _ value: String) -> Bool {
        let tchar = Set("!#$%&'*+-.^_`|~0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".unicodeScalars)
        return !name.isEmpty && name.unicodeScalars.allSatisfy { tchar.contains($0) } && !value.contains("\r") && !value.contains("\n") && !value.contains("\0")
    }

    public func send(_ r: OutgoingRequest, timeout: TimeInterval?) async throws -> HTTPResponse {
        guard let url = URL(string: r.url), let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https", url.host != nil else {
            throw HTTPError(description: "builder error: invalid URL '\(r.url)'")
        }
        var req = URLRequest(url: url)
        let method = r.method.uppercased()
        req.httpMethod = !method.isEmpty && URLSessionHTTPClient.validHeader(method, "") ? method : "GET"
        for (k, v) in r.headers where URLSessionHTTPClient.validHeader(k, v) {
            req.setValue(v, forHTTPHeaderField: k)
        }
        if !r.body.isEmpty { req.httpBody = r.body }
        if let timeout { req.timeoutInterval = timeout }
        let start = Date()
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: req)
        } catch {
            throw HTTPError(description: "error sending request for url (\(r.url)): \(error.localizedDescription)")
        }
        let ms = Int(Date().timeIntervalSince(start) * 1000)
        guard let http = response as? HTTPURLResponse else {
            throw HTTPError(description: "no HTTP response from \(r.url)")
        }
        var headers: [String: String] = [:]
        for (k, v) in http.allHeaderFields {
            headers[String(describing: k).lowercased()] = String(describing: v)
        }
        return HTTPResponse(status: http.statusCode, headers: headers, body: String(decoding: data, as: UTF8.self), durationMs: ms)
    }
}
