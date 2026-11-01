import Foundation
import XCTest
@testable import HitopKit

final class RunnerTests: XCTestCase {
    func runner() throws -> Runner { Runner(engine: try requireEngine()) }

    func testSampleCollectionAgainstTheMockServer() async throws {
        let server = try MockServer.require()
        let dir = try sampleCopy(port: server.port!)
        let folder = try CollectionFolder.read(dir)
        try server.reset()
        let report = try await FolderRun.run(folder, environment: "mock", store: MemorySecretStore(), lookupEnv: { _ in nil }, runner: try runner())
        XCTAssertTrue(report.passed, report.jsonText)
        XCTAssertEqual(report.requests, Report.Counts(total: 3, passed: 3, failed: 0))
        XCTAssertEqual(report.assertions, Report.Counts(total: 5, passed: 5, failed: 0))
        let requests = report.collections[0].iterations[0].requests
        XCTAssertEqual(requests.map(\.url), ["\(server.base)/users/1?fields=name,email", "\(server.base)/orders", "\(server.base)/orders/42"])
        XCTAssertEqual(requests.map(\.file), ["sample/requests/get-user.yaml", "sample/requests/orders/create-order.yaml", "sample/requests/orders/get-order.yaml"])
        XCTAssertEqual(requests.map(\.status), [200, 201, 200])
        XCTAssertEqual(report.collections[0].environment, "mock")

        let seen = try server.requests()
        XCTAssertEqual(seen.count, 3)
        // Collection auth (bearer) on the first two; the request's own API key on the third.
        XCTAssertEqual(seen[0]["headers"]?["authorization"]?.stringValue, "Bearer mock-token")
        XCTAssertEqual(seen[0]["headers"]?["accept"]?.stringValue, "application/json")
        XCTAssertNil(seen[0]["headers"]?["x-debug"], "disabled header")
        XCTAssertEqual(seen[1]["body"]?.stringValue, "{\n  \"product\": 42,\n  \"quantity\": 1\n}\n")
        XCTAssertEqual(seen[2]["headers"]?["x-api-key"]?.stringValue, "mock-key")
        XCTAssertNil(seen[2]["headers"]?["authorization"])
    }

    func testSecretsFromTheStoreAndMasking() async throws {
        let server = try MockServer.require()
        let dir = try sampleCopy(port: server.port!)
        try """
        variables:
          base: http://127.0.0.1:\(server.port!)
          apiKey:
            secret: true
          token:
            secret: true
        """.write(to: dir.appendingPathComponent("environments/vault.yaml"), atomically: true, encoding: .utf8)
        let folder = try CollectionFolder.read(dir)
        let ws = folder.collection.workspaceID
        let store = MemorySecretStore(["\(ws)/vault/apiKey": "k3y-s3cr3t"])
        let env: (String) -> String? = { $0 == "HITOP_SECRET_VAULT_TOKEN" ? "t0k-s3cr3t" : nil }
        let report = try await FolderRun.run(folder, environment: "vault", store: store, lookupEnv: env, runner: try runner())
        XCTAssertTrue(report.passed, report.jsonText)
        XCTAssertFalse(report.jsonText.contains("s3cr3t"))
        let seen = try server.requests().suffix(3)
        XCTAssertEqual(seen.first?["headers"]?["authorization"]?.stringValue, "Bearer t0k-s3cr3t")
        XCTAssertEqual(seen.last?["headers"]?["x-api-key"]?.stringValue, "k3y-s3cr3t")

        // Without a value the run doesn't start.
        do {
            _ = try await FolderRun.run(folder, environment: "vault", store: MemorySecretStore(), lookupEnv: { _ in nil }, runner: try runner())
            XCTFail("expected missing secrets")
        } catch let e as MissingSecrets {
            XCTAssertEqual(e.names, ["apiKey", "token"])
        }
        do {
            _ = try await FolderRun.run(folder, environment: "nope", store: store, runner: try runner())
            XCTFail("expected an unknown environment")
        } catch let e as FolderRun.UnknownEnvironment {
            XCTAssertTrue(e.description.contains("local, mock, prod, vault"), e.description)
        }
    }

