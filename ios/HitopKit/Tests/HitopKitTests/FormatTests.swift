import Foundation
import XCTest
@testable import HitopKit

final class YAMLTests: XCTestCase {
    func testCoreSchemaKeepsOnYesNoAsStrings() throws {
        let v = try YAMLValue.parse("""
        a: on
        b: yes
        c: no
        d: off
        e: true
        f: False
        port: 8080
        n: ~
        m: null
        empty:
        hex: 0x10
        oct: 0o17
        lead: 007
        f1: 1.50
        f2: 1e3
        neg: -3
        q: '1'
        dq: "true"
        ver: 1.2.3
        """)
        func s(_ k: String) throws -> String { try v[k]!.scalarString(k) }
        XCTAssertEqual(try s("a"), "on")
        XCTAssertEqual(try s("b"), "yes")
        XCTAssertEqual(try s("c"), "no")
        XCTAssertEqual(try s("d"), "off")
        guard case .bool(true) = v["e"]!, case .bool(false) = v["f"]! else { return XCTFail("booleans") }
        XCTAssertEqual(try s("port"), "8080")
        XCTAssertEqual(try s("n"), "")
        XCTAssertEqual(try s("m"), "")
        XCTAssertEqual(try s("empty"), "")
        XCTAssertEqual(try s("hex"), "16")
        XCTAssertEqual(try s("oct"), "15")
        XCTAssertEqual(try s("lead"), "7")
        XCTAssertEqual(try s("f1"), "1.5")
        XCTAssertEqual(try s("f2"), "1000.0")
        XCTAssertEqual(try s("neg"), "-3")
        XCTAssertEqual(try s("q"), "1")
        XCTAssertEqual(try s("dq"), "true")
        guard case .string("true") = v["dq"]! else { return XCTFail("quoted true is a string") }
        XCTAssertEqual(try s("ver"), "1.2.3")
    }

    func testOnAsAKeyIsAString() throws {
        let v = try YAMLValue.parse("on: 1\nyes: 2\n")
        XCTAssertEqual(v.keys, ["on", "yes"])
    }

    func testBlockScalarsStayText() throws {
        let v = try YAMLValue.parse("x: |\n  true\n")
        guard case .string("true\n") = v["x"]! else { return XCTFail("\(v)") }
    }
}

final class FolderTests: XCTestCase {
    let sample = repoRoot.appendingPathComponent("cli/fixtures/sample")

    func testReadsTheSampleFixture() throws {
        let f = try CollectionFolder.read(sample)
        let c = f.collection
        XCTAssertEqual(c.name, "Sample API")
        XCTAssertEqual(c.id, 2_220_540_611_480_096) // FNV-1a of "collection/Sample API"
        XCTAssertEqual(c.workspaceID, "2220540611480096")
        XCTAssertEqual(c.variables, ["base": "http://localhost:8080"])
        XCTAssertEqual(c.auth, .bearer(token: "${token}"))
        XCTAssertEqual(c.requests.map(\.path), ["get-user", "orders/create-order", "orders/get-order"])

        let get = c.requests[0]
        XCTAssertEqual(get.id, 8_924_970_193_556_500)
        XCTAssertEqual(get.name, "Get user")
        XCTAssertEqual(get.url, "${base}/users/1?fields=name,email")
        XCTAssertEqual(get.disabledParams, [Param(key: "debug", value: "1")])
        XCTAssertEqual(get.headers, [Header("Accept", "application/json"), Header("X-Debug", "1", enabled: false)])
        XCTAssertNil(get.auth)
        XCTAssertTrue(get.postRequestScript.contains("assert.status(200);"))
        XCTAssertEqual(get.file, "requests/get-user.yaml")

        let create = c.requests[1]
        XCTAssertEqual(create.id, 101_053_153_215_215)
        XCTAssertEqual(create.method, "POST")
        XCTAssertNil(create.bodyType, "json is what the Content-Type gives")
        XCTAssertEqual(create.body, "{\n  \"product\": 42,\n  \"quantity\": 1\n}\n")

        let order = c.requests[2]
        XCTAssertEqual(order.auth, .apiKey(key: "X-API-Key", value: "${apiKey}", location: .header))
        XCTAssertTrue(order.preRequestScript.hasPrefix("setUrl("))

        XCTAssertEqual(f.environments.map(\.name), ["local", "prod"])
        XCTAssertEqual(f.environments[1].secrets, ["apiKey", "token"])
        XCTAssertEqual(f.environments[1].variables, ["base": "https://api.example.com"])
        XCTAssertNil(f.globals)
    }

