#if os(macOS) || os(Linux)
import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import XCTest
@testable import HITOP

/// I4 against real local servers (`cargo build -p hitop --example test_servers` in cli/, path in
/// HITOP_TEST_SERVERS_BIN; CI job `core` sets it and HITOP_E2E_REQUIRED): hitop-core's gRPC client
/// through SyncFFI against hitop-core's Greeter test server (reflection v1, v1alpha, none), Server-
/// Sent Events read live from a real HTTP server, and URLSessionWebSocketTask against the CLI's
/// WebSocket echo server (macOS: the handshake headers, subprotocol, text, binary, close code).
/// Needs `Process` (macOS, Linux; not the simulator).
@MainActor
final class ProtocolEndToEndTests: XCTestCase {
    var process: Process?
    var addresses: [String: String] = [:]

    override func tearDown() async throws {
        process?.terminate()
        process?.waitUntilExit()
    }

    func startServers() throws {
        let env = ProcessInfo.processInfo.environment
        guard let bin = env["HITOP_TEST_SERVERS_BIN"], !bin.isEmpty else {
            if env["HITOP_E2E_REQUIRED"] != nil { XCTFail("HITOP_TEST_SERVERS_BIN must be set") }
            throw XCTSkip("HITOP_TEST_SERVERS_BIN not set")
        }
        let p = Process()
        p.executableURL = URL(fileURLWithPath: bin)
        let out = Pipe()
        p.standardOutput = out
        p.standardError = FileHandle.nullDevice
        try p.run()
        process = p
        // The first line: {"ws": "127.0.0.1:…", "grpc": …, "grpcV1alpha": …, "grpcNone": …, "http": …}.
        var line = Data()
        while true {
            let b = out.fileHandleForReading.readData(ofLength: 1)
            if b.isEmpty || b == Data([0x0A]) { break }
            line.append(b)
        }
        addresses = try JSONDecoder().decode([String: String].self, from: line)
    }

    func until(_ what: String, timeout: TimeInterval = 10, _ cond: @escaping () -> Bool) async {
        let end = Date().addingTimeInterval(timeout)
        while !cond() && Date() < end { try? await Task.sleep(for: .milliseconds(25)) }
        let ok = cond()
        XCTAssertTrue(ok, what)
    }

    func server(_ key: String, metadata: [[String]] = []) -> GrpcServer {
        GrpcServer(address: addresses[key]!, plaintext: true, metadata: metadata, timeoutMs: 5000)
    }

    func events(_ h: GrpcCallHandle) async -> [GrpcEvent] {
        var all: [GrpcEvent] = []
        for await e in h.events { all.append(e) }
        return all
    }

    // MARK: gRPC

