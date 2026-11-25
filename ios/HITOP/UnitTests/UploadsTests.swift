import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HitopKit
import XCTest
@testable import HITOP

final class UploadsTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("uploads-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDown() { try? FileManager.default.removeItem(at: root) }

    private func picked(_ bytes: [UInt8]) throws -> URL {
        let u = root.appendingPathComponent("source.bin")
        try Data(bytes).write(to: u)
        return u
    }

    private func request(_ token: String?, name: String = "a.png") -> SavedRequest {
        var r = SavedRequest(name: "up", url: "https://up.test/x", method: "POST")
        r.bodyMode = "multipart"
        var extras: [String: JSONAny] = ["fileName": .string(name)]
        if let token { extras["fileToken"] = .string(token) }
        r.formFields = [FormRow("doc", type: "file", extras: extras), FormRow("t", "v")]
        return r
    }

    /// The exact request body a Send makes, through HitopKit's runner.
    private func sent(_ r: SavedRequest, _ uploads: Uploads?) async -> (body: Data?, error: String?) {
        final class Client: HTTPClient, @unchecked Sendable {
            var body: Data?
            func send(_ request: OutgoingRequest, timeout: TimeInterval?) async throws -> HTTPResponse {
                body = request.body
                throw HTTPError(description: "recorded")
            }
        }
        let client = Client()
        var options = RunOptions()
        options.file = Uploads.source(uploads)
        let steps = await Runner(engine: nil, makeClient: { client }).run(HitopKit.Collection(id: 1, name: "c", requests: [Sender.kitRequest(r)]), environment: nil, options: options)
        return (client.body, steps.first?.error)
    }

    func testAPickedFileGoesOutByteForByte() async throws {
        let uploads = Uploads(root: root)
        var r = request(nil)
        // Bytes that aren't UTF-8, and text that looks like a secret reference.
        let bytes = [0, 255, 10, 13] + Array("${secret:vault:a/b#c}".utf8)
        let token = try uploads.add(try picked(bytes), name: "a.png", type: "image/png", request: r.id)
        r.formFields[0].extras["fileToken"] = .string(token)
        // Another start of the app.
        let out = await sent(r, Uploads(root: root))
        XCTAssertEqual(out.error, "recorded")
        let part = Data("name=\"doc\"; filename=\"a.png\"\r\nContent-Type: image/png\r\n\r\n".utf8) + Data(bytes) + Data("\r\n".utf8)
        XCTAssertNotNil(try XCTUnwrap(out.body).range(of: part))
    }

    func testAFileIsSentOnlyWithItsRequest() async throws {
        let uploads = Uploads(root: root)
        let token = try uploads.add(try picked([1]), name: "a.png", type: "image/png", request: 7)
        for u in [uploads, nil] {
            let out = await sent(request(token), u)
            XCTAssertNil(out.body)
            XCTAssertTrue(out.error?.contains("isn't on this device") == true, out.error ?? "")
        }
        // A row without a token stays an empty part, as everywhere in hitop.
        let plain = await sent(request(nil), uploads)
        XCTAssertNotNil(plain.body?.range(of: Data("filename=\"a.png\"\r\nContent-Type: application/octet-stream\r\n\r\n\r\n".utf8)))
    }

    func testLimitTypeAndPrune() throws {
        let uploads = Uploads(root: root)
        XCTAssertThrowsError(try uploads.add(try picked([1, 2, 3]), name: "a", type: "", request: 1, max: 2))
        let a = try uploads.add(try picked([1]), name: "a", type: "x/y\r\nEvil: 1", request: 1)
        let b = try uploads.add(try picked([2]), name: "b", type: "", request: 1)
        XCTAssertEqual(try Uploads.source(uploads)(HitopKit.Request(id: 1, name: "", url: ""), FormField(key: "k", value: "", fileToken: a))?.contentType, "application/octet-stream")
        uploads.prune(keeping: [b])
        XCTAssertFalse(uploads.has(a, request: 1))
        XCTAssertTrue(uploads.has(b, request: 1))
        XCTAssertFalse(uploads.has(b, request: 2))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("uploads").path), [b])
        // A damaged index is set aside, and the files stay.
        try Data("{".utf8).write(to: root.appendingPathComponent("uploads.json"))
        let damaged = Uploads(root: root)
        damaged.prune(keeping: [])
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).contains { $0.hasPrefix("uploads.json.damaged-") })
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("uploads").path), [b])
        XCTAssertTrue(damaged.has(try damaged.add(try picked([1]), name: "a", type: "", request: 1), request: 1))
    }

    func testRowKeysAreKeptAndTheTokenStaysHere() throws {
        let json = #"{"key":"f","value":"","type":"file","enabled":true,"fileName":"a.png","fileToken":"t","note":"n"}"#
        let row = try JSONDecoder().decode(FormRow.self, from: Data(json.utf8))
        let back = try JSONDecoder().decode([String: JSONAny].self, from: JSONEncoder().encode(row))
        XCTAssertEqual(back, try JSONDecoder().decode([String: JSONAny].self, from: Data(json.utf8)))
        var r = request("secret-token")
        r.id = 5
        for f in Importing.Format.allCases where f != .zip {
            let text = String(decoding: try Importing.export(SavedCollection(name: "C", requests: [r]), as: f).data, as: UTF8.self)
            XCTAssertFalse(text.contains("secret-token"), text)
            if f == .hitop { XCTAssertTrue(text.contains("a.png"), text) }
        }
    }
}
