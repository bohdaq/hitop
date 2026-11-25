import XCTest

/// Compare, undo and file rows.
final class WorkflowTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    func testTwoAnswersAreCompared() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        let create = app.el("request-Create order")
        wait(create)
        create.tap()
        wait(app.el("send"))
        app.el("send").tap()
        waitForResponse(app)
        app.el("close-response").tap()
        wait(app.el("body-kind"))
        app.el("send").tap()
        waitForResponse(app)
        let compare = app.el("compare")
        var swipes = 0
        while !(compare.exists && compare.isHittable) && swipes < 6 {
            app.swipeUp()
            swipes += 1
        }
        wait(compare)
        compare.tap()
        wait(app.el("compare-from"))
        shot(app, "42-compare")
        app.el("compare-close").tap()
        XCTAssertTrue(app.el("compare-close").waitForNonExistence(timeout: 10))
        wait(app.el("status"))
    }

    @MainActor
    func testAFileRowAsksForItsFile() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        let create = app.el("request-Create order")
        wait(create)
        create.tap()
        wait(app.el("body-kind"))
        app.el("body-kind").tap()
        let multipart = app.buttons["Multipart"]
        wait(multipart)
        multipart.tap()
        wait(app.el("add-file"))
        app.el("add-file").tap()
        wait(app.el("pick-file"))
        shot(app, "43-file-row")
    }

    @MainActor
    func testADeletedRequestComesBack() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        let create = app.el("request-Create order")
        wait(create)
        create.press(forDuration: 1.2)
        let delete = app.buttons["Delete"].firstMatch
        wait(delete)
        delete.tap()
        let undo = app.el("undo")
        wait(undo)
        XCTAssertTrue(create.waitForNonExistence(timeout: 10))
        undo.tap()
        wait(app.el("request-Create order"))
        XCTAssertTrue(undo.waitForNonExistence(timeout: 10))
    }
}
