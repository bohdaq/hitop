import XCTest

/// Account → Delete account (App Store 5.1.1(v)) against the in-app fake server: the
/// confirmation says what goes and what stays, deleting signs out here and keeps the collections;
/// a refusal (the only owner of an organization others use) keeps everything. No purchase words.
final class AccountDeletionFlowTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    private func launch(_ extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication.seeded()
        app.launchArguments += extra
        app.launch()
        return app
    }

    @MainActor
    private func signIn(_ app: XCUIApplication) {
        app.el("account-pill").tap()
        let signIn = app.el("sign-in")
        wait(signIn)
        signIn.tap()
        let google = app.el("continue-google")
        wait(google)
        google.tap()
        wait(app.el("account-email"), 20)
    }

    /// Scrolls the Account screen until `element` is on screen (List cells below the fold aren't
    /// in the hierarchy until then).
    @MainActor
    private func scrollTo(_ app: XCUIApplication, _ element: XCUIElement, file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(25)
        while Date() < deadline {
            if element.exists && element.isHittable { return }
            app.swipeUp(velocity: .slow)
            RunLoop.current.run(until: Date().addingTimeInterval(0.4))
        }
        XCTAssertTrue(element.exists, "missing \(element)", file: file, line: line)
    }

    @MainActor
    private func assertNothingToBuy(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let predicate = NSPredicate(format: "label CONTAINS[c] 'upgrade' OR label CONTAINS[c] 'pricing' OR label CONTAINS[c] 'buy' OR label CONTAINS '$' OR label CONTAINS[c] 'billing' OR label CONTAINS[c] 'subscription' OR label CONTAINS[c] 'price'")
        let hits = app.descendants(matching: .any).matching(predicate)
        XCTAssertEqual(hits.count, 0, "purchase words: \(hits.allElementsBoundByIndex.map(\.label))", file: file, line: line)
    }

    @MainActor
    private func openDeleteSheet(_ app: XCUIApplication) {
        let delete = app.el("delete-account")
        scrollTo(app, delete)
        delete.tap()
        wait(app.el("confirm-delete-account"))
    }

    @MainActor
    func testDeleteAccountSignsOutAndKeepsCollections() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = launch()
        let shop = app.el("collection-shop-api")
        wait(shop)
        signIn(app)
        openDeleteSheet(app)
        // What goes, what stays.
        XCTAssertTrue(app.el("delete-account-title").label.contains("ada@example.com"), app.el("delete-account-title").label)
        XCTAssertTrue(app.el("delete-account-stays").label.contains("stay here"), app.el("delete-account-stays").label)
        assertNothingToBuy(app)
        // Cancel first: nothing happens.
        app.el("cancel-delete-account").tap()
        wait(app.el("account-email"))
        openDeleteSheet(app)
        app.el("confirm-delete-account").tap()
        // Signed out, told so; the collections are still here.
        wait(app.el("account-deleted"), 20)
        XCTAssertTrue(app.el("account-deleted").label.contains("was deleted"), app.el("account-deleted").label)
        XCTAssertTrue(app.el("sign-in").exists)
        XCTAssertFalse(app.el("account-email").exists)
        app.el("account-done").tap()
        wait(shop)
    }

    @MainActor
    func testARefusalKeepsTheAccount() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = launch(["-UITestDeleteRefused"])
        wait(app.el("collection-shop-api"))
        signIn(app)
        openDeleteSheet(app)
        app.el("confirm-delete-account").tap()
        let error = app.el("delete-account-error")
        wait(error, 20)
        XCTAssertTrue(error.label.contains("make another member an owner"), error.label)
        assertNothingToBuy(app)
        app.el("cancel-delete-account").tap()
        wait(app.el("account-email"))
    }
}
