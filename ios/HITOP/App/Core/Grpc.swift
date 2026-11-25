import Foundation
import Observation
import HitopKit

// I4 gRPC: a request with `kind: grpc` calls a method (spec §3 "gRPC"). The URL is the server
// address; enabled headers and auth are the metadata. The schema comes from server reflection
// (v1, then v1alpha); `.proto` files aren't read on iPhone and iPad. The calls themselves are
// hitop-core's client (tonic + prost-reflect) through SyncFFI (`grpc*` operations), the same
// code as `hitop grpc` and the desktop app.

/// A server to talk to, variables filled in (the desktop's `GrpcServerArgs`).
struct GrpcServer: Equatable, Sendable {
    var address: String
    var plaintext: Bool
    /// Metadata in order: enabled headers, then the auth's header.
    var metadata: [[String]]
    var timeoutMs = 10_000

    var json: JSONAny {
        .object(["address": .string(address), "plaintext": .bool(plaintext),
                 "metadata": .array(metadata.map { .array($0.map(JSONAny.string)) }), "timeoutMs": .number(Double(timeoutMs))])
    }

    /// The request's server and metadata: `${variables}` filled in, the auth (the collection's when
    /// the request has none) as metadata; an API key `in: query` isn't sent (gRPC has no query).
    static func of(_ r: SavedRequest, collectionAuth: AuthSpec?, variables: [String: String]) throws -> GrpcServer {
        let fill = { (s: String) throws -> String in try Interpolation.interpolate(s, variables) }
        let address = try fill(r.url.trimmingCharacters(in: .whitespaces))
        let headers = try r.headers.filter { $0.enabled && !$0.name.isEmpty }.map { (try fill($0.name), try fill($0.value)) }
        let (_, metadata) = try AuthApply.apply(Sender.kitAuth(r.auth ?? collectionAuth), url: "", headers: headers, fill)
        return GrpcServer(address: address, plaintext: r.grpc.plaintext, metadata: metadata.map { [$0.0, $0.1] })
    }
}

struct GrpcMethodInfo: Codable, Hashable, Sendable, Identifiable {
    var name: String
    var fullName: String
    var input: String
    var output: String
    var clientStreaming: Bool
    var serverStreaming: Bool
    var id: String { fullName }

    /// "unary", "server streaming"…. A method that streams from the client gets the message text's JSON values one after another.
    var shape: String {
        switch (clientStreaming, serverStreaming) {
        case (false, false): "unary"
        case (false, true): "server streaming"
        case (true, false): "client streaming"
        case (true, true): "bidirectional"
        }
    }
}

struct GrpcServiceInfo: Codable, Hashable, Sendable, Identifiable {
    var name: String
    var methods: [GrpcMethodInfo]
    var id: String { name }
}

struct GrpcListing: Equatable, Sendable {
    /// `reflection v1` or `reflection v1alpha`.
    var source: String
    var services: [GrpcServiceInfo]
}

struct GrpcDescription: Equatable, Sendable {
    var description: String
    /// A JSON message to start from, field order as the schema has it.
    var template: String?
}

enum GrpcEvent: Hashable, Sendable {
    case headers([[String]])
    /// A response message as JSON text.
    case message(String)
    case status(code: Int, name: String, message: String, trailers: [[String]])
    /// A problem before anything was sent (no server, no such method, bad JSON).
    case error(String)

    init?(json e: JSONAny) {
        let pairs = { (v: JSONAny?) in v?.array?.compactMap { $0.array?.compactMap(\.string) } ?? [] }
        switch e["type"]?.string {
        case "headers": self = .headers(pairs(e["metadata"]))
        case "message": self = .message(e["data"]?.string ?? "")
        case "status": self = .status(code: e["code"]?.int ?? 2, name: e["name"]?.string ?? "UNKNOWN", message: e["message"]?.string ?? "", trailers: pairs(e["trailers"]))
        case "error": self = .error(e["message"]?.string ?? "")
        default: return nil
        }
    }
}

struct GrpcLogEntry: Identifiable, Hashable, Sendable {
    var id: Int
    var time: Date
    var event: GrpcEvent
}

