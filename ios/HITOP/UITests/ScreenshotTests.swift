import XCTest

final class ScreenshotTests: XCTestCase {
    @MainActor
    func testScreenshots() {
        let app = XCUIApplication()
        app.launchArguments += ["-UITestSeed"]
        app.launch()
        XCTAssertTrue(app.buttons["Send"].waitForExistence(timeout: 10))
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "01-collections"
        shot.lifetime = .keepAlways
        add(shot)
    }
}
