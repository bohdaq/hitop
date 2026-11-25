import XCTest

/// Import: pasted text is read, shown, and added.
final class ImportFlowTests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    func testPastedPostmanCollectionIsAdded() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        let plus = app.el("new-request")
        wait(plus)
        plus.press(forDuration: 1.2)
        let item = app.buttons["Import…"]
        wait(item)
        item.tap()
        let text = app.el("import-text")
        wait(text)
        text.tap()
        text.typeText(#"{"info":{"name":"Imported API"},"item":[{"name":"Ping","request":{"method":"GET","url":"{{base}}/ping"}}]}"#)
        app.el("import-read").tap()
        wait(app.el("import-summary"))
        shot(app, "40-import")
        app.el("import-add").tap()
        // Below the others in the list, which makes only the rows on screen.
        let added = app.el("collection-Imported API")
        let deadline = Date().addingTimeInterval(40)
        while !added.exists && Date() < deadline {
            app.swipeUp()
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        }
        shot(app, "41-imported")
        let seen = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'collection-'")).allElementsBoundByIndex.map(\.identifier)
        XCTAssertTrue(added.exists, "not in the list; on screen: \(seen)")
    }
}
