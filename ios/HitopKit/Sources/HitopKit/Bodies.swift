import Foundation

/// Auth presets applied to a prepared request (hitop-core `auth.rs`).
enum AuthApply {
    /// Adds the auth's header or query parameter. Bearer and basic don't
    /// replace an `Authorization` header set by hand; an API key header
    /// doesn't replace a header of its name; an API key in the query replaces
    /// the parameter of its name.
    static func apply(_ auth: Auth?, url: String, headers: [(String, String)], _ interpolate: (String) throws -> String) throws -> (String, [(String, String)]) {
        let has = { (name: String) in headers.contains { $0.0.lowercased() == name.lowercased() && !$0.1.isEmpty } }
        var headers = headers
        switch auth {
        case let .bearer(token)? where !token.isEmpty:
            if !has("Authorization") { headers.append(("Authorization", "Bearer \(try interpolate(token))")) }
            return (url, headers)
        case let .basic(username, password)? where !username.isEmpty || !password.isEmpty:
            if !has("Authorization") {
                let creds = "\(try interpolate(username)):\(try interpolate(password))"
                headers.append(("Authorization", "Basic \(Data(creds.utf8).base64EncodedString())"))
            }
            return (url, headers)
        case let .apiKey(k, v, location)? where !k.isEmpty && !v.isEmpty:
            let key = try interpolate(k)
            let value = try interpolate(v)
            switch location {
            case .query:
                var rows = Query.parse(url).filter { $0.0 != key }
                rows.append((key, urlencode(value)))
                return (Query.with(url, rows), headers)
            case .header:
                if !has(key) { headers.append((key, value)) }
                return (url, headers)
            }
        default:
            return (url, headers)
        }
    }

    /// Percent-encodes everything but `A-Z a-z 0-9 - _ . ~`.
    static func urlencode(_ s: String) -> String {
        var out = ""
        for b in s.utf8 {
            switch b {
            case 0x41...0x5A, 0x61...0x7A, 0x30...0x39, 0x2D, 0x5F, 0x2E, 0x7E: out.unicodeScalars.append(Unicode.Scalar(b))
            default: out += String(format: "%%%02X", b)
            }
        }
        return out
    }
}

/// GraphQL-over-HTTP bodies (hitop-core `graphql.rs`).
public enum GraphQLBody {
    public static let contentType = "application/json"

    /// The variables text as JSON: blank is none; otherwise a JSON object or `null`.
    static func parseVariables(_ text: String) throws -> JSONValue? {
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return nil }
        let v: JSONValue
        do { v = try JSONValue.parse(text) } catch { throw FormatError("GraphQL variables are not valid JSON: \(error)") }
        switch v {
        case .object: return v
        case .null: return nil
        case .bool: throw FormatError("GraphQL variables must be a JSON object, got a boolean")
        case .number: throw FormatError("GraphQL variables must be a JSON object, got a number")
        case .string: throw FormatError("GraphQL variables must be a JSON object, got a string")
        case .array: throw FormatError("GraphQL variables must be a JSON object, got an array")
        }
    }

    /// `{"query":...,"variables":...,"operationName":...}` in that order, the
    /// variables spliced in as typed (so their key order stays), empty ones
    /// left out.
    public static func requestBody(query: String, variables: String, operationName: String) throws -> String {
        var out = "{\"query\":\(JSONValue.quote(query))"
        if try parseVariables(variables) != nil {
            out += ",\"variables\":" + variables.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        let name = operationName.trimmingCharacters(in: .whitespacesAndNewlines)
        if !name.isEmpty { out += ",\"operationName\":\(JSONValue.quote(name))" }
        return out + "}"
    }
}

/// Form bodies (hitop-core `form.rs`, the web app's `urlEncodedBody` and
/// `multipartBody`).
public enum FormBody {
    /// JavaScript's `encodeURIComponent`.
    public static func encodeComponent(_ s: String) -> String {
        var out = ""
        for b in s.utf8 {
            let c = Unicode.Scalar(b)
            if (b < 0x80 && (c.properties.isAlphabetic || (0x30...0x39).contains(b))) || "-_.!~*'()".unicodeScalars.contains(c) {
                out.unicodeScalars.append(c)
            } else {
                out += String(format: "%%%02X", b)
            }
        }
        return out
    }

