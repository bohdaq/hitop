import Foundation
import XCTest
@testable import HITOP

final class StorageTests: XCTestCase {
    var dir: URL!

    override func setUp() {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("hitop-tests-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: dir)
    }

    /// A collection as the web app stores it, with keys the app doesn't model.
    static let webCollection = """
    {"id": 1700000000001, "name": "Shop", "variables": {"baseUrl": "https://x.test"},
     "auth": {"type": "bearer", "token": "${token}"},
     "folders": ["orders"],
     "openapi": {"source": "spec.yaml"},
     "requests": [
       {"id": 1700000000002, "name": "List", "url": "${baseUrl}/orders?limit=5", "method": "get",
        "headers": [{"name": "Accept", "value": "application/json"}, {"name": "X-Off", "value": "1", "enabled": false}, {"name": "", "value": ""}],
        "body": "", "preRequestScript": "", "postRequestScript": "pm.test('ok', () => {});",
        "bodyMode": "raw", "file": "orders/list",
        "examples": [{"name": "200", "status": 200, "body": "[]"}],
        "settings": {"timeoutMs": 5000}},
       {"id": "req-legacy", "name": "Socket", "url": "wss://x.test/ws", "method": "GET", "kind": "websocket",
        "websocket": {"messages": [{"name": "hi", "content": "hello"}]}},
       {"id": 1700000000003, "name": "Q", "url": "https://x.test/graphql", "method": "POST", "body": "{ me { id } }",
        "bodyMode": "raw", "bodyType": "graphql", "graphql": {"variables": "{\\"a\\": 1}", "operationName": "Me"}},
       {"id": 1700000000004, "name": "Form", "url": "https://x.test/f", "method": "POST", "bodyMode": "urlencoded",
        "formFields": [{"key": "a", "value": "1", "type": "text", "enabled": true}],
        "auth": {"type": "apikey", "key": "X-Key", "value": "k", "in": "query"}}
     ]}
    """

    func testWebCollectionDecodesAndKeepsUnknownKeys() throws {
        let c = try JSONDecoder().decode(SavedCollection.self, from: Data(Self.webCollection.utf8))
        XCTAssertEqual(c.requests.count, 4)
        let list = c.requests[0]
        XCTAssertEqual(list.method, "GET")
        XCTAssertEqual(list.headers.count, 2, "the blank row is dropped")
        XCTAssertFalse(list.headers[1].enabled)
        XCTAssertEqual(list.folder, "orders")
        XCTAssertNotNil(list.extras["examples"])
        XCTAssertNotNil(c.extras["openapi"])
        XCTAssertEqual(c.requests[1].id, derivedID("req-legacy"))
        XCTAssertEqual(c.requests[1].kind, "websocket")
        XCTAssertEqual(c.requests[2].bodyKind, .graphql)
        XCTAssertEqual(c.requests[2].graphqlOperationName, "Me")
        XCTAssertEqual(c.requests[3].bodyKind, .form)
        XCTAssertEqual(c.requests[3].auth, .apiKey(key: "X-Key", value: "k", inQuery: true))
        XCTAssertEqual(c.auth, .bearer(token: "${token}"))

        // Round trip: written and read again, everything is still there.
        let again = try JSONDecoder().decode(SavedCollection.self, from: JSONEncoder().encode(c))
        XCTAssertEqual(again, c)
        let json = String(decoding: try JSONEncoder().encode(c), as: UTF8.self)
        XCTAssertTrue(json.contains("\"examples\""))
        XCTAssertTrue(json.contains("\"websocket\""))
        XCTAssertTrue(json.contains("\"openapi\""))
        XCTAssertTrue(json.contains("\"timeoutMs\":5000"))
    }

    func testFileStoreRoundTrip() throws {
        let store = FileLibraryStore(root: dir)
        let c = try JSONDecoder().decode(SavedCollection.self, from: Data(Self.webCollection.utf8))
        try store.saveCollection(c)
        try store.saveEnvironments([SavedEnvironment(id: "e1", name: "Staging", variables: ["a": "1"], secrets: ["token"])])
        try store.saveGlobals(SavedEnvironment(id: "globals", name: "Globals", variables: ["g": "2"]))
        try store.saveState(AppState(activeEnvironmentID: "e1", pinnedRequests: [7]))
        let reread = FileLibraryStore(root: dir)
        XCTAssertEqual(reread.loadCollections(), [c])
        XCTAssertEqual(reread.loadEnvironments().first?.secrets, ["token"])
        XCTAssertEqual(reread.loadGlobals().variables["g"], "2")
        XCTAssertEqual(reread.loadState().activeEnvironmentID, "e1")
        // One file per collection, with its sync bookkeeping.
        let file = dir.appendingPathComponent("collections/\(c.id).json")
        let wrapped = try JSONDecoder().decode(CollectionFile.self, from: Data(contentsOf: file))
        XCTAssertEqual(wrapped.formatVersion, 1)
        XCTAssertGreaterThan(wrapped.updatedAt, 0)
        try store.deleteCollection(id: c.id)
        XCTAssertTrue(reread.loadCollections().isEmpty)
    }

    func testHistoryIsCapped() throws {
        let store = FileLibraryStore(root: dir)
        let entries = (0..<150).map { HistoryEntry(id: UInt64($0), timestamp: UInt64($0), request: SavedRequest(name: "r\($0)")) }
        try store.saveHistory(entries)
        XCTAssertEqual(store.loadHistory().count, FileLibraryStore.historyLimit)
    }

    // MARK: - The old app's files

    func writeLegacy(_ name: String, _ text: String) throws {
        try Data(text.utf8).write(to: dir.appendingPathComponent(name))
    }

    @MainActor
    func testLegacyImport() throws {
        try writeLegacy("collections.json", """
        [{"id": 1712000000000, "name": "Old API", "variables": {"host": "https://old.test"},
          "requests": [{"id": 1712000000001, "name": "Ping", "url": "${host}/ping", "method": "GET",
                        "headers": [{"name": "Accept", "value": "*/*"}], "body": "", "preRequestScript": "", "postRequestScript": ""}]}]
        """)
        try writeLegacy("contexts.json", #"{"1712000000000": {"token": "abc", "host": "ignored"}}"#)
        try writeLegacy("history.json", """
        [{"id": 1, "timestamp": 1712000001000, "url": "https://old.test/a", "method": "GET", "statusCode": 200, "success": true,
          "collectionName": "Old API", "requestName": "Ping", "durationMs": 12, "headers": [], "body": "", "preRequestScript": "", "postRequestScript": ""},
         {"id": 2, "timestamp": 1712000002000, "url": "https://old.test/b", "method": "POST", "statusCode": 0, "success": false,
          "durationMs": 0, "headers": [{"name": "A", "value": "b"}], "body": "x", "preRequestScript": "", "postRequestScript": ""}]
        """)
        let legacy = LegacyImport.read(from: dir)
        XCTAssertEqual(legacy.collections.count, 1)
        XCTAssertEqual(legacy.collections[0].requests[0].headers, [KeyValue("Accept", "*/*")])
        XCTAssertEqual(legacy.collections[0].variables["token"], "abc", "script-set values become collection variables")
        XCTAssertEqual(legacy.collections[0].variables["host"], "https://old.test", "but never replace one")
        XCTAssertEqual(legacy.history.map(\.id), [2, 1], "newest first")
        XCTAssertNil(legacy.history[0].response)
        XCTAssertNotNil(legacy.history[0].error)
        XCTAssertEqual(legacy.history[1].response?.status, 200)
        XCTAssertEqual(legacy.history[0].request.name, "POST /b")

        // Imported once, on first launch; the old files stay.
        let store = MemoryLibraryStore()
        let model = AppModel(store: store, vault: MemoryVault())
        model.load(legacyDirectory: dir)
        XCTAssertTrue(model.collections.contains { $0.name == "Old API" })
        XCTAssertEqual(model.history.count, 2)
        XCTAssertTrue(store.loadState().legacyImported)
        XCTAssertTrue(FileManager.default.fileExists(atPath: dir.appendingPathComponent("collections.json").path))
        let second = AppModel(store: store, vault: MemoryVault())
        try writeLegacy("collections.json", #"[{"id": 5, "name": "New after import", "requests": []}]"#)
        second.load(legacyDirectory: dir)
        XCTAssertFalse(second.collections.contains { $0.name == "New after import" })
    }

    @MainActor
    func testNoLegacyDataGivesMyRequests() {
        let model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault())
        model.load(legacyDirectory: dir.appendingPathComponent("missing"))
        XCTAssertEqual(model.collections.map(\.name), ["My requests"])
    }
}
