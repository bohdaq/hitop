import Foundation
import XCTest
@testable import HitopKit

final class ScriptTests: XCTestCase {
    func testShimIsTheSharedFile() throws {
        let shared = try String(contentsOf: repoRoot.appendingPathComponent("shared/pm-shim.js"), encoding: .utf8)
        XCTAssertFalse(Scripts.pmShim.isEmpty)
        XCTAssertEqual(Scripts.pmShim, shared, "copy shared/pm-shim.js to ios/HitopKit/Sources/HitopKit/Resources/")
    }

    func pre(_ script: String, url: String = "https://x", headers: [(String, String)] = [], variables: [String: String] = [:],
             context: [String: JSONValue] = [:]) throws -> PreRequestResult {
        try Scripts.runPreRequest(script, PreRequestInput(url: url, method: "GET", headers: headers, body: "", variables: variables,
                                                          context: context, globals: [:]), engine: try requireEngine())
    }

    func testPreRequestAPI() throws {
        let out = try pre("setUrl(getVariable('base') + '/o/' + getContext('id')); setHeader('X-A', '1'); setHeader('Accept', 'x'); setContext('k', {n: 1}); setBody('b')",
                          headers: [("Accept", "*/*")], variables: ["base": "http://h"], context: ["id": .number("7")])
        XCTAssertEqual(out.url, "http://h/o/7")
        XCTAssertEqual(out.body, "b")
        XCTAssertEqual(out.headers.map { "\($0.0)=\($0.1)" }, ["Accept=x", "X-A=1"])
        XCTAssertEqual(out.contextUpdates["k"], .object([("n", .number("1"))]))
        XCTAssertTrue(out.variables.isEmpty)
    }

    func testPostmanVariablesAndHeaders() throws {
        let out = try pre("pm.environment.set('ts', 42); pm.variables.set('v', 'new'); pm.environment.unset('gone'); pm.request.headers.remove('X-Old')",
                          headers: [("X-Old", "1"), ("Keep", "2")], variables: ["gone": "x"])
        XCTAssertEqual(out.variables["ts"], .some("42"))
        XCTAssertEqual(out.variables["v"], .some("new"))
        XCTAssertEqual(out.variables["gone"], .some(nil))
        XCTAssertEqual(out.headers.map(\.0), ["Keep"])
    }

    func testScriptErrorsAreErrors() throws {
        XCTAssertThrowsError(try pre("this is not js(")) { XCTAssertTrue("\($0)".hasPrefix("pre-request script error"), "\($0)") }
        XCTAssertThrowsError(try pre("throw new Error('boom')")) { XCTAssertTrue("\($0)".contains("boom"), "\($0)") }
    }

    func testAssertionsAreRecordedNotThrown() throws {
        let out = try Scripts.runPostRequest("""
        assert.status(200); assert.status(201); assert.bodyContains('Ada'); assert.header('Content-Type', 'json');
        assert.header('X-None'); assert.equal(getResponseValue('user.name'), 'Ada', 'name'); assert.ok(false, 'flag');
        assert.responseTime(1000); setContext('id', getResponseValue('id')); pm.environment.set('seen', 'yes');
        """, PostRequestInput(responseBody: #"{"id": 9, "user": {"name": "Ada"}}"#, responseHeaders: ["content-type": "application/json"],
                              statusCode: 200, durationMs: 5, variables: [:], context: [:], request: nil, globals: nil), engine: try requireEngine())
        XCTAssertEqual(out.tests.map(\.name), ["status is 200", "status is 201", "body contains \"Ada\"", "header Content-Type is \"json\"",
                                                "has header X-None", "name", "flag", "responds within 1000 ms"])
        XCTAssertEqual(out.tests.map(\.pass), [true, false, true, true, false, true, false, true])
        XCTAssertEqual(out.tests[1].detail, "got 200")
        XCTAssertEqual(out.tests[4].detail, "header missing")
        XCTAssertEqual(out.tests[6].detail, "was false")
        XCTAssertEqual(out.contextUpdates["id"], .number("9"))
        XCTAssertEqual(out.variables["seen"], .some("yes"))
    }

    func testPostmanTests() throws {
        let out = try Scripts.runPostRequest("""
        pm.test('ok', function () { pm.response.to.have.status(200); pm.expect(pm.response.json().a).to.eql(1); });
        pm.test('bad', function () { pm.expect(pm.response.code).to.equal(201); });
        """, PostRequestInput(responseBody: #"{"a": 1}"#, responseHeaders: [:], statusCode: 200, durationMs: 1, variables: [:], context: [:],
                              request: SentRequest(url: "http://x", method: "GET", headers: [], body: ""), globals: [:]), engine: try requireEngine())
        XCTAssertEqual(out.tests.map(\.name), ["ok", "bad"])
        XCTAssertEqual(out.tests.map(\.pass), [true, false])
    }
}

