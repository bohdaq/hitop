import XCTest

final class HiTopUITests: XCTestCase {

    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-uitesting"]
        app.launch()
    }

    override func tearDownWithError() throws {
        app = nil
    }

    // MARK: - Launch

    func testAppLaunchShowsNavigationTitle() {
        // The sidebar should show the "HiTop" navigation title
        let navBar = app.navigationBars["HiTop"]
        XCTAssertTrue(navBar.waitForExistence(timeout: 3))
    }

    func testDefaultCollectionVisibleOnLaunch() {
        let collectionHeader = app.staticTexts["Default"]
        XCTAssertTrue(collectionHeader.waitForExistence(timeout: 3))
    }

    func testDefaultRequestVisibleOnLaunch() {
        let requestCell = app.staticTexts["New Request"]
        XCTAssertTrue(requestCell.waitForExistence(timeout: 3))
    }

    // MARK: - Navigation to request detail

    func testTappingRequestShowsURLBar() {
        let requestCell = app.staticTexts["New Request"]
        XCTAssertTrue(requestCell.waitForExistence(timeout: 3))
        requestCell.tap()

        let urlField = app.textFields["urlTextField"]
        XCTAssertTrue(urlField.waitForExistence(timeout: 3))
    }

    func testTappingRequestShowsSendButton() {
        let requestCell = app.staticTexts["New Request"]
        XCTAssertTrue(requestCell.waitForExistence(timeout: 3))
        requestCell.tap()

        let sendButton = app.buttons["sendButton"]
        XCTAssertTrue(sendButton.waitForExistence(timeout: 3))
    }

    func testTappingRequestShowsMethodMenu() {
        let requestCell = app.staticTexts["New Request"]
        XCTAssertTrue(requestCell.waitForExistence(timeout: 3))
        requestCell.tap()

        let methodMenu = app.buttons["methodMenu"]
        XCTAssertTrue(methodMenu.waitForExistence(timeout: 3))
    }

    // MARK: - URL field interaction

    func testURLFieldAcceptsInput() {
        let requestCell = app.staticTexts["New Request"]
        XCTAssertTrue(requestCell.waitForExistence(timeout: 3))
        requestCell.tap()

        let urlField = app.textFields["urlTextField"]
        XCTAssertTrue(urlField.waitForExistence(timeout: 3))

        urlField.tap()
        urlField.typeText("https://httpbin.org/get")
        XCTAssertEqual(urlField.value as? String, "https://httpbin.org/get")
    }

    // MARK: - Tab switching

    func testBodyTabShowsEditor() {
        navigateToRequest()

        let bodyTab = app.buttons["Body"]
        XCTAssertTrue(bodyTab.waitForExistence(timeout: 3))
        bodyTab.tap()

        // A TextEditor or ScrollView should appear for the body
        let bodyEditor = app.textViews.firstMatch
        XCTAssertTrue(bodyEditor.waitForExistence(timeout: 3))
    }

    func testScriptsTabShowsPreRequestLabel() {
        navigateToRequest()

        let scriptsTab = app.buttons["Scripts"]
        XCTAssertTrue(scriptsTab.waitForExistence(timeout: 3))
        scriptsTab.tap()

        let preLabel = app.staticTexts["Pre-Request Script"]
        XCTAssertTrue(preLabel.waitForExistence(timeout: 3))
    }

    func testResponseTabShowsPlaceholder() {
        navigateToRequest()

        let responseTab = app.buttons["Response"]
        XCTAssertTrue(responseTab.waitForExistence(timeout: 3))
        responseTab.tap()

        let placeholder = app.staticTexts["Send a request to see the response"]
        XCTAssertTrue(placeholder.waitForExistence(timeout: 3))
    }

    // MARK: - Add collection

    func testAddCollectionViaToolbarButton() {
        // Look for + button in toolbar
        let addButton = app.buttons["Add Collection"]
        XCTAssertTrue(addButton.waitForExistence(timeout: 3))
        addButton.tap()

        // Alert should appear
        let alert = app.alerts.firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: 3))

        // Type collection name and confirm
        let textField = alert.textFields.firstMatch
        textField.typeText("My Test Collection")
        alert.buttons["Add"].tap()

        // New collection should appear in sidebar
        let newCollection = app.staticTexts["My Test Collection"]
        XCTAssertTrue(newCollection.waitForExistence(timeout: 3))
    }

    func testAddCollectionAlertCancelDoesNotCreateCollection() {
        let addButton = app.buttons["Add Collection"]
        XCTAssertTrue(addButton.waitForExistence(timeout: 3))
        addButton.tap()

        let alert = app.alerts.firstMatch
        XCTAssertTrue(alert.waitForExistence(timeout: 3))
        alert.buttons["Cancel"].tap()

        // Only the default collection should remain
        let defaultCollection = app.staticTexts["Default"]
        XCTAssertTrue(defaultCollection.exists)
    }

    // MARK: - Method picker

    func testMethodMenuCyclesThroughMethods() {
        navigateToRequest()

        let methodMenu = app.buttons["methodMenu"]
        XCTAssertTrue(methodMenu.waitForExistence(timeout: 3))
        methodMenu.tap()

        // POST option should appear in the menu
        let postOption = app.buttons["POST"]
        XCTAssertTrue(postOption.waitForExistence(timeout: 3))
        postOption.tap()

        XCTAssertTrue(app.buttons["methodMenu"].waitForExistence(timeout: 3))
    }

    // MARK: - History sheet

    func testHistoryToolbarButtonOpensSheet() {
        navigateToRequest()

        // History button is in the sidebar toolbar
        let historyButton = app.buttons["History"]
        XCTAssertTrue(historyButton.waitForExistence(timeout: 3))
        historyButton.tap()

        // Sheet should appear with "History" title
        let historyTitle = app.staticTexts["History"]
        XCTAssertTrue(historyTitle.waitForExistence(timeout: 3))
    }

    // MARK: - Helper

    private func navigateToRequest() {
        let requestCell = app.staticTexts["New Request"]
        guard requestCell.waitForExistence(timeout: 3) else {
            XCTFail("New Request cell not found in sidebar")
            return
        }
        requestCell.tap()
        _ = app.textFields["urlTextField"].waitForExistence(timeout: 3)
    }
}
