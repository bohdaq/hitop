import Foundation
import XCTest
@testable import HitopKit

/// spec/vectors/send.json: the Content-Type of a raw body and the URL's
/// scheme, the same cases hitop-core and the web app test.
final class SendRulesTests: XCTestCase {
    func vectors() throws -> JSONValue {
        try JSONValue.parse(try String(contentsOf: repoRoot.appendingPathComponent("spec/vectors/send.json"), encoding: .utf8))
    }

    func pairs(_ v: JSONValue?) -> [[String]] {
        (v?.arrayValue ?? []).map { ($0.arrayValue ?? []).compactMap(\.stringValue) }
    }

    func testContentTypeDetect() throws {
        let cases = try vectors()["contentType"]?["detect"]?.arrayValue ?? []
        XCTAssertGreaterThan(cases.count, 20)
        for c in cases {
            let body = c["body"]!.stringValue!
            XCTAssertEqual(Sending.rawContentType(body), c["contentType"]?.stringValue, "body \(body.debugDescription)")
        }
    }

    func testContentTypeApply() throws {
        let cases = try vectors()["contentType"]?["apply"]?.arrayValue ?? []
        XCTAssertGreaterThan(cases.count, 5)
        for c in cases {
            var headers = pairs(c["headers"]).map { ($0[0], $0[1]) }
            Sending.addContentType(method: c["method"]!.stringValue!, headers: &headers, body: c["body"]!.stringValue!)
            XCTAssertEqual(headers.map { [$0.0, $0.1] }, pairs(c["sent"]), c.compact)
        }
    }

    func testScheme() throws {
        let cases = try vectors()["scheme"]?["cases"]?.arrayValue ?? []
        XCTAssertGreaterThan(cases.count, 30)
        for c in cases {
            let url = c["url"]!.stringValue!
            if let want = c["error"]?.stringValue {
                XCTAssertThrowsError(try Sending.withScheme(url), url.debugDescription) { XCTAssertEqual("\($0)", want) }
            } else {
                XCTAssertEqual(try Sending.withScheme(url), c["sent"]?.stringValue, url.debugDescription)
            }
        }
    }

    /// The runner sends the header and the scheme; the collection keeps neither.
    func testRunnerSendsThem() async throws {
        let server = try MockServer.require()
        let collection = Collection(name: "Bodies", requests: [
            Request(name: "Json", url: "${base}/echo/json", method: "POST", body: " {\"a\": 1}"),
            Request(name: "Own", url: "${base}/echo/own", method: "PUT", headers: [Header("Content-Type", "text/csv")], body: "[1]"),
        ], variables: ["base": "127.0.0.1:\(server.port!)"])
        try server.reset()
        let steps = await Runner(engine: nil).run(collection, environment: nil)
        XCTAssertEqual(steps.map(\.error), [nil, nil])
        XCTAssertEqual(steps.first?.url, "http://127.0.0.1:\(server.port!)/echo/json")
        let seen = try server.requests().map { $0["headers"]?["content-type"]?.stringValue }
        XCTAssertEqual(seen, ["application/json", "text/csv"])
        XCTAssertEqual(collection.requests[0].headers.count, 0)
    }
}
