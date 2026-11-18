import XCTest

/// HITOP Cloud on iPhone against the in-app fake server (StubURLProtocol → FakeCloud at
/// cloud.hitop.test; the provider page approves at once): sign in, the sync state on the cards,
/// a conflict copy, the device-limit step (store mode: nothing to buy), sign out. Screenshots
/// 06–09 (CI artifact "screenshots").
final class CloudFlowTests: XCTestCase {
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

    /// Waits until `element`'s label contains `text`.
    @MainActor
    private func waitLabel(_ element: XCUIElement, contains text: String, _ timeout: TimeInterval = 20, file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if element.exists && element.label.contains(text) { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        }
        XCTFail("\(element) label \"\(element.exists ? element.label : "missing")\" doesn't contain \"\(text)\"", file: file, line: line)
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
    }

    @MainActor
    func testSignInSyncStateAndSignOut() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = launch()
        let shop = app.el("collection-shop-api")
        wait(shop)
        // Signed out, everything is on this iPhone.
        waitLabel(shop, contains: "on this iPhone")

        app.el("account-pill").tap()
        wait(app.el("sign-in"))
        shot(app, "06-account-signed-out")
        app.el("sign-in").tap()
        // Only what the server offers: Apple, Google, GitHub.
        wait(app.el("continue-apple"))
        XCTAssertTrue(app.el("continue-google").exists)
        XCTAssertTrue(app.el("continue-github").exists)
        shot(app, "07-sign-in")
        app.el("continue-google").tap()

        // Signed in: the account, this device, the plan in neutral words.
        let email = app.el("account-email")
        wait(email, 20)
        XCTAssertEqual(email.label, "ada@example.com")
        wait(app.el("device-HITOP on iPhone"))
        waitLabel(app.el("sync-status"), contains: "Synced")
        wait(app.el("plan-name"))
        assertNothingToBuy(app)
        shot(app, "08-account")
        app.el("account-done").tap()

        // The cards: synced, and the empty-of-nothing collections went up too (the seed has none empty).
        waitLabel(shop, contains: "synced")
        waitLabel(app.el("collection-webhooks"), contains: "synced")
        waitLabel(app.el("account-pill"), contains: "Organization: Personal")
        shot(app, "09-synced-collections")

        // Sign out: the collections stay, on this iPhone.
        app.el("account-pill").tap()
        let out = app.el("sign-out")
        wait(out)
        out.tap()
        let confirm = app.el("confirm-sign-out")
        wait(confirm)
        confirm.tap()
        wait(app.el("sign-in"))
        app.el("account-done").tap()
        waitLabel(shop, contains: "on this iPhone")
    }

    @MainActor
    func testAConflictKeepsAThisIPhoneCopy() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = launch(["-UITestCloudConflict"])
        signIn(app)
        wait(app.el("account-email"), 20)
        app.el("account-done").tap()
        let shop = app.el("collection-shop-api")
        waitLabel(shop, contains: "synced")

        // Renamed here while another browser changed it: theirs stays, ours is a copy.
        shop.press(forDuration: 1.2)
        let rename = app.buttons["Rename"].firstMatch
        wait(rename)
        rename.tap()
        let field = app.el("name-field")
        wait(field)
        field.typeText(" v2")
        app.buttons["Save"].firstMatch.tap()

        let copy = app.el("collection-shop-api v2 (this iPhone)")
        wait(copy, 25)
        wait(app.el("collection-shop-api (edited)"))
        let notice = app.el("sync-notice")
        wait(notice)
        XCTAssertTrue(notice.label.contains("shop-api v2 (this iPhone)"), notice.label)
        waitLabel(copy, contains: "synced")
        shot(app, "10-conflict-copy")
    }

    @MainActor
    func testDeviceLimitRemovesOneAndSignsIn() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = launch(["-UITestCloudDeviceLimit"])
        signIn(app)
        let step = app.el("device-limit")
        wait(step, 20)
        XCTAssertTrue(app.el("remove-Old iPad").exists)
        XCTAssertTrue(app.el("remove-Work laptop").exists)
        assertNothingToBuy(app)
        shot(app, "11-device-limit")
        app.el("remove-Old iPad").tap()

        let email = app.el("account-email")
        wait(email, 20)
        wait(app.el("device-Work laptop"))
        wait(app.el("device-HITOP on iPhone"))
        XCTAssertFalse(app.el("device-Old iPad").exists)
        assertNothingToBuy(app)
    }

    /// Store mode (App Store 3.1.1): no price, plan offer, upgrade or purchase link anywhere.
    @MainActor
    private func assertNothingToBuy(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let predicate = NSPredicate(format: "label CONTAINS[c] 'upgrade' OR label CONTAINS[c] 'see plans' OR label CONTAINS[c] 'pricing' OR label CONTAINS[c] 'buy' OR label CONTAINS '$' OR label CONTAINS[c] 'billing' OR label CONTAINS[c] 'change the plan'")
        let hits = app.descendants(matching: .any).matching(predicate)
        XCTAssertEqual(hits.count, 0, "purchase words: \(hits.allElementsBoundByIndex.map(\.label))", file: file, line: line)
    }
}