    public static func urlencoded(_ pairs: [(String, String)]) -> (body: Data, contentType: String) {
        let body = pairs.map { "\(encodeComponent($0.0))=\(encodeComponent($0.1))" }.joined(separator: "&")
        return (Data(body.utf8), "application/x-www-form-urlencoded")
    }

    public enum Part: Sendable {
        case text(name: String, value: String)
        case file(name: String, filename: String, contentType: String, data: Data)
    }

    private static func quote(_ s: String) -> String {
        s.replacingOccurrences(of: "\"", with: "%22").replacingOccurrences(of: "\r", with: " ").replacingOccurrences(of: "\n", with: " ")
    }

    /// A multipart body with a random boundary no part contains.
    public static func multipart(_ parts: [Part]) -> (body: Data, contentType: String) {
        var boundary: String
        repeat {
            boundary = "----hitop" + randomHex(32)
        } while parts.contains { p in
            switch p {
            case let .text(_, value): return value.contains(boundary)
            case let .file(_, _, _, data): return data.range(of: Data(boundary.utf8)) != nil
            }
        }
        var body = Data()
        for p in parts {
            switch p {
            case let .text(name, value):
                body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(quote(name))\"\r\n\r\n\(value)\r\n".utf8))
            case let .file(name, filename, contentType, data):
                body.append(Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(quote(name))\"; filename=\"\(quote(filename))\"\r\nContent-Type: \(contentType)\r\n\r\n".utf8))
                body.append(data)
                body.append(Data("\r\n".utf8))
            }
        }
        body.append(Data("--\(boundary)--\r\n".utf8))
        return (body, "multipart/form-data; boundary=\(boundary)")
    }

    static func randomHex(_ n: Int) -> String {
        var g = SystemRandomNumberGenerator()
        return (0..<n).map { _ in String(Int.random(in: 0..<16, using: &g), radix: 16) }.joined()
    }

    /// A saved request's form body with variables filled in; nil for a text
    /// body. File rows carry no file in a collection, so they go out as empty
    /// parts named after the row's `fileName`, as the other hitop tools send them.
    static func requestBody(_ r: Request, _ variables: [String: String]) throws -> (body: Data, contentType: String)? {
        guard let mode = r.bodyMode, mode == "urlencoded" || mode == "multipart" else { return nil }
        let rows = r.formFields.filter { $0.enabled && !$0.key.isEmpty }
        if mode == "urlencoded" {
            return urlencoded(try rows.map { (try Interpolation.interpolate($0.key, variables), try Interpolation.interpolate($0.value, variables)) })
        }
        return multipart(try rows.map { f in
            let name = try Interpolation.interpolate(f.key, variables)
            if f.type == "file" {
                return .file(name: name, filename: f.fileName ?? "", contentType: "application/octet-stream", data: Data())
            }
            return .text(name: name, value: try Interpolation.interpolate(f.value, variables))
        })
    }

    /// The headers and body a request goes out with after its pre-request
    /// script: its form body (with a `Content-Type` unless a header sets one)
    /// when it has one and the script didn't replace the body, else the
    /// prepared text body with the Content-Type its text implies, unless a
    /// header sets one (`Sending.addContentType`).
    static func outgoing(_ r: Request, headers: [(String, String)], body: String, _ variables: [String: String]) throws -> ([(String, String)], Data) {
        var headers = headers
        let unchanged = (try? Interpolation.interpolate(r.body, variables)) == body
        if let built = try requestBody(r, variables), unchanged {
            if !headers.contains(where: { $0.0.lowercased() == "content-type" && !$0.1.isEmpty }) {
                headers.append(("Content-Type", built.contentType))
            }
            return (headers, built.body)
        }
        // A raw body (or one the script replaced) says what it is.
        Sending.addContentType(method: r.method, headers: &headers, body: body)
        return (headers, Data(body.utf8))
    }
}
