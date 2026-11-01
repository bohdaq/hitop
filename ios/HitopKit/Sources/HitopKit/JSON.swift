import Foundation

/// A JSON value with object keys in order. hitop writes and reads JSON in a
/// few places (script inputs and outputs, GraphQL bodies, `report.json`),
/// and the text has to match what the Rust core writes (`serde_json`): no
/// escaped `/`, non-ASCII left as is, `null` kept. Foundation's
/// `JSONSerialization` differs between Apple platforms and Linux on exactly
/// those points, so hitop has its own small reader and writer.
public indirect enum JSONValue: Equatable, Sendable {
    case null
    case bool(Bool)
    /// The number as written (`7`, `1.5`, `-2e3`).
    case number(String)
    case string(String)
    case array([JSONValue])
    case object([(String, JSONValue)])

    public static func == (a: JSONValue, b: JSONValue) -> Bool {
        switch (a, b) {
        case (.null, .null): return true
        case let (.bool(x), .bool(y)): return x == y
        case let (.number(x), .number(y)): return x == y || (Double(x) != nil && Double(x) == Double(y))
        case let (.string(x), .string(y)): return x == y
        case let (.array(x), .array(y)): return x == y
        case let (.object(x), .object(y)):
            return x.count == y.count && x.allSatisfy { k, v in y.first(where: { $0.0 == k })?.1 == v }
        default: return false
        }
    }

    /// The value at `key` of an object.
    public subscript(key: String) -> JSONValue? {
        if case let .object(pairs) = self { return pairs.last(where: { $0.0 == key })?.1 }
        return nil
    }

    public var stringValue: String? {
        if case let .string(s) = self { return s }
        return nil
    }

    public var arrayValue: [JSONValue]? {
        if case let .array(a) = self { return a }
        return nil
    }

    public var objectValue: [(String, JSONValue)]? {
        if case let .object(o) = self { return o }
        return nil
    }

    /// A string map as a JSON object, keys sorted (the order doesn't matter
    /// to scripts; sorted keeps the text stable).
    public static func strings(_ map: [String: String]) -> JSONValue {
        .object(map.keys.sorted().map { ($0, .string(map[$0]!)) })
    }
}

// MARK: - Writing

extension JSONValue {
    /// Compact JSON, as `serde_json::to_string`.
    public var compact: String {
        var out = ""
        write(into: &out, indent: nil, level: 0)
        return out
    }

    /// Two-space indented JSON, as `serde_json::to_string_pretty`.
    public var pretty: String {
        var out = ""
        write(into: &out, indent: "  ", level: 0)
        return out
    }

    private func write(into out: inout String, indent: String?, level: Int) {
        switch self {
        case .null: out += "null"
        case let .bool(b): out += b ? "true" : "false"
        case let .number(n): out += n
        case let .string(s): out += JSONValue.quote(s)
        case let .array(items):
            if items.isEmpty { out += "[]"; return }
            out += "["
            for (i, item) in items.enumerated() {
                if i > 0 { out += "," }
                if let indent { out += "\n" + String(repeating: indent, count: level + 1) }
                item.write(into: &out, indent: indent, level: level + 1)
            }
            if let indent { out += "\n" + String(repeating: indent, count: level) }
            out += "]"
        case let .object(pairs):
            if pairs.isEmpty { out += "{}"; return }
            out += "{"
            for (i, (k, v)) in pairs.enumerated() {
                if i > 0 { out += "," }
                if let indent { out += "\n" + String(repeating: indent, count: level + 1) }
                out += JSONValue.quote(k)
                out += indent == nil ? ":" : ": "
                v.write(into: &out, indent: indent, level: level + 1)
            }
            if let indent { out += "\n" + String(repeating: indent, count: level) }
            out += "}"
        }
    }

    /// A JSON string literal, escaped as serde_json escapes it.
    public static func quote(_ s: String) -> String {
        var out = "\""
        out.reserveCapacity(s.utf8.count + 2)
        for u in s.unicodeScalars {
            switch u {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            case "\u{08}": out += "\\b"
            case "\u{0C}": out += "\\f"
            default:
                if u.value < 0x20 {
                    out += String(format: "\\u%04x", u.value)
                } else {
                    out.unicodeScalars.append(u)
                }
            }
        }
        out += "\""
        return out
    }
}

// MARK: - Reading

public struct JSONError: Error, CustomStringConvertible, Sendable {
    public let description: String
}

extension JSONValue {
    /// Parses JSON text (RFC 8259; surrounding whitespace allowed).
    public static func parse(_ text: String) throws -> JSONValue {
        var p = JSONParser(Array(text.utf8))
        p.skipSpace()
        let v = try p.value(depth: 0)
        p.skipSpace()
        guard p.atEnd else { throw p.error("trailing characters") }
        return v
    }
}

private struct JSONParser {
    let b: [UInt8]
    var i = 0

    init(_ bytes: [UInt8]) { b = bytes }

    var atEnd: Bool { i >= b.count }

    func error(_ what: String) -> JSONError {
        JSONError(description: "\(what) at byte \(i)")
    }

    mutating func skipSpace() {
        while i < b.count, b[i] == 0x20 || b[i] == 0x0A || b[i] == 0x0D || b[i] == 0x09 { i += 1 }
    }

    mutating func literal(_ word: String) -> Bool {
        let w = Array(word.utf8)
        guard i + w.count <= b.count, Array(b[i..<i + w.count]) == w else { return false }
        i += w.count
        return true
    }