    func testStepsNeverCarryASecretValue() async throws {
        // Port 9 refuses the connection: the error names the URL.
        let c = Collection(name: "c", requests: [Request(name: "r", url: "http://127.0.0.1:9/x?key=${apiKey}", headers: [Header("Authorization", "Bearer ${apiKey}")])])
        let env = Environment(id: "prod", name: "prod", secrets: ["apiKey"])
        let (secrets, _) = try resolveSecrets(workspace: "w", environment: env, store: MemorySecretStore(), lookupEnv: { $0 == "HITOP_SECRET_PROD_APIKEY" ? "s3cr3t key" : nil })
        var seen: [String] = []
        let steps = try await runner().run(c, environment: env, secrets: secrets) { seen.append("\($0.url) \($0.error ?? "")") }
        XCTAssertEqual(steps[0].url, "http://127.0.0.1:9/x?key=••••")
        XCTAssertNotNil(steps[0].error)
        XCTAssertFalse("\(seen) \(steps)".contains("s3cr3t"))
    }

    func testGlobalsSitBelowTheCollection() async throws {
        let server = try MockServer.require()
        let dir = try makeFolder([
            "hitop.yaml": "format: 1\nvariables:\n  who: collection\n",
            "globals.yaml": "variables:\n  base: \(server.base)\n  who: global\n  g:\n    secret: true\n",
            "environments/e.yaml": "variables:\n  env: e\n",
            "requests/a.yaml": "url: ${base}/echo/${who}/${env}/${g}\n",
        ])
        let folder = try CollectionFolder.read(dir)
        let store = MemorySecretStore(["\(folder.collection.workspaceID)/global/g": "gl0bal-s3cr3t"])
        let report = try await FolderRun.run(folder, environment: "e", store: store, lookupEnv: { _ in nil }, runner: try runner())
        let r = report.collections[0].iterations[0].requests[0]
        XCTAssertEqual(r.url, "\(server.base)/echo/collection/e/••••")
        XCTAssertTrue(r.passed)
        XCTAssertEqual(try server.requests().last?["path"]?.stringValue, "/echo/collection/e/gl0bal-s3cr3t")
    }

    func testOverridesBeatTheEnvironment() async throws {
        let c = Collection(name: "c", requests: [Request(name: "r", url: "http://${host}/${who}?k=${key}")], variables: ["host": "127.0.0.1:1"])
        let env = Environment(id: "e", name: "e", variables: ["host": "127.0.0.1:2", "who": "env"])
        let steps = try await runner().run(c, environment: env, options: RunOptions(timeout: 2, overrides: ["host": "127.0.0.1:9", "key": "plain"]))
        XCTAssertEqual(steps[0].url, "http://127.0.0.1:9/env?k=plain")
        XCTAssertNotNil(steps[0].error)
    }

