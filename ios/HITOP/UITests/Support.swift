import XCTest

extension XCUIApplication {
    /// Any element with this accessibility identifier.
    func el(_ id: String) -> XCUIElement { descendants(matching: .any).matching(identifier: id).firstMatch }

    static func seeded() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments += ["-UITestSeed"]
        return app
    }
}

extension XCTestCase {
    @MainActor
    func wait(_ e: XCUIElement, _ timeout: TimeInterval = 15, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(e.waitForExistence(timeout: timeout), "missing \(e)", file: file, line: line)
    }

    @MainActor
    func shot(_ app: XCUIApplication, _ name: String) {
        let a = XCTAttachment(screenshot: app.screenshot())
        a.name = name
        a.lifetime = .keepAlways
        add(a)
    }

    /// The URL field (a text view when it grows to several lines).
    @MainActor
    func urlField(_ app: XCUIApplication) -> XCUIElement {
        let view = app.textViews["url"]
        return view.exists ? view : app.textFields["url"]
    }
}
