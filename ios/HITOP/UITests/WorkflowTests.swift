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
        XCTAssertTrue(app.el("compare-to").label.contains("This response"), app.el("compare-to").label)
        XCTAssertFalse(app.el("compare-from").label.contains("This response"), app.el("compare-from").label)
        shot(app, "42-compare")
        app.el("compare-from").tap()
        let current = app.buttons["This response"].firstMatch
        wait(current)
        current.tap()
        let picked = NSPredicate(format: "label CONTAINS 'This response'")
        let changed = expectation(for: picked, evaluatedWith: app.el("compare-from"))
        wait(for: [changed], timeout: 15)
        wait(app.el("compare-same"))
        app.el("compare-close").tap()
        XCTAssertTrue(app.el("compare-close").waitForNonExistence(timeout: 10))
        wait(app.el("status"))
    }

    @MainActor
    func testARunIsPutOnASchedule() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        app.tabBars.buttons["Runs"].tap()
        let run = app.el("run-collection-shop-api")
        wait(run)
        run.tap()
        let picker = app.el("run-schedule")
        wait(picker)
        if !picker.isHittable { app.swipeUp() }
        picker.tap()
        let hour = app.buttons["Every hour"].firstMatch
        wait(hour)
        hour.tap()
        let chosen = expectation(for: NSPredicate(format: "label CONTAINS 'Not before the time is up'"), evaluatedWith: app.el("run-schedule-text"))
        wait(for: [chosen], timeout: 15)
        shot(app, "44-schedule")
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

    @MainActor
    func testSettingsOpenFromTheAccount() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        wait(app.el("account-pill"))
        app.el("account-pill").tap()
        let settings = app.el("settings")
        var swipes = 0
        while !(settings.exists && settings.isHittable) && swipes < 5 {
            app.swipeUp()
            swipes += 1
        }
        wait(settings)
        settings.tap()
        wait(app.el("settings-timeout"))
        wait(app.el("settings-add-ca"))
        wait(app.el("settings-add-p12"))
        shot(app, "44-settings")
    }
}
