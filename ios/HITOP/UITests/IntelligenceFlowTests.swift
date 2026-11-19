import XCTest

/// I5 on the iPhone: Explain response + Suggest a test, a request from a curl command and from
/// plain text, the Apple-Intelligence-off path, and the GraphQL variables box with the keyboard
/// up (I4 review). The model is the deterministic `FakeAssistant` (`-UITestSeed`): CI simulators
/// have no on-device model. Screenshots 30–34.
final class IntelligenceFlowTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    private func waitLabel(_ e: XCUIElement, contains text: String, timeout: TimeInterval = 15, file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if e.exists && e.label.contains(text) { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        }
        XCTFail("\(e) never said \(text): \(e.exists ? e.label : "missing")", file: file, line: line)
    }

    @MainActor
    private func sendCreateOrder(_ app: XCUIApplication) {
        let create = app.el("request-Create order")
        wait(create)
        create.tap()
        wait(app.el("body-preview"))
        app.el("send").tap()
        waitForResponse(app)
    }

    /// The "+" menu's "New request from text or curl…" (a long press opens the menu).
    @MainActor
    private func openFromText(_ app: XCUIApplication) {
        let plus = app.el("new-request")
        wait(plus)
        plus.press(forDuration: 1.2)
        let item = app.buttons["New request from text or curl…"]
        wait(item)
        item.tap()
        wait(app.el("request-text"))
    }

    @MainActor
    func testExplainAndSuggestATest() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        sendCreateOrder(app)
        let explain = app.el("explain-response")
        wait(explain)
        XCTAssertEqual(explain.label, "Explain with Apple Intelligence")
        explain.tap()
        waitLabel(app.el("explain-text"), contains: "201 Created")
        XCTAssertTrue(app.el("on-device-note").exists || app.staticTexts["On device · nothing leaves your iPhone"].exists)
        shot(app, "30-explain-response")

        app.el("suggest-test").tap()
        let script = app.el("suggested-test")
        wait(script)
        XCTAssertTrue(script.value as? String == "Status is 201", "\(String(describing: script.value))")
        shot(app, "31-suggested-test")
        app.el("add-test").tap()
        wait(app.el("test-added"))
        app.el("explain-done").tap()
        app.el("close-response").tap()
        XCTAssertEqual(app.el("save-state").label, "Unsaved changes", "the test is in the request's Tests, not saved yet")
        app.el("tab-Tests").tap()
        let tests = app.el("tests-script")
        wait(tests)
        XCTAssertTrue((tests.value as? String ?? "").contains("pm.test(\"Status is 201\", () => pm.response.to.have.status(201));"),
                      "\(String(describing: tests.value))")
    }

    @MainActor
    func testRequestFromCurlAndFromText() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        openFromText(app)
        let field = app.el("request-text")
        field.tap()
        field.typeText("curl -X PUT https://stub.example/echo -H 'X-Trace: t-1' -d '{\"a\":1}'")
        waitLabel(app.el("request-text-source"), contains: "A curl command")
        shot(app, "32-request-from-curl")
        app.el("request-text-create").tap()
        let method = app.el("method")
        wait(method)
        XCTAssertEqual(method.label, "Method PUT")
        XCTAssertTrue(app.el("url-display").label.contains("https://stub.example/echo"), app.el("url-display").label)
        app.el("send").tap()
        waitForResponse(app)
        app.el("close-response").tap()

        app.navigationBars.buttons.element(boundBy: 0).tap()
        openFromText(app)
        let text = app.el("request-text")
        text.tap()
        text.typeText("POST a JSON order to https://stub.example/echo")
        waitLabel(app.el("request-text-source"), contains: "Apple Intelligence")
        app.el("request-text-create").tap()
        wait(app.el("method"))
        XCTAssertEqual(app.el("method").label, "Method POST")
        XCTAssertTrue(app.el("url-display").label.contains("https://stub.example/echo"), app.el("url-display").label)
    }

    @MainActor
    func testAppleIntelligenceOff() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launchArguments += ["-UITestAIUnavailable"]
        app.launch()
        sendCreateOrder(app)
        XCTAssertFalse(app.el("explain-response").exists, "no Explain without the model")
        XCTAssertTrue(app.el("share-response").exists, "Share keeps its place")
        app.el("close-response").tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()

        openFromText(app)
        let field = app.el("request-text")
        field.tap()
        field.typeText("list the orders")
        waitLabel(app.el("request-text-source"), contains: "isn't available on this device")
        XCTAssertFalse(app.el("request-text-create").isEnabled, "free text needs the model")
        shot(app, "33-ai-unavailable")
        field.typeText(XCUIKeyboardKey.delete.rawValue.repeated(15))
        field.typeText("curl https://stub.example/echo")
        waitLabel(app.el("request-text-source"), contains: "A curl command")
        XCTAssertTrue(app.el("request-text-create").isEnabled, "curl needs no model")
        app.el("request-text-create").tap()
        wait(app.el("method"))
        XCTAssertEqual(app.el("method").label, "Method GET")
    }

    /// I4 review: with the keyboard up, the floating Save + Send bar covered the GraphQL variables.
    @MainActor
    func testGraphQLVariablesStayVisibleWithTheKeyboard() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        app.el("new-request").tap()
        let url = urlField(app)
        wait(url)
        url.tap()
        url.typeText("https://stub.example/graphql")
        // The keyboard's bar has Done, Save and Send while the floating bar steps aside.
        wait(app.el("keyboard-done"))
        app.el("keyboard-done").tap()
        app.el("tab-Body").tap()
        app.el("body-kind").tap()
        app.buttons["GraphQL"].tap()
        let variables = app.el("graphql-variables")
        wait(variables)
        for _ in 0..<4 where !variables.isHittable { app.swipeUp(velocity: .slow) }
        variables.tap()
        variables.typeText("{\"first\": 3}")
        let done = app.el("keyboard-done")
        wait(done)
        XCTAssertTrue(variables.isHittable, "the variables box is not covered")
        XCTAssertLessThan(variables.frame.minY + 30, done.frame.minY, "the variables box shows above the keyboard's bar")
        shot(app, "34-graphql-variables-keyboard")
        done.tap()
        wait(app.el("send"))
    }
}

private extension String {
    func repeated(_ n: Int) -> String { String(repeating: self, count: n) }
}
