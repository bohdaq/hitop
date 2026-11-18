import XCTest

/// Collection runs (I3) on the seeded shop-api (StubURLProtocol, no network): run from the Runs tab
/// and from the collection's menu, the result, the report with the failure open, run history;
/// the widgets and Live Activity in the in-app gallery. Screenshots 17–20.
final class RunFlowTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testRunFromTheRunsTabAndReadTheReport() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        wait(app.el("request-Create order"))
        app.tabBars.buttons["Runs"].tap()
        let runShop = app.el("run-collection-shop-api")
        wait(runShop)
        runShop.tap()

        let start = app.el("run-start")
        wait(start)
        shot(app, "17-run-options")
        start.tap()

        let summary = app.el("run-summary")
        wait(summary, 30)
        XCTAssertEqual(summary.label, "11 of 12 passed")
        wait(app.el("failure-Cancel order"))
        sleep(1)
        shot(app, "18-run-result")

        app.el("run-report").tap()
        let cancel = app.el("report-failure-Cancel order")
        wait(cancel)
        XCTAssertEqual(cancel.label, "HTTP 409")
        wait(app.el("share-report"))
        shot(app, "19-run-report")

        // The run is in the Runs tab's history.
        app.el("run-close").tap()
        let row = app.el("run-shop-api")
        wait(row)
        shot(app, "20-runs")

        // History → the report.
        row.tap()
        wait(app.el("report-summary"))
        wait(app.el("report-failure-Cancel order"))
    }

    @MainActor
    func testRunFromTheCollectionsMenuWithBail() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launch()
        let header = app.el("collection-shop-api")
        wait(header)
        header.press(forDuration: 1.2)
        let run = app.buttons["Run collection"]
        wait(run)
        run.tap()

        let bail = app.switches["run-bail"]
        wait(bail)
        bail.coordinate(withNormalizedOffset: CGVector(dx: 0.93, dy: 0.5)).tap()
        XCTAssertEqual(bail.value as? String, "1")
        app.el("run-start").tap()
        let summary = app.el("run-summary")
        wait(summary, 30)
        XCTAssertEqual(summary.label, "6 of 7 passed", "stopped at Cancel order")
        // Nothing to buy anywhere (store mode).
        for word in ["Upgrade", "Buy", "Price", "Subscribe", "Pro plan"] {
            XCTAssertFalse(app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] %@", word)).firstMatch.exists, word)
        }
    }

    @MainActor
    func testWidgetGallery() {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let app = XCUIApplication.seeded()
        app.launchArguments += ["-UITestWidgetGallery"]
        app.launch()
        wait(app.el("widget-last-run"), 30)
        wait(app.el("widget-pinned"))
        wait(app.el("live-activity"))
        sleep(1)
        shot(app, "21-widgets")
    }
}
