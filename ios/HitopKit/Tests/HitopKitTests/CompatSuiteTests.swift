import Foundation
import XCTest
@testable import HitopKit

/// The format v1 compatibility suite (`spec/compat/v1`): every case folder
/// must read as its `expected.json`, for the parts of the JSON model HitopKit
/// keeps (it drops what only editors need: unknown keys, examples, OpenAPI
/// records, WebSocket and gRPC details, `file`).
final class CompatSuiteTests: XCTestCase {
    let suite = repoRoot.appendingPathComponent("spec/compat/v1")

    static let requestKeys = ["id", "name", "url", "method", "headers", "body", "preRequestScript", "postRequestScript", "auth",
                              "bodyMode", "formFields", "disabledParams", "bodyType", "graphql", "kind"]

    /// The parts HitopKit models, with defaults written out so both sides compare.
    func project(request r: JSONValue) -> JSONValue {
        var out: [(String, JSONValue)] = []
        for k in Self.requestKeys {
            var v = r[k]
            switch k {
            case "headers":
                v = .array((v?.arrayValue ?? []).map { h in
                    .object([("name", h["name"] ?? .string("")), ("value", h["value"] ?? .string("")), ("enabled", h["enabled"] ?? .bool(true))])
                })
            case "formFields":
                v = .array((v?.arrayValue ?? []).map { f in
                    .object([("key", f["key"] ?? .string("")), ("value", f["value"] ?? .string("")), ("type", f["type"] ?? .string("text")),
                             ("enabled", f["enabled"] ?? .bool(true))])
                })
            case "disabledParams":
                v = .array((v?.arrayValue ?? []).map { p in .object([("key", p["key"] ?? .string("")), ("value", p["value"] ?? .string(""))]) })
            case "graphql":
                v = v.map { g in .object([("variables", g["variables"] ?? .string("")), ("operationName", g["operationName"] ?? .string(""))]) }
            case "auth":
                if let a = v, a["type"]?.stringValue == "apikey" {
                    v = .object([("type", .string("apikey")), ("key", a["key"] ?? .string("")), ("value", a["value"] ?? .string("")), ("in", a["in"] ?? .string("header"))])
                }
            case "kind":
                if v?.stringValue == "http" { v = nil }
            default: break
            }
            if let v { out.append((k, v)) }
        }
        return .object(out)
    }

    func project(environment e: JSONValue?) -> JSONValue {
        guard let e, e != .null else { return .null }
        return .object([("id", e["id"] ?? .null), ("name", e["name"] ?? .null), ("variables", e["variables"] ?? .object([])),
                        ("secrets", e["secrets"] ?? .array([]))])
    }

    func project(_ expected: JSONValue) -> JSONValue {
        let c = expected["collection"] ?? .null
        return .object([
            ("collection", .object([
                ("id", c["id"] ?? .null), ("name", c["name"] ?? .null), ("variables", c["variables"] ?? .object([])),
                ("workspace", c["workspace"] ?? .null), ("auth", c["auth"] ?? .null),
                ("requests", .array((c["requests"]?.arrayValue ?? []).map { project(request: $0) })),
            ])),
            ("environments", .array((expected["environments"]?.arrayValue ?? []).map { project(environment: $0) })),
            ("globals", project(environment: expected["globals"])),
        ])
    }

    /// HitopKit's model as JSON, through Codable (the model's JSON keys).
    func model(_ f: CollectionFolder) throws -> JSONValue {
        let e = JSONEncoder()
        e.outputFormatting = [.sortedKeys]
        func json<T: Encodable>(_ v: T) throws -> JSONValue { try JSONValue.parse(String(decoding: try e.encode(v), as: UTF8.self)) }
        var c = try json(f.collection)
        // Codable leaves nil out; the projection wants null.
        if c["workspace"] == nil, case var .object(pairs) = c { pairs.append(("workspace", .null)); c = .object(pairs) }
        if c["auth"] == nil, case var .object(pairs) = c { pairs.append(("auth", .null)); c = .object(pairs) }
        let env = try f.environments.map { try json($0) }
        return project(.object([("collection", c), ("environments", .array(env)), ("globals", try f.globals.map(json) ?? .null)]))
    }