struct GrpcFailure: Error, LocalizedError, Equatable, Sendable {
    var message: String
    var errorDescription: String? { message }

    /// hitop-core's words for a server without a reflection service.
    var noReflection: Bool { message.contains("no reflection service") }
}

/// A call in flight: its events, ending with the status (or an error), and Cancel.
struct GrpcCallHandle: Sendable {
    var events: AsyncStream<GrpcEvent>
    var cancel: @Sendable () -> Void
}

/// Lists, describes and calls: hitop-core through SyncFFI in the app, a canned Greeter in UI tests.
protocol GrpcClient: Sendable {
    func list(_ server: GrpcServer) async throws -> GrpcListing
    func describe(_ server: GrpcServer, symbol: String) async throws -> GrpcDescription
    func call(_ server: GrpcServer, service: String, method: String, message: String, maxTimeMs: Int?) -> GrpcCallHandle
}

/// hitop-core's gRPC client (SyncFFI `grpcList`, `grpcDescribe`, `grpcStart`/`grpcNext`/`grpcCancel`).
/// The C calls block, so they run on a queue of their own, never on Swift's cooperative pool.
struct FFIGrpcClient: GrpcClient {
    private static let queue = DispatchQueue(label: "hitop.grpc", qos: .userInitiated, attributes: .concurrent)

    private static func run<T: Sendable>(_ work: @escaping @Sendable () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { c in
            queue.async {
                do { c.resume(returning: try work()) } catch { c.resume(throwing: error) }
            }
        }
    }

    private static func ffi(_ op: String, _ args: [String: JSONAny]) throws -> JSONAny {
        do { return try SyncCore.call(op, args) } catch let e as SyncCore.Failure { throw GrpcFailure(message: e.message) }
    }

    func list(_ server: GrpcServer) async throws -> GrpcListing {
        try await Self.run {
            let v = try Self.ffi("grpcList", ["server": server.json])
            let services = try v["services"].map { try $0.decode([GrpcServiceInfo].self) } ?? []
            return GrpcListing(source: v["source"]?.string ?? "", services: services)
        }
    }

    func describe(_ server: GrpcServer, symbol: String) async throws -> GrpcDescription {
        try await Self.run {
            let v = try Self.ffi("grpcDescribe", ["server": server.json, "symbol": .string(symbol)])
            return GrpcDescription(description: v["description"]?.string ?? "", template: v["template"]?.string)
        }
    }

    func call(_ server: GrpcServer, service: String, method: String, message: String, maxTimeMs: Int?) -> GrpcCallHandle {
        let (stream, out) = AsyncStream<GrpcEvent>.makeStream()
        let state = CallState()
        var args: [String: JSONAny] = ["server": server.json, "service": .string(service), "method": .string(method), "message": .string(message)]
        if let maxTimeMs { args["maxTimeMs"] = .number(Double(maxTimeMs)) }
        let startArgs = args
        Self.queue.async {
            guard let id = (try? Self.ffi("grpcStart", startArgs))?["id"]?.int else {
                out.yield(.error("The call didn't start."))
                out.finish()
                return
            }
            guard state.started(id) else {
                _ = try? Self.ffi("grpcCancel", ["id": .number(Double(id))])
                out.finish()
                return
            }
            while !state.isCancelled {
                guard let next = try? Self.ffi("grpcNext", ["id": .number(Double(id)), "waitMs": .number(250)]) else { break }
                for e in next["events"]?.array ?? [] { if let ev = GrpcEvent(json: e) { out.yield(ev) } }
                if next["done"]?.bool == true { break }
            }
            out.finish()
        }
        return GrpcCallHandle(events: stream) {
            if let id = state.cancel() { Self.queue.async { _ = try? Self.ffi("grpcCancel", ["id": .number(Double(id))]) } }
        }
    }

