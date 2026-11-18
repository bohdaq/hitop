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
        let app = XCUIApplication.seeded()
        app.launchArguments += ["-UITestLandscape"]
        app.launch()
        sleep(2)
        // The runner's simulators stay in portrait, where the split view hides the sidebar until asked.
        let create = app.el("request-Create order")
        let showSidebar = app.buttons["Show Sidebar"]
        if !create.waitForExistence(timeout: 5), showSidebar.exists { showSidebar.tap() }
        wait(create)
        create.tap()
        sleep(1)
        if app.buttons["Hide Sidebar"].exists, !app.el("send").isHittable { app.buttons["Hide Sidebar"].tap() }
        let send = app.el("send")
        wait(send)
        send.tap()
        // The first tap can land while the sidebar overlay is still closing.
        if !app.el("status").waitForExistence(timeout: 5) { send.tap() }
        waitForResponse(app)
        if showSidebar.exists {
            showSidebar.tap()
            sleep(1)
        }
        sleep(1)
        shot(app, "01-ipad")
    }
}
