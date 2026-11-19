import XCTest

/// Screenshots of the seeded app (CI artifact "screenshots"): the five iPhone screens and the iPad
/// three-column layout. Compared by hand with the design drafts (ios/HITOP_IOS.md).
final class ScreenshotTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testScreenshots() {
        if UIDevice.current.userInterfaceIdiom == .pad {
            pad()
        } else {
            phone()
        }
    }

    @MainActor
    private func phone() {
        let app = XCUIApplication.seeded()
        app.launch()
        let create = app.el("request-Create order")
        wait(create)
        shot(app, "01-collections")

        create.tap()
        wait(app.el("body-preview"))
        shot(app, "02-request")

        app.el("send").tap()
        waitForResponse(app)
        sleep(1)
        shot(app, "03-response")
        app.el("close-response").tap()

        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.tabBars.buttons["History"].tap()
        wait(app.el("history-Create order"))
        shot(app, "04-history")

        app.tabBars.buttons["Environments"].tap()
        wait(app.el("environment-Staging"))
        shot(app, "05-environments")
    }

    @MainActor
    private func pad() {
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = XCUIApplication.seeded()
        app.launch()
        let create = app.el("request-Create order")
        wait(create)
        create.tap()
        let send = app.el("send")
        wait(send)
        send.tap()
        waitForResponse(app)
        sleep(1)
        // The whole screen: an app capture of a rotated iPad comes back clipped. Its pixels are in the
        // panel's portrait orientation; CI's export step turns it upright.
        screen("01-ipad")

        // I5: the response column's Explain response (the drafts' iPad page), then its sheet.
        let explain = app.el("explain-response")
        wait(explain)
        for _ in 0..<6 where !explain.isHittable { app.el("json-body").swipeUp(velocity: .slow) }
        XCTAssertTrue(explain.isEnabled)
        screen("30-ipad-explain-button")
        explain.tap()
        wait(app.el("explain-text"))
        app.el("suggest-test").tap()
        wait(app.el("suggested-test"))
        sleep(1)
        screen("31-ipad-explain")
    }

    @MainActor
    private func screen(_ name: String) {
        let a = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }
}