    func testDefaultsHiddenFilesOrderAndGlobals() throws {
        let dir = try makeFolder([
            "hitop.yaml": "format: 1\norder:\n- b\n- missing\n- a\n",
            "requests/a.yaml": "url: http://x/a\n",
            "requests/b.yml": "name: Bee\nmethod: PUT\nurl: http://x/b\nkind: http\n",
            "requests/c/d.yaml": "url: ws://x\nkind: websocket\nwebsocket:\n  subprotocols: [v1]\n",
            "requests/.hidden.yaml": "url: nope\n",
            "requests/notes.txt": "not yaml",
            "environments/z.yaml": "name: Zed\nvariables:\n  port: 8080\n  on: yes\n",
            "environments/a.yml": "variables: {}\n",
            "globals.yaml": "variables:\n  region: eu\n  key:\n    secret: true\n",
        ], name: "my-api")
        let f = try CollectionFolder.read(dir)
        XCTAssertEqual(f.collection.name, "my-api")
        XCTAssertEqual(f.collection.requests.map(\.path), ["b", "a", "c/d"])
        let b = f.collection.requests[0]
        XCTAssertEqual(b.name, "Bee")
        XCTAssertEqual(b.method, "PUT")
        XCTAssertNil(b.kind)
        XCTAssertEqual(b.file, "requests/b.yml")
        XCTAssertEqual(f.collection.requests[1].name, "a")
        XCTAssertEqual(f.collection.requests[1].method, "GET")
        XCTAssertTrue(f.collection.requests[2].isWebSocket)
        XCTAssertEqual(f.environments.map(\.name), ["a", "Zed"])
        XCTAssertEqual(f.environments[1].variables, ["port": "8080", "on": "yes"])
        XCTAssertEqual(f.globals?.name, "globals")
        XCTAssertEqual(f.globals?.variables, ["region": "eu"])
        XCTAssertEqual(f.globals?.secrets, ["key"])
    }

    func testSymlinksAreSkipped() throws {
        let dir = try makeFolder(["hitop.yaml": "format: 1\n", "requests/a.yaml": "url: http://x\n"])
        try FileManager.default.createSymbolicLink(at: dir.appendingPathComponent("requests/link.yaml"),
                                                   withDestinationURL: dir.appendingPathComponent("requests/a.yaml"))
        XCTAssertEqual(try CollectionFolder.read(dir).collection.requests.map(\.path), ["a"])
    }

