import Foundation
import XCTest
import HitopKit
@testable import HITOP

/// I5: what the on-device model is given (secrets never), suggested tests checked against the
/// response and rendered as pm.test, the free-text request shape, the fake assistant.
@MainActor
final class IntelligenceTests: XCTestCase {
    var model: AppModel!

    override func setUp() async throws {
        model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault(), sender: Sender(configuration: StubURLProtocol.configuration),
                         webSockets: EchoWebSocketTransport(), grpcClient: FakeGrpcClient())
        StubURLProtocol.enabled = true
        model.load()
        model.seed()
    }

    /// Create order (1104) sent in Staging: 201 with the drafts' body, a Set-Cookie, the token secret.
    func createOrder() async throws -> (SavedRequest, SendOutcome, [String]) {
        let editor = try XCTUnwrap(model.editor(for: 1104))
        editor.send()
        await editor.waitForSend()
        guard case let .success(o)? = editor.outcome else { throw XCTSkip("no response: \(String(describing: editor.outcome))") }
        return (editor.draft, o, model.secretValues(for: editor.collectionID))
    }

    // MARK: Masking

    func testNoSecretReachesAnyPrompt() async throws {
        var (request, outcome, secrets) = try await createOrder()
        XCTAssertTrue(secrets.contains("tok_staging_8f2"))
        // A request carrying credentials every way: a literal bearer token, an API key in the
        // query, a cookie, a password in the body, and a secret value echoed back by the server.
        request.headers.append(KeyValue("Authorization", "Bearer literal-bearer-123"))
        request.headers.append(KeyValue("Cookie", "sid=cookie-456"))
        request.auth = .basic(username: "ada", password: "basic-pw-789")
        request.body = #"{"user":"ada","password":"body-pw-000","note":"tok_staging_8f2"}"#
        outcome.sentURL = "https://staging.shop.example/v2/orders?expand=items&api_key=query-key-111&token=tok_staging_8f2"
        outcome.text = #"{"id":"ord_1","access_token":"resp-token-222","nested":{"client_secret":"resp-secret-333"},"echo":"tok_staging_8f2"}"#
        outcome.headers.append(["X-Api-Key", "hdr-key-444"])
        let facts = ResponseFacts(outcome)
        let prompts = [Prompts.explain(request: request, facts: facts, secrets: secrets),
                       Prompts.suggestTest(request: request, facts: facts, secrets: secrets)]
        let leaks = ["tok_staging_8f2", "hunter2", "literal-bearer-123", "cookie-456", "basic-pw-789", "body-pw-000", "query-key-111",
                     "resp-token-222", "resp-secret-333", "hdr-key-444", "s_91x"]
        for p in prompts {
            for leak in leaks {
                XCTAssertFalse(p.text.contains(leak), "\(leak) in \(p.text)")
                XCTAssertFalse(p.instructions.contains(leak))
            }
            XCTAssertTrue(p.text.contains("Authorization: Bearer ••••"), "the scheme stays, the credential doesn't")
            XCTAssertTrue(p.text.contains("Auth: basic (value hidden)"))
            XCTAssertTrue(p.text.contains("expand=items"), "other query values stay")
            XCTAssertTrue(p.text.contains("ord_1"))
        }
    }

    func testSensitiveNames() {
        for h in ["Authorization", "Set-Cookie", "X-API-Key", "X-Auth-Token", "x-session-id"] { XCTAssertTrue(ModelInput.isSensitiveHeader(h), h) }
        for h in ["Content-Type", "Cache-Control", "X-RateLimit-Remaining", "Keep-Alive", "Location"] { XCTAssertFalse(ModelInput.isSensitiveHeader(h), h) }
        for k in ["password", "access_token", "apiKey", "client_secret", "key", "sessionId", "auth"] { XCTAssertTrue(ModelInput.isSensitiveKey(k), k) }
        for k in ["id", "status", "author", "total", "monkey"] { XCTAssertFalse(ModelInput.isSensitiveKey(k), k) }
    }

    // MARK: Bodies cut to fit

    func testLargeBodiesAreCutAndShaped() {
        let items = (0..<500).map { #"{"sku":"SKU-\#($0)","name":"\#(String(repeating: "x", count: 400))"}"# }.joined(separator: ",")
        let big = #"{"total":500,"items":[\#(items)]}"#
        let b = ModelInput.body(big, secrets: [])
        XCTAssertTrue(b.cut)
        XCTAssertLessThanOrEqual(b.text.count, ModelInput.bodyLimit + 1)
        XCTAssertTrue(b.text.contains("497 more items"), b.text)
        XCTAssertFalse(b.text.contains("SKU-3"), "only the first three items")
        XCTAssertEqual(b.shape, "object {total: number, items: array of 500 object {sku: string, name: string}}")
        let text = String(repeating: "log line\n", count: 2_000)
        let t = ModelInput.body(text, secrets: [])
        XCTAssertTrue(t.cut)
        XCTAssertNil(t.shape)
        XCTAssertEqual(t.text.count, ModelInput.bodyLimit + 1)
        let small = ModelInput.body(#"{"ok":true}"#, secrets: [])
        XCTAssertFalse(small.cut)
        XCTAssertEqual(small.text, "{\n  \"ok\": true\n}")
        let prompt = Prompts.explain(request: SavedRequest(url: "https://x.test"), facts: ResponseFacts(status: 200, headers: [], text: big, durationMs: 5,
                                                                                                         timing: nil, tests: [], sentURL: "https://x.test"),
                                     secrets: [])
        XCTAssertLessThan(prompt.text.count + prompt.instructions.count, 5_000, "fits the on-device context window")
    }

    func testExplainPromptHasTheFacts() async throws {
        let (request, outcome, secrets) = try await createOrder()
        let p = Prompts.explain(request: request, facts: ResponseFacts(outcome), secrets: secrets)
        XCTAssertTrue(p.text.contains("Status: 201 Created"), p.text)
        XCTAssertTrue(p.text.contains("POST https://staging.shop.example/v2/orders?expand=items"))
        XCTAssertTrue(p.text.contains("pending_payment"))
        XCTAssertTrue(p.text.contains("BODY SHAPE\nobject {id: string, status: string, total: object"))
        XCTAssertTrue(p.text.contains("Set-Cookie: ••••"))
        XCTAssertFalse(p.text.contains("s_91x"))
    }

    // MARK: Suggested tests

    func testChecksAreValidatedAgainstTheResponse() async throws {
        let (_, outcome, _) = try await createOrder()
        let facts = ResponseFacts(outcome)
        let checks = [
            SuggestedCheck(.status, expected: "201", name: "Order is created"),
            SuggestedCheck(.status, expected: "200"),
            SuggestedCheck(.equals, path: "status", expected: "pending_payment", name: "order waits for payment"),
            SuggestedCheck(.equals, path: "total.amount", expected: "6480"),
            SuggestedCheck(.equals, path: "total.amount", expected: "999"),
            SuggestedCheck(.type, path: "links.pay", expected: "String"),
            SuggestedCheck(.exists, path: "items[1].sku"),
            SuggestedCheck(.exists, path: "items[5].sku"),
            SuggestedCheck(.header, path: "Content-Type", expected: "application/json"),
            SuggestedCheck(.header, path: "X-Missing"),
            SuggestedCheck(.responseTime, expected: "1"),
            SuggestedCheck(.equals, path: "bad[path", expected: "1"),
            SuggestedCheck(.status, expected: "201", name: "duplicate"),
        ]
        let s = TestScript.suggestion(checks, facts: facts)
        XCTAssertEqual(s.dropped, 7)
        XCTAssertEqual(s.checks.map(\.kind), [.status, .header, .equals, .equals, .exists, .type].prefix(TestScript.maxChecks).map { $0 })
        XCTAssertEqual(s.script, """
        const body = pm.response.json();
        pm.test("Order is created", () => pm.response.to.have.status(201));
        pm.test("Content-Type includes application/json", () => pm.expect(pm.response.headers.get("Content-Type")).to.include("application/json"));
        pm.test("order waits for payment", () => pm.expect(body.status).to.eql("pending_payment"));
        pm.test("total.amount is 6480", () => pm.expect(body.total.amount).to.eql(6480));
        pm.test("items[1].sku is present", () => pm.expect(body.items[1].sku).to.exist);
        pm.test("links.pay is a string", () => pm.expect(body.links.pay).to.be.a("string"));
        """)
    }

    func testPaths() {
        XCTAssertEqual(JSONPath.parse("a.b[0].c"), [.key("a"), .key("b"), .index(0), .key("c")])
        XCTAssertEqual(JSONPath.parse("$.a[\"x-y\"]"), [.key("a"), .key("x-y")])
        XCTAssertEqual(JSONPath.parse("body.a"), [.key("a")])
        XCTAssertNil(JSONPath.parse("a[b"))
        XCTAssertEqual(JSONPath.js("a[\"x-y\"][2]"), "body.a[\"x-y\"][2]")
        XCTAssertEqual(JSONPath.js("ünï"), "body[\"ünï\"]")
        let v = try! JSONValue.parse(#"{"a":{"x-y":[1,2,{"z":null}]}}"#)
        XCTAssertEqual(JSONPath.value(v, "a[\"x-y\"][1]"), .number("2"))
        XCTAssertEqual(JSONPath.value(v, "a[\"x-y\"][2].z"), .null)
        XCTAssertNil(JSONPath.value(v, "a.q"))
    }

    func testHeuristicAndResponseTime() {
        let facts = ResponseFacts(status: 200, headers: [["Content-Type", "application/json; charset=utf-8"]],
                                  text: #"{"state":"ok","count":3,"token":"t","items":[]}"#, durationMs: 120, timing: nil, tests: [], sentURL: "")
        let h = TestScript.heuristic(facts)
        XCTAssertEqual(h.map(\.kind), [.status, .header, .equals, .type, .type])
        XCTAssertFalse(h.contains { $0.path == "token" }, "never a credential's value or type")
        XCTAssertEqual(TestScript.validated(h, facts: facts).dropped, 0)
        XCTAssertTrue(TestScript.holds(SuggestedCheck(.responseTime, expected: "1000"), facts))
        XCTAssertFalse(TestScript.holds(SuggestedCheck(.responseTime, expected: "200"), facts), "no test that fails at random")
        XCTAssertEqual(TestScript.appending("b();", to: "a();\n\n"), "a();\n\nb();")
        XCTAssertEqual(TestScript.appending("b();", to: "  "), "b();")
    }

    /// The rendered script passes when the request is sent again (JavaScriptCore + pm-shim).
    func testSuggestedScriptPassesOnTheSameResponse() async throws {
        let (request, outcome, secrets) = try await createOrder()
        let session = ExplainSession(assistant: FakeAssistant(), request: request, facts: ResponseFacts(outcome), secrets: secrets)
        let fake = FakeAssistant()
        fake.checks = TestScript.heuristic(ResponseFacts(outcome)) + [SuggestedCheck(.exists, path: "links.pay"), SuggestedCheck(.status, expected: "500")]
        let s2 = ExplainSession(assistant: fake, request: request, facts: ResponseFacts(outcome), secrets: secrets)
        await s2.suggest()
        let suggestion = try XCTUnwrap(s2.suggestion)
        XCTAssertEqual(suggestion.dropped, 1)
        XCTAssertNil(session.suggestion)
        #if canImport(JavaScriptCore)
        let editor = try XCTUnwrap(model.editor(for: 1104))
        editor.draft.postRequestScript = suggestion.script
        editor.send()
        await editor.waitForSend()
        guard case let .success(again)? = editor.outcome else { return XCTFail("no response") }
        XCTAssertEqual(again.tests.count, suggestion.checks.count, "\(again.tests)")
        XCTAssertTrue(again.tests.allSatisfy(\.pass), "\(again.tests)")
        #endif
    }

    // MARK: The fake and the sessions

    func testExplainStreamsAndFailsGently() async throws {
        let (request, outcome, secrets) = try await createOrder()
        let fake = FakeAssistant()
        let s = ExplainSession(assistant: fake, request: request, facts: ResponseFacts(outcome), secrets: secrets)
        await s.explain()
        XCTAssertTrue(s.text.hasPrefix("The server answered 201 Created."), s.text)
        XCTAssertNil(s.failure)
        XCTAssertEqual(fake.prompts.count, 1)
        fake.failure = .tooLong
        await s.explain()
        XCTAssertEqual(s.failure, AssistantFailure.tooLong.message)
        let off = ExplainSession(assistant: FakeAssistant(availability: .turnedOff), request: request, facts: ResponseFacts(outcome), secrets: secrets)
        await off.explain()
        await off.suggest()
        XCTAssertEqual(off.failure, AssistantAvailability.turnedOff.reason)
        XCTAssertEqual(off.suggestFailure, AssistantAvailability.turnedOff.reason)
    }

    func testUnavailableReasonsHaveNoPurchaseWords() {
        let all = [AssistantAvailability.notEligible, .turnedOff, .notReady, .unsupported].compactMap(\.reason)
            + [AssistantFailure.tooLong, .refused, .language, .busy, .other].map(\.message)
        for t in all {
            for w in ["upgrade", "buy", "purchase", "price", "plan", "subscribe", "$"] { XCTAssertFalse(t.lowercased().contains(w), t) }
        }
    }

    // MARK: A request from text or curl

    func testRequestFromCurlNeedsNoModel() async throws {
        let off = FakeAssistant(availability: .notEligible)
        let made = try await RequestFromText.make("curl -X POST https://x.test/orders -H 'Content-Type: application/json' -d '{\"a\":1}' --frobnicate",
                                                 assistant: off, variables: [])
        XCTAssertEqual(made.request.method, "POST")
        XCTAssertEqual(made.request.bodyKind, .json)
        XCTAssertEqual(made.notes, ["Left out: --frobnicate."])
        XCTAssertTrue(off.prompts.isEmpty, "curl never goes to the model")
        do {
            _ = try await RequestFromText.make("POST a JSON order", assistant: off, variables: [])
            XCTFail("free text needs the model")
        } catch {
            XCTAssertEqual((error as? AssistantFailure)?.message, AssistantAvailability.notEligible.reason)
        }
    }

    func testRequestFromText() async throws {
        let fake = FakeAssistant()
        let made = try await RequestFromText.make("POST a JSON order to ${baseUrl}/v2/orders.", assistant: fake, variables: ["baseUrl", "token"])
        XCTAssertEqual(made.request.method, "POST")
        XCTAssertEqual(made.request.url, "${baseUrl}/v2/orders")
        XCTAssertEqual(made.request.bodyKind, .json)
        XCTAssertEqual(made.request.body, "{\n  \"example\": true\n}")
        XCTAssertTrue(fake.prompts[0].text.contains("VARIABLES (use as ${name})\nbaseUrl, token"))
        XCTAssertEqual(RequestFromText.source("curl https://x"), .curl)
        XCTAssertEqual(RequestFromText.source("get the users"), .model)
    }

    func testDraftMapping() {
        let r = RequestDraft(name: " Create order ", method: "post", url: " https://x.test/a b ",
                             headers: [.init(name: "Content-Type", value: "application/json"), .init(name: "Bad Name", value: "x"), .init(name: "", value: "y")],
                             bodyKind: "json", body: #"{"a":1}"#).savedRequest
        XCTAssertEqual(r.method, "POST")
        XCTAssertEqual(r.url, "https://x.test/ab")
        XCTAssertEqual(r.headers.map(\.name), ["Content-Type"])
        XCTAssertEqual(r.body, "{\n  \"a\": 1\n}")
        XCTAssertEqual(r.name, "Create order")
        XCTAssertEqual(RequestDraft(method: "FETCH", url: "https://x").savedRequest.method, "GET")
        let form = RequestDraft(method: "POST", url: "https://x", bodyKind: "form", body: "a=1&b=two%20words").savedRequest
        XCTAssertEqual(form.bodyKind, .form)
        XCTAssertEqual(form.formFields.map(\.value), ["1", "two words"])
        XCTAssertEqual(RequestDraft(method: "GET", url: "https://x", bodyKind: "json", body: "").savedRequest.bodyKind, .none)
        XCTAssertEqual(RequestDraft(url: "https://x").savedRequest.name, AutoName.untitled)
    }
}