    func testEveryCaseReadsAsExpected() throws {
        let cases = try FileManager.default.contentsOfDirectory(atPath: suite.path)
            .filter { FileManager.default.fileExists(atPath: suite.appendingPathComponent("\($0)/folder").path) }
            .sorted()
        XCTAssertGreaterThanOrEqual(cases.count, 10, "spec/compat/v1 cases: \(cases)")
        var compared = 0
        for name in cases {
            let dir = suite.appendingPathComponent(name)
            let expected = try JSONValue.parse(try String(contentsOf: dir.appendingPathComponent("expected.json"), encoding: .utf8))
            let folder: CollectionFolder
            do { folder = try CollectionFolder.read(dir.appendingPathComponent("folder")) } catch {
                XCTFail("\(name): \(error)")
                continue
            }
            let want = project(expected)
            let got = try model(folder)
            compared += want["collection"]?["requests"]?.arrayValue?.count ?? 0
            if got != want {
                // Name the first request that differs, for a readable failure.
                let w = want["collection"]?["requests"]?.arrayValue ?? []
                let g = got["collection"]?["requests"]?.arrayValue ?? []
                let diff = zip(w, g).first { $0 != $1 }.map { "\nwant \($0.0.compact)\ngot  \($0.1.compact)" } ?? ""
                XCTFail("\(name) differs:\(diff)\nwant \(want.compact)\ngot  \(got.compact)")
            }
        }
        XCTAssertGreaterThanOrEqual(compared, 20, "requests compared")
        // The comparison isn't vacuous: a changed field is caught.
        let http = try CollectionFolder.read(suite.appendingPathComponent("http/folder"))
        var changed = http
        changed.collection.requests[0].headers.append(Header("X", "y"))
        let expected = project(try JSONValue.parse(try String(contentsOf: suite.appendingPathComponent("http/expected.json"), encoding: .utf8)))
        XCTAssertEqual(try model(http), expected)
        XCTAssertNotEqual(try model(changed), expected)
    }

    /// Every key of the suite's v1 report is in HitopKit's report, with the same JSON type.
    func testReportHasTheV1Shape() throws {
        let sample = try JSONValue.parse(try String(contentsOf: suite.appendingPathComponent("run-report/report.json"), encoding: .utf8))
        var b = ReportBuilder()
        b.collection("C", environment: "e", folderName: "api")
        b.startIteration()
        b.iteration(data: ["row": "1"], steps: [
            StepResult(requestName: "ok", method: "GET", url: "u", response: HTTPResponse(status: 200, headers: [:], body: "", durationMs: 3),
                       error: nil, tests: [TestResult(name: "status is 200", pass: true, detail: "")], durationMs: 4, notes: [], path: "ok", fileExtension: "yaml"),
            StepResult(requestName: "bad", method: "GET", url: "u", response: nil, error: "refused",
                       tests: [TestResult(name: "x", pass: false, detail: "got 1")], durationMs: 1, notes: [], path: "bad", fileExtension: "yml"),
        ])
        let ours = b.finish(bailed: false).json

        func kind(_ v: JSONValue) -> String {
            switch v {
            case .null: return "null"
            case .bool: return "bool"
            case .number: return "number"
            case .string: return "string"
            case .array: return "array"
            case .object: return "object"
            }
        }
        func check(_ want: JSONValue, _ got: JSONValue, _ path: String) {
            switch (want, got) {
            case let (.object(w), .object):
                for (k, v) in w {
                    guard let g = got[k] else { XCTFail("\(path).\(k) missing"); continue }
                    check(v, g, "\(path).\(k)")
                }
            case let (.array(w), .array(g)):
                if let first = w.first, let other = g.first { check(first, other, "\(path)[0]") }
            default:
                // "or null" keys: either side may be null.
                if kind(want) != kind(got) && kind(want) != "null" && kind(got) != "null" {
                    XCTFail("\(path): \(kind(got)), the v1 report has \(kind(want))")
                }
            }
        }
        check(sample, ours, "report")
    }
}
