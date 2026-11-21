import XCTest

/// The App Store screenshots (I6, CI job `store-screenshots`, artifact `app-store-screenshots`):
/// six iPhone screens on a 6.9" simulator and four iPad screens (landscape) on a 13" one, seeded
/// demo data only (`-UITestSeed`: the shop-api sample on *.shop.example hosts, StubURLProtocol,
/// the deterministic Apple Intelligence stand-in), store mode, the whole screen with the status bar
/// (CI overrides it to 9:41). One test per screenshot, so one broken flow loses one picture; the
/// file name is the upload order. The main test run skips this class.
final class StoreScreenshotTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    private var isPad: Bool { UIDevice.current.userInterfaceIdiom == .pad }

    // MARK: iPhone (6.9")

    @MainActor
    func testPhone01Collections() {
        guard !isPad else { return }
        let app = launch()
        wait(app.el("request-Create order"))
        sleep(1)
        store(app, "iphone-01-collections")
    }

    @MainActor
    func testPhone02Request() {
        guard !isPad else { return }
        let app = launch()
        openCreateOrder(app)
        store(app, "iphone-02-request")
    }

    @MainActor
    func testPhone03Response() {
        guard !isPad else { return }
        let app = launch()
        openCreateOrder(app)
        app.el("send").tap()
        waitForResponse(app)
        wait(app.el("explain-response"))
        sleep(1)
        store(app, "iphone-03-response")
    }

    @MainActor
    func testPhone04RunResult() {
        guard !isPad else { return }
        let app = launch()
        wait(app.el("request-Create order"))
        app.tabBars.buttons["Runs"].tap()
        let runShop = app.el("run-collection-shop-api")
        wait(runShop)
        runShop.tap()
        let start = app.el("run-start")
        wait(start)
        start.tap()
        wait(app.el("run-summary"), 30)
        wait(app.el("failure-Cancel order"))
        sleep(1)
        store(app, "iphone-04-run-result")
    }

    @MainActor
    func testPhone05WebSocket() {
        guard !isPad else { return }
        let app = launch()
        let shop = app.el("collection-shop-api")
        wait(shop)
        if shop.value as? String == "expanded" { shop.tap() }
        let realtime = app.el("collection-realtime")
        scrollTo(app, realtime)
        if realtime.value as? String != "expanded" { realtime.tap() }
        let row = app.el("request-Live prices")
        scrollTo(app, row)
        row.tap()
        wait(app.el("ws-add-message"))
        app.el("send").tap()
        waitLabel(app.el("socket-status"), contains: "Connected")
        let subscribe = app.el("ws-saved-subscribe")
        wait(subscribe)
        subscribe.tap()
        waitLabel(app.el("ws-log-received"), contains: "TEE-0193")
        sleep(2)
        store(app, "iphone-05-websocket")
    }

    @MainActor
    func testPhone06ExplainResponse() {
        guard !isPad else { return }
        let app = launch()
        openCreateOrder(app)
        app.el("send").tap()
        waitForResponse(app)
        let explain = app.el("explain-response")
        wait(explain)
        explain.tap()
        waitLabel(app.el("explain-text"), contains: "where to go from here", timeout: 30)
        app.el("suggest-test").tap()
        wait(app.el("suggested-test"))
        sleep(1)
        store(app, "iphone-06-explain")
    }

    // MARK: iPad (13", landscape)

    @MainActor
    func testPad01Request() {
        guard isPad else { return }
        let app = launchPad()
        sendOnPad(app, "Create order")
        wait(app.el("explain-response"))
        sleep(1)
        store(app, "ipad-01-request")
    }

    @MainActor
    func testPad02ExplainResponse() {
        guard isPad else { return }
        let app = launchPad()
        sendOnPad(app, "Create order")
        let explain = app.el("explain-response")
        wait(explain)
        for _ in 0..<6 where !explain.isHittable { app.el("json-body").swipeUp(velocity: .slow) }
        explain.tap()
        waitLabel(app.el("explain-text"), contains: "where to go from here", timeout: 30)
        // The explanation with "Suggest a test" under it: the iPad's form sheet would cut a suggested
        // script after its first line.
        wait(app.el("suggest-test"))
        sleep(1)
        store(app, "ipad-02-explain")
    }

    @MainActor
    func testPad03RunResult() {
        guard isPad else { return }
        let app = launchPad()
        wait(app.el("request-Create order"))
        // The sidebar's Runs page (below the collections), then Run shop-api.
        let runs = app.el("pad-page-Runs")
        for _ in 0..<10 where !(runs.exists && runs.isHittable) {
            // Drag inside the sidebar (left column) upwards.
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.12, dy: 0.8))
                .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.12, dy: 0.3)))
        }
        wait(runs)
        runs.tap()
        let run = app.el("run-collection-shop-api")
        wait(run)
        run.tap()
        let start = app.el("run-start")
        wait(start)
        start.tap()
        wait(app.el("run-summary"), 30)
        wait(app.el("failure-Cancel order"))
        sleep(1)
        store(app, "ipad-03-run-result")
    }

    @MainActor
    func testPad04GraphQL() {
        guard isPad else { return }
        let app = launchPad()
        sendOnPad(app, "Catalog query")
        // The answer stays as the app shows it ({ "data": { 1 key } }): tapping a line to unfold it
        // hit the wrong line on the CI iPad and folded the whole answer.
        wait(app.el("json-body"))
        sleep(1)
        store(app, "ipad-04-graphql")
    }

    // MARK: Helpers

    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication.seeded()
        app.launchArguments += ["-UITestStoreScreens"]
        app.launch()
        return app
    }

    @MainActor
    private func launchPad() -> XCUIApplication {
        XCUIDevice.shared.orientation = .landscapeLeft
        return launch()
    }

    @MainActor
    private func openCreateOrder(_ app: XCUIApplication) {
        let create = app.el("request-Create order")
        wait(create)
        create.tap()
        wait(app.el("body-preview"))
    }

    @MainActor
    private func sendOnPad(_ app: XCUIApplication, _ name: String) {
        let row = app.el("request-\(name)")
        wait(row)
        for _ in 0..<6 where !row.isHittable { app.el("request-Log in").swipeUp(velocity: .slow) }
        // The first request opens by itself at launch and can win over an early tap: tap until the
        // request column shows this one.
        let title = app.navigationBars[name]
        for _ in 0..<5 where !title.exists {
            row.tap()
            _ = title.waitForExistence(timeout: 3)
        }
        XCTAssertTrue(title.exists, "\(name) didn't open")
        let send = app.el("send")
        wait(send)
        send.tap()
        waitForResponse(app)
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

    @MainActor
    private func waitLabel(_ e: XCUIElement, contains text: String, timeout: TimeInterval = 15, file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if e.exists && e.label.contains(text) { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        }
        XCTFail("\(e) never said \(text) (\(e.exists ? e.label : "missing"))", file: file, line: line)
    }

    /// Nothing to buy on a store screenshot (store mode), then the whole screen. The iPad capture is
    /// in the panel's portrait orientation; CI turns it upright.
    @MainActor
    private func store(_ app: XCUIApplication, _ name: String, file: StaticString = #filePath, line: UInt = #line) {
        // Capitalised words match case-sensitively ("subscribe" is a saved WebSocket message, and
        // "price" a JSON key of the demo data); phrases and currency amounts in any case.
        for word in ["Upgrade", "Buy", "Purchase", "Subscribe", "Pricing", "Checkout", "Billing", "Pro plan", "Team plan", "Business plan"] {
            let hit = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", word)).firstMatch
            XCTAssertFalse(hit.exists, "'\(word)' on \(name): \(hit.exists ? hit.label : "")", file: file, line: line)
        }
        for phrase in ["free trial", "per month", "per seat", "in-app purchase", "buy now"] {
            XCTAssertFalse(app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] %@", phrase)).firstMatch.exists,
                           "'\(phrase)' on \(name)", file: file, line: line)
        }
        let money = NSPredicate(format: "label MATCHES %@", ".*[$€£][ ]?[0-9].*")
        XCTAssertFalse(app.staticTexts.containing(money).firstMatch.exists, "a price on \(name)", file: file, line: line)

        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }
}