    func testGrpcThroughHitopCore() async throws {
        try startServers()
        let client = FFIGrpcClient()
        let v1 = try await client.list(server("grpc"))
        XCTAssertEqual(v1.source, "reflection v1")
        let greeter = try XCTUnwrap(v1.services.first { $0.name == "hitop.test.Greeter" })
        XCTAssertEqual(greeter.methods.map(\.name), ["SayHello", "StreamHellos", "Fail", "Chat", "CollectHellos"])
        XCTAssertEqual(greeter.methods.map(\.shape), ["unary", "server streaming", "unary", "bidirectional", "client streaming"])
        let alpha = try await client.list(server("grpcV1alpha"))
        XCTAssertEqual(alpha.source, "reflection v1alpha")
        do {
            _ = try await client.list(server("grpcNone"))
            XCTFail("a server without reflection")
        } catch let f as GrpcFailure {
            XCTAssertTrue(f.noReflection, f.message)
        }
        let d = try await client.describe(server("grpc"), symbol: "hitop.test.Greeter/SayHello")
        let template = try XCTUnwrap(d.template)
        XCTAssertTrue(template.contains("\"times\": 0"), template)
        XCTAssertTrue(d.description.contains("SayHello"), d.description)

        // Unary, metadata from headers and auth (as the request builds it).
        var r = SavedRequest(name: "Hello", url: addresses["grpc"]!, headers: [KeyValue("x-tenant", "${tenant}")])
        r.kind = SavedRequest.grpcKind
        r.grpc = GrpcFields(service: "hitop.test.Greeter", method: "SayHello", plaintext: true)
        let s = try GrpcServer.of(r, collectionAuth: .bearer(token: "${token}"), variables: ["tenant": "acme", "token": "t0k"])
        let hello = await events(client.call(s, service: "hitop.test.Greeter", method: "SayHello", message: #"{"name": "Ada"}"#, maxTimeMs: 5000))
        XCTAssertEqual(hello.count, 3, "\(hello)")
        guard case let .headers(h) = hello[0], case let .message(m) = hello[1] else { return XCTFail("\(hello)") }
        XCTAssertTrue(h.contains(["x-served-by", "hitop-test"]), "\(h)")
        let reply = try JSONDecoder().decode(JSONAny.self, from: Data(m.utf8))
        XCTAssertEqual(reply["message"], .string("Hello, Ada"))
        XCTAssertEqual(reply["tenant"], .string("acme"))
        XCTAssertEqual(reply["authorization"], .string("Bearer t0k"))
        XCTAssertEqual(reply["count"], .string("9007199254740993"), "int64 as a JSON string, as proto3 JSON has it")
        XCTAssertEqual(hello[2], .status(code: 0, name: "OK", message: "", trailers: []))

        let stream = await events(client.call(server("grpc"), service: "hitop.test.Greeter", method: "StreamHellos", message: #"{"name": "Ada", "times": 3}"#, maxTimeMs: nil))
        XCTAssertEqual(stream.count, 5, "\(stream)")
        guard case let .message(third) = stream[3] else { return XCTFail("\(stream)") }
        XCTAssertTrue(third.contains("\"index\": 2") && third.contains("\"message\": \"Hello #2, Ada\""), third)
        XCTAssertEqual(stream[4], .status(code: 0, name: "OK", message: "", trailers: []))

        let fail = await events(client.call(server("grpc"), service: "hitop.test.Greeter", method: "Fail", message: #"{"name": "Bo"}"#, maxTimeMs: nil))
        guard case let .status(code, name, message, trailers)? = fail.last else { return XCTFail("\(fail)") }
        XCTAssertEqual([String(code), name, message], ["5", "NOT_FOUND", "no user named Bo"])
        XCTAssertTrue(trailers.contains(["x-reason", "test"]), "\(trailers)")
        let chat = await events(client.call(server("grpc"), service: "hitop.test.Greeter", method: "Chat", message: #"{"name": "Ada"} {"name": "Bo"}"#, maxTimeMs: nil))
        XCTAssertEqual(chat.count, 4, "\(chat)")
        guard case let .message(second) = chat[2] else { return XCTFail("\(chat)") }
        XCTAssertTrue(second.contains("Hello, Bo"), second)
        XCTAssertEqual(chat[3], .status(code: 0, name: "OK", message: "", trailers: []))
        let bad = await events(client.call(server("grpc"), service: "hitop.test.Greeter", method: "SayHello", message: "{not json", maxTimeMs: nil))
        guard case .error? = bad.first else { return XCTFail("\(bad)") }

        // Cancel stops a stream that would go on for long.
        let long = client.call(server("grpc"), service: "hitop.test.Greeter", method: "StreamHellos", message: #"{"times": 1000000}"#, maxTimeMs: nil)
        var seen = 0
        for await e in long.events {
            if case .message = e { seen += 1 }
            if seen == 5 { long.cancel() }
        }
        XCTAssertLessThan(seen, 1_000_000)
    }

    func testGrpcFromTheRequestScreen() async throws {
        try startServers()
        let model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault())
        model.load()
        var r = model.newRequest(in: model.myRequests().id)
        r.url = addresses["grpc"]!
        r.kind = SavedRequest.grpcKind
        r.grpc = GrpcFields(service: "hitop.test.Greeter", method: "StreamHellos", plaintext: true)
        model.save(r)
        let editor = try XCTUnwrap(model.editor(for: r.id))
        await editor.loadGrpcSchema()
        XCTAssertEqual(editor.grpcSession.listing?.source, "reflection v1")
        await editor.useGrpcTemplate()
        XCTAssertTrue(editor.draft.grpc.message.contains("\"times\": 0"), editor.draft.grpc.message)
        editor.draft.grpc.message = #"{"name": "Grace", "times": 2}"#
        editor.send()
        await until("status") { editor.grpcSession.lastStatus != nil && !editor.grpcSession.calling }
        XCTAssertEqual(editor.grpcSession.log.count, 4)
        XCTAssertEqual(editor.grpcSession.lastStatus, .status(code: 0, name: "OK", message: "", trailers: []))
    }

    // MARK: Server-Sent Events

    func testEventStreamFromARealServer() async throws {
        try startServers()
        let model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault())
        model.load()
        var r = model.newRequest(in: model.myRequests().id)
        r.url = "http://\(addresses["http"]!)/events"
        r.setMode(.sse)
        model.save(r)
        let editor = try XCTUnwrap(model.editor(for: r.id))
        editor.send()
        await editor.waitForSend()
        guard case let .success(o)? = editor.outcome else { return XCTFail("\(String(describing: editor.outcome))") }
        XCTAssertEqual(o.status, 200)
        XCTAssertEqual(o.events?.map(\.event), ["price", "message", "done"])
        XCTAssertEqual(o.events?.map(\.data), [#"{"sku":"TEE-0193","price":2190}"#, "line one\nline two", "bye"])
        XCTAssertEqual(o.events?.map(\.lastEventID), ["1", nil, "3"])
        #if canImport(Darwin)
        await until("read live") { editor.liveEvents.count == 3 }

        // A stream that never ends: Stop.
        editor.draft.url = "http://\(addresses["http"]!)/live"
        editor.send()
        await until("ticks") { editor.liveEvents.count >= 3 }
        editor.stopStream()
        await editor.waitForSend()
        guard case let .success(live)? = editor.outcome else { return XCTFail("\(String(describing: editor.outcome))") }
        XCTAssertTrue(live.stopped)
        XCTAssertEqual(live.events?.prefix(3).map(\.data), ["0", "1", "2"])
        #endif
    }

    // MARK: WebSocket

    #if canImport(Darwin)
    func testWebSocketAgainstTheEchoServer() async throws {
        try startServers()
        var r = SavedRequest(name: "Echo", url: "http://\(addresses["ws"]!)/socket?k=${k}", headers: [KeyValue("X-Client", "hitop")])
        r.kind = SavedRequest.websocketKind
        r.websocket = WebSocketFields(subprotocols: ["chat.v2", "chat.v1"])
        let target = try WebSocketTargets.make(r, collectionAuth: .bearer(token: "${token}"), variables: ["k": "1", "token": "t0k"])
        XCTAssertEqual(target.url, "ws://\(addresses["ws"]!)/socket?k=1")
        let s = SocketSession()
        s.connect(target, transport: URLSessionWebSocketTransport(), secrets: ["t0k"])
        await until("open") { s.status == .open }
        XCTAssertEqual(s.negotiated, "chat.v2")
        await s.send("hello")
        await until("echo") { s.log.contains { $0.kind == .received && $0.text == "hello" } }
        await s.send("bin")
        await until("binary") { s.log.contains { $0.bytes == Data([0x00, 0xFF]) } }
        await s.send("close")
        await until("closed") { s.status == .closed }
        XCTAssertEqual(s.log.last?.text, "Closed 4001 bye")

        let (data, _) = try await URLSession.shared.data(from: URL(string: "http://\(addresses["http"]!)/handshakes")!)
        let handshakes = try JSONDecoder().decode([[[String]]].self, from: data)
        let first = try XCTUnwrap(handshakes.first)
        let header = { (n: String) in first.first { $0[0].lowercased() == n }?[1] }
        XCTAssertEqual(header("x-client"), "hitop")
        XCTAssertEqual(header("authorization"), "Bearer t0k")
        XCTAssertEqual(header("sec-websocket-protocol"), "chat.v2, chat.v1")

        // Nobody there: an error in the log, closed.
        let gone = SocketSession()
        gone.connect(WebSocketTarget(url: "ws://127.0.0.1:1/x", headers: [], subprotocols: []), transport: URLSessionWebSocketTransport())
        await until("refused") { gone.status == .closed && gone.log.contains { $0.kind == .error } }
    }
    #endif
}
#endif
