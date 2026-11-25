import Foundation
import XCTest
import HitopKit
@testable import HITOP

/// I4: the saved shapes of WebSocket and gRPC requests, the method menu's rules, Server-Sent
/// Events, WebSocket targets and sessions, gRPC servers and sessions, GraphQL schemas and
/// suggestions. Linux too (no network: the echo socket, the canned Greeter, StubURLProtocol).
@MainActor
final class ProtocolTests: XCTestCase {
    var model: AppModel!

    override func setUp() async throws {
        model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault(), sender: Sender(configuration: StubURLProtocol.configuration),
                         webSockets: EchoWebSocketTransport(), grpcClient: FakeGrpcClient())
        StubURLProtocol.enabled = true
        model.load()
        model.seed()
    }

    func decode(_ json: String) throws -> SavedRequest { try JSONDecoder().decode(SavedRequest.self, from: Data(json.utf8)) }
    func encode(_ r: SavedRequest) throws -> JSONAny { try JSONAny.from(r) }

    func until(_ what: String, timeout: TimeInterval = 5, _ cond: @escaping () -> Bool) async {
        let end = Date().addingTimeInterval(timeout)
        while !cond() && Date() < end { try? await Task.sleep(for: .milliseconds(20)) }
        let ok = cond()
        XCTAssertTrue(ok, what)
    }

    // MARK: Saved shapes

    func testWebSocketFieldsKeepTheirShapeAndUnknownKeys() throws {
        var r = try decode("""
        {"id": 7, "name": "Live", "url": "wss://x.test/ws", "method": "GET", "kind": "websocket",
         "websocket": {"subprotocols": ["a.v2", "a.v1"], "messages": [{"name": "hi", "content": "hello", "pinned": true}], "future": 1}}
        """)
        XCTAssertTrue(r.isWebSocket)
        XCTAssertEqual(r.mode, .websocket)
        XCTAssertEqual(r.websocket.subprotocols, ["a.v2", "a.v1"])
        XCTAssertEqual(r.websocket.messages.map(\.content), ["hello"])
        r.websocket.messages.append(WSSavedMessage(name: "ping", content: "ping"))
        let out = try encode(r)
        XCTAssertEqual(out["kind"], .string("websocket"))
        XCTAssertEqual(out["websocket"]?["future"], .number(1))
        XCTAssertEqual(out["websocket"]?["messages"]?.array?.first?["pinned"], .bool(true))
        XCTAssertEqual(out["websocket"]?["messages"]?.array?.last, .object(["name": .string("ping"), "content": .string("ping")]))
        // Back to HTTP: `kind` goes, the WebSocket fields stay (the web app keeps them too).
        r.setMode(.http("GET"))
        let http = try encode(r)
        XCTAssertNil(http["kind"])
        XCTAssertNotNil(http["websocket"])
    }

    func testGrpcFieldsAsTheCLIWritesThem() throws {
        var r = try decode("""
        {"id": 8, "name": "Hello", "url": "localhost:50051", "method": "GET", "kind": "grpc",
         "grpc": {"service": "hitop.test.Greeter", "method": "SayHello", "message": "{}", "protos": ["p.proto"], "plaintext": true}}
        """)
        XCTAssertTrue(r.isGrpc)
        XCTAssertEqual(r.grpc.protos, ["p.proto"])
        r.grpc.method = "StreamHellos"
        r.grpc.plaintext = false
        let g = try encode(r)["grpc"]
        XCTAssertEqual(g, .object(["service": .string("hitop.test.Greeter"), "method": .string("StreamHellos"), "message": .string("{}"),
                                   "protos": .array([.string("p.proto")])]), "empty fields and plaintext false are left out")
        var empty = SavedRequest()
        empty.grpc = GrpcFields()
        XCTAssertNil(try encode(empty)["grpc"])
    }

    func testMethodMenuRules() {
        var r = SavedRequest(url: "https://x.test/feed", method: "POST", headers: [KeyValue("Accept", "application/json"), KeyValue("X-A", "1")])
        r.setMode(.sse)
        XCTAssertEqual(r.method, "GET")
        XCTAssertEqual(r.headers.map(\.name), ["X-A", "Accept"])
        XCTAssertEqual(r.headers.last?.value, "text/event-stream")
        XCTAssertEqual(r.mode, .sse)
        XCTAssertEqual(WidgetSnapshot.methodLabel(r), "SSE")
        // SSE → POST keeps the header (POST streams exist); SSE → GET takes it out.
        var post = r
        post.setMode(.http("POST"))
        XCTAssertTrue(post.asksForEvents)
        XCTAssertEqual(post.mode, .http("POST"))
        r.setMode(.http("GET"))
        XCTAssertFalse(r.asksForEvents)
        XCTAssertEqual(r.headers.map(\.name), ["X-A"])
        r.setMode(.grpc)
        XCTAssertEqual(WidgetSnapshot.methodLabel(r), "GRPC")
        XCTAssertEqual(r.headers.map(\.name), ["X-A"], "switching kind keeps the rest")
    }

    // MARK: Server-Sent Events

    func testEventsFollowTheWebAppsParser() {
        let (events, rest) = SSE.parse(": comment\nevent: price\nid: 7\ndata: {\"a\":1}\n\ndata: one\ndata:two\r\n\r\nevent:\ndata: x\n\nid: 9\n\ndata: unfinished")
        XCTAssertEqual(events, [SSE.Parsed(event: "price", data: "{\"a\":1}", id: "7"), SSE.Parsed(event: "message", data: "one\ntwo", id: nil),
                                SSE.Parsed(event: "message", data: "x", id: nil)], "an event without data is dropped")
        XCTAssertEqual(rest, "data: unfinished")
    }

    func testTheReaderTakesPiecesSplitAnywhere() {
        let text = "event: tick\ndata: prix 12 €\n\ndata: ✓ done\n\n"
        let bytes = Array(text.utf8)
        var r = SSE.Reader()
        var out: [SSEEvent] = []
        for b in bytes { out += r.feed(Data([b])) }
        XCTAssertEqual(out.map(\.data), ["prix 12 €", "✓ done"])
        XCTAssertEqual(out.map(\.event), ["tick", "message"])
        XCTAssertEqual(out.map(\.id), [0, 1])
        XCTAssertEqual(SSE.events(inBody: Data("data: last".utf8)).map(\.data), ["last"], "a whole body's last event ends with it")
    }

    func testAnEventStreamComesBackWithItsEvents() async throws {
        let feed = try XCTUnwrap(model.request(1402))
        XCTAssertEqual(feed.mode, .sse)
        let editor = RequestEditor(request: feed, model: model)
        editor.send()
        await editor.waitForSend()
        guard case let .success(o)? = editor.outcome else { return XCTFail("\(String(describing: editor.outcome))") }
        XCTAssertEqual(o.events?.map(\.event), ["order.created", "order.packed", "order.shipped"])
        XCTAssertEqual(o.events?.last?.data, "{\"id\":\"ord_8KQ2mZ41\",\"carrier\":\"DHL\",\n\"tracking\":\"JD0146 0001 2345\"}")
        XCTAssertEqual(o.events?.first?.lastEventID, "1")
        #if canImport(Darwin)
        await until("read live") { editor.liveEvents.count == 3 }
        #endif
    }

    #if canImport(Darwin)
    func testStopEndsAStreamThatNeverEnds() async throws {
        let editor = RequestEditor(request: try XCTUnwrap(model.request(1403)), model: model)
        editor.send()
        await until("ticks arrive") { editor.liveEvents.count >= 3 }
        XCTAssertTrue(editor.isStreaming)
        XCTAssertEqual(editor.streamOpen?.status, 200)
        editor.stopStream()
        await editor.waitForSend()
        guard case let .success(o)? = editor.outcome else { return XCTFail("\(String(describing: editor.outcome))") }
        XCTAssertTrue(o.stopped)
        XCTAssertGreaterThanOrEqual(o.events?.count ?? 0, 3)
        XCTAssertEqual(model.history.first?.response?.status, 200, "a stopped stream is in the history")
    }
    #endif

    func testRunsAndIntentsStillLeaveSocketsAndCallsOut() async throws {
        let r = await model.sender.send(try XCTUnwrap(model.request(1401)), collection: model.collection(1004), context: model.context(for: 1004))
        guard case let .failure(f) = r.outcome else { return XCTFail() }
        XCTAssertTrue(f.message.contains("WebSocket"))
    }

    // MARK: WebSocket

    func testWebSocketTargets() throws {
        XCTAssertEqual(try WebSocketTargets.wsURL("https://h/x"), "wss://h/x")
        XCTAssertEqual(try WebSocketTargets.wsURL("HTTP://h"), "ws://h")
        XCTAssertEqual(try WebSocketTargets.wsURL("wss://h"), "wss://h")
        XCTAssertThrowsError(try WebSocketTargets.wsURL("ftp://h"))
        XCTAssertThrowsError(try WebSocketTargets.wsURL("h:1"))
        let r = try XCTUnwrap(model.request(1401))
        let vars = model.context(for: 1004).merged
        let t = try WebSocketTargets.make(r, collectionAuth: model.collection(1004)?.auth, variables: vars)
        XCTAssertEqual(t.url, "wss://stream.shop.example/v2/stock?market=eu")
        XCTAssertEqual(t.headers, [["X-Client", "hitop"], ["Authorization", "Bearer tok_staging_8f2"]])
        XCTAssertEqual(t.subprotocols, ["stock.v2", "stock.v1"])
        var key = r
        key.auth = .apiKey(key: "key", value: "${token}", inQuery: true)
        XCTAssertEqual(try WebSocketTargets.make(key, collectionAuth: nil, variables: vars).url, "wss://stream.shop.example/v2/stock?market=eu&key=tok_staging_8f2")
        var missing = r
        missing.url = "wss://h/${nope}"
        XCTAssertThrowsError(try WebSocketTargets.make(missing, collectionAuth: nil, variables: vars)) { XCTAssertTrue("\($0.localizedDescription)".contains("nope")) }
    }

    func testAConnectionAndItsLog() async throws {
        let editor = RequestEditor(request: try XCTUnwrap(model.request(1401)), model: model)
        let s = editor.socket
        editor.send()
        await until("open") { s.status == .open }
        XCTAssertEqual(s.negotiated, "stock.v2")
        XCTAssertEqual(s.log.first?.text, "Connecting to wss://stream.shop.example/v2/stock?market=eu (subprotocols stock.v2, stock.v1)")
        await editor.sendMessage(#"{"op": "subscribe", "symbol": "${symbol}"}"#)
        await until("echo") { s.log.contains { $0.kind == .received && $0.text == #"{"op": "subscribe", "symbol": "TEE-0193"}"# } }
        XCTAssertTrue(s.log.contains { $0.kind == .sent && $0.text == #"{"op": "subscribe", "symbol": "TEE-0193"}"# })
        await editor.sendMessage("bin")
        await until("binary") { s.log.contains { $0.bytes == Data([0, 255]) } }
        XCTAssertEqual(s.log.last { $0.bytes != nil }?.display, "Binary, 2 B · base64 AP8=")
        await editor.sendMessage("the token is ${token}")
        await until("masked") { s.log.contains { $0.kind == .received && $0.text == "the token is ••••" } }
        await editor.sendMessage("${nope}")
        XCTAssertTrue(s.log.last?.text?.hasPrefix("Not sent:") == true)
        await editor.sendMessage("close")
        await until("closed") { s.status == .closed }
        XCTAssertEqual(s.log.last?.text, "Closed 4001 bye")
        await editor.sendMessage("late")
        XCTAssertEqual(s.log.last?.text, "Not sent: connect first.")
        // Send toggles: connect, then disconnect.
        editor.send()
        await until("open again") { s.status == .open }
        editor.send()
        await until("closed by us") { s.status == .closed }
        XCTAssertEqual(s.log.last?.text, "Closed 1000")
        s.clear()
        XCTAssertTrue(s.log.isEmpty)
    }

    func testAFailedConnection() async throws {
        var r = try XCTUnwrap(model.request(1401))
        r.url = "wss://unreachable.test/ws"
        let editor = RequestEditor(request: r, model: model)
        editor.connect()
        await until("closed") { editor.socket.status == .closed && editor.socket.log.count >= 2 }
        XCTAssertEqual(editor.socket.log.map(\.kind), [.system, .error])
        XCTAssertTrue(editor.socket.log[1].text?.hasPrefix("Couldn't connect") == true)
    }

    func testAViewerConnectsAndCalls() async throws {
        model.sharedRoles[1004] = "viewer"
        XCTAssertTrue(model.isReadOnly(1004))
        let editor = RequestEditor(request: try XCTUnwrap(model.request(1401)), model: model)
        editor.send()
        await until("open") { editor.socket.status == .open }
        let call = RequestEditor(request: try XCTUnwrap(model.request(1404)), model: model)
        call.send()
        await until("status") { call.grpcSession.lastStatus != nil }
        XCTAssertEqual(call.grpcSession.lastStatus, .status(code: 0, name: "OK", message: "", trailers: []))
    }

    // MARK: gRPC

    func testGrpcServerMetadata() throws {
        let r = try XCTUnwrap(model.request(1404))
        let s = try GrpcServer.of(r, collectionAuth: model.collection(1004)?.auth, variables: model.context(for: 1004).merged)
        XCTAssertEqual(s.address, "grpc.shop.example:443")
        XCTAssertFalse(s.plaintext)
        XCTAssertEqual(s.metadata, [["x-tenant", "acme"], ["Authorization", "Bearer tok_staging_8f2"]])
        var q = r
        q.auth = .apiKey(key: "k", value: "v", inQuery: true)
        XCTAssertEqual(try GrpcServer.of(q, collectionAuth: nil, variables: [:]).metadata, [["x-tenant", "acme"]], "an API key in the query isn't sent")
        XCTAssertEqual(GrpcEvent(json: .object(["type": .string("status"), "code": .number(5), "name": .string("NOT_FOUND"), "message": .string("no"),
                                                "trailers": .array([.array([.string("x-reason"), .string("test")])])])),
                       .status(code: 5, name: "NOT_FOUND", message: "no", trailers: [["x-reason", "test"]]))
        XCTAssertEqual(GrpcEvent(json: .object(["type": .string("message"), "data": .string("{}")])), .message("{}"))
        XCTAssertNil(GrpcEvent(json: .object(["type": .string("other")])))
    }

    func testSchemaTemplateAndCalls() async throws {
        let editor = RequestEditor(request: try XCTUnwrap(model.request(1405)), model: model)
        let g = editor.grpcSession
        await editor.loadGrpcSchema()
        XCTAssertEqual(g.listing?.source, "reflection v1")
        XCTAssertEqual(g.listing?.services.first?.methods.map(\.shape), ["unary", "server streaming", "unary", "bidirectional"])
        editor.draft.grpc.message = ""
        await editor.useGrpcTemplate()
        XCTAssertTrue(editor.draft.grpc.message.contains("\"times\": 0"))
        editor.draft.grpc.message = #"{"name": "Ada", "times": 3}"#
        editor.send()
        await until("done") { !g.calling && g.lastStatus != nil }
        guard g.log.count == 5 else { return XCTFail("\(g.log.map(\.event))") }
        if case let .headers(h) = g.log[0].event { XCTAssertEqual(h, [["x-served-by", "hitop-test"]]) } else { XCTFail() }
        XCTAssertEqual(g.log[3].event, .message("{\n  \"index\": 2,\n  \"message\": \"Hello #2, Ada\"\n}"))
        XCTAssertEqual(g.lastStatus, .status(code: 0, name: "OK", message: "", trailers: []))
        // Cancel part way.
        editor.draft.grpc.message = #"{"times": 40}"#
        editor.send()
        await until("a message") { g.log.count > 7 }
        editor.send()
        XCTAssertFalse(g.calling)
        XCTAssertEqual(g.lastStatus, .status(code: 1, name: "CANCELLED", message: "Cancelled", trailers: []))
        let after = g.log.count
        try await Task.sleep(for: .milliseconds(400))
        XCTAssertEqual(g.log.count, after, "nothing after Cancel")
    }

    func testNoReflectionIsSaidPlainly() async throws {
        let editor = RequestEditor(request: try XCTUnwrap(model.request(1406)), model: model)
        await editor.loadGrpcSchema()
        XCTAssertTrue(editor.grpcSession.noReflection)
        XCTAssertTrue(editor.grpcSession.schemaError?.contains("no reflection service") == true)
        editor.send()
        XCTAssertEqual(editor.grpcSession.log.last?.event, .error("No gRPC method: pick the service and method (load them from the server, or type package.Service and Method)."))
    }

    // MARK: GraphQL

    func testIntrospectionThroughTheSendPath() async throws {
        let editor = RequestEditor(request: try XCTUnwrap(model.request(1110)), model: model)
        let before = model.history.count
        await editor.fetchSchema()
        XCTAssertNil(editor.schemaError)
        let schema = try XCTUnwrap(editor.schema)
        XCTAssertEqual(schema.queryType, "Query")
        XCTAssertEqual(schema.browsable.prefix(3).map(\.name), ["Query", "Mutation", "Category"])
        XCTAssertEqual(schema.type("Query")?.fields.first?.signature, "products(first: Int, category: ID): [Product!]!")
        XCTAssertNil(schema.type("__Schema"))
        XCTAssertEqual(model.history.count, before, "introspection isn't in the history")
        let q = GraphQLSchema.introspectionRequest(for: editor.draft)
        XCTAssertEqual(q.method, "POST")
        XCTAssertEqual(q.graphqlOperationName, "IntrospectionQuery")
        XCTAssertEqual(q.url, editor.draft.url)
    }

    func testSchemaErrorsAreWorded() {
        XCTAssertThrowsError(try GraphQLSchema.parse(#"{"errors":[{"message":"introspection is disabled"}]}"#)) {
            XCTAssertEqual($0.localizedDescription, "introspection is disabled")
        }
        XCTAssertThrowsError(try GraphQLSchema.parse(#"{"data":{}}"#)) {
            XCTAssertEqual($0.localizedDescription, "the answer has no __schema (is introspection turned off?)")
        }
        XCTAssertThrowsError(try GraphQLSchema.parse("<html>"))
    }

    func testSuggestionsFollowTheCursor() throws {
        let s = try GraphQLSchema.parse(StubGraphQL.introspection)
        let q = "query Catalog($first: Int) {\n  products(first: $first) { sku na"
        XCTAssertEqual(s.context(in: q, at: q.count), .init(type: "Product", prefix: "na"))
        XCTAssertEqual(s.suggestions(in: q, at: q.count).map(\.name), ["name"])
        let root = "{ "
        XCTAssertEqual(s.suggestions(in: root, at: root.count).map(\.name), ["products", "product", "customer", "orders", "__typename"])
        let mutation = "mutation { createOrder(input: {customerId: 1, skus: [\"a\"]}) { items { pro"
        XCTAssertEqual(s.context(in: mutation, at: mutation.count), .init(type: "OrderItem", prefix: "pro"))
        let closed = "{ customer(id: 1) { orders { id } t"
        XCTAssertEqual(s.context(in: closed, at: closed.count).type, "Customer")
        let fragment = "fragment F on Product { cat"
        XCTAssertEqual(s.suggestions(in: fragment, at: fragment.count).map(\.detail), ["Category"])
        let inArgs = "{ products(fir"
        XCTAssertNil(s.context(in: inArgs, at: inArgs.count).type)
        let comment = "{ # products {\n cu"
        XCTAssertEqual(s.suggestions(in: comment, at: comment.count).map(\.name), ["customer"])
        let applied = GraphQLSchema.apply("name", to: q + " }", at: q.count)
        XCTAssertTrue(applied.text.hasSuffix("{ sku name }"))
        XCTAssertEqual(applied.cursor, q.count + 2)
    }
}
