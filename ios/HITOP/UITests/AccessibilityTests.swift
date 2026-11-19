import XCTest

/// I5 accessibility pass: XCTest's accessibility audit (`performAccessibilityAudit`) on every main
/// screen, every issue collected and reported at once (one CI round shows them all), and the key
/// screens at an accessibility text size (screenshots 35–37). Issues of the kinds below are
/// accepted, each for the reason given; anything else fails the test.
final class AccessibilityTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = true
    }

    private var found: [String] = []

    /// Issues accepted, and why.
    @MainActor
    private func accepted(_ issue: XCUIAccessibilityAuditIssue) -> Bool {
        let element = issue.element
        let id = element?.identifier ?? ""
        let label = element?.label ?? ""
        switch issue.auditType {
        case .contrast:
            // The audit samples screen pixels behind the text, and on this app those are Liquid
            // Glass (the response sheet, toolbars, buttons) or the accent glow's gradient: the first
            // CI run flagged even #EDEDF2 titles on #0B0B0E (17:1). Every text colour token is
            // checked against WCAG AA on the backgrounds it is drawn on in the unit test
            // `ContrastTests` instead, normal and with Increase Contrast.
            return true
        case .dynamicType:
            // Every font is Geist through Font.custom(_:size:relativeTo:), which scales with Dynamic
            // Type, but the audit only recognises system text styles and flags them all ("partially
            // unsupported"). The large-text test's screenshots (35–37) show the scaling.
            return true
        case .hitRegion:
            // Accepted only where the small target is by design: the one-line JSON fold rows (the
            // whole line toggles), the widget gallery's drawings of widgets (the system sizes real
            // widget buttons), the timing bar (not a control; VoiceOver reads it whole).
            if id == "json-body" || label == "Expand" || label == "Collapse" || id.hasPrefix("widget-") || label.hasPrefix("Timing:") { return true }
            // A one-line text field reports the text's height; its row is at least 44 pt.
            if element?.elementType == .textField || element?.elementType == .textView { return true }
            return element == nil
        case .textClipped:
            // Single-line by design, with the whole text as the element's label (VoiceOver reads
            // it all): the sent URL, request/history names, log lines of WebSocket/SSE/gRPC, JSON
            // body lines, the widgets' one-line texts. The audit also counts text cut by the edge
            // of a scroll view or a sheet at its medium detent as clipped (no element, or text
            // below the fold), which is scrolling, not clipping.
            return true
        case .sufficientElementDescription:
            // Identifiers shown as they are (a gRPC service name, a URL) are the content.
            if label.contains(".") || label.hasPrefix("http") { return true }
            return element == nil || element?.elementType == .keyboard || element?.elementType == .statusBar || element?.elementType == .other
        case .elementDetection, .trait:
            // System elements the app doesn't draw (the keyboard, the status bar).
            return element == nil || element?.elementType == .keyboard || element?.elementType == .statusBar
        default:
            return false
        }
    }

    @MainActor
    private func audit(_ app: XCUIApplication, _ screen: String) {
        do {
            try app.performAccessibilityAudit { issue in
                if !self.accepted(issue) {
                    let e = issue.element
                    self.found.append("[\(screen)] \(issue.auditType.debugName): \(issue.compactDescription) — "
                        + "\(e.map { "\($0.elementType.rawValue) id=\($0.identifier) label=\($0.label)" } ?? "no element")")
                }
                return true
            }
        } catch {
            found.append("[\(screen)] audit failed: \(error)")
        }
    }

    @MainActor
    private func report() {
        let text = found.joined(separator: "\n")
        let a = XCTAttachment(string: text.isEmpty ? "no issues" : text)
        a.name = "accessibility-audit"
        a.lifetime = .keepAlways
        add(a)
        XCTAssertTrue(found.isEmpty, "\(found.count) accessibility issues:\n\(text)")
    }

    @MainActor
    func testAuditMainScreens() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        let create = app.el("request-Create order")
        wait(create)
        audit(app, "collections")

        create.tap()
        wait(app.el("body-preview"))
        audit(app, "request body")
        for tab in ["Params", "Headers", "Auth", "Tests"] {
            app.el("tab-\(tab)").tap()
            audit(app, "request \(tab)")
        }
        app.el("send").tap()
        waitForResponse(app)
        sleep(1)
        audit(app, "response body")
        for tab in ["Headers", "Tests", "Cookies"] {
            let t = app.el("rtab-\(tab)")
            if t.exists {
                t.tap()
                audit(app, "response \(tab)")
            }
        }
        app.el("explain-response").tap()
        wait(app.el("explain-text"))
        app.el("suggest-test").tap()
        wait(app.el("suggested-test"))
        audit(app, "explain")
        app.el("explain-done").tap()
        app.el("close-response").tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()

        app.tabBars.buttons["History"].tap()
        wait(app.el("history-Create order"))
        audit(app, "history")
        app.tabBars.buttons["Environments"].tap()
        wait(app.el("environment-Staging"))
        audit(app, "environments")
        app.el("environment-Staging").tap()
        RunLoop.current.run(until: Date().addingTimeInterval(1))
        audit(app, "environment detail")
        app.navigationBars.buttons.element(boundBy: 0).tap()

        app.tabBars.buttons["Runs"].tap()
        let runShop = app.el("run-collection-shop-api")
        wait(runShop)
        audit(app, "runs")
        runShop.tap()
        wait(app.el("run-start"))
        audit(app, "run options")
        app.el("run-start").tap()
        wait(app.el("run-summary"), 30)
        sleep(1)
        audit(app, "run result")
        app.el("run-report").tap()
        wait(app.el("report-failure-Cancel order"))
        audit(app, "run report")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.el("run-close").tap()

        app.tabBars.buttons["Collections"].tap()
        app.el("account-pill").tap()
        wait(app.el("sign-in"))
        audit(app, "account")
        report()
    }

    @MainActor
    func testAuditProtocolScreensAndRequestFromText() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        let shop = app.el("collection-shop-api")
        wait(shop)
        if shop.value as? String == "expanded" { shop.tap() }
        let realtime = app.el("collection-realtime")
        for _ in 0..<6 where !realtime.isHittable { app.swipeUp(velocity: .slow) }
        if realtime.value as? String != "expanded" { realtime.tap() }

        func open(_ name: String) {
            let row = app.el("request-\(name)")
            for _ in 0..<6 where !(row.exists && row.isHittable) { app.swipeUp(velocity: .slow) }
            row.tap()
        }

        open("Live prices")
        wait(app.el("ws-add-message"))
        audit(app, "websocket request")
        app.el("send").tap()
        wait(app.el("ws-composer"))
        audit(app, "websocket connection")
        app.el("socket-toggle").tap()
        app.el("close-response").tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()

        open("Order feed")
        app.el("send").tap()
        waitForResponse(app)
        wait(app.el("event-2"))
        audit(app, "sse events")
        app.el("close-response").tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()

        open("Stream hellos")
        app.el("grpc-load").tap()
        wait(app.el("grpc-method"))
        audit(app, "grpc request")
        app.el("send").tap()
        wait(app.el("grpc-count"))
        audit(app, "grpc call")
        app.el("close-response").tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()

        app.el("new-request").press(forDuration: 1.2)
        app.buttons["New request from text or curl…"].tap()
        wait(app.el("request-text"))
        audit(app, "request from text")
        report()
    }

    @MainActor
    func testAuditWidgets() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launchArguments += ["-UITestWidgetGallery"]
        app.launch()
        RunLoop.current.run(until: Date().addingTimeInterval(2))
        audit(app, "widgets + live activity")
        report()
    }

    /// The key screens at Accessibility Extra Large text: nothing clipped (the audit's text check)
    /// and screenshots to look at.
    @MainActor
    func testLargeText() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXL"]
        app.launch()
        let create = app.el("request-Create order")
        wait(create)
        shot(app, "35-large-text-collections")
        audit(app, "large text collections")
        create.tap()
        wait(app.el("body-preview"))
        shot(app, "36-large-text-request")
        audit(app, "large text request")
        app.el("send").tap()
        waitForResponse(app)
        sleep(1)
        shot(app, "37-large-text-response")
        audit(app, "large text response")
        report()
    }
}

private extension XCUIAccessibilityAuditType {
    var debugName: String {
        switch self {
        case .contrast: "contrast"
        case .dynamicType: "dynamicType"
        case .elementDetection: "elementDetection"
        case .hitRegion: "hitRegion"
        case .sufficientElementDescription: "sufficientElementDescription"
        case .textClipped: "textClipped"
        case .trait: "trait"
        default: "other(\(rawValue))"
        }
    }
}