final class SecretTests: XCTestCase {
    func testNamesAndAccounts() {
        XCTAssertEqual(SecretNames.envVar(environment: "prod", name: "token"), "HITOP_SECRET_PROD_TOKEN")
        XCTAssertEqual(SecretNames.envVar(environment: "my-env", name: "api.key"), "HITOP_SECRET_MY_ENV_API_KEY")
        XCTAssertEqual(SecretNames.envVar(environment: "é", name: "x"), "HITOP_SECRET___X")
        XCTAssertEqual(SecretNames.account(workspace: "w", environment: "prod", name: "token"), "w/prod/token")
    }

    func testResolveOrder() throws {
        let env = Environment(id: "prod", name: "prod", secrets: ["a", "b", "c"])
        let store = MemorySecretStore(["w/prod/a": "from-keychain-a", "w/prod/b": "from-keychain-b", "w/prod/c": "from-keychain-c"])
        let lookup: (String) -> String? = { $0 == "HITOP_SECRET_PROD_A" ? "from-env" : nil }
        let (s, notes) = try resolveSecrets(workspace: "w", environment: env, given: ["b": "given", "extra": "x"], store: store, lookupEnv: lookup)
        XCTAssertEqual(s["a"], "from-env")
        XCTAssertEqual(s["b"], "given")
        XCTAssertEqual(s["c"], "from-keychain-c")
        XCTAssertEqual(s["extra"], "x")
        XCTAssertEqual(s.values["c"]?.origin, .keychain)
        XCTAssertTrue(notes.isEmpty)
    }

    func testMissingAndNoKeychain() {
        let env = Environment(id: "prod", name: "prod", secrets: ["token", "key"])
        XCTAssertThrowsError(try resolveSecrets(workspace: "w", environment: env, store: NoSecretStore("none here"), lookupEnv: { _ in nil })) { e in
            let m = e as? MissingSecrets
            XCTAssertEqual(m?.names, ["key", "token"])
            XCTAssertEqual(m?.note, "no keychain available: none here")
        }
    }

    func testKeychainStoreRoundTrip() throws {
        let store = MemorySecretStore()
        try store.set(account: "w/e/n", value: "v")
        XCTAssertEqual(try store.get(account: "w/e/n"), "v")
        XCTAssertTrue(try store.delete(account: "w/e/n"))
        XCTAssertFalse(try store.delete(account: "w/e/n"))
        XCTAssertNil(try store.get(account: "w/e/n"))
        #if canImport(Security) && os(macOS)
        // The real keychain, only where a test may touch it.
        if ProcessInfo.processInfo.environment["HITOP_TEST_KEYCHAIN"] == "1" {
            let k = KeychainSecretStore()
            let account = "hitopkit-test/\(UUID().uuidString)/x"
            try k.set(account: account, value: "s3cr3t")
            XCTAssertEqual(try k.get(account: account), "s3cr3t")
            XCTAssertTrue(try k.delete(account: account))
        }
        #endif
    }

    func testMasker() {
        let m = Masker(["s3cr3t", "a b", "s3", ""])
        XCTAssertEqual(m.mask("x=s3cr3t&y=a%20b z=a b"), "x=••••&y=•••• z=••••")
        XCTAssertEqual(m.mask("s3 and s3cr3t"), "•••• and ••••")
        XCTAssertTrue(Masker([]).isEmpty)
    }

    func testFindsSecretManagerReferences() {
        XCTAssertEqual(SecretManagerReference.find("Bearer ${secret:vault:kv/app#token}"), "secret:vault:kv/app#token")
        XCTAssertNil(SecretManagerReference.find("${token}"))
    }

    func testRunStatusStore() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("hitopkit-status-\(UUID())")
        let store = RunStatusStore(directory: dir)
        XCTAssertTrue(store.all().isEmpty)
        var b = ReportBuilder()
        b.collection("C", environment: "e")
        b.iteration(steps: [StepResult(requestName: "r", method: "GET", url: "u", response: nil, error: "refused", tests: [],
                                       durationMs: 1, notes: [], path: "r", fileExtension: "yaml")])
        let status = RunStatus(id: "one", report: b.finish(bailed: false), finishedAt: Date(timeIntervalSince1970: 1_700_000_000))
        try store.save(status)
        try store.save(RunStatus(id: "two", report: b.finish(bailed: false)))
        try store.save(status)
        XCTAssertEqual(store.all().map(\.id), ["two", "one"])
        XCTAssertEqual(store.status("one"), status)
        XCTAssertEqual(status.summary, "0/1 passed")
        XCTAssertEqual(status.firstFailure, "r: refused")
        XCTAssertFalse(status.passed)
    }
}
