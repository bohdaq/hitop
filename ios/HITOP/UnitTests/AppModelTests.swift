import Foundation
import XCTest
import HitopKit
@testable import HITOP

@MainActor
final class AppModelTests: XCTestCase {
    var store: MemoryLibraryStore!
    var vault: MemoryVault!
    var model: AppModel!

    override func setUp() async throws {
        store = MemoryLibraryStore()
        vault = MemoryVault()
        model = AppModel(store: store, vault: vault, sender: Sender(configuration: StubURLProtocol.configuration))
        StubURLProtocol.enabled = true
        model.load()
        model.seed()
    }

    func shop() -> SavedCollection { model.collections.first { $0.name == "shop-api" }! }

    func testSeedLooksLikeTheDrafts() {
        XCTAssertEqual(model.collections.map(\.name), ["shop-api", "webhooks", "My requests"])
        XCTAssertEqual(shop().requests.count, 13)
        XCTAssertEqual(shop().allFolders, ["auth", "orders", "catalog", "customers"])
        XCTAssertEqual(model.activeEnvironment?.name, "Staging")
    }

    func testNewRequestIsAddedAtOnceAndNamedFromItsURL() {
        let r = model.newRequest(in: shop().id, folder: "orders")
        XCTAssertEqual(r.name, "Untitled")
        XCTAssertEqual(r.folder, "orders")
        XCTAssertNotNil(model.request(r.id))
        let editor = RequestEditor(request: r, model: model)
        editor.draft.url = "${baseUrl}/v2/refunds?x=1"
        editor.draft.method = "POST"
        XCTAssertEqual(editor.draft.name, "POST /v2/refunds")
        XCTAssertTrue(editor.isDirty)
        XCTAssertEqual(model.request(r.id)?.name, "Untitled", "the collection keeps the saved name until Save")
        editor.save()
        XCTAssertFalse(editor.isDirty)
        XCTAssertEqual(model.request(r.id)?.name, "POST /v2/refunds")
        XCTAssertEqual(model.request(r.id)?.file, "orders/post-v2-refunds")
        editor.draft.url = "${baseUrl}/v3"
        XCTAssertEqual(editor.draft.name, "POST /v2/refunds", "saved under a name: no longer follows")
        XCTAssertEqual(store.loadCollections().first { $0.id == shop().id }?.requests.count, 14)
    }

    func testNewRequestWithoutCollectionGoesToMyRequests() {
        let r = model.newRequest()
        XCTAssertEqual(model.collectionOf(request: r.id)?.name, "My requests")
    }

    func testRenameDeleteMovePinDuplicate() {
        let id: UInt64 = 1104
        model.renameRequest(id, to: "Place order")
        XCTAssertEqual(model.request(id)?.name, "Place order")
        XCTAssertEqual(model.request(id)?.file, "orders/place-order")
        model.togglePin(request: id)
        XCTAssertFalse(model.isPinned(request: id))
        model.togglePin(request: id)
        XCTAssertTrue(model.isPinned(request: id))
        let copy = model.duplicateRequest(id)!
        XCTAssertEqual(copy.name, "Place order copy")
        XCTAssertEqual(copy.folder, "orders")
        let mine = model.collections.first { $0.name == "My requests" }!
        model.moveRequest(id, to: mine.id, folder: "drafts")
        XCTAssertEqual(model.collectionOf(request: id)?.id, mine.id)
        XCTAssertEqual(model.request(id)?.folder, "drafts")
        XCTAssertTrue(model.collection(mine.id)!.allFolders.contains("drafts"))
        model.deleteRequest(id)
        XCTAssertNil(model.request(id))
        XCTAssertFalse(model.isPinned(request: id))
    }

    func testFolders() {
        model.addFolder("refunds", in: shop().id, parent: "orders")
        XCTAssertTrue(shop().allFolders.contains("orders/refunds"))
        model.renameFolder("orders", to: "sales", in: shop().id)
        XCTAssertTrue(shop().allFolders.contains("sales/refunds"))
        XCTAssertEqual(model.request(1104)?.folder, "sales")
        model.deleteFolder("sales", in: shop().id)
        XCTAssertNil(model.request(1104))
        XCTAssertFalse(shop().allFolders.contains { $0.hasPrefix("sales") })
    }

    func testCollections() {
        let c = model.addCollection(name: "payments")
        model.renameCollection(c.id, to: "billing")
        XCTAssertEqual(model.collection(c.id)?.name, "billing")
        model.togglePin(collection: c.id)
        XCTAssertTrue(model.isPinned(collection: c.id))
        model.deleteCollection(c.id)
        XCTAssertNil(model.collection(c.id))
        XCTAssertFalse(store.loadCollections().contains { $0.id == c.id })
    }

