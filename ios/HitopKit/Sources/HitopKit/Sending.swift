import Foundation

/// What every hitop surface does to a request on its way out (hitop-core
/// `sending.rs`, vectors in spec/vectors/send.json): a Content-Type for a raw
/// body that has none, and a scheme for a URL typed without one. Whitespace
/// and character classes are JavaScript's, as the web app first had them.
public enum Sending {
    /// The methods a raw body gets a Content-Type with.
    public static let bodyMethods = ["POST", "PUT", "PATCH"]

    /// JavaScript's `trim` whitespace: U+FEFF in, U+0085 out.
    static func isJSWhitespace(_ c: Unicode.Scalar) -> Bool {
        switch c.value {
        case 0x9...0xD, 0x20, 0xA0, 0x1680, 0x2000...0x200A, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000, 0xFEFF: return true
        default: return false
        }
    }

    static func trimmedStart(_ s: String) -> Substring.UnicodeScalarView {
        let scalars = s.unicodeScalars
        return scalars[(scalars.firstIndex { !isJSWhitespace($0) } ?? scalars.endIndex)...]
    }

    static func trimmed(_ s: String) -> String {
        let start = trimmedStart(s)
        guard let last = start.lastIndex(where: { !isJSWhitespace($0) }) else { return "" }
        return String(String.UnicodeScalarView(start[...last]))
    }

    /// The Content-Type a raw body gets when the request sets none: JSON,
    /// XML or HTML by its first character, else plain text; nil when blank.
    public static func rawContentType(_ body: String) -> String? {
        let text = Array(String(String.UnicodeScalarView(trimmedStart(body))).utf8)
        guard let first = text.first else { return nil }
        if first == UInt8(ascii: "{") || first == UInt8(ascii: "[") { return "application/json" }
        func starts(_ p: String) -> Bool {
            let p = Array(p.utf8)
            return text.count >= p.count && zip(text, p).allSatisfy { lower($0) == $1 }
        }
        if starts("<!doctype html") || starts("<html") { return "text/html" }
        if first == UInt8(ascii: "<") { return "application/xml" }
        return "text/plain"
    }

    /// Adds the raw body's Content-Type when the method sends a body and no
    /// Content-Type header has a value. Never saved to the collection.
    public static func addContentType(method: String, headers: inout [(String, String)], body: String) {
        let m = method.trimmingCharacters(in: .whitespaces).uppercased()
        guard bodyMethods.contains(m) else { return }
        if headers.contains(where: { $0.0.lowercased() == "content-type" && !$0.1.isEmpty }) { return }
        if let t = rawContentType(body) { headers.append(("Content-Type", t)) }
    }

    static func lower(_ b: UInt8) -> UInt8 { (0x41...0x5A).contains(b) ? b + 32 : b }
    static func isDigit(_ b: UInt8) -> Bool { (0x30...0x39).contains(b) }
    static func isAlpha(_ b: UInt8) -> Bool { (0x41...0x5A).contains(b) || (0x61...0x7A).contains(b) }
    static func isWord(_ b: UInt8) -> Bool { isAlpha(b) || isDigit(b) || b == UInt8(ascii: "_") || b == UInt8(ascii: "-") }

    static func at(_ s: [UInt8], _ i: Int) -> UInt8? { i < s.count ? s[i] : nil }

    /// `[\w-]+(\.[\w-]+)*` (ASCII) at the start: the length matched.
    static func hostLength(_ s: [UInt8]) -> Int {
        var i = 0
        while true {
            let start = i
            while i < s.count && isWord(s[i]) { i += 1 }
            if i == start { return max(start - 1, 0) }
            if at(s, i) == UInt8(ascii: ".") { i += 1 } else { return i }
        }
    }

    static func digits(_ s: [UInt8], from i: Int) -> Int {
        var n = 0
        while i + n < s.count && isDigit(s[i + n]) { n += 1 }
        return n
    }

    static func hasScheme(_ s: [UInt8]) -> Bool {
        guard let f = s.first, isAlpha(f) else { return false }
        var n = 1
        while n < s.count && (isAlpha(s[n]) || isDigit(s[n]) || [UInt8(ascii: "+"), UInt8(ascii: "."), UInt8(ascii: "-")].contains(s[n])) { n += 1 }
        guard at(s, n) == UInt8(ascii: ":") else { return false }
        // `host:8080` is a host and a port, not a scheme.
        let h = hostLength(s)
        return !(h > 0 && at(s, h) == UInt8(ascii: ":") && at(s, h + 1).map(isDigit) == true)
    }

    static func isLocal(_ s: [UInt8]) -> Bool {
        func prefix(_ p: String) -> Bool { let p = Array(p.utf8); return s.count >= p.count && Array(s[0..<p.count]) == p }
        var host: Int
        if s.count >= 9 && zip(s, Array("localhost".utf8)).allSatisfy({ lower($0) == $1 }) {
            host = 9
        } else if prefix("0.0.0.0") {
            host = 7
        } else if prefix("[::1]") {
            host = 5
        } else if prefix("127.") {
            var i = 4
            for part in 0..<3 {
                let d = digits(s, from: i)
                if d == 0 { return false }
                i += d
                if part < 2 {
                    if at(s, i) != UInt8(ascii: ".") { return false }
                    i += 1
                }
            }
            host = i
        } else {
            return false
        }
        func end(_ i: Int) -> Bool { i == s.count || [UInt8(ascii: "/"), UInt8(ascii: "?"), UInt8(ascii: "#")].contains(s[i]) }
        var i = host
        if at(s, i) == UInt8(ascii: ":") && digits(s, from: i + 1) > 0 { i += 1 + digits(s, from: i + 1) }
        return end(i) || end(host)
    }

    static func isHostLike(_ text: String) -> Bool {
        let s = Array(text.utf8)
        let h = hostLength(s)
        if h == 0 { return false }
        var i = h
        if at(s, i) == UInt8(ascii: ":") && digits(s, from: i + 1) > 0 { i += 1 + digits(s, from: i + 1) }
        let rest = String(decoding: s[i...], as: UTF8.self)
        let shape = rest.isEmpty || ("/?#".contains(rest.first!) && !rest.unicodeScalars.contains(where: isJSWhitespace))
        var first = 0
        while first < s.count && isWord(s[first]) { first += 1 }
        let dotted = at(s, first) == UInt8(ascii: ".") && at(s, first + 1).map(isWord) == true
        let ported = at(s, first) == UInt8(ascii: ":") && at(s, first + 1).map(isDigit) == true
        return shape && (dotted || ported)
    }

    public struct NotAURL: Error, CustomStringConvertible, Equatable {
        public let url: String
        public var description: String { "\"\(url)\" isn't a URL. Enter a full URL, such as https://api.example.com/users" }
    }

    /// A URL as typed, with a scheme: `api.example.com/users` gets
    /// `https://`, `localhost:3000` gets `http://`; anything else that isn't
    /// a URL throws.
    public static func withScheme(_ url: String) throws -> String {
        let text = trimmed(url)
        let s = Array(text.utf8)
        if hasScheme(s) { return text }
        if isLocal(s) { return "http://\(text)" }
        if isHostLike(text) { return "https://\(text)" }
        throw NotAURL(url: text)
    }
}