    func testBodiesFormsAndGraphQL() throws {
        let dir = try makeFolder([
            "hitop.yaml": "format: 1\n",
            "requests/form.yaml": """
            method: POST
            url: http://x/form
            body:
              type: form
              fields:
              - name: a
                value: '1'
              - name: off
                value: x
                enabled: false
            """,
            "requests/multi.yaml": """
            method: POST
            url: http://x/m
            body:
              type: multipart
              fields:
              - name: f
                type: file
                fileName: a.bin
            """,
            "requests/gql.yaml": """
            method: POST
            url: http://x/graphql
            body:
              type: graphql
              content: '{ me { id } }'
              variables: '{"id": "${id}"}'
              operationName: Me
            """,
            "requests/text.yaml": "url: http://x\nbody:\n  type: text\n  content: '{\"a\": 1}'\n",
        ])
        let r = try CollectionFolder.read(dir).collection.requests
        let byPath = Dictionary(uniqueKeysWithValues: r.map { ($0.path, $0) })
        XCTAssertEqual(byPath["form"]?.bodyMode, "urlencoded")
        XCTAssertEqual(byPath["form"]?.formFields, [FormField(key: "a", value: "1"), FormField(key: "off", value: "x", enabled: false)])
        XCTAssertEqual(byPath["multi"]?.bodyMode, "multipart")
        XCTAssertEqual(byPath["multi"]?.formFields.first?.fileName, "a.bin")
        XCTAssertEqual(byPath["gql"]?.bodyType, "graphql")
        XCTAssertTrue(byPath["gql"]?.isGraphQL ?? false)
        XCTAssertEqual(byPath["gql"]?.graphql, GraphQL(variables: "{\"id\": \"${id}\"}", operationName: "Me"))
        XCTAssertEqual(byPath["text"]?.bodyType, "text", "text differs from the inferred json")
    }

    func testErrorsLikeTheCore() throws {
        let cases: [([String: String], String)] = [
            (["hitop.yaml": "name: x\n"], "`format: 1` is missing"),
            (["hitop.yaml": "format: 2\n"], "format 2 needs a newer hitop"),
            (["hitop.yaml": "format: 1\norder:\n- ../x\n"], "invalid path"),
            (["hitop.yaml": "format: 1\nrequests: []\n"], "the key 'requests'"),
            (["hitop.yaml": "format: 1\n", "requests/a.yaml": "url: x\nbody:\n  type: json\n  conten: x\n"], "unknown key 'conten'"),
            (["hitop.yaml": "format: 1\n", "requests/a.yaml": "url: x\nscripts:\n  pre: ''\n  test: ''\n"], "unknown key 'test'"),
            (["hitop.yaml": "format: 1\n", "requests/a.yaml": "url: x\nbodyMode: raw\n"], "the key 'bodyMode'"),
            (["hitop.yaml": "format: 1\n", "requests/a.yaml": "url: x\n", "requests/a.yml": "url: y\n"], "are the same request"),
            (["hitop.yaml": "format: 1\n", "requests/a.yaml": "url: x\nparams:\n- name: a\n  value: b\n  note: c\n"], "only kept on disabled parameters"),
            (["hitop.yaml": "format: 1\n", "requests/a.yaml": "url: x\nauth:\n  type: oauth2\n"], "unknown auth type 'oauth2'"),
            (["hitop.yaml": "format: 1\n", "environments/e.yaml": "variables:\n  t:\n    secret: false\n"], "must be a value or {secret: true}"),
            (["hitop.yaml": "format: 1\n", "environments/e.yaml": "secrets: [a]\n"], "the key 'secrets'"),
        ]
        for (files, want) in cases {
            let dir = try makeFolder(files)
            XCTAssertThrowsError(try CollectionFolder.read(dir), want) { e in
                XCTAssertTrue("\(e)".contains(want), "\(e) should contain \(want)")
            }
        }
        XCTAssertThrowsError(try CollectionFolder.read(FileManager.default.temporaryDirectory.appendingPathComponent("no-such-\(UUID())"))) { e in
            XCTAssertTrue("\(e)".contains("is not a collection folder"))
        }
    }
}

final class ParamsAndInterpolationTests: XCTestCase {
    func testQueryRowsRoundTripWithoutTouchingVariables() {
        let url = "${base}/posts?userId=${id}&flag&q=a%20b#top"
        let parsed = Query.parse(url)
        XCTAssertEqual(parsed.map { "\($0.0)=\($0.1)" }, ["userId=${id}", "flag=", "q=a%20b"])
        XCTAssertEqual(Query.with(url, parsed), url)
        XCTAssertEqual(Query.with("http://x/?a=1#f", [("a b", "c&d"), ("", "")]), "http://x/?a%20b=c%26d#f")
        XCTAssertEqual(Query.append("http://x", [("k", "v")]), "http://x?k=v")
        XCTAssertEqual(Query.with("http://x?a=1", []), "http://x")
    }

