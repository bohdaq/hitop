import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// A local stand-in for the network, used by the UI tests and screenshots (launch argument
/// `-UITestStub`, on with `-UITestSeed`): every request is answered here, in the app process,
/// so tests never touch the network. Answers follow the seeded shop-api.
final class StubURLProtocol: URLProtocol {
    struct Answer {
        var status: Int
        var headers: [String: String]
        var body: Data
    }

    /// Fixed timing for the stubbed host, so screenshots show the timing bar the drafts show.
    static func timing(for url: URL) -> TimingPhases? {
        guard enabled else { return nil }
        return TimingPhases(dns: 11, connect: nil, tls: 20, waiting: 82, download: 29)
    }

    nonisolated(unsafe) static var enabled = false

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    /// The live event stream's ticks (`/v2/stock/live`), until the client stops loading.
    private var ticker: Timer?
    private var ticks = 0

    override func startLoading() {
        let answer = Self.answer(for: request)
        let url = request.url ?? URL(string: "https://stub.invalid/")!
        let response = HTTPURLResponse(url: url, statusCode: answer.status, httpVersion: "HTTP/2", headerFields: answer.headers)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: answer.body)
        if url.path == "/v2/stock/live" {
            // Never ends: a tick every 0.4 s until Stop (stopLoading).
            // The timer runs on this loading thread's run loop, where the client is called.
            nonisolated(unsafe) let me = self
            let t = Timer(timeInterval: 0.4, repeats: true) { _ in
                me.ticks += 1
                let stock = max(0, 120 - me.ticks)
                me.client?.urlProtocol(me, didLoad: Data("event: tick\nid: \(me.ticks + 2)\ndata: {\"sku\":\"TEE-0193\",\"stock\":\(stock)}\n\n".utf8))
            }
            RunLoop.current.add(t, forMode: .common)
            ticker = t
            return
        }
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {
        ticker?.invalidate()
        ticker = nil
    }

    static let eventStreamHeaders = ["Content-Type": "text/event-stream", "Cache-Control": "no-store", "Server": "shop-api"]

    private static func json(_ status: Int, _ text: String, extra: [String: String] = [:]) -> Answer {
        var headers = ["Content-Type": "application/json; charset=utf-8", "Date": "Tue, 17 Nov 2026 10:42:00 GMT",
                       "Server": "shop-api", "X-Request-Id": "req_7f3a91c2", "Cache-Control": "no-store"]
        for (k, v) in extra { headers[k] = v }
        return Answer(status: status, headers: headers, body: Data(text.utf8))
    }

    static func bodyText(_ request: URLRequest) -> String {
        if let b = request.httpBody { return String(decoding: b, as: UTF8.self) }
        guard let stream = request.httpBodyStream else { return "" }
        stream.open()
        defer { stream.close() }
        var data = Data()
        var buf = [UInt8](repeating: 0, count: 4096)
        while stream.hasBytesAvailable {
            let n = stream.read(&buf, maxLength: buf.count)
            if n <= 0 { break }
            data.append(buf, count: n)
        }
        return String(decoding: data, as: UTF8.self)
    }

    static func answer(for request: URLRequest) -> Answer {
        let method = request.httpMethod ?? "GET"
        let path = request.url?.path ?? "/"
        let host = request.url?.host ?? ""
        if host == FakeCloud.host, let url = request.url {
            // HITOP Cloud in UI tests: the in-memory sync server.
            let body = bodyText(request)
            let r = FakeCloud.shared.handle(CloudRequest(method: method, url: url, headers: request.allHTTPHeaderFields ?? [:], body: body.isEmpty ? nil : Data(body.utf8)))
            return Answer(status: r.status, headers: ["Content-Type": "application/json"], body: r.body)
        }
        if host.hasPrefix("unreachable") {
            return Answer(status: 503, headers: ["Content-Type": "text/plain"], body: Data("upstream unavailable".utf8))
        }
        switch (method, path) {
        case ("POST", "/v2/orders"):
            return json(201, """
            {"id":"ord_8KQ2mZ41","status":"processing","total":{"amount":6480,"currency":"EUR"},"items":[{"sku":"TEE-0193","qty":2},{"sku":"CAP-0007","qty":1}],"customer":{"id":"cus_42","name":"Ada","tier":"gold"},"links":{"track":"/v2/orders/ord_8KQ2mZ41/tracking"},"createdAt":"2026-11-17T10:42Z"}
            """, extra: ["Location": "/v2/orders/ord_8KQ2mZ41", "Set-Cookie": "session=s_91x; Path=/; Secure; HttpOnly"])
        case ("GET", "/v2/orders"):
            return json(200, """
            [{"id":"ord_8KQ2mZ41","status":"processing","total":6480},{"id":"ord_7HP1aX02","status":"shipped","total":2190}]
            """)
        case ("POST", "/v2/auth/login"):
            return json(200, #"{"token":"tok_live_5Yq","expiresIn":3600}"#, extra: ["Set-Cookie": "session=s_91x; Path=/; Secure; HttpOnly"])
        case ("GET", "/echo"), ("POST", "/echo"):
            let env = request.value(forHTTPHeaderField: "X-Env") ?? ""
            return json(200, "{\"method\":\"\(method)\",\"path\":\"\(path)\",\"env\":\"\(env)\",\"ok\":true}")
        case ("GET", "/v2/orders/stream"):
            return Answer(status: 200, headers: eventStreamHeaders, body: Data("""
            : order events for shop-api
            event: order.created
            id: 1
            data: {"id":"ord_8KQ2mZ41","status":"processing","total":6480}

            event: order.packed
            id: 2
            data: {"id":"ord_8KQ2mZ41","status":"packed"}

            event: order.shipped
            id: 3
            data: {"id":"ord_8KQ2mZ41","carrier":"DHL",
            data: "tracking":"JD0146 0001 2345"}


            """.utf8))
        case ("GET", "/v2/stock/live"):
            return Answer(status: 200, headers: eventStreamHeaders, body: Data("""
            event: tick
            id: 1
            data: {"sku":"TEE-0193","stock":120}

            event: tick
            id: 2
            data: {"sku":"CAP-0007","stock":64}


            """.utf8))
        case ("POST", "/graphql"):
            if bodyText(request).contains("__schema") { return json(200, StubGraphQL.introspection) }
            return json(200, #"{"data":{"products":[{"sku":"TEE-0193","name":"HITOP tee","stock":120,"rating":4.8},{"sku":"CAP-0007","name":"HITOP cap","stock":64,"rating":4.6}]}}"#)
        case (_, let p) where p.hasPrefix("/v2/orders/"):
            if method == "DELETE" { return json(409, #"{"error":"order already shipped"}"#) }
            return json(200, #"{"id":"ord_8KQ2mZ41","status":"processing"}"#)
        default:
            return json(200, "{\"method\":\"\(method)\",\"path\":\"\(path)\",\"host\":\"\(host)\",\"ok\":true}")
        }
    }

    /// A configuration whose every request is answered by this stub.
    static let configuration: @Sendable () -> URLSessionConfiguration = {
        let config = Sender.defaultConfiguration()
        config.protocolClasses = [StubURLProtocol.self]
        return config
    }
}
