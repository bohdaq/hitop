import Foundation
import HitopKit

/// A JSON body as lines with fold points, for the response viewer: keys in the server's order,
/// two-space indent, a folded object or array drawn as `{ 3 keys }` / `[ 2 items ]`.
enum JSONOutline {
    enum Role: Hashable, Sendable { case punct, key, string, number, literal, summary }

    struct Token: Hashable, Sendable {
        var text: String
        var role: Role
    }

    struct Line: Hashable, Sendable, Identifiable {
        var id: Int
        var depth: Int
        /// The fold point's path ("$", "$.items", "$.items[0]"), when the line opens one.
        var fold: String?
        var folded: Bool
        var tokens: [Token]

        var text: String { String(repeating: "  ", count: depth) + tokens.map(\.text).joined() }
    }

    /// The parsed body, or nil when it isn't JSON.
    static func parse(_ text: String) -> JSONValue? {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard t.hasPrefix("{") || t.hasPrefix("[") else { return nil }
        return try? JSONValue.parse(t)
    }

    static func lines(_ value: JSONValue, folded: Set<String>) -> [Line] {
        var out: [Line] = []
        func add(_ depth: Int, _ tokens: [Token], fold: String? = nil, isFolded: Bool = false) {
            out.append(Line(id: out.count, depth: depth, fold: fold, folded: isFolded, tokens: tokens))
        }
        func scalar(_ v: JSONValue) -> Token? {
            switch v {
            case .null: Token(text: "null", role: .literal)
            case let .bool(b): Token(text: b ? "true" : "false", role: .literal)
            case let .number(n): Token(text: n, role: .number)
            case let .string(s): Token(text: JSONValue.quote(s), role: .string)
            default: nil
            }
        }
        func walk(_ v: JSONValue, key: String?, path: String, depth: Int, comma: Bool) {
            var lead: [Token] = []
            if let key { lead = [Token(text: JSONValue.quote(key), role: .key), Token(text: ": ", role: .punct)] }
            let tail: [Token] = comma ? [Token(text: ",", role: .punct)] : []
            if let s = scalar(v) {
                add(depth, lead + [s] + tail)
                return
            }
            let isObject: Bool
            let count: Int
            switch v {
            case let .object(o): isObject = true; count = o.count
            case let .array(a): isObject = false; count = a.count
            default: return
            }
            let (open, close) = isObject ? ("{", "}") : ("[", "]")
            if path != "$", let inline = inlineTokens(v), (lead + inline + tail).map(\.text).joined().count + depth * 2 <= inlineWidth {
                add(depth, lead + inline + tail)
                return
            }
            if count == 0 {
                add(depth, lead + [Token(text: open + close, role: .punct)] + tail)
                return
            }
            if folded.contains(path) {
                let what = isObject ? (count == 1 ? "1 key" : "\(count) keys") : (count == 1 ? "1 item" : "\(count) items")
                add(depth, lead + [Token(text: open + " ", role: .punct), Token(text: what, role: .summary), Token(text: " " + close, role: .punct)] + tail,
                    fold: path, isFolded: true)
                return
            }
            add(depth, lead + [Token(text: open, role: .punct)], fold: path)
            switch v {
            case let .object(o):
                for (i, (k, child)) in o.enumerated() {
                    walk(child, key: k, path: "\(path).\(k)", depth: depth + 1, comma: i < o.count - 1)
                }
            case let .array(a):
                for (i, child) in a.enumerated() {
                    walk(child, key: nil, path: "\(path)[\(i)]", depth: depth + 1, comma: i < a.count - 1)
                }
            default: break
            }
            add(depth, [Token(text: close, role: .punct)] + tail)
        }
        walk(value, key: nil, path: "$", depth: 0, comma: false)
        return out
    }

    /// The parsed body for views that don't import HitopKit.
    static func box(_ text: String) -> JSONValueBox? { parse(text).map(JSONValueBox.init) }

    /// A small container of scalars as one line (`{ "amount": 6480, "currency": "EUR" }`), as
    /// the drafts show it; nil for anything else.
    /// Characters a one-line container may take (a phone sheet at 13.5 pt).
    static let inlineWidth = 44

