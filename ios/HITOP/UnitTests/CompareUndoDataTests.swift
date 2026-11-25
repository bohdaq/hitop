import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HitopKit
import XCTest
@testable import HITOP

@MainActor
final class CompareUndoDataTests: XCTestCase {
    private var store: MemoryLibraryStore!
    private var model: AppModel!

    override func setUp() async throws {
        store = MemoryLibraryStore()
        model = AppModel(store: store, vault: MemoryVault(), sender: Sender(configuration: StubURLProtocol.configuration),
                         webSockets: EchoWebSocketTransport(), grpcClient: FakeGrpcClient())
        StubURLProtocol.enabled = true
        model.load()
        model.seed()
    }

    private var shop: SavedCollection { model.collections.first { $0.name == "shop-api" }! }

    // MARK: Compare

    private func r(_ body: String, status: Int = 200, headers: [String: String] = [:]) -> ComparedResponse {
        ComparedResponse(id: "x", label: "x", status: status, headers: headers, body: body)
    }

    func testJSONIsComparedByPath() {
        let d = ResponseDiff.compare(r(#"{"a":1,"b":{"c":[1,2]},"id":9007199254740993,"gone":true}"#),
                                     r(#"{"b":{"c":[1,3,4]},"a":1.0,"id":9007199254740992,"new-key":null}"#, status: 201))
        XCTAssertTrue(d.json)
        XCTAssertEqual(d.status?.after, "201")
        XCTAssertEqual(d.body.map(\.path), ["$.b.c[1]", "$.b.c[2]", "$.gone", "$.id", "$[\"new-key\"]"])
        XCTAssertEqual(d.body.map(\.kind), [.changed, .added, .removed, .changed, .added])
        XCTAssertTrue(ResponseDiff.compare(r("{\"a\":1,\"b\":2}"), r("{ \"b\": 2, \"a\": 1 }")).same)
    }

    func testTextIsComparedByLineAndHeadersByName() {
        let d = ResponseDiff.compare(r("one\ntwo\nthree", headers: ["date": "a", "x": "1"]), r("one\r\n2\nthree\nfour", headers: ["date": "b", "y": "2"]))
        XCTAssertEqual(d.headers.map(\.path), ["x", "y"])
        XCTAssertEqual(d.body.map(\.kind), [.same, .removed, .added, .same, .added])
        XCTAssertEqual(ResponseDiff.lines("a\nb", "c\nd", limit: 1).map(\.kind), [.removed, .removed, .added, .added])
        XCTAssertTrue(ResponseDiff.compare(r(""), r("")).same)
    }

    func testSourcesAreExamplesAndEarlierAnswers() async {
        let q = shop.requests[0]
        for _ in 0..<2 { _ = await model.send(q, collectionID: shop.id) }
        var edited = q
        edited.extras["examples"] = .array([.object(["name": .string("Saved"), "status": .number(200), "headers": .array([.object(["name": .string("X"), "value": .string("1")])]), "body": .object(["a": .number(1)])])])
        guard case let .success(now) = await model.send(q, collectionID: shop.id) else { return XCTFail("no answer") }
        let s = ComparedResponse.sources(edited, outcome: now, history: model.history)
        XCTAssertEqual(s.first?.id, "current")
        XCTAssertEqual(s[1].label, "Example: Saved")
        XCTAssertEqual(s[1].body, "{\"a\":1}")
        XCTAssertEqual(s[1].headers, ["x": "1"])
        XCTAssertGreaterThanOrEqual(s.count, 4)
        XCTAssertFalse(s.contains { $0.id == "h\(model.history[0].id)" })
    }

    // MARK: Undo

    func testDeletesComeBackWhereTheyWere() {
        let before = shop
        let r = before.requests[2]
        model.togglePin(request: r.id)
        let pinned = model.isPinned(request: r.id)
        model.undoable("Deleted") { model.deleteRequest(r.id) }
        // Changed since: kept.
        model.renameRequest(before.requests[0].id, to: "Renamed since")
        model.undo()
        XCTAssertEqual(shop.requests.map(\.id), before.requests.map(\.id))
        XCTAssertEqual(shop.requests[0].name, "Renamed since")
        XCTAssertEqual(model.isPinned(request: r.id), pinned)
        XCTAssertNil(model.undoOffer)

        let whole = shop
        model.undoable("Deleted orders") { model.deleteFolder("orders", in: whole.id) }
        XCTAssertFalse(shop.allFolders.contains("orders"))
        model.undo()
        XCTAssertEqual(shop.requests, whole.requests)
        XCTAssertEqual(shop.allFolders, whole.allFolders)
    }

    func testAMoveAndADeletedCollectionComeBack() throws {
        let before = shop
        let target = try XCTUnwrap(model.collections.first { $0.name == "webhooks" })
        model.undoable("Moved") { model.moveRequest(before.requests[1].id, to: target.id, folder: "new") }
        model.undo()
        XCTAssertEqual(shop.requests, before.requests)
        XCTAssertEqual(model.collection(target.id)?.requests, target.requests)
        XCTAssertEqual(model.collection(target.id)?.allFolders, target.allFolders)

        model.undoable("Deleted") { model.deleteCollection(target.id) }
        model.undo()
        let back = try XCTUnwrap(model.collections.first { $0.name == "webhooks" })
        // Cloud may have the deletion of the old id.
        XCTAssertNotEqual(back.id, target.id)
        XCTAssertEqual(back.requests, target.requests)
        XCTAssertTrue(store.loadCollections().contains { $0.id == back.id })
    }

    func testASharedCollectionThatLeftIsNotBroughtBack() throws {
        let c = try XCTUnwrap(model.collections.first { $0.name == "webhooks" })
        model.sharedRoles[c.id] = "editor"
        model.undoable("Deleted") { model.deleteCollection(c.id) }
        model.undo()
        XCTAssertNil(model.collections.first { $0.name == "webhooks" })
    }

    // MARK: Data files

    func testRowsAreReadAsHitopRunReadsThem() throws {
        let csv = try RunData(name: "d.csv", data: Data("user,n\nada,\"1,5\"\nbob,2\n".utf8))
        XCTAssertEqual(csv.rows, [["user": "ada", "n": "1,5"], ["user": "bob", "n": "2"]])
        XCTAssertEqual(try RunData(name: "d.json", data: Data(#"[{"n":1.5,"on":true,"x":null}]"#.utf8)).rows, [["n": "1.5", "on": "true", "x": ""]])
        XCTAssertThrowsError(try RunData(name: "d.csv", data: Data("a,b\n1\n".utf8))) { XCTAssertTrue("\($0)".contains("line 2"), "\($0)") }
        XCTAssertThrowsError(try RunData(name: "d.csv", data: Data("a\n".utf8)))
        XCTAssertThrowsError(try RunData(name: "d.csv", data: Data([0xff, 0xfe])))
    }

    func testEachRowIsAnIteration() async throws {
        var q = SavedRequest(name: "One", url: "https://api.shop.example/${who}", method: "GET")
        q.headers = []
        let c = SavedCollection(name: "Rows", requests: [q], variables: ["who": "collection"])
        model.addCollectionCopy(c)
        let center = RunCenter(model: model, store: MemoryRunStore())
        var settings = RunSettings()
        settings.iterations = 5
        settings.data = try RunData(name: "d.csv", data: Data("who\nada\nbob\n".utf8))
        let record = await center.start(collectionID: c.id, settings: settings)
        let report = try XCTUnwrap(record?.report)
        XCTAssertEqual(report.collections[0].iterations.map(\.data), [["who": "ada"], ["who": "bob"]])
        XCTAssertEqual(report.requests.map(\.url), ["https://api.shop.example/ada", "https://api.shop.example/bob"])
        XCTAssertEqual(center.lastFinished?.iterations, 2)
    }
}
