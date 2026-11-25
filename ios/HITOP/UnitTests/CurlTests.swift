import Foundation
import XCTest
@testable import HITOP

/// I5: the curl importer, case for case with the web app's `tests/unit-ux2.mjs` ((2) curl
/// commands as requests), plus every curl.sh hitop-core's snippet goldens write.
final class CurlTests: XCTestCase {
    typealias H = CurlImport.Header

    func testIsCurl() {
        XCTAssertTrue(CurlImport.isCurl("curl https://x"))
        XCTAssertTrue(CurlImport.isCurl("  $ curl -sS https://x"))
        XCTAssertTrue(CurlImport.isCurl("curl.exe https://x"))
        XCTAssertFalse(CurlImport.isCurl("curling.example.com"))
        XCTAssertFalse(CurlImport.isCurl("https://x"))
        XCTAssertFalse(CurlImport.isCurl("POST a JSON order to https://x"))
    }

    func testShellWords() throws {
        XCTAssertEqual(try CurlImport.shellWords(#"a 'b c' "d \"e\" \$f" g\ h $'i\nj' ''"#), ["a", "b c", #"d "e" $f"#, "g h", "i\nj", ""])
        XCTAssertEqual(try CurlImport.shellWords("a \\\n  b ^\n c `\n d"), ["a", "b", "c", "d"], "bash, cmd and PowerShell line continuations")
        XCTAssertThrowsError(try CurlImport.shellWords("a 'b")) { XCTAssertTrue("\($0.localizedDescription)".contains("isn't closed")) }
        XCTAssertEqual(try CurlImport.shellWords(#"$'\x41é\q'"#), ["Aé\\q"])
    }

    func testSimpleAndPost() throws {
        let simple = try CurlImport.parse("curl https://httpbin.org/get")
        XCTAssertEqual(simple, CurlImport.Request(method: "GET", url: "https://httpbin.org/get", headers: [], body: "", bodyMode: "raw", formFields: [], auth: nil, warnings: []))
        let post = try CurlImport.parse("$ curl -X POST 'https://httpbin.org/post' \\\n  -H 'Content-Type: application/json' \\\n  -H \"Authorization: Bearer abc\" \\\n  -d '{\"name\": \"Ada\"}'")
        XCTAssertEqual(post.method, "POST")
        XCTAssertEqual(post.url, "https://httpbin.org/post")
        XCTAssertEqual(post.headers, [H(name: "Content-Type", value: "application/json"), H(name: "Authorization", value: "Bearer abc")])
        XCTAssertEqual(post.body, "{\"name\": \"Ada\"}")
    }

    func testDataFormsAndFlags() throws {
        let form = try CurlImport.parse("curl https://x.test/login -d user=ada -d 'pass=a b'")
        XCTAssertEqual(form.method, "POST")
        XCTAssertEqual(form.body, "user=ada&pass=a b")
        XCTAssertEqual(form.headers, [H(name: "Content-Type", value: "application/x-www-form-urlencoded")])
        XCTAssertEqual(try CurlImport.parse("curl https://x.test -d '{\"a\":1}'").headers, [], "JSON: the app's own Content-Type rule decides")
        XCTAssertEqual(try CurlImport.parse("curl -G https://x.test/s -d q=1 --data-urlencode 'name=a b'").url, "https://x.test/s?q=1&name=a%20b")
        XCTAssertEqual(try CurlImport.parse("curl -G https://x.test/s -d q=1").method, "GET")
        XCTAssertEqual(try CurlImport.parse("curl -I https://x.test").method, "HEAD")
        XCTAssertEqual(try CurlImport.parse("curl -XDELETE https://x.test/1").method, "DELETE", "a value glued to -X")
        XCTAssertEqual(try CurlImport.parse("curl -sSL --request=patch https://x.test/1").method, "PATCH")
        XCTAssertEqual(try CurlImport.parse("curl -sSX PUT https://x.test/1").method, "PUT", "a cluster whose last flag takes the next word")
        XCTAssertEqual(try CurlImport.parse("curl --url https://x.test/a -u ada:p:w").auth, .basic(username: "ada", password: "p:w"))
        XCTAssertEqual(try CurlImport.parse("curl https://x.test -A hitop/1 -e https://ref -b 'a=1; b=2'").headers,
                       [H(name: "User-Agent", value: "hitop/1"), H(name: "Referer", value: "https://ref"), H(name: "Cookie", value: "a=1; b=2")])
        let j = try CurlImport.parse(#"curl https://x.test/j --json '{"a": 1}'"#)
        XCTAssertEqual(j.method, "POST")
        XCTAssertEqual(j.headers, [H(name: "Content-Type", value: "application/json"), H(name: "Accept", value: "application/json")])
        let multi = try CurlImport.parse(#"curl https://x.test/up -F 'title=Hello' -F 'avatar=@"me.png";type=image/png' --form-string 'raw=@x'"#)
        XCTAssertEqual(multi.method, "POST")
        XCTAssertEqual(multi.bodyMode, "multipart")
        XCTAssertEqual(multi.formFields.map { [$0.key, $0.value, $0.type, $0.fileName] },
                       [["title", "Hello", "text", ""], ["avatar", "", "file", "me.png"], ["raw", "@x", "text", ""]])
        XCTAssertEqual(try CurlImport.parse("curl https://x.test/{{id}} -H 'X-Key: {{ apiKey }}'").url, "https://x.test/${id}")
        XCTAssertEqual(try CurlImport.parse("curl https://x.test -H 'X-Key: {{ apiKey }}'").headers[0].value, "${apiKey}")
        XCTAssertEqual(try CurlImport.parse("curl -o out.json --max-time 5 -k https://x.test --frobnicate").warnings, ["Left out: --frobnicate."])
        XCTAssertEqual(try CurlImport.parse("curl https://a.test && echo done").url, "https://a.test", "the first command only")
        XCTAssertEqual(try CurlImport.parse("curl -d @body.json https://x.test").warnings.first, "The body comes from the file body.json: paste its contents into Body.")
        XCTAssertEqual(try CurlImport.parse("curl --oauth2-bearer {{token}} https://x.test").auth, .bearer(token: "${token}"))
        // The iOS keyboard's smart punctuation.
        let smart = try CurlImport.parse("curl \u{2014}request PUT \u{2018}https://x.test/a b\u{2019} -d \u{201C}{}\u{201D}")
        XCTAssertEqual(smart.method, "PUT")
        XCTAssertEqual(smart.url, "https://x.test/a b")
        XCTAssertEqual(smart.body, "{}")
    }

    func testErrors() {
        func message(_ text: String) -> String {
            do {
                _ = try CurlImport.parse(text)
                return ""
            } catch { return error.localizedDescription }
        }
        XCTAssertTrue(message("curl -X POST").contains("no URL"))
        XCTAssertTrue(message("wget https://x").contains("isn't a curl command"))
        XCTAssertTrue(message("curl -H").contains("needs a value"))
        XCTAssertTrue(message("curl 'https://x").contains("isn't closed"))
    }

    func testAsSavedRequest() throws {
        let r = try CurlImport.parse("curl -X POST https://x.test/o -H 'Content-Type: application/json' -u ada:pw -d '{\"a\":1}'").savedRequest
        XCTAssertEqual(r.method, "POST")
        XCTAssertEqual(r.url, "https://x.test/o")
        XCTAssertEqual(r.headers.map(\.name), ["Content-Type"])
        XCTAssertEqual(r.auth, .basic(username: "ada", password: "pw"))
        XCTAssertEqual(r.bodyKind, .json)
        XCTAssertEqual(r.name, AutoName.untitled, "the editor names it from its URL")
        let form = try CurlImport.parse("curl https://x.test/up -F 'title=Hello' -F 'avatar=@me.png'").savedRequest
        XCTAssertEqual(form.bodyKind, .multipart)
        XCTAssertEqual(form.formFields.map(\.type), ["text", "file"])
        XCTAssertEqual(form.formFields.last?.fileName, "me.png")
        let text = try CurlImport.parse("curl https://x.test -H 'Content-Type: text/plain' -d hello").savedRequest
        XCTAssertEqual(text.bodyKind, .text)
        XCTAssertEqual(try CurlImport.parse("curl https://x.test").savedRequest.bodyKind, .none)
    }

    // MARK: hitop-core's snippet goldens

    static func goldens() throws -> URL {
        var places: [URL] = []
        if let d = ProcessInfo.processInfo.environment["HITOP_SNIPPET_GOLDENS"], !d.isEmpty { places.append(URL(fileURLWithPath: d)) }
        for b in Bundle.allBundles + [Bundle(for: CurlTests.self)] {
            if let u = b.url(forResource: "snippets", withExtension: nil) { places.append(u) }
        }
        let here = URL(fileURLWithPath: #filePath).resolvingSymlinksInPath()
        places.append(here.deletingLastPathComponent().appendingPathComponent("../../../cli/crates/hitop-core/tests/golden/snippets").standardizedFileURL)
        for p in places where FileManager.default.fileExists(atPath: p.appendingPathComponent("cases.json").path) { return p }
        struct Missing: Error {}
        XCTFail("no hitop-core snippet goldens: set HITOP_SNIPPET_GOLDENS (core-tests.sh does) or bundle them")
        throw Missing()
    }

    func testEverySnippetGoldenReadsBack() throws {
        let dir = try Self.goldens()
        func read(_ name: String) throws -> CurlImport.Request {
            try CurlImport.parse(String(contentsOf: dir.appendingPathComponent(name).appendingPathComponent("curl.sh"), encoding: .utf8))
        }
        let cases = try JSONSerialization.jsonObject(with: Data(contentsOf: dir.appendingPathComponent("cases.json"))) as? [[String: Any]] ?? []
        let body = { (name: String) in ((cases.first { $0["name"] as? String == name })?["request"] as? [String: Any])?["body"] as? String }

        let getq = try read("get-query-bearer")
        XCTAssertEqual(getq.method, "GET")
        XCTAssertEqual(getq.url, "https://api.example.test/users?page=2&q=a%20b")
        XCTAssertEqual(getq.headers, [H(name: "Accept", value: "application/json"), H(name: "Authorization", value: "Bearer tok-123")])
        let pj = try read("post-json-basic")
        XCTAssertEqual(pj.body, body("post-json-basic"), "quotes, backslashes, $ and ` survive the round trip")
        XCTAssertEqual(pj.auth, .basic(username: "ada", password: #"p'w"d"#))
        let gql = try read("graphql-query")
        let parsed = try JSONSerialization.jsonObject(with: Data(gql.body.utf8)) as? [String: Any]
        XCTAssertEqual(parsed?["operationName"] as? String, "User")
        XCTAssertTrue(gql.headers.contains { $0.value == "Bearer ${token}" })
        let up = try read("multipart-apikey-header")
        XCTAssertEqual(up.formFields.map { [$0.key, $0.value, $0.type, $0.fileName] }, [["title", "@not-a-file", "text", ""], ["avatar", "", "file", "photo.png"]])
        XCTAssertEqual(try read("custom-method").method, "PURGE")
        XCTAssertEqual(try read("custom-method").auth, .basic(username: "svc", password: ""))
        XCTAssertEqual(try read("urlencoded-apikey-query").body, "name=Ada%20Lovelace&note=a%26b%3Dc%2F%C3%A9")
        XCTAssertEqual(try read("urlencoded-apikey-query").method, "PUT")
        let names = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            .filter { FileManager.default.fileExists(atPath: dir.appendingPathComponent($0).appendingPathComponent("curl.sh").path) }
        XCTAssertGreaterThanOrEqual(names.count, 7)
        for n in names { XCTAssertNoThrow(try read(n), n) }
    }
}