    private final class CallState: @unchecked Sendable {
        private let lock = NSLock()
        private var id: Int?
        private var cancelled = false
        var isCancelled: Bool { lock.withLock { cancelled } }
        /// False when cancelled before it started.
        func started(_ id: Int) -> Bool { lock.withLock { self.id = id; return !cancelled } }
        func cancel() -> Int? { lock.withLock { cancelled = true; return id } }
    }
}

/// The hitop test server's Greeter (hitop-core tests/fixtures/grpc/greeter.proto), answered in the
/// app process for UI tests and screenshots (`-UITestSeed`), as the web app's tauri-mock does. An
/// address with `noreflect` has no reflection service; one with `unreachable` doesn't answer.
struct FakeGrpcClient: GrpcClient {
    static let greeter = GrpcServiceInfo(name: "hitop.test.Greeter", methods: [
        .init(name: "SayHello", fullName: "hitop.test.Greeter.SayHello", input: "hitop.test.HelloRequest", output: "hitop.test.HelloReply", clientStreaming: false, serverStreaming: false),
        .init(name: "StreamHellos", fullName: "hitop.test.Greeter.StreamHellos", input: "hitop.test.HelloRequest", output: "hitop.test.HelloReply", clientStreaming: false, serverStreaming: true),
        .init(name: "Fail", fullName: "hitop.test.Greeter.Fail", input: "hitop.test.HelloRequest", output: "hitop.test.HelloReply", clientStreaming: false, serverStreaming: false),
        .init(name: "Chat", fullName: "hitop.test.Greeter.Chat", input: "hitop.test.HelloRequest", output: "hitop.test.HelloReply", clientStreaming: true, serverStreaming: true),
    ])

    static let template = """
    {
      "attributes": {},
      "labels": [],
      "mood": "MOOD_UNSPECIFIED",
      "name": "",
      "tag": {
        "value": ""
      },
      "times": 0
    }
    """

    private func check(_ server: GrpcServer) throws {
        if server.address.contains("unreachable") { throw GrpcFailure(message: "couldn't connect to \(server.address): transport error") }
        if server.address.contains("noreflect") {
            throw GrpcFailure(message: "the server has no reflection service (grpc.reflection.v1 or v1alpha): give its .proto files")
        }
    }

    func list(_ server: GrpcServer) async throws -> GrpcListing {
        try check(server)
        return GrpcListing(source: "reflection v1", services: [Self.greeter])
    }

    func describe(_ server: GrpcServer, symbol: String) async throws -> GrpcDescription {
        try check(server)
        return GrpcDescription(description: "rpc \(symbol.split(separator: "/").last ?? "")(hitop.test.HelloRequest) returns (hitop.test.HelloReply);", template: Self.template)
    }

    func call(_ server: GrpcServer, service: String, method: String, message: String, maxTimeMs: Int?) -> GrpcCallHandle {
        let (stream, out) = AsyncStream<GrpcEvent>.makeStream()
        let json = (try? JSONDecoder().decode(JSONAny.self, from: Data(message.utf8))) ?? .object([:])
        let name = json["name"]?.string ?? ""
        let tenant = server.metadata.first { $0.first?.lowercased() == "x-tenant" }?.last ?? ""
        let auth = server.metadata.first { $0.first?.lowercased() == "authorization" }?.last ?? ""
        let task = Task {
            if server.address.contains("unreachable") || server.address.contains("noreflect") {
                out.yield(.error(server.address.contains("noreflect") ? "the server has no reflection service (grpc.reflection.v1 or v1alpha): give its .proto files" : "couldn't connect to \(server.address): transport error"))
                out.finish()
                return
            }
            let headers: [[String]] = [["x-served-by", "hitop-test"]]
            switch method {
            case "SayHello":
                out.yield(.headers(headers))
                out.yield(.message("""
                {
                  "at": "2026-11-01T10:00:00Z",
                  "authorization": "\(auth.isEmpty ? "" : SecretNames.mask)",
                  "count": "9007199254740993",
                  "index": 0,
                  "message": "Hello, \(name)",
                  "tenant": "\(tenant)"
                }
                """))
                out.yield(.status(code: 0, name: "OK", message: "", trailers: []))
            case "StreamHellos":
                out.yield(.headers(headers))
                let times = json["times"]?.int ?? 0
                for i in 0..<max(0, min(times, 50)) {
                    try? await Task.sleep(for: .milliseconds(120))
                    if Task.isCancelled { break }
                    out.yield(.message("{\n  \"index\": \(i),\n  \"message\": \"Hello #\(i), \(name)\"\n}"))
                }
                if !Task.isCancelled { out.yield(.status(code: 0, name: "OK", message: "", trailers: [])) }
            case "Fail":
                out.yield(.status(code: 5, name: "NOT_FOUND", message: "no user named \(name)", trailers: [["x-reason", "test"]]))
            case "Chat":
                out.yield(.headers(headers))
                out.yield(.message("{\n  \"index\": 0,\n  \"message\": \"Hello, \(name)\"\n}"))
                out.yield(.status(code: 0, name: "OK", message: "", trailers: []))
            default:
                out.yield(.status(code: 12, name: "UNIMPLEMENTED", message: "\(method) is not served", trailers: []))
            }
            out.finish()
        }
        return GrpcCallHandle(events: stream) {
            task.cancel()
            out.finish()
        }
    }
}

