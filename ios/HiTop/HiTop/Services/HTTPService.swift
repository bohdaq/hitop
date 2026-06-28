import Foundation

struct HTTPResponse {
    let statusCode: Int
    let headers: [String: String]
    let body: String
    let contentType: ResponseContentType
    let durationMs: UInt64
}

enum ResponseContentType {
    case json, html, xml, text
}

enum HTTPServiceError: LocalizedError {
    case invalidURL(String)
    case networkError(String)

    var errorDescription: String? {
        switch self {
        case .invalidURL(let url): return "Invalid URL: \(url)"
        case .networkError(let msg): return msg
        }
    }
}

final class HTTPService {
    static let shared = HTTPService()

    private let session: URLSession = {
        let config = URLSessionConfiguration.default
        config.timeoutIntervalForRequest = 30
        return URLSession(configuration: config)
    }()

    func send(
        url urlString: String,
        method: String,
        headers: [Header],
        body: String
    ) async throws -> HTTPResponse {
        guard let url = URL(string: urlString) else {
            throw HTTPServiceError.invalidURL(urlString)
        }

        var request = URLRequest(url: url)
        request.httpMethod = method

        for header in headers where !header.name.isEmpty && !header.value.isEmpty {
            request.setValue(header.value, forHTTPHeaderField: header.name)
        }

        let bodyMethods = ["POST", "PUT", "PATCH"]
        if bodyMethods.contains(method.uppercased()) && !body.isEmpty {
            request.httpBody = body.data(using: .utf8)
        }

        let start = Date()
        let (data, urlResponse) = try await session.data(for: request)
        let durationMs = UInt64(Date().timeIntervalSince(start) * 1000)

        guard let httpResponse = urlResponse as? HTTPURLResponse else {
            throw HTTPServiceError.networkError("Invalid response")
        }

        var responseHeaders: [String: String] = [:]
        for (key, value) in httpResponse.allHeaderFields {
            responseHeaders["\(key)"] = "\(value)"
        }

        let bodyString = String(data: data, encoding: .utf8) ?? String(data: data, encoding: .isoLatin1) ?? ""
        let contentTypeHeader = responseHeaders["Content-Type"] ?? responseHeaders["content-type"] ?? ""
        let contentType = detectContentType(contentTypeHeader)

        return HTTPResponse(
            statusCode: httpResponse.statusCode,
            headers: responseHeaders,
            body: bodyString,
            contentType: contentType,
            durationMs: durationMs
        )
    }

    private func detectContentType(_ header: String) -> ResponseContentType {
        if header.contains("application/json") { return .json }
        if header.contains("text/html") { return .html }
        if header.contains("text/xml") || header.contains("application/xml") { return .xml }
        return .text
    }
}
