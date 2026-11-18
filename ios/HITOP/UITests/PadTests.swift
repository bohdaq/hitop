import XCTest

/// iPad: the keyboard shortcuts of the three-column layout (⌘↩, ⌘S, ⌘K, ⌘N).
final class PadTests: XCTestCase {
    @MainActor
    func testKeyboardShortcuts() {
        guard UIDevice.current.userInterfaceIdiom == .pad else { return }
        XCUIDevice.shared.orientation = .landscapeLeft
        let app = XCUIApplication.seeded()
        app.launch()
        // A tap first: key commands reach the app once its window has taken focus.
        let create = app.el("request-Create order")
        wait(create)
        create.tap()
        wait(app.el("send"))

        // ⌘↩ sends.
        app.typeKey(.return, modifierFlags: .command)
        XCTAssertTrue(app.el("status").waitForExistence(timeout: 10), "⌘↩")

        // ⌘S saves an edit (the method).
        app.el("method").tap()
        let put = app.buttons["PUT"]
        wait(put)
        put.tap()
        XCTAssertEqual(app.el("save").label, "Save")
        app.typeKey("s", modifierFlags: .command)
        let saved = expectation(for: NSPredicate(format: "label == 'Saved'"), evaluatedWith: app.el("save"))
        XCTAssertEqual(XCTWaiter.wait(for: [saved], timeout: 5), .completed, "⌘S")

        // ⌘K opens search.
        app.typeKey("k", modifierFlags: .command)
        let close = app.buttons["Close"]
        XCTAssertTrue(close.waitForExistence(timeout: 5), "⌘K")
        if close.exists { close.tap() }

        // ⌘N opens a new request.
        app.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(app.staticTexts["Untitled"].waitForExistence(timeout: 5), "⌘N")
    }
}
