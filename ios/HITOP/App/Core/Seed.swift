import Foundation

/// The shop-api sample the design drafts show, for UI tests and screenshots (launch argument
/// `-UITestSeed`). Every host is answered by `StubURLProtocol`.
enum Seed {
    static let createOrderBody = """
    {
      "customerId": ${customerId},
      "currency": "EUR",
      "items": [
        { "sku": "TEE-0193", "qty": 2 },
        { "sku": "CAP-0007", "qty": 1 }
      ],
      "shipping": { "method": "express" }
    }
    """

    static let createOrderTests = """
    const order = pm.response.json();
    pm.test("Status is 201", () => pm.response.to.have.status(201));
    pm.test("Total equals sum of items", () => pm.expect(order.total.amount).to.eql(6480));
    pm.test("Responds under 1 s", () => pm.expect(pm.response.responseTime).to.be.below(1000));
    pm.test("Has a payment link", () => pm.expect(order.links.pay).to.be.ok);
    """

    static func collections() -> [SavedCollection] {
        func r(_ id: UInt64, _ method: String, _ name: String, _ url: String, _ folder: String, body: String = "",
               headers: [KeyValue] = [], tests: String = "", bodyType: String? = nil, extras: [String: JSONAny] = [:]) -> SavedRequest {
            let slug = name.lowercased().replacingOccurrences(of: " ", with: "-")
            return SavedRequest(id: id, name: name, url: url, method: method, headers: headers, body: body, postRequestScript: tests,
                                bodyType: bodyType ?? (body.isEmpty ? nil : "json"), file: folder.isEmpty ? slug : "\(folder)/\(slug)", extras: extras)
        }
        let json = [KeyValue("Content-Type", "application/json"), KeyValue("Accept", "application/json"), KeyValue("Idempotency-Key", "${$guid}", enabled: false)]
        let shop = SavedCollection(id: 1001, name: "shop-api", requests: [
            r(1101, "POST", "Log in", "${baseUrl}/v2/auth/login", "auth", body: "{\n  \"email\": \"ada@example.com\",\n  \"password\": \"${password}\"\n}", headers: [json[0]]),
            r(1102, "POST", "Refresh token", "${baseUrl}/v2/auth/refresh", "auth"),
            r(1103, "GET", "List orders", "${baseUrl}/v2/orders?status=open", "orders",
              tests: "pm.test(\"Status is 200\", () => pm.response.to.have.status(200));"),
            r(1104, "POST", "Create order", "${baseUrl}/v2/orders?expand=items", "orders", body: createOrderBody, headers: json, tests: createOrderTests),
            r(1105, "GET", "Get order", "${baseUrl}/v2/orders/${orderId}", "orders"),
            r(1106, "PATCH", "Update shipping", "${baseUrl}/v2/orders/${orderId}/shipping", "orders", body: "{\n  \"method\": \"standard\"\n}", headers: [json[0]]),
            r(1107, "GET", "Order events", "wss://api.shop.example/v2/orders/events", "orders", extras: ["kind": .string("websocket")]),
            r(1108, "DELETE", "Cancel order", "${baseUrl}/v2/orders/${orderId}", "orders"),
            r(1109, "GET", "List products", "${baseUrl}/v2/products?limit=20", "catalog"),
            r(1110, "POST", "Catalog query", "${baseUrl}/graphql", "catalog",
              body: "query Catalog($first: Int) {\n  products(first: $first) { sku name price }\n}", bodyType: "graphql"),
            r(1111, "POST", "Stock levels", "${baseUrl}/v2/stock/query", "catalog", body: "{\n  \"skus\": [\"TEE-0193\"]\n}", headers: [json[0]]),
            r(1112, "POST", "Create customer", "${baseUrl}/v2/customers", "customers", body: "{\n  \"name\": \"Ada\"\n}", headers: [json[0]]),
            r(1113, "GET", "Get customer", "${baseUrl}/v2/customers/${customerId}", "customers"),
        ], variables: ["orderId": "ord_8KQ2mZ41"], auth: .bearer(token: "${token}"), folders: ["auth", "orders", "catalog", "customers"])

        let hooks = SavedCollection(id: 1002, name: "webhooks", requests: [
            r(1201, "POST", "Register endpoint", "${baseUrl}/v2/webhooks", ""),
            r(1202, "GET", "List endpoints", "${baseUrl}/v2/webhooks", ""),
            r(1203, "POST", "Send test event", "${baseUrl}/v2/webhooks/test", ""),
            r(1204, "GET", "Delivery log", "${baseUrl}/v2/webhooks/deliveries", ""),
            r(1205, "PUT", "Rotate secret", "${baseUrl}/v2/webhooks/secret", ""),
            r(1206, "DELETE", "Remove endpoint", "${baseUrl}/v2/webhooks/wh_1", ""),
        ])
        let mine = SavedCollection(id: 1003, name: AutoName.myRequests, requests: [
            r(1301, "GET", "GET /get", "https://httpbin.org/get", ""),
            r(1302, "POST", "POST /anything", "https://httpbin.org/anything", "", body: "{\n  \"hello\": \"world\"\n}"),
            r(1303, "GET", "GET /status/418", "https://httpbin.org/status/418", ""),
            r(1304, "GET", "GET /image/png", "https://httpbin.org/image/png", ""),
        ])
        return [shop, hooks, mine, realtime()]
    }