    func testInterpolation() throws {
        let vars = ["host": "https://api.example.com", "a": "x", "scheme": "https"]
        XCTAssertEqual(try Interpolation.interpolate("${host}/todos", vars), "https://api.example.com/todos")
        XCTAssertEqual(try Interpolation.interpolate("${a}+${a}", vars), "x+x")
        XCTAssertEqual(try Interpolation.interpolate("no vars", [:]), "no vars")
        XCTAssertEqual(try Interpolation.interpolate("$ ${} $x", [:]), "$ ${} $x")
        // One pass: a value isn't scanned again.
        XCTAssertEqual(try Interpolation.interpolate("${v}", ["v": "${a}"]), "${a}")
        XCTAssertThrowsError(try Interpolation.interpolate("${missing}", [:])) { XCTAssertEqual("\($0)", "variable 'missing' is not defined") }
        var missing: [String] = []
        XCTAssertEqual(Interpolation.lenient("${a}/${b}/${b}/${c}", ["a": "1"], missing: &missing), "1/${b}/${b}/${c}")
        XCTAssertEqual(missing, ["b", "c"])
        XCTAssertEqual(try Interpolation.fill("${b}-${x}", names: missing, ["b": "2"]), "2-${x}")
        XCTAssertThrowsError(try Interpolation.fill("${c}", names: missing, [:]))
    }

    func testAuth() throws {
        let same: (String) throws -> String = { $0.replacingOccurrences(of: "${t}", with: "tok") }
        var (_, h) = try AuthApply.apply(.bearer(token: "${t}"), url: "https://x", headers: [], same)
        XCTAssertEqual(h.map { "\($0.0): \($0.1)" }, ["Authorization: Bearer tok"])
        (_, h) = try AuthApply.apply(.basic(username: "ada", password: "s3cret"), url: "https://x", headers: [], same)
        XCTAssertEqual(h[0].1, "Basic YWRhOnMzY3JldA==")
        (_, h) = try AuthApply.apply(.bearer(token: "x"), url: "https://x", headers: [("authorization", "Bearer mine")], same)
        XCTAssertEqual(h.map(\.1), ["Bearer mine"])
        let (url, h2) = try AuthApply.apply(.apiKey(key: "k", value: "a b", location: .query), url: "https://x/?k=old&q=1", headers: [], same)
        XCTAssertEqual(url, "https://x/?q=1&k=a%20b")
        XCTAssertTrue(h2.isEmpty)
        (_, h) = try AuthApply.apply(.apiKey(key: "X-Key", value: "v", location: .header), url: "https://x", headers: [], same)
        XCTAssertEqual(h.map { "\($0.0): \($0.1)" }, ["X-Key: v"])
        (_, h) = try AuthApply.apply(.bearer(token: ""), url: "https://x", headers: [], same)
        XCTAssertTrue(h.isEmpty)
    }