    mutating func value(depth: Int) throws -> JSONValue {
        guard depth < 512 else { throw error("nested too deeply") }
        guard i < b.count else { throw error("unexpected end") }
        switch b[i] {
        case UInt8(ascii: "{"):
            i += 1
            var pairs: [(String, JSONValue)] = []
            skipSpace()
            if i < b.count, b[i] == UInt8(ascii: "}") { i += 1; return .object(pairs) }
            while true {
                skipSpace()
                guard i < b.count, b[i] == UInt8(ascii: "\"") else { throw error("expected a key") }
                let k = try string()
                skipSpace()
                guard i < b.count, b[i] == UInt8(ascii: ":") else { throw error("expected ':'") }
                i += 1
                skipSpace()
                pairs.append((k, try value(depth: depth + 1)))
                skipSpace()
                guard i < b.count else { throw error("unexpected end") }
                if b[i] == UInt8(ascii: ",") { i += 1; continue }
                if b[i] == UInt8(ascii: "}") { i += 1; return .object(pairs) }
                throw error("expected ',' or '}'")
            }
        case UInt8(ascii: "["):
            i += 1
            var items: [JSONValue] = []
            skipSpace()
            if i < b.count, b[i] == UInt8(ascii: "]") { i += 1; return .array(items) }
            while true {
                skipSpace()
                items.append(try value(depth: depth + 1))
                skipSpace()
                guard i < b.count else { throw error("unexpected end") }
                if b[i] == UInt8(ascii: ",") { i += 1; continue }
                if b[i] == UInt8(ascii: "]") { i += 1; return .array(items) }
                throw error("expected ',' or ']'")
            }
        case UInt8(ascii: "\""):
            return .string(try string())
        case UInt8(ascii: "t"):
            if literal("true") { return .bool(true) }
        case UInt8(ascii: "f"):
            if literal("false") { return .bool(false) }
        case UInt8(ascii: "n"):
            if literal("null") { return .null }
        default:
            return .number(try number())
        }
        throw error("unexpected character")
    }

    mutating func number() throws -> String {
        let start = i
        func digits() -> Int {
            let s = i
            while i < b.count, b[i] >= 0x30, b[i] <= 0x39 { i += 1 }
            return i - s
        }
        if i < b.count, b[i] == UInt8(ascii: "-") { i += 1 }
        guard i < b.count else { throw error("expected a value") }
        if b[i] == UInt8(ascii: "0") {
            i += 1
        } else if digits() == 0 {
            throw error("expected a value")
        }
        if i < b.count, b[i] == UInt8(ascii: ".") {
            i += 1
            guard digits() > 0 else { throw error("expected digits") }
        }
        if i < b.count, b[i] == UInt8(ascii: "e") || b[i] == UInt8(ascii: "E") {
            i += 1
            if i < b.count, b[i] == UInt8(ascii: "+") || b[i] == UInt8(ascii: "-") { i += 1 }
            guard digits() > 0 else { throw error("expected digits") }
        }
        return String(decoding: b[start..<i], as: UTF8.self)
    }

    mutating func hex4() throws -> UInt32 {
        guard i + 4 <= b.count else { throw error("bad \\u escape") }
        var v: UInt32 = 0
        for _ in 0..<4 {
            let c = b[i]
            let d: UInt8
            switch c {
            case 0x30...0x39: d = c - 0x30
            case 0x41...0x46: d = c - 0x41 + 10
            case 0x61...0x66: d = c - 0x61 + 10
            default: throw error("bad \\u escape")
            }
            v = v * 16 + UInt32(d)
            i += 1
        }
        return v
    }

    mutating func string() throws -> String {
        i += 1 // opening quote
        var bytes: [UInt8] = []
        while true {
            guard i < b.count else { throw error("unterminated string") }
            let c = b[i]
            if c == UInt8(ascii: "\"") { i += 1; break }
            if c < 0x20 { throw error("control character in string") }
            if c != UInt8(ascii: "\\") { bytes.append(c); i += 1; continue }
            i += 1
            guard i < b.count else { throw error("unterminated string") }
            let e = b[i]
            i += 1
            switch e {
            case UInt8(ascii: "\""): bytes.append(0x22)
            case UInt8(ascii: "\\"): bytes.append(0x5C)
            case UInt8(ascii: "/"): bytes.append(0x2F)
            case UInt8(ascii: "b"): bytes.append(0x08)
            case UInt8(ascii: "f"): bytes.append(0x0C)
            case UInt8(ascii: "n"): bytes.append(0x0A)
            case UInt8(ascii: "r"): bytes.append(0x0D)
            case UInt8(ascii: "t"): bytes.append(0x09)
            case UInt8(ascii: "u"):
                var code = try hex4()
                if (0xD800..<0xDC00).contains(code), i + 1 < b.count, b[i] == UInt8(ascii: "\\"), b[i + 1] == UInt8(ascii: "u") {
                    let save = i
                    i += 2
                    let low = try hex4()
                    if (0xDC00..<0xE000).contains(low) {
                        code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00)
                    } else {
                        i = save
                    }
                }
                let scalar = Unicode.Scalar(code) ?? "\u{FFFD}"
                bytes.append(contentsOf: Array(String(Character(scalar)).utf8))
            default:
                throw error("bad escape")
            }
        }
        return String(decoding: bytes, as: UTF8.self)
    }
}