    /// I4: a WebSocket, two event streams and two gRPC calls (StubURLProtocol, the in-app echo
    /// socket and the canned Greeter answer them).
    static func realtime() -> SavedCollection {
        var prices = SavedRequest(id: 1401, name: "Live prices", url: "wss://stream.shop.example/v2/prices?market=${market}",
                                  headers: [KeyValue("X-Client", "hitop")], file: "live-prices")
        prices.kind = SavedRequest.websocketKind
        prices.websocket = WebSocketFields(subprotocols: ["prices.v2", "prices.v1"], messages: [
            WSSavedMessage(name: "subscribe", content: #"{"op": "subscribe", "symbol": "${symbol}"}"#),
            WSSavedMessage(name: "ping", content: "ping"),
            WSSavedMessage(name: "binary", content: "bin"),
        ])
        let events = KeyValue("Accept", "text/event-stream")
        let feed = SavedRequest(id: 1402, name: "Order feed", url: "${baseUrl}/v2/orders/stream", headers: [events], file: "order-feed")
        let ticks = SavedRequest(id: 1403, name: "Price ticks", url: "${baseUrl}/v2/prices/live", headers: [events], file: "price-ticks")
        var hello = SavedRequest(id: 1404, name: "Say hello", url: "grpc.shop.example:443", headers: [KeyValue("x-tenant", "acme")], file: "say-hello")
        hello.kind = SavedRequest.grpcKind
        hello.grpc = GrpcFields(service: "hitop.test.Greeter", method: "SayHello", message: "{\n  \"name\": \"${customer}\"\n}")
        var stream = SavedRequest(id: 1405, name: "Stream hellos", url: "grpc.shop.example:443", file: "stream-hellos")
        stream.kind = SavedRequest.grpcKind
        stream.grpc = GrpcFields(service: "hitop.test.Greeter", method: "StreamHellos", message: "{\n  \"name\": \"Ada\",\n  \"times\": 3\n}")
        var noReflection = SavedRequest(id: 1406, name: "Legacy service", url: "noreflect.shop.example:443", file: "legacy-service")
        noReflection.kind = SavedRequest.grpcKind
        return SavedCollection(id: 1004, name: "realtime", requests: [prices, feed, ticks, hello, stream, noReflection],
                               variables: ["market": "eu", "symbol": "TEE-0193", "customer": "Ada"], auth: .bearer(token: "${token}"))
    }

    static func environments() -> [SavedEnvironment] {
        [
            SavedEnvironment(id: "env-staging", name: "Staging", variables: ["baseUrl": "https://staging.shop.example", "customerId": "42", "password": ""], secrets: ["token", "password"]),
            SavedEnvironment(id: "env-production", name: "Production", variables: ["baseUrl": "https://api.shop.example", "customerId": "1001"], secrets: ["token"]),
            SavedEnvironment(id: "env-local", name: "Local", variables: ["baseUrl": "http://localhost:8080", "customerId": "1"]),
        ]
    }

    static let secrets: [(String, String, String)] = [("env-staging", "token", "tok_staging_8f2"), ("env-staging", "password", "hunter2"),
                                                       ("env-production", "token", "tok_live_5Yq")]

    static func history(now: Date = Date(timeIntervalSince1970: 1_794_912_120)) -> [HistoryEntry] {
        let ms = UInt64(now.timeIntervalSince1970 * 1000)
        let cols = collections()
        func entry(_ id: UInt64, _ ago: UInt64, _ requestID: UInt64, _ status: Int, _ dur: Int, _ size: Int) -> HistoryEntry {
            let req = cols.flatMap(\.requests).first { $0.id == requestID }!
            return HistoryEntry(id: id, timestamp: ms - ago * 1000, collectionName: "shop-api", request: req,
                                response: ResponseSnapshot(status: status, headers: [["Content-Type", "application/json"]], body: "{\"ok\":true}",
                                                           truncated: false, durationMs: dur, size: size, tests: []))
        }
        return [
            entry(9001, 120, 1104, 201, 142, 412),
            entry(9002, 380, 1103, 200, 74, 1290),
            entry(9003, 900, 1101, 200, 96, 88),
            entry(9004, 3600, 1108, 409, 61, 34),
            entry(9005, 7200, 1106, 200, 88, 120),
        ]
    }
}