    func testGraphQLBodies() throws {
        XCTAssertEqual(try GraphQLBody.requestBody(query: "{ me { id } }", variables: "", operationName: ""), #"{"query":"{ me { id } }"}"#)
        XCTAssertEqual(try GraphQLBody.requestBody(query: "query Q($id: ID!) { u(id: $id) { n } }", variables: " {\"z\": \"7\", \"a\": 1} ", operationName: " Q "),
                       #"{"query":"query Q($id: ID!) { u(id: $id) { n } }","variables":{"z": "7", "a": 1},"operationName":"Q"}"#)
        XCTAssertEqual(try GraphQLBody.requestBody(query: "q", variables: "null", operationName: ""), #"{"query":"q"}"#)
        XCTAssertEqual(try GraphQLBody.requestBody(query: "a\n\"b\"/c", variables: "", operationName: ""), #"{"query":"a\n\"b\"/c"}"#)
        XCTAssertThrowsError(try GraphQLBody.requestBody(query: "q", variables: "{id: 1}", operationName: "")) {
            XCTAssertTrue("\($0)".hasPrefix("GraphQL variables are not valid JSON"))
        }
        XCTAssertThrowsError(try GraphQLBody.requestBody(query: "q", variables: "[1]", operationName: "")) {
            XCTAssertEqual("\($0)", "GraphQL variables must be a JSON object, got an array")
        }
    }

    func testFormBodies() throws {
        let b = FormBody.urlencoded([("a b", "x&y=z/é"), ("k", "(ok)!*'~")])
        XCTAssertEqual(String(decoding: b.body, as: UTF8.self), "a%20b=x%26y%3Dz%2F%C3%A9&k=(ok)!*'~")
        XCTAssertEqual(b.contentType, "application/x-www-form-urlencoded")
        let m = FormBody.multipart([.text(name: "note", value: "hi"), .file(name: "fi\"le", filename: "a.bin", contentType: "image/png", data: Data([0, 255]))])
        let boundary = String(m.contentType.dropFirst("multipart/form-data; boundary=".count))
        var want = Data("--\(boundary)\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nhi\r\n--\(boundary)\r\nContent-Disposition: form-data; name=\"fi%22le\"; filename=\"a.bin\"\r\nContent-Type: image/png\r\n\r\n".utf8)
        want.append(Data([0, 255]))
        want.append(Data("\r\n--\(boundary)--\r\n".utf8))
        XCTAssertEqual(m.body, want)

        var r = Request(name: "r", url: "http://x", bodyMode: "urlencoded", formFields: [
            FormField(key: "name", value: "${n}"), FormField(key: "off", value: "x", enabled: false), FormField(key: "", value: "x"),
        ])
        XCTAssertEqual(try FormBody.requestBody(r, ["n": "Ada"]).map { String(decoding: $0.body, as: UTF8.self) }, "name=Ada")
        r.bodyMode = nil
        XCTAssertNil(try FormBody.requestBody(r, [:]))
    }

    func testInferBodyType() {
        XCTAssertEqual(BodyType.infer(headers: [], body: ""), "none")
        XCTAssertEqual(BodyType.infer(headers: [], body: " {\"a\":1}"), "json")
        XCTAssertEqual(BodyType.infer(headers: [], body: "{nope"), "text")
        XCTAssertEqual(BodyType.infer(headers: [], body: "<a/>"), "xml")
        XCTAssertEqual(BodyType.infer(headers: [Header("content-type", "text/plain")], body: "{}"), "text")
        XCTAssertEqual(BodyType.infer(headers: [Header("Content-Type", "application/xml")], body: "x"), "xml")
    }

    func testIdsAndPaths() {
        XCTAssertEqual(Names.requestID("get-user"), 8_924_970_193_556_500)
        XCTAssertNoThrow(try Names.checkRelPath("orders/create-order"))
        for bad in ["", "/abs", "a//b", "../x", "a/./b", ".hidden", "a\\b", "c:x"] {
            XCTAssertThrowsError(try Names.checkRelPath(bad), bad)
        }
    }

    func testJSONTextMatchesSerde() throws {
        XCTAssertEqual(JSONValue.quote("a/b\"c\\\n\u{1}é"), "\"a/b\\\"c\\\\\\n\\u0001é\"")
        let v = try JSONValue.parse(#" {"a": [1, 2.5, -3e2, true, null, "x\u00e9\ud83d\ude00"], "b": {}} "#)
        XCTAssertEqual(v.compact, #"{"a":[1,2.5,-3e2,true,null,"xé😀"],"b":{}}"#)
        XCTAssertEqual(JSONValue.object([("a", .array([])), ("b", .null)]).pretty, "{\n  \"a\": [],\n  \"b\": null\n}")
        XCTAssertThrowsError(try JSONValue.parse("{a: 1}"))
        XCTAssertThrowsError(try JSONValue.parse("[1,]"))
        XCTAssertThrowsError(try JSONValue.parse("01"))
    }
}
