import XCTest

@MainActor
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

    /// Waits for the response: fails with the error card's text when the request failed.
    @MainActor
    func waitForResponse(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let status = app.el("status")
        let error = app.el("error-headline")
        let deadline = Date().addingTimeInterval(20)
        while Date() < deadline {
            if status.exists { return }
            if error.exists {
                let texts = app.staticTexts.allElementsBoundByIndex.prefix(40).map(\.label).joined(separator: " | ")
                XCTFail("request failed: \(error.label): \(texts)", file: file, line: line)
                return
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        }
        XCTFail("no response", file: file, line: line)
    }

    /// The URL field (a text view when it grows to several lines).
    @MainActor
    func urlField(_ app: XCUIApplication) -> XCUIElement {
        let view = app.textViews["url"]
        return view.exists ? view : app.textFields["url"]
    }
}
