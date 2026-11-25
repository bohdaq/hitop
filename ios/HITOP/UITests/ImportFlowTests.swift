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
        wait(app.el("collection-Imported API"))
    }
}
