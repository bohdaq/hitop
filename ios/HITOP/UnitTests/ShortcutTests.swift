import Foundation
import XCTest
@testable import HITOP

/// The keyboard shortcuts' actions (⌘↩ send, ⌘S save, ⌘N new request, ⌘K search), as the scene's
/// "Request" menu calls them, on both layouts. Synthetic key presses on the CI simulator drop
/// key commands at random, so the shortcuts are tested here rather than in a UI test.
@MainActor
final class ShortcutTests: XCTestCase {
    var model: AppModel!
    var nav: Navigator!

    override func setUp() async throws {
        model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault(), sender: Sender(configuration: StubURLProtocol.configuration))
        StubURLProtocol.enabled = true
        model.load()
        model.seed()
        nav = Navigator()
    }

    func testCurrentRequestOnBothLayouts() {
        XCTAssertNil(nav.currentRequest)
        nav.open(request: 1104)
        XCTAssertEqual(nav.currentRequest, 1104, "iPhone: the request on top of the Collections stack")
        nav.tab = .history
        XCTAssertNil(nav.currentRequest, "iPhone: another tab has no open request")
        nav.isPad = true
        XCTAssertEqual(nav.currentRequest, 1104, "iPad: the middle column's request")
        nav.padPage = .environments
        XCTAssertNil(nav.currentRequest)
    }

    func testSendAndSave() async {
        nav.isPad = true
        nav.open(request: 1104)
        nav.sendCurrent(model)
        let editor = model.editor(for: 1104)!
        await editor.waitForSend()
        guard case let .success(o)? = editor.outcome else { return XCTFail("⌘↩ didn't send") }
        XCTAssertEqual(o.status, 201)

        editor.draft.method = "PUT"
        XCTAssertTrue(editor.isDirty)
        nav.saveCurrent(model)
        XCTAssertFalse(editor.isDirty, "⌘S")
        XCTAssertEqual(model.request(1104)?.method, "PUT")
    }

    func testNewRequestAndSearch() {
        let before = model.collections.flatMap(\.requests).count
        nav.newRequest(model)
        XCTAssertEqual(model.collections.flatMap(\.requests).count, before + 1, "⌘N")
        XCTAssertEqual(model.request(nav.currentRequest!)?.name, "Untitled")

        nav.showSearch()
        XCTAssertEqual(nav.tab, .search, "⌘K on iPhone: the search tab")
        nav.isPad = true
        nav.showSearch()
        XCTAssertTrue(nav.padSearch, "⌘K on iPad: the search sheet")
    }

    func testNothingOpenIsHarmless() {
        nav.sendCurrent(model)
        nav.saveCurrent(model)
        XCTAssertEqual(model.history.count, Seed.history().count)
    }
}