// MARK: - Session

/// One request's gRPC state (the web app's `store.grpcTabs` entry): the schema from reflection,
/// the call in flight and its log (headers, messages, status), never saved.
@MainActor
@Observable
final class GrpcSession {
    static let logLimit = 500

    private(set) var listing: GrpcListing?
    private(set) var loadingSchema = false
    private(set) var schemaError: String?
    /// The server answered that it has no reflection service.
    private(set) var noReflection = false
    private(set) var calling = false
    private(set) var log: [GrpcLogEntry] = []
    private(set) var lastStatus: GrpcEvent?

    @ObservationIgnored private var handle: GrpcCallHandle?
    @ObservationIgnored private var nextID = 0
    @ObservationIgnored private var token = 0

    init() {}

    func loadSchema(_ server: GrpcServer, client: GrpcClient) async {
        loadingSchema = true
        schemaError = nil
        noReflection = false
        defer { loadingSchema = false }
        do {
            listing = try await client.list(server)
        } catch {
            let f = error as? GrpcFailure ?? GrpcFailure(message: error.localizedDescription)
            noReflection = f.noReflection
            schemaError = f.noReflection
                ? "This server has no reflection service. HITOP for iPhone and iPad reads a server's schema by reflection only; type the service, method and message by hand, or use the desktop app or the CLI with its .proto files."
                : f.message
        }
    }

    /// The method's request message as a JSON template.
    func template(_ server: GrpcServer, service: String, method: String, client: GrpcClient) async -> String? {
        do {
            return try await client.describe(server, symbol: "\(service)/\(method)").template
        } catch {
            schemaError = error.localizedDescription
            return nil
        }
    }

    func setFailure(_ message: String) {
        add(.error(message))
    }

    private func add(_ e: GrpcEvent) {
        log.append(GrpcLogEntry(id: nextID, time: Date(), event: e))
        nextID += 1
        if log.count > Self.logLimit { log.removeFirst(log.count - Self.logLimit) }
        if case .status = e { lastStatus = e }
        if case .error = e { lastStatus = e }
    }

    func call(_ server: GrpcServer, service: String, method: String, message: String, client: GrpcClient, maxTimeMs: Int? = nil) {
        guard !calling else { return }
        token += 1
        let mine = token
        calling = true
        lastStatus = nil
        let handle = client.call(server, service: service, method: method, message: message, maxTimeMs: maxTimeMs)
        self.handle = handle
        Task { [weak self] in
            for await e in handle.events {
                guard let self, self.token == mine else { return }
                self.add(e)
            }
            guard let self, self.token == mine else { return }
            self.calling = false
            self.handle = nil
        }
    }

    func cancel() {
        guard calling else { return }
        token += 1
        handle?.cancel()
        handle = nil
        calling = false
        add(.status(code: 1, name: "CANCELLED", message: "Cancelled", trailers: []))
    }

    func clear() {
        log = []
        lastStatus = nil
    }
}
