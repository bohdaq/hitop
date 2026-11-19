import XCTest

/// I4 on the iPhone, against the seeded "realtime" collection and shop-api's GraphQL request: the
/// in-app echo socket, StubURLProtocol's event streams and GraphQL schema, the canned gRPC Greeter
/// (no network). Screenshots 22–27.
final class ProtocolFlowTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    private func scrollTo(_ app: XCUIApplication, _ element: XCUIElement, file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(25)
        while Date() < deadline {
            if element.exists && element.isHittable { return }
            app.swipeUp(velocity: .slow)
            RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        }
        XCTAssertTrue(element.exists, "missing \(element)", file: file, line: line)
    }

    /// Opens a request of the realtime collection (shop-api folded first, so it is near the top).
    @MainActor
    private func open(_ app: XCUIApplication, _ name: String) {
        let shop = app.el("collection-shop-api")
        wait(shop)
        if shop.value as? String == "expanded" { shop.tap() }
        let realtime = app.el("collection-realtime")
        scrollTo(app, realtime)
        if realtime.value as? String != "expanded" { realtime.tap() }
        let row = app.el("request-\(name)")
        scrollTo(app, row)
        row.tap()
    }

    @MainActor
    private func waitLabel(_ e: XCUIElement, contains text: String, timeout: TimeInterval = 15, file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if e.exists && e.label.contains(text) { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        }
        XCTFail("\(e) never said \(text) (\(e.exists ? e.label : "missing"))", file: file, line: line)
    }

    @MainActor
    func testWebSocketConnectSendAndLog() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        open(app, "Live prices")
        wait(app.el("ws-add-message"))
        XCTAssertEqual(app.el("method").label, "Method WebSocket")
        shot(app, "22-websocket-request")

        app.el("send").tap()
        waitLabel(app.el("socket-status"), contains: "Connected · prices.v2")
        let subscribe = app.el("ws-saved-subscribe")
        wait(subscribe)
        subscribe.tap()
        waitLabel(app.el("ws-log-received"), contains: "TEE-0193")
        let composer = app.el("ws-composer")
        composer.tap()
        composer.typeText("hello from HITOP")
        app.el("ws-send").tap()
        waitLabel(app.el("ws-log-received"), contains: "hello from HITOP")
        app.el("ws-saved-binary").tap()
        waitLabel(app.el("ws-log-received"), contains: "base64 AP8=")
        shot(app, "23-websocket-log")
        app.el("socket-toggle").tap()
        waitLabel(app.el("socket-status"), contains: "Not connected")
    }

    @MainActor
    func testServerSentEventsLiveAndStop() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        open(app, "Price ticks")
        XCTAssertEqual(app.el("method").label, "Method Server-Sent Events")
        app.el("send").tap()
        let count = app.el("events-count")
        waitLabel(count, contains: "Live ·")
        RunLoop.current.run(until: Date().addingTimeInterval(1.2))
        XCTAssertFalse(count.label.contains("· 2 events"), count.label)
        shot(app, "24-sse-live")
        app.el("stop-stream").tap()
        waitForResponse(app)
        wait(app.el("event-0"))
        XCTAssertTrue(app.el("stream-stopped").exists)
        app.el("close-response").tap()

        // A stream that ends: its events in the response's Events tab.
        app.navigationBars.buttons.element(boundBy: 0).tap()
        open(app, "Order feed")
        app.el("send").tap()
        waitForResponse(app)
        wait(app.el("event-2"))
        XCTAssertTrue(app.el("event-2").label.contains("order.shipped"), app.el("event-2").label)
        shot(app, "25-sse-events")
    }

    @MainActor
    func testGrpcReflectionCallAndNoReflection() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        open(app, "Stream hellos")
        let load = app.el("grpc-load")
        wait(load)
        load.tap()
        waitLabel(app.el("grpc-source"), contains: "reflection v1")
        wait(app.el("grpc-method"))
        shot(app, "26-grpc-request")
        app.el("send").tap()
        waitLabel(app.el("grpc-status"), contains: "OK")
        XCTAssertTrue(app.el("grpc-count").label.hasPrefix("3 messages"), app.el("grpc-count").label)
        shot(app, "27-grpc-call")
        app.el("close-response").tap()

        app.navigationBars.buttons.element(boundBy: 0).tap()
        open(app, "Legacy service")
        app.el("grpc-load").tap()
        wait(app.el("grpc-no-reflection"))
        XCTAssertTrue(app.el("grpc-no-reflection").label.contains("reflection only"), app.el("grpc-no-reflection").label)
    }

    @MainActor
    func testGraphQLSchemaSuggestionsAndBrowser() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        app.el("new-request").tap()
        let url = urlField(app)
        wait(url)
        url.tap()
        url.typeText("https://stub.example/graphql\n")
        app.el("tab-Body").tap()
        app.el("body-kind").tap()
        app.buttons["GraphQL"].tap()
        let load = app.el("graphql-load-schema")
        wait(load)
        load.tap()
        waitLabel(app.el("graphql-schema-summary"), contains: "types")
        let query = app.el("graphql-query")
        query.tap()
        query.typeText("{ pro")
        let products = app.el("gql-suggestion-products")
        wait(products)
        XCTAssertTrue(app.el("gql-suggestion-product").exists)
        shot(app, "28-graphql-suggestions")
        products.tap()
        XCTAssertTrue((query.value as? String ?? "").contains("{ products"), "\(String(describing: query.value))")
        app.el("graphql-browse").tap()
        let queryType = app.el("schema-type-Query")
        wait(queryType)
        queryType.tap()
        wait(app.el("schema-field-products"))
        shot(app, "29-graphql-schema")
    }

    @MainActor
    func testMethodMenuSwitchesKinds() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        app.el("new-request").tap()
        let method = app.el("method")
        wait(method)
        method.tap()
        app.buttons["WebSocket"].tap()
        XCTAssertEqual(method.label, "Method WebSocket")
        wait(app.el("ws-add-message"))
        method.tap()
        app.buttons["gRPC"].tap()
        wait(app.el("grpc-load"))
        method.tap()
        app.buttons["Server-Sent Events"].tap()
        XCTAssertEqual(method.label, "Method Server-Sent Events")
        app.el("tab-Headers").tap()
        XCTAssertTrue(app.textFields["text/event-stream"].exists || app.textFields.matching(NSPredicate(format: "value == 'text/event-stream'")).count > 0)
    }
}