    static func inlineTokens(_ v: JSONValue) -> [Token]? {
        func scalar(_ v: JSONValue) -> Token? {
            switch v {
            case .null: Token(text: "null", role: .literal)
            case let .bool(b): Token(text: b ? "true" : "false", role: .literal)
            case let .number(n): Token(text: n, role: .number)
            case let .string(s): Token(text: JSONValue.quote(s), role: .string)
            default: nil
            }
        }
        var out: [Token] = []
        switch v {
        case let .object(o) where !o.isEmpty && o.count <= 3:
            out.append(Token(text: "{ ", role: .punct))
            for (i, (k, child)) in o.enumerated() {
                guard let s = scalar(child) else { return nil }
                out += [Token(text: JSONValue.quote(k), role: .key), Token(text: ": ", role: .punct), s]
                out.append(Token(text: i < o.count - 1 ? ", " : " }", role: .punct))
            }
        case let .array(a) where !a.isEmpty && a.count <= 3:
            out.append(Token(text: "[", role: .punct))
            for (i, child) in a.enumerated() {
                guard let s = scalar(child) else { return nil }
                out.append(s)
                out.append(Token(text: i < a.count - 1 ? ", " : "]", role: .punct))
            }
        default: return nil
        }
        return out
    }

    /// What starts folded: in a long body, the top-level objects and arrays that don't fit on one
    /// line (the drafts' `▸ "items": [ 2 items ]`). A wrapper with one key (GraphQL's `"data"`,
    /// `{"result": …}`) is never folded itself: the rule applies to what it wraps, so the answer
    /// opens on its contents instead of `"data": { 1 key }`.
    static func defaultFolds(_ value: JSONValue) -> Set<String> {
        guard lines(value, folded: []).count > 12 else { return [] }
        return folds(value, path: "$", depth: 1)
    }

    private static func isContainer(_ v: JSONValue) -> Bool {
        switch v {
        case let .object(c): !c.isEmpty
        case let .array(c): !c.isEmpty
        default: false
        }
    }

    private static func folds(_ value: JSONValue, path: String, depth: Int) -> Set<String> {
        guard case let .object(o) = value else { return [] }
        if o.count == 1, let only = o.first, isContainer(only.1) {
            return folds(only.1, path: "\(path).\(only.0)", depth: depth + 1)
        }
        var out: Set<String> = []
        for (k, child) in o where isContainer(child) {
            let lead = JSONValue.quote(k) + ": "
            let fits = inlineTokens(child).map { (lead + $0.map(\.text).joined() + ",").count + depth * 2 <= inlineWidth } ?? false
            if !fits { out.insert("\(path).\(k)") }
        }
        return out
    }

    /// Pretty-printed JSON, or nil when the text isn't JSON.
    static func pretty(_ text: String) -> String? { parse(text)?.pretty }

    /// Whether a request body is valid JSON; `${var}` references count as values.
    static func validate(_ body: String) -> (valid: Bool, message: String?) {
        if body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return (true, nil) }
        let filled = body.replacingOccurrences(of: "\\$\\{[^}]*\\}", with: "0", options: .regularExpression)
        do {
            _ = try JSONValue.parse(filled)
            return (true, nil)
        } catch {
            return (false, "\(error)")
        }
    }

    /// The body pretty-printed, `${var}` references kept as they were (bare or inside strings).
    static func format(_ body: String) -> String? {
        var refs: [String] = []
        var inString = false
        var escaped = false
        var marked = ""
        for seg in VariableText.segments(body) {
            switch seg {
            case let .text(t):
                for ch in t {
                    if escaped { escaped = false } else if ch == "\\" && inString { escaped = true } else if ch == "\"" { inString.toggle() }
                }
                marked += t
            case let .variable(n):
                refs.append(n)
                let mark = "\u{E000}\(refs.count - 1)\u{E000}"
                marked += inString ? mark : "\"\(mark)\""
            }
        }
        guard let v = try? JSONValue.parse(marked) else { return nil }
        var out = v.pretty
        for (i, n) in refs.enumerated() {
            let form = "\u{E000}\(i)\u{E000}"
            out = out.replacingOccurrences(of: "\"\(form)\"", with: "${\(n)}")
            out = out.replacingOccurrences(of: form, with: "${\(n)}")
        }
        return out
    }
}

/// A parsed JSON body (HitopKit's `JSONValue`) for the views.
struct JSONValueBox: Sendable {
    let value: JSONValue

    func lines(folded: Set<String>) -> [JSONOutline.Line] { JSONOutline.lines(value, folded: folded) }
    var defaultFolds: Set<String> { JSONOutline.defaultFolds(value) }
}
