import XCTest

/// Shared collections (I2b) on iPhone against the in-app fake server, with Grace's phone running
/// in the app process (`SharedScenario`: real encryption, her device key fixed): joining by an
/// invite code with the trust prompt, the "Shared with Acme" badge, a conflict choice, a
/// viewer's read-only editor, the Share sheet's members. Screenshots 12–16.
final class SharedFlowTests: XCTestCase {
    /// Grace's device key fingerprint (SharedScenario.peerSecret's), as the trust prompt shows it.
    static let graceFingerprint = "61e8 0907 4aee 9e56 fafc 0cbb 6bef 6749"
    /// The code Grace sends: `<token>.<fingerprint>`.
    static let code = "TEAMINVITE.61e809074aee9e56fafc0cbb6bef6749"

    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    private func launch(_ scenario: String) -> XCUIApplication {
        let app = XCUIApplication.seeded()
        app.launchArguments += [scenario]
        app.launch()
        return app
    }

    @MainActor
    private func waitLabel(_ element: XCUIElement, contains text: String, _ timeout: TimeInterval = 30, file: StaticString = #filePath, line: UInt = #line) {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if element.exists && element.label.contains(text) { return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        }
        XCTFail("\(element) label \"\(element.exists ? element.label : "missing")\" doesn't contain \"\(text)\"", file: file, line: line)
    }

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
    func testJoinByInviteCodeWithTheTrustPrompt() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = launch("-UITestSharedInvite")
        wait(app.el("account-pill"), 60)
        waitLabel(app.el("account-pill"), contains: "Acme")
        app.el("account-pill").tap()
        wait(app.el("account-email"), 20)
        // This device's fingerprint, for the owner to compare.
        let mine = app.el("device-fingerprint")
        scrollTo(app, mine)
        XCTAssertEqual(mine.label.split(separator: " ").count, 8, mine.label)
        let join = app.el("join-invite")
        scrollTo(app, join)
        join.tap()

        let field = app.el("invite-code")
        wait(field)
        field.tap()
        field.typeText(Self.code)
        app.el("invite-continue").tap()
        // The trust prompt shows the fingerprint the code names: Grace's device.
        let fp = app.el("inviter-fingerprint")
        wait(fp)
        XCTAssertEqual(fp.label, Self.graceFingerprint)
        shot(app, "13-join-trust")
        app.el("trust-accept").tap()
        // Accepted ("waiting for the owner", often only a moment), then Grace adds Ada and the
        // collection opens here by itself.
        let opened = app.el("join-opened")
        let deadline = Date().addingTimeInterval(60)
        while !opened.exists && Date() < deadline {
            if app.el("join-error").exists { XCTFail("join refused: \(app.el("join-error").label)"); return }
            RunLoop.current.run(until: Date().addingTimeInterval(0.3))
        }
        XCTAssertTrue(opened.exists, "the collection didn't open")
        app.el("join-done").tap()
        app.el("account-done").tap()

        let team = app.el("collection-Team API")
        wait(team, 20)
        waitLabel(team, contains: "Shared with Acme")
        XCTAssertTrue(app.el("shared-badge-Team API").exists)
        shot(app, "12-shared-collections")
    }

    @MainActor
    func testAConflictChoiceAndAViewerReadsOnly() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = launch("-UITestSharedJoined")
        let team = app.el("collection-Team API")
        wait(team, 60)
        waitLabel(team, contains: "Shared with Acme")
        let partner = app.el("collection-Partner API")
        wait(partner)
        waitLabel(partner, contains: "view only")

        // The viewer's collection: everything shows, nothing saves.
        if partner.value as? String != "expanded" { partner.tap() }
        let status = app.el("request-Partner status")
        wait(status)
        status.tap()
        wait(app.el("view-only"))
        XCTAssertFalse(app.el("save").exists)
        XCTAssertTrue(app.el("save-state").label.hasPrefix("View only"))
        shot(app, "16-viewer")
        app.navigationBars.buttons.element(boundBy: 0).tap()

        // Ada changes "List orders" while Grace changed it too: a conflict, resolved by a choice.
        if team.value as? String != "expanded" { team.tap() }
        let list = app.el("request-List orders")
        wait(list)
        list.tap()
        app.el("method").tap()
        let put = app.buttons["PUT"].firstMatch
        wait(put)
        put.tap()
        app.el("save").tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        let count = app.el("conflict-count-Team API")
        wait(count, 40)
        XCTAssertEqual(count.label, "1 conflict")

        team.press(forDuration: 1.0)
        let people = app.buttons["People and sharing"].firstMatch
        wait(people)
        people.tap()
        let row = app.el("conflict-List orders")
        wait(row, 20)
        XCTAssertTrue(app.el("keep-mine").exists)
        XCTAssertTrue(app.el("keep-both").exists)
        app.el("view-both").tap()
        shot(app, "14-conflict")
        app.el("take-theirs").tap()
        let deadline = Date().addingTimeInterval(20)
        while row.exists && Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.3)) }
        XCTAssertFalse(row.exists, "the conflict is resolved")
        wait(app.el("member-grace@example.com"), 20)
        wait(app.el("member-ada@example.com"))
        shot(app, "15-share-members")
        app.el("share-done").tap()
        XCTAssertFalse(app.el("conflict-count-Team API").exists)
        // Theirs: GET again.
        list.tap()
        waitLabel(app.el("method"), contains: "GET")
    }
}
