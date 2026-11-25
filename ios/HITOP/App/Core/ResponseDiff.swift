import Foundation
import HitopKit

/// A response to compare: the current one, a saved example, or a history entry's.
struct ComparedResponse: Hashable, Sendable, Identifiable {
    var id: String
    var label: String
    var status: Int?
    /// Lower-case names.
    var headers: [String: String]
    var body: String

    private static func headers(_ rows: [[String]]) -> [String: String] {
        Dictionary(rows.filter { $0.count == 2 }.map { ($0[0].lowercased(), $0[1]) }, uniquingKeysWith: { _, b in b })
    }

    init(id: String, label: String, status: Int?, headers: [String: String], body: String) {
        self.id = id
        self.label = label
        self.status = status
        self.headers = headers
        self.body = body
    }

    init(current o: SendOutcome) {
        self.init(id: "current", label: "This response", status: o.status, headers: Self.headers(o.headers), body: o.text)
    }

    init(history e: HistoryEntry, _ r: ResponseSnapshot) {
        self.init(id: "h\(e.id)", label: "Sent \(e.date.formatted(date: .abbreviated, time: .standard))", status: r.status, headers: Self.headers(r.headers), body: r.body)
    }

    /// A request's saved example (free-form: headers as rows or as a map, a body of any JSON type).
    init(example e: JSONAny, index: Int) {
        let name = e["name"]?.string ?? ""
        var h: [String: String] = [:]
        if let rows = e["headers"]?.array {
            for r in rows { if let n = r["name"]?.string ?? r["key"]?.string { h[n.lowercased()] = r["value"]?.string ?? "" } }
        } else if case let .object(o)? = e["headers"] {
            for (k, v) in o { h[k.lowercased()] = v.string ?? ResponseDiff.text(v) }
        }
        let status: Int? = switch e["status"] {
        case let .number(n)?: Int(exactly: n)
        case let .string(s)?: Int(s)
        default: nil
        }
        let body: String = switch e["body"] {
        case nil, .null?: ""
        case let .string(s)?: s
        case let v?: ResponseDiff.text(v)
        }
        self.init(id: "e\(index)", label: "Example: \(name.isEmpty ? "\(index + 1)" : name)", status: status, headers: h, body: body)
    }

    /// What a request's response can be compared with: the last one, its examples, its history.
    static func sources(_ r: SavedRequest, outcome: SendOutcome?, history: [HistoryEntry]) -> [ComparedResponse] {
        var out: [ComparedResponse] = []
        if let o = outcome, !o.isImage { out.append(ComparedResponse(current: o)) }
        out += (r.extras["examples"]?.array ?? []).enumerated().map { ComparedResponse(example: $0.element, index: $0.offset) }
        // The newest entry of a request with a response on screen is that response.
        var mine = history.filter { $0.request.id == r.id && $0.response != nil }
        if outcome != nil, !mine.isEmpty { mine.removeFirst() }
        out += mine.prefix(20).map { ComparedResponse(history: $0, $0.response!) }
        return out
    }
}

/// The web app's response diff (src/diff.js): JSON by path (key order ignored), other bodies by
/// line, headers by name.
enum ResponseDiff {
    enum Kind: Hashable, Sendable { case added, removed, changed, same }

    struct Change: Hashable, Sendable {
        var kind: Kind
        var path = ""
        var before: String?
        var after: String?
    }

    struct Result: Hashable, Sendable {
        var status: Change?
        var headers: [Change]
        var body: [Change]
        var json: Bool
        var same: Bool { status == nil && headers.isEmpty && !body.contains { $0.kind != .same } }
    }

    /// Headers that differ on every response.
    private static let volatile: Set<String> = ["date", "age", "expires", "x-request-id"]

    static func text(_ v: JSONAny) -> String {
        let enc = JSONEncoder()
        enc.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return (try? enc.encode(v)).map { String(decoding: $0, as: UTF8.self) } ?? ""
    }

    private static func text(_ v: JSONValue) -> String {
        switch v {
        case .null: "null"
        case let .bool(b): b ? "true" : "false"
        case let .number(n): n
        case let .string(s): JSONValue.quote(s)
        case let .array(a): "[" + a.map { text($0) }.joined(separator: ",") + "]"
        case let .object(o): "{" + o.map { "\(JSONValue.quote($0.0)):\(text($0.1))" }.joined(separator: ",") + "}"
        }
    }

    private static func child(_ path: String, _ key: String) -> String {
        key.range(of: "^[A-Za-z_$][A-Za-z0-9_$]*$", options: .regularExpression) != nil ? "\(path).\(key)" : "\(path)[\(JSONValue.quote(key))]"
    }

