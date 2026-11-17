import Foundation
import XCTest
@testable import HITOP

final class TextTests: XCTestCase {
    func testAutoNameFollowsTheWebApp() {
        XCTAssertEqual(AutoName.name(method: "GET", url: ""), "Untitled")
        XCTAssertEqual(AutoName.name(method: "GET", url: "https://api.example.com/users?x=1"), "GET /users")
        XCTAssertEqual(AutoName.name(method: "post", url: "https://api.example.com"), "POST api.example.com")
        XCTAssertEqual(AutoName.name(method: "GET", url: "https://api.example.com/"), "GET api.example.com")
        XCTAssertEqual(AutoName.name(method: "GET", url: "${baseUrl}/v2/orders#top"), "GET /v2/orders")
        XCTAssertEqual(AutoName.name(method: "GET", url: "{{host}}"), "GET {{host}}")
        let long = "https://x.test/" + String(repeating: "a", count: 60)
        XCTAssertEqual(AutoName.name(method: "GET", url: long).count, "GET ".count + 48)
    }

    func testSegments() {
        XCTAssertEqual(VariableText.segments("${baseUrl}/v2/orders?id=${id}"),
                       [.variable("baseUrl"), .text("/v2/orders?id="), .variable("id")])
        XCTAssertEqual(VariableText.segments("no vars"), [.text("no vars")])
        XCTAssertEqual(VariableText.segments("half ${open"), [.text("half ${open")])
        XCTAssertEqual(VariableText.names("${a}${b}"), ["a", "b"])
    }

    func testExplainError() {
        let host = ExplainedError.explain("error sending request for url (https://nope.test/): A server with the specified hostname could not be found.")
        XCTAssertEqual(host.headline, "Host not found")
        XCTAssertTrue(host.known)
        XCTAssertEqual(ExplainedError.explain("error sending request for url (http://localhost:1/): Could not connect to the server.").headline, "Connection refused")
        XCTAssertEqual(ExplainedError.explain("The request timed out.").headline, "No answer in time")
        XCTAssertEqual(ExplainedError.explain("An SSL error has occurred and a secure connection to the server cannot be made.").headline,
                       "The TLS (HTTPS) handshake failed")
        let missing = ExplainedError.explain("variable 'baseUrl' is not defined")
        XCTAssertEqual(missing.headline, "A variable has no value")
        XCTAssertTrue(missing.next.contains("${baseUrl}"))
        let other = ExplainedError.explain("Error: something odd")
        XCTAssertFalse(other.known)
        XCTAssertEqual(other.raw, "something odd")
    }

    func testJSONOutlineFolds() throws {
        let v = try XCTUnwrap(JSONOutline.parse(#"{"id": "a", "items": [1, 2], "customer": {"x": true, "y": null, "z": 1.5}}"#))
        let open = JSONOutline.lines(v, folded: [])
        XCTAssertEqual(open.first?.text, "{")
        XCTAssertEqual(open.first?.fold, "$")
        XCTAssertTrue(open.contains { $0.text == #"  "items": ["# && $0.fold == "$.items" })
        XCTAssertEqual(open.last?.text, "}")
        let folded = JSONOutline.lines(v, folded: ["$.items", "$.customer"])
        XCTAssertTrue(folded.contains { $0.text == #"  "items": [ 2 items ],"# && $0.folded })
        XCTAssertTrue(folded.contains { $0.text == #"  "customer": { 3 keys }"# })
        XCTAssertEqual(folded.count, 5)
        XCTAssertNil(JSONOutline.parse("plain text"))
    }

    func testJSONValidateAndFormatKeepVariables() {
        XCTAssertTrue(JSONOutline.validate(#"{"a": ${id}, "b": "x ${y}"}"#).valid)
        XCTAssertFalse(JSONOutline.validate(#"{"a": }"#).valid)
        XCTAssertTrue(JSONOutline.validate("").valid)
        let formatted = JSONOutline.format(#"{"customerId":${customerId},"auth":"Bearer ${token}","n":[1,2]}"#)
        XCTAssertEqual(formatted, """
        {
          "customerId": ${customerId},
          "auth": "Bearer ${token}",
          "n": [
            1,
            2
          ]
        }
        """)
        XCTAssertNil(JSONOutline.format("{oops"))
    }

    func testVariableContextOrder() {
        var ctx = VariableContext()
        ctx.globals = ["a": "g", "b": "g", "c": "g", "d": "g"]
        ctx.collection = ["b": "col", "c": "col", "d": "col"]
        ctx.environmentName = "Staging"
        ctx.environment = ["c": "env", "d": "env"]
        ctx.scriptSet = ["d": "script"]
        ctx.environmentSecrets = ["s": "hunter2", "empty": ""]
        ctx.secretNames = ["s", "empty"]
        XCTAssertEqual(ctx.merged["a"], "g")
        XCTAssertEqual(ctx.merged["b"], "col")
        XCTAssertEqual(ctx.merged["c"], "env")
        XCTAssertEqual(ctx.merged["d"], "script")
        XCTAssertEqual(ctx.source("c")?.scope, "environment Staging")
        XCTAssertEqual(ctx.source("s")?.secret, true)
        XCTAssertEqual(ctx.source("empty")?.missing, true)
        XCTAssertNil(ctx.source("nope"))
        XCTAssertEqual(ctx.missing(in: "${a}/${nope}/${empty}"), ["nope", "empty"])
    }
}
