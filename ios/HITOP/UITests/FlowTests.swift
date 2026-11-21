import XCTest

/// iPhone flows against the seeded library; every request is answered by the in-app stub.
final class FlowTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testCreateSendSaveAndHistory() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()

        // A new request is added to My requests at once and named from its URL.
        app.el("new-request").tap()
        let url = urlField(app)
        wait(url)
        url.tap()
        url.typeText("https://stub.example/echo?x=1")
        app.el("send").tap()
        waitForResponse(app)
        let status = app.el("status")
        XCTAssertEqual(status.label, "Status 200 OK")
        XCTAssertTrue(app.el("sent-url").label.contains("https://stub.example/echo?x=1"))
        app.el("close-response").tap()

        XCTAssertTrue(app.staticTexts["GET /echo"].exists)
        let state = app.el("save-state")
        XCTAssertEqual(state.label, "Unsaved changes")
        app.el("save").tap()
        XCTAssertTrue(app.el("save-state").label.contains("My requests"), app.el("save-state").label)

        // The send is in the history; it opens as a new request.
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.tabBars.buttons["History"].tap()
        let row = app.el("history-GET /echo")
        wait(row)
        row.tap()
        wait(app.el("open-as-new"))
        app.el("open-as-new").tap()
        wait(app.el("url-display"))
        XCTAssertTrue(app.el("url-display").label.contains("https://stub.example/echo?x=1"))
    }

    @MainActor
    func testResponseSheetTestsAndEnvironmentSwitch() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()

        let create = app.el("request-Create order")
        wait(create)
        create.tap()
        wait(app.el("body-preview"))
        XCTAssertEqual(app.el("json-validity").label, "Valid")

        // Peek: a tap on the ${baseUrl} chip shows its value and where it comes from.
        app.el("url-display").coordinate(withNormalizedOffset: CGVector(dx: 0.04, dy: 0.5)).tap()
        let peek = app.el("peek-value")
        wait(peek)
        XCTAssertEqual(peek.label, "https://staging.shop.example")
        // Dismissed with a tap outside it.
        app.otherElements["PopoverDismissRegion"].firstMatch.tap()
        XCTAssertTrue(app.el("peek-value").waitForNonExistence(timeout: 5))

        app.el("send").tap()
        waitForResponse(app)
        XCTAssertEqual(app.el("status").label, "Status 201 Created")
        XCTAssertTrue(app.el("sent-url").label.contains("https://staging.shop.example/v2/orders"))
        XCTAssertTrue(app.el("json-body").exists)

        // Tests tab: the four checks of the post-request script passed (JavaScriptCore + pm-shim).
        app.el("rtab-Tests").tap()
        wait(app.el("test-pass-Status is 201"))
        XCTAssertTrue(app.el("test-pass-Total equals sum of items").exists)
        XCTAssertTrue(app.el("test-pass-Responds under 1 s").exists)
        XCTAssertTrue(app.el("test-pass-Has a tracking link").exists)
        app.el("rtab-Cookies").tap()
        XCTAssertTrue(app.staticTexts["session"].waitForExistence(timeout: 5))
        app.el("close-response").tap()

        // Switch to Production: the same request goes to the other host.
        app.el("environment-menu").tap()
        let production = app.buttons["Production"]
        wait(production)
        production.tap()
        XCTAssertTrue(app.el("environment-menu").label.contains("Production"))
        app.el("send").tap()
        waitForResponse(app)
        XCTAssertTrue(app.el("sent-url").label.contains("https://api.shop.example/v2/orders"), app.el("sent-url").label)
        app.el("close-response").tap()

        // No environment: ${baseUrl} has no value, and the error says what to do.
        app.el("environment-menu").tap()
        app.buttons["No environment"].tap()
        app.el("send").tap()
        wait(app.el("error-headline"))
        XCTAssertEqual(app.el("error-headline").label, "A variable has no value")
    }

    @MainActor
    func testEnvironmentsTabSwitchesAndKeepsSecretsMasked() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        app.tabBars.buttons["Environments"].tap()
        wait(app.el("use-Local"))
        app.el("use-Local").tap()
        XCTAssertTrue(app.el("use-Local").label.contains("active"))
        app.el("environment-Staging").tap()
        let token = app.el("variable-token")
        wait(token)
        XCTAssertTrue(token.label.contains("••••••"), token.label)
        XCTAssertFalse(token.label.contains("tok_staging"), token.label)
    }
}