    func testSecretsStayOutOfTheFiles() {
        let env = model.environments[0]
        model.setVariable(env, name: "apiKey", value: "sk_live_1", secret: true)
        let saved = store.loadEnvironments().first { $0.id == env.id }!
        XCTAssertTrue(saved.secrets.contains("apiKey"))
        XCTAssertNil(saved.variables["apiKey"])
        XCTAssertEqual(vault.value(environment: env.id, name: "apiKey"), "sk_live_1")
        XCTAssertEqual(model.context(for: shop().id).environmentSecrets["apiKey"], "sk_live_1")
        // Made plain again: the value moves out of the vault.
        model.setVariable(model.environments[0], name: "apiKey", value: "visible", secret: false)
        XCTAssertNil(vault.value(environment: env.id, name: "apiKey"))
        XCTAssertEqual(store.loadEnvironments()[0].variables["apiKey"], "visible")
        // Globals' secrets use the "global" vault id.
        model.setVariable(model.globals, name: "gs", value: "g1", secret: true)
        XCTAssertEqual(vault.value(environment: "global", name: "gs"), "g1")
        XCTAssertNil(store.loadGlobals().variables["gs"])
    }

    func testEnvironmentSwitchChangesTheContext() {
        XCTAssertEqual(model.context(for: shop().id).merged["baseUrl"], "https://staging.shop.example")
        model.setActiveEnvironment("env-production")
        XCTAssertEqual(model.context(for: shop().id).merged["baseUrl"], "https://api.shop.example")
        XCTAssertEqual(store.loadState().activeEnvironmentID, "env-production")
        model.setActiveEnvironment(nil)
        XCTAssertNil(model.context(for: shop().id).merged["baseUrl"])
    }

    func testSendRecordsHistory() async throws {
        let before = model.history.count
        let editor = RequestEditor(request: model.request(1104)!, model: model)
        editor.send()
        await editor.waitForSend()
        guard case let .success(o)? = editor.outcome else { return XCTFail("no response: \(String(describing: editor.outcome))") }
        XCTAssertEqual(o.status, 201)
        XCTAssertTrue(o.text.contains("ord_8KQ2mZ41"))
        XCTAssertTrue(o.sentURL.hasPrefix("https://staging.shop.example/v2/orders"))
        XCTAssertEqual(o.cookies.first?.name, "session")
        XCTAssertNotNil(o.timing)
        if defaultScriptEngine != nil {
            XCTAssertEqual(o.tests.count, 4)
            XCTAssertTrue(o.tests.allSatisfy(\.pass), "\(o.tests)")
        }
        XCTAssertEqual(model.history.count, before + 1)
        XCTAssertEqual(model.history[0].response?.status, 201)
        XCTAssertEqual(model.history[0].request.id, 1104)
        let opened = model.openAsNewRequest(model.history[0])
        XCTAssertNotEqual(opened.id, 1104)
        XCTAssertEqual(model.collectionOf(request: opened.id)?.name, "My requests")
        XCTAssertEqual(opened.url, model.request(1104)?.url)
    }

    func testMissingVariableIsExplained() async {
        model.setActiveEnvironment(nil)
        let editor = RequestEditor(request: model.request(1103)!, model: model)
        editor.send()
        await editor.waitForSend()
        guard case let .failure(f)? = editor.outcome else { return XCTFail("expected a failure") }
        XCTAssertEqual(f.explained.headline, "A variable has no value")
        XCTAssertNotNil(model.history[0].error)
    }

    func testWebSocketIsNotSentYet() async {
        let editor = RequestEditor(request: model.request(1107)!, model: model)
        editor.send()
        await editor.waitForSend()
        guard case let .failure(f)? = editor.outcome else { return XCTFail("expected a failure") }
        XCTAssertTrue(f.message.contains("WebSocket"))
    }

    func testParamsEditTheURL() {
        let editor = RequestEditor(request: model.request(1103)!, model: model)
        var rows = editor.params
        XCTAssertEqual(rows.map(\.key), ["status"])
        rows.append(.init(id: 1, key: "page", value: "2", enabled: true))
        rows.append(.init(id: 2, key: "debug", value: "1", enabled: false))
        editor.setParams(rows)
        XCTAssertEqual(editor.draft.url, "${baseUrl}/v2/orders?status=open&page=2")
        XCTAssertEqual(editor.draft.disabledParams, [ParamRow(key: "debug", value: "1")])
        XCTAssertEqual(editor.params.count, 3)
    }

    func testFormatBody() {
        let editor = RequestEditor(request: model.request(1104)!, model: model)
        editor.draft.body = #"{"a":${customerId},"b":[1]}"#
        XCTAssertTrue(editor.jsonCheck.valid)
        XCTAssertTrue(editor.formatBody())
        XCTAssertEqual(editor.draft.body, "{\n  \"a\": ${customerId},\n  \"b\": [\n    1\n  ]\n}")
        editor.revert()
        XCTAssertFalse(editor.isDirty)
    }

    func testSearch() {
        let hits = model.search("order")
        XCTAssertTrue(hits.contains { $0.kind == .request && $0.title == "Create order" })
        XCTAssertTrue(hits.contains { $0.kind == .history })
        XCTAssertTrue(model.search("  ").isEmpty)
    }
}
