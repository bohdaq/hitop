import Foundation
import Yams

/// A YAML value read with the YAML 1.2 core schema (spec section 1): `on`,
/// `yes` and `no` are strings, only `true`/`false` (any case form the schema
/// lists) are booleans. Yams' default resolver is YAML 1.1, so hitop parses
/// with `Resolver.basic` (every scalar a string) and resolves plain scalars
/// itself here.
public indirect enum YAMLValue: Sendable {
    case null
    case bool(Bool)
    /// An integer or float, as hitop turns it into text (`8080`, `0x10` → `16`).
    case number(String)
    case string(String)
    case sequence([YAMLValue])
    case mapping([(String, YAMLValue)])

    /// The value at `key` of a mapping.
    subscript(key: String) -> YAMLValue? {
        if case let .mapping(pairs) = self { return pairs.last(where: { $0.0 == key })?.1 }
        return nil
    }

    var keys: [String] {
        if case let .mapping(pairs) = self { return pairs.map(\.0) }
        return []
    }

    /// Scalars as text, as the format reads a value where a string is
    /// expected: null → "", true → "true", numbers as written in decimal.
    func scalarString(_ what: @autoclosure () -> String) throws -> String {
        switch self {
        case .null: return ""
        case let .bool(b): return b ? "true" : "false"
        case let .number(n): return n
        case let .string(s): return s
        case .sequence, .mapping: throw FormatError("\(what()): expected a string, found a \(kindName)")
        }
    }

    var kindName: String {
        switch self {
        case .null: return "null"
        case .bool: return "boolean"
        case .number: return "number"
        case .string: return "string"
        case .sequence: return "list"
        case .mapping: return "map"
        }
    }

    /// Parses one YAML document; an empty document is `null`.
    static func parse(_ text: String) throws -> YAMLValue {
        guard let node = try Yams.compose(yaml: text, .basic) else { return .null }
        return try YAMLValue(node)
    }

    init(_ node: Node) throws {
        switch node {
        case let .scalar(s):
            // Quoted and block scalars keep the text as written. (Explicit
            // tags such as `!!str 8080` aren't honoured: Yams resolves a
            // node's tag lazily, so an implicit one can't be told apart.)
            self = s.style == .plain ? Self.resolvePlain(s.string) : .string(s.string)
        case let .sequence(seq):
            self = .sequence(try seq.map { try YAMLValue($0) })
        case let .mapping(map):
            var pairs: [(String, YAMLValue)] = []
            for (k, v) in map {
                let key: String
                switch try YAMLValue(k) {
                case .null: key = ""
                case let .bool(b): key = b ? "true" : "false"
                case let .number(n): key = n
                case let .string(s): key = s
                default: throw FormatError("a map key must be a string")
                }
                pairs.append((key, try YAMLValue(v)))
            }
            self = .mapping(pairs)
        case .alias:
            // Yams dereferences aliases while composing; nothing reaches here.
            throw FormatError("unexpected YAML alias")
        }
    }

    /// The YAML 1.2 core schema for a plain (unquoted) scalar.
    static func resolvePlain(_ s: String) -> YAMLValue {
        switch s {
        case "", "~", "null", "Null", "NULL": return .null
        case "true", "True", "TRUE": return .bool(true)
        case "false", "False", "FALSE": return .bool(false)
        case ".inf", ".Inf", ".INF", "+.inf", "+.Inf", "+.INF": return .number("inf")
        case "-.inf", "-.Inf", "-.INF": return .number("-inf")
        case ".nan", ".NaN", ".NAN": return .number("NaN")
        default: break
        }
        let u = Array(s.utf8)
        func all(_ from: Int, _ ok: (UInt8) -> Bool) -> Bool { from < u.count && u[from...].allSatisfy(ok) }
        let isDigit: (UInt8) -> Bool = { $0 >= 0x30 && $0 <= 0x39 }
        // [-+]?[0-9]+
        let signed = u.first == UInt8(ascii: "-") || u.first == UInt8(ascii: "+")
        if all(signed ? 1 : 0, isDigit) {
            return .number(decimal(String(s.drop(while: { $0 == "+" })), radix: 10) ?? s)
        }
        if s.hasPrefix("0o"), all(2, { $0 >= 0x30 && $0 <= 0x37 }) {
            return .number(decimal(String(s.dropFirst(2)), radix: 8) ?? s)
        }
        if s.hasPrefix("0x"), all(2, { isDigit($0) || ($0 >= 0x41 && $0 <= 0x46) || ($0 >= 0x61 && $0 <= 0x66) }) {
            return .number(decimal(String(s.dropFirst(2)), radix: 16) ?? s)
        }
        if isFloat(u), let d = Double(s) {
            return .number(floatText(d))
        }
        return .string(s)
    }

    /// An integer in decimal without leading zeros; nil when it doesn't fit 64 bits.
    private static func decimal(_ digits: String, radix: Int) -> String? {
        if radix == 10 {
            let negative = digits.hasPrefix("-")
            let body = negative ? String(digits.dropFirst()) : digits
            let trimmed = String(body.drop(while: { $0 == "0" }))
            let text = trimmed.isEmpty ? "0" : trimmed
            if Int64(text) == nil && UInt64(text) == nil { return nil }
            return negative && text != "0" ? "-" + text : text
        }
        return UInt64(digits, radix: radix).map { String($0) }
    }

    /// `[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`
    private static func isFloat(_ u: [UInt8]) -> Bool {
        var i = 0
        let digit: (UInt8) -> Bool = { $0 >= 0x30 && $0 <= 0x39 }
        if i < u.count, u[i] == UInt8(ascii: "-") || u[i] == UInt8(ascii: "+") { i += 1 }
        var intDigits = 0
        while i < u.count, digit(u[i]) { i += 1; intDigits += 1 }
        var fracDigits = 0
        if i < u.count, u[i] == UInt8(ascii: ".") {
            i += 1
            while i < u.count, digit(u[i]) { i += 1; fracDigits += 1 }
        }
        if intDigits == 0 && fracDigits == 0 { return false }
        if i < u.count, u[i] == UInt8(ascii: "e") || u[i] == UInt8(ascii: "E") {
            i += 1
            if i < u.count, u[i] == UInt8(ascii: "-") || u[i] == UInt8(ascii: "+") { i += 1 }
            var e = 0
            while i < u.count, digit(u[i]) { i += 1; e += 1 }
            if e == 0 { return false }
        }
        return i == u.count
    }

    /// A float as serde_json writes one: `1.5`, `1000.0`, `1e100`.
    private static func floatText(_ d: Double) -> String {
        var t = "\(d)"
        t = t.replacingOccurrences(of: "e+", with: "e")
        return t
    }
}
