import XCTest

/// iPad: the keyboard shortcuts of the three-column layout (⌘N, ⌘K, ⌘↩, ⌘S).
final class PadTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testKeyboardShortcuts() {
        guard UIDevice.current.userInterfaceIdiom == .pad else { return }
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = XCUIApplication.seeded()
        app.launch()
        let create = app.el("request-Create order")
        wait(create)
        create.tap()
        wait(app.el("send"))

        // ⌘↩ sends.
        app.typeKey(.return, modifierFlags: .command)
        waitForResponse(app)
        XCTAssertEqual(app.el("status").label, "Status 201 Created")

        // ⌘S saves an edit: change the method, then save.
        app.el("method").tap()
        let put = app.buttons["PUT"]
        wait(put)
        put.tap()
        XCTAssertEqual(app.el("save").label, "Save")
        app.typeKey("s", modifierFlags: .command)
        XCTAssertTrue(app.el("save").waitForExistence(timeout: 5))
        let saved = NSPredicate(format: "label == 'Saved'")
        expectation(for: saved, evaluatedWith: app.el("save"))
        waitForExpectations(timeout: 5)

        // ⌘K opens search.
        app.typeKey("k", modifierFlags: .command)
        let close = app.buttons["Close"]
        wait(close)
        close.tap()

        // ⌘N opens a new request.
        app.typeKey("n", modifierFlags: .command)
        wait(app.el("url"))
        XCTAssertTrue(app.staticTexts["Untitled"].waitForExistence(timeout: 5))
    }
}
