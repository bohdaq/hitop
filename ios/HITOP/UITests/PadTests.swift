import XCTest

/// iPad: the keyboard shortcuts of the three-column layout (⌘↩, ⌘S, ⌘K, ⌘N).
final class PadTests: XCTestCase {
    /// Presses a shortcut and waits for `appears`. The simulator drops the first key command after
    /// a launch (whichever shortcut comes first), so a shortcut gets one more press if nothing happened.
    @MainActor
    private func press(_ app: XCUIApplication, _ key: String, _ name: String, until appears: XCUIElement, file: StaticString = #filePath, line: UInt = #line) {
        app.typeKey(key, modifierFlags: .command)
        if appears.waitForExistence(timeout: 5) { return }
        app.typeKey(key, modifierFlags: .command)
        XCTAssertTrue(appears.waitForExistence(timeout: 5), name, file: file, line: line)
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
        press(app, XCUIKeyboardKey.return.rawValue, "⌘↩", until: app.el("status"))

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
        let close = app.buttons["Close"]
        press(app, "k", "⌘K", until: close)
        if close.exists { close.tap() }

        // ⌘N opens a new request.
        press(app, "n", "⌘N", until: app.staticTexts["Untitled"])
    }
}