    /// `1`, `1.0` and `1e0` are one number; long ones are compared as written.
    private static func sameNumber(_ a: String, _ b: String) -> Bool {
        if a == b { return true }
        guard a.count <= 30, b.count <= 30, let x = Decimal(string: a), let y = Decimal(string: b) else { return false }
        return x == y
    }

    private static func json(_ a: JSONValue, _ b: JSONValue, _ path: String, _ out: inout [Change]) {
        switch (a, b) {
        case let (.object(x), .object(y)):
            var l: [String: JSONValue] = [:], r: [String: JSONValue] = [:]
            for (k, v) in x { l[k] = v }
            for (k, v) in y { r[k] = v }
            for k in Set(l.keys).union(r.keys).sorted() {
                switch (l[k], r[k]) {
                case let (v?, nil): out.append(Change(kind: .removed, path: child(path, k), before: text(v)))
                case let (nil, v?): out.append(Change(kind: .added, path: child(path, k), after: text(v)))
                case let (v?, w?): json(v, w, child(path, k), &out)
                default: break
                }
            }
        case let (.array(x), .array(y)):
            for i in 0..<max(x.count, y.count) {
                if i >= y.count { out.append(Change(kind: .removed, path: "\(path)[\(i)]", before: text(x[i]))) }
                else if i >= x.count { out.append(Change(kind: .added, path: "\(path)[\(i)]", after: text(y[i]))) }
                else { json(x[i], y[i], "\(path)[\(i)]", &out) }
            }
        case let (.number(x), .number(y)):
            if !sameNumber(x, y) { out.append(Change(kind: .changed, path: path, before: x, after: y)) }
        default:
            let (x, y) = (text(a), text(b))
            if x != y { out.append(Change(kind: .changed, path: path, before: x, after: y)) }
        }
    }

    /// By line (longest common subsequence); past `limit` cells the middle is removed, then added.
    static func lines(_ a: String, _ b: String, limit: Int = 1_000_000) -> [Change] {
        func split(_ s: String) -> [String] { s.isEmpty ? [] : s.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n") }
        func same(_ t: String) -> Change { Change(kind: .same, before: t, after: t) }
        func removed(_ t: String) -> Change { Change(kind: .removed, before: t) }
        func added(_ t: String) -> Change { Change(kind: .added, after: t) }
        let x = split(a), y = split(b)
        var start = 0
        while start < x.count, start < y.count, x[start] == y[start] { start += 1 }
        var ex = x.count, ey = y.count
        while ex > start, ey > start, x[ex - 1] == y[ey - 1] { ex -= 1; ey -= 1 }
        let mx = Array(x[start..<ex]), my = Array(y[start..<ey])
        var mid: [Change] = []
        if mx.count * my.count > limit {
            mid = mx.map(removed) + my.map(added)
        } else {
            let w = my.count + 1
            var t = [Int32](repeating: 0, count: (mx.count + 1) * w)
            for i in mx.indices.reversed() {
                for j in my.indices.reversed() {
                    t[i * w + j] = mx[i] == my[j] ? t[(i + 1) * w + j + 1] + 1 : max(t[(i + 1) * w + j], t[i * w + j + 1])
                }
            }
            var i = 0, j = 0
            while i < mx.count, j < my.count {
                if mx[i] == my[j] { mid.append(same(mx[i])); i += 1; j += 1 }
                else if t[(i + 1) * w + j] >= t[i * w + j + 1] { mid.append(removed(mx[i])); i += 1 }
                else { mid.append(added(my[j])); j += 1 }
            }
            mid += mx[i...].map(removed) + my[j...].map(added)
        }
        return x[..<start].map(same) + mid + x[ex...].map(same)
    }

    static func compare(_ a: ComparedResponse, _ b: ComparedResponse) -> Result {
        let headers: [Change] = Set(a.headers.keys).union(b.headers.keys).subtracting(volatile).sorted().compactMap { k in
            switch (a.headers[k], b.headers[k]) {
            case let (x?, nil): Change(kind: .removed, path: k, before: x)
            case let (nil, y?): Change(kind: .added, path: k, after: y)
            case let (x?, y?): x == y ? nil : Change(kind: .changed, path: k, before: x, after: y)
            default: nil
            }
        }
        let status = a.status == b.status ? nil : Change(kind: .changed, path: "status", before: a.status.map(String.init) ?? "none", after: b.status.map(String.init) ?? "none")
        if let x = JSONOutline.parse(a.body), let y = JSONOutline.parse(b.body) {
            var out: [Change] = []
            json(x, y, "$", &out)
            return Result(status: status, headers: headers, body: out, json: true)
        }
        return Result(status: status, headers: headers, body: lines(a.body, b.body), json: false)
    }
}