    func testFailuresAndReportShape() async throws {
        let server = try MockServer.require()
        let c = Collection(name: "Shop", requests: [
            Request(name: "missing var", url: "\(server.base)/echo/${nope}"),
            Request(name: "server error", url: "\(server.base)/status/500"),
            Request(name: "bad assert", url: "\(server.base)/status/200", postRequestScript: "assert.status(201); assert.ok(true, 'fine');"),
            Request(name: "script error", url: "\(server.base)/status/200", postRequestScript: "throw new Error('kaput')"),
            Request(name: "socket", url: "ws://127.0.0.1:9", kind: "websocket"),
            Request(name: "call", url: "127.0.0.1:9", kind: "grpc"),
            Request(name: "vault", url: "\(server.base)/echo", headers: [Header("X-T", "${secret:vault:kv/app#token}")]),
            Request(name: "vault var", url: "\(server.base)/echo/${t}"),
            Request(name: "bad url", url: "not a url"),
        ], variables: ["t": "${secret:aws:name}"])
        var b = ReportBuilder()
        b.collection(c.name, environment: nil)
        b.startIteration()
        let steps = try await runner().run(c, environment: nil)
        b.iteration(steps: steps)
        let report = b.finish(bailed: false)
        let r = report.collections[0].iterations[0].requests
        XCTAssertEqual(r.map(\.name), ["missing var", "server error", "bad assert", "script error", "vault", "vault var", "bad url"], "websocket and gRPC are left out")
        XCTAssertEqual(r[0].error, "variable 'nope' is not defined")
        XCTAssertEqual(r[0].url, "\(server.base)/echo/${nope}")
        XCTAssertNil(r[0].status)
        XCTAssertEqual(r[1].failure, "HTTP 500")
        XCTAssertEqual(r[1].status, 500)
        XCTAssertEqual(r[2].failure, "status is 201: got 200")
        XCTAssertEqual(r[2].assertions.map(\.passed), [false, true])
        XCTAssertTrue(r[3].failure?.hasPrefix("post-request script: post-request script error") ?? false, r[3].failure ?? "")
        XCTAssertTrue(r[4].error?.contains("${secret:vault:kv/app#token} is a secret manager reference") ?? false, r[4].error ?? "")
        XCTAssertTrue(r[5].error?.contains("${secret:aws:name}") ?? false, r[5].error ?? "")
        XCTAssertNotNil(r[6].error)
        XCTAssertFalse(report.passed)
        XCTAssertEqual(report.requests, Report.Counts(total: 7, passed: 0, failed: 7))

        // report.json: every documented key, nulls included.
        let json = try JSONValue.parse(report.jsonText)
        XCTAssertEqual(json["reportVersion"], .number("1"))
        XCTAssertEqual(json["tool"]?["name"], .string("hitop"))
        XCTAssertEqual(json.objectValue?.map(\.0), ["reportVersion", "tool", "startedAt", "durationMs", "passed", "bailed", "summary", "collections"])
        let req = json["collections"]?.arrayValue?[0]["iterations"]?.arrayValue?[0]["requests"]?.arrayValue?[0]
        XCTAssertEqual(req?.objectValue?.map(\.0), ["name", "method", "url", "passed", "status", "responseTimeMs", "durationMs", "error", "failure", "assertions", "file"])
        XCTAssertEqual(req?["status"], .null)
        XCTAssertEqual(req?["file"], .null)
        XCTAssertEqual(json["collections"]?.arrayValue?[0]["environment"], .null)
        let started = json["startedAt"]?.stringValue ?? ""
        XCTAssertNotNil(started.range(of: #"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$"#, options: .regularExpression), started)
    }

    func testStopOnFail() async throws {
        let server = try MockServer.require()
        let c = Collection(name: "c", requests: [
            Request(name: "a", url: "\(server.base)/status/404"), Request(name: "b", url: "\(server.base)/status/200"),
        ])
        let steps = try await runner().run(c, environment: nil, options: RunOptions(stopOnFail: true))
        XCTAssertEqual(steps.map(\.requestName), ["a"])
    }

    func testGraphQLFormsAndScriptsOnTheWire() async throws {
        let server = try MockServer.require()
        let c = Collection(name: "c", requests: [
            Request(name: "gql", url: "\(server.base)/echo/graphql", method: "POST", body: "query U($id: ID!) { user(id: $id) { name } }",
                    bodyType: "graphql", graphql: GraphQL(variables: "{\"id\": ${id}}", operationName: "U")),
            Request(name: "form", url: "\(server.base)/echo/form", method: "POST", bodyMode: "urlencoded",
                    formFields: [FormField(key: "name", value: "${who}"), FormField(key: "x", value: "a&b")]),
            Request(name: "multi", url: "\(server.base)/echo/multi", method: "POST", bodyMode: "multipart",
                    formFields: [FormField(key: "note", value: "hi"), FormField(key: "f", value: "", type: "file", fileName: "a.bin")]),
            Request(name: "pre", url: "\(server.base)/echo/${ts}", headers: [Header("X-Ts", "${ts}")],
                    preRequestScript: "pm.environment.set('ts', 42); setHeader('X-Script', getVariable('who'))"),
            Request(name: "uses script var", url: "\(server.base)/echo/again/${ts}",
                    postRequestScript: "assert.equal(getResponseValue('path'), '/echo/again/42', 'path')"),
            Request(name: "apikey query", url: "\(server.base)/echo/q?k=old&a=1", auth: .apiKey(key: "k", value: "${who} x", location: .query)),
            Request(name: "no auth", url: "\(server.base)/echo/none", auth: Auth.none),
        ], variables: ["id": "7", "who": "Ada"], auth: .basic(username: "u", password: "p"))
        try server.reset()
        let steps = try await runner().run(c, environment: nil)
        XCTAssertTrue(steps.allSatisfy(\.passed), steps.map { "\($0.requestName): \($0.error ?? "") \($0.tests)" }.joined(separator: "\n"))
        let seen = try server.requests()
        XCTAssertEqual(seen[0]["body"]?.stringValue, #"{"query":"query U($id: ID!) { user(id: $id) { name } }","variables":{"id": 7},"operationName":"U"}"#)
        XCTAssertEqual(seen[0]["headers"]?["content-type"]?.stringValue, "application/json")
        XCTAssertEqual(seen[0]["headers"]?["authorization"]?.stringValue, "Basic dTpw")
        XCTAssertEqual(seen[1]["body"]?.stringValue, "name=Ada&x=a%26b")
        XCTAssertEqual(seen[1]["headers"]?["content-type"]?.stringValue, "application/x-www-form-urlencoded")
        let multi = seen[2]["body"]?.stringValue ?? ""
        XCTAssertTrue(multi.contains("name=\"note\"\r\n\r\nhi\r\n") && multi.contains("filename=\"a.bin\""), multi)
        XCTAssertTrue(seen[2]["headers"]?["content-type"]?.stringValue?.hasPrefix("multipart/form-data; boundary=----hitop") ?? false)
        XCTAssertEqual(seen[3]["path"]?.stringValue, "/echo/42")
        XCTAssertEqual(seen[3]["headers"]?["x-ts"]?.stringValue, "42")
        XCTAssertEqual(seen[3]["headers"]?["x-script"]?.stringValue, "Ada")
        XCTAssertEqual(seen[5]["query"]?.stringValue, "a=1&k=Ada%20x")
        XCTAssertNil(seen[6]["headers"]?["authorization"])
    }

    func testCookiesCarryWithinARunOnly() async throws {
        let server = try MockServer.require()
        let c = Collection(name: "c", requests: [Request(name: "set", url: "\(server.base)/set-cookie"), Request(name: "use", url: "\(server.base)/needs-cookie")])
        let steps = try await runner().run(c, environment: nil)
        XCTAssertEqual(steps.map { $0.response?.status ?? 0 }, [200, 200])
        let second = try await runner().run(Collection(name: "c", requests: [c.requests[1]]), environment: nil)
        XCTAssertEqual(second[0].response?.status, 401, "a new run starts without cookies")
    }

    func testWithoutAnEngineScriptsAreReportedNotRun() async throws {
        let server = try MockServer.require()
        let c = Collection(name: "c", requests: [Request(name: "r", url: "\(server.base)/status/200", preRequestScript: "setHeader('a','b')", postRequestScript: "assert.status(500)")])
        let steps = await Runner(engine: nil).run(c, environment: nil)
        XCTAssertTrue(steps[0].passed)
        XCTAssertEqual(steps[0].notes, ["pre-request script not run (no JavaScript engine here)", "post-request script not run (no JavaScript engine here)"])
        var b = ReportBuilder()
        b.iteration(steps: steps)
        XCTAssertTrue(b.finish(bailed: false).jsonText.contains("\"notes\""))
    }
}

/// HitopKit and the Rust CLI run the same folder against the same server:
/// the requests on the wire and the reports (timings aside) must match.
/// Needs a built CLI: `HITOP_BIN=cli/target/debug/hitop swift test`.
final class CLICompatibilityTests: XCTestCase {
    func normalized(_ v: JSONValue) -> JSONValue {
        switch v {
        case let .object(pairs):
            return .object(pairs.filter { !["durationMs", "responseTimeMs", "startedAt", "tool"].contains($0.0) }.map { ($0.0, normalized($0.1)) })
        case let .array(a): return .array(a.map(normalized))
        default: return v
        }
    }

    /// What the server saw, without headers each client adds on its own.
    func wire(_ seen: [JSONValue]) -> [JSONValue] {
        let own: Set<String> = ["user-agent", "accept-encoding", "accept-language", "connection", "host", "content-length", "accept"]
        return seen.map { r in
            .object((r.objectValue ?? []).map { k, v in
                guard k == "headers", case let .object(h) = v else { return (k, v) }
                return (k, .object(h.filter { !own.contains($0.0) }.sorted { $0.0 < $1.0 }))
            })
        }
    }

    func testSameRequestsAndReportAsTheCLI() async throws {
        guard let bin = ProcessInfo.processInfo.environment["HITOP_BIN"] else { throw XCTSkip("set HITOP_BIN to a built hitop") }
        let server = try MockServer.require()
        let dir = try sampleCopy(port: server.port!)
        // A few more shapes than the sample has.
        try """
        name: Echo all
        method: POST
        url: ${base}/echo/all
        params:
        - name: q
          value: a b
        headers:
        - name: X-Env
          value: ${env}
        auth:
          type: apikey
          key: key
          value: ${apiKey}
          in: query
        body:
          type: graphql
          content: '{ me { id } }'
          variables: '{"n": 1}'
        scripts:
          post: |
            assert.status(200);
            assert.header('content-type', 'json');
            pm.test('pm', function () { pm.expect(pm.response.json().method).to.equal('POST'); });
        """.write(to: dir.appendingPathComponent("requests/echo.yaml"), atomically: true, encoding: .utf8)
        try "variables:\n  env: g\n".write(to: dir.appendingPathComponent("globals.yaml"), atomically: true, encoding: .utf8)
        let out = dir.deletingLastPathComponent().appendingPathComponent("out")

        try server.reset()
        let home = dir.deletingLastPathComponent().appendingPathComponent("home")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let (status, stdout, stderr) = try runProgram(URL(fileURLWithPath: bin), ["run", "sample", "--env", "mock", "--reporter", "json", "--out", out.path, "--no-cookies"],
                                               cwd: dir.deletingLastPathComponent(),
                                               env: ["HOME": home.path, "HITOP_KEYCHAIN": "off", "PATH": "/usr/bin:/bin", "XDG_CONFIG_HOME": home.path])
        XCTAssertEqual(status, 0, stdout + stderr)
        let cliSeen = wire(try server.requests())
        let cliReport = try JSONValue.parse(try String(contentsOf: out.appendingPathComponent("report.json"), encoding: .utf8))

        try server.reset()
        let report = try await FolderRun.run(try CollectionFolder.read(dir), environment: "mock", store: MemorySecretStore(), lookupEnv: { _ in nil },
                                             runner: Runner(engine: try requireEngine()))
        let kitSeen = wire(try server.requests())

        XCTAssertEqual(cliSeen.count, 4)
        XCTAssertEqual(kitSeen.count, cliSeen.count)
        for (k, c) in zip(kitSeen, cliSeen) { XCTAssertEqual(k, c, "\nkit: \(k.compact)\ncli: \(c.compact)") }
        let kit = normalized(try JSONValue.parse(report.jsonText))
        let cli = normalized(cliReport)
        XCTAssertEqual(kit, cli, "\nkit: \(kit.pretty)\ncli: \(cli.pretty)")
    }
}
