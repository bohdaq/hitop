import Foundation
import XCTest
@testable import HITOP

@MainActor
final class ImportingTests: XCTestCase {
    private func model() -> (AppModel, MemoryVault) {
        let vault = MemoryVault()
        let m = AppModel(store: MemoryLibraryStore(), vault: vault, sender: Sender(configuration: StubURLProtocol.configuration),
                         webSockets: EchoWebSocketTransport(), grpcClient: FakeGrpcClient())
        m.load()
        m.seed()
        return (m, vault)
    }

    private let postman = #"{"info":{"name":"P"},"item":[{"name":"Folder","item":[{"name":"A","request":{"method":"POST","url":"{{base}}/a","auth":{"type":"bearer","bearer":[{"key":"token","value":"{{t}}"}]}},"event":[{"listen":"test","script":{"exec":["pm.sendRequest('x');"]}}]}]}]}"#

    func testPostmanAndItsReport() throws {
        let p = try Importing.read(text: postman)
        XCTAssertEqual(p.title, "Postman collection")
        XCTAssertEqual(p.summary, "1 collection, 1 request")
        let r = try XCTUnwrap(p.collections.first?.requests.first)
        XCTAssertEqual([r.url, r.method, r.folder], ["${base}/a", "POST", "folder"])
        XCTAssertEqual(r.auth, .bearer(token: "${t}"))
        XCTAssertTrue(p.issues.contains { $0.contains("pm.sendRequest") }, "\(p.issues)")
    }

    func testInsomniaHarAndOpenAPI() throws {
        let ins = try Importing.read(text: "type: collection.insomnia.rest/5.0\nname: I\ncollection:\n  - name: A\n    url: \"{{ _.base }}/a\"\n    method: GET\nenvironments:\n  data: {base: http://h}\n  subEnvironments:\n    - name: Dev\n      data: {base: http://dev}\n")
        XCTAssertEqual(ins.title, "Insomnia")
        XCTAssertEqual(ins.collections.first?.requests.first?.url, "${base}/a")
        XCTAssertEqual(ins.environments.map(\.environment.name), ["Dev"])
        let har = try Importing.read(text: #"{"log":{"entries":[{"request":{"method":"GET","url":"https://h.test/x","headers":[{"name":"Authorization","value":"Bearer s3"}]}}]}}"#)
        XCTAssertEqual(har.kind, "har")
        XCTAssertEqual(har.collections.first?.requests.first?.headers.count, 0)
        let api = try Importing.read(text: "openapi: 3.0.0\ninfo: {title: O}\nservers: [{url: /v1}]\npaths:\n  /a: {get: {operationId: a, responses: {}}}\n", source: "https://x.test/spec.yaml")
        XCTAssertEqual(api.collections.first?.variables["baseUrl"], "https://x.test/v1")
        for bad in ["hello", "", "{\"a\":1}"] { XCTAssertThrowsError(try Importing.read(text: bad)) }
        XCTAssertThrowsError(try Importing.read(Data([0x50, 0x4B, 0x03, 0x04, 1])))
    }

    func testEnvironmentSecretsGoToTheVault() throws {
        let (m, vault) = model()
        m.apply(try Importing.read(text: #"{"name":"S","values":[{"key":"a","value":"1"},{"key":"t","value":"s3","type":"secret"}]}"#))
        let e = try XCTUnwrap(m.environments.first { $0.name == "S" })
        XCTAssertEqual(e.variables, ["a": "1"])
        XCTAssertEqual(e.secrets, ["t"])
        XCTAssertEqual(vault.value(environment: e.id, name: "t"), "s3")
    }

    func testImportedAgainGetsNewIds() throws {
        let (m, _) = model()
        let p = try Importing.read(text: postman)
        let first = m.apply(p), second = m.apply(p)
        XCTAssertNotEqual(first, second)
        let ids = m.collections.flatMap { $0.requests.map(\.id) }
        XCTAssertEqual(Set(ids).count, ids.count)
    }

    func testEveryExportReadsBack() throws {
        let (m, _) = model()
        let shop = try XCTUnwrap(m.collections.first { $0.name == "shop-api" })
        for f in Importing.Format.allCases {
            let (name, data) = try Importing.export(shop, as: f)
            let back = try Importing.read(data, name: name)
            let c = try XCTUnwrap(back.collections.first, f.rawValue)
            XCTAssertEqual(c.name, "shop-api", f.rawValue)
            XCTAssertEqual(c.requests.map(\.name).sorted(), shop.requests.map(\.name).sorted(), f.rawValue)
            XCTAssertEqual(Importing.isZip(data), f == .zip)
        }
        XCTAssertTrue(try Importing.export(shop, as: .zip).name == "shop-api.zip")
    }
}
