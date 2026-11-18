#if os(macOS) || os(Linux)
import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HitopKit
import XCTest
@testable import HITOP

/// A shared collection between the app's Core (Ada) and the hitop CLI (Grace, HITOP_BIN, built in
/// CI) against a real sync server: share, invite with the fingerprint, accept, add, link, nothing
/// to push on either side after joining, edits both ways, a conflict on each side (theirs, mine),
/// removal → rotation → Grace's old key opens nothing written after it, and her CLI can't pull.
extension CloudEndToEndTests {
    struct CLI {
        let bin: String
        let home: URL
        let server: String

        @discardableResult
        func run(_ args: [String]) throws -> (code: Int32, out: String, err: String) {
            let p = Process()
            p.executableURL = URL(fileURLWithPath: bin)
            p.arguments = args
            p.currentDirectoryURL = home
            var e = ProcessInfo.processInfo.environment
            e["HOME"] = home.path
            e["XDG_CONFIG_HOME"] = home.appendingPathComponent("config").path
            e["HITOP_KEYCHAIN"] = "off"
            e["HITOP_SYNC_KEY_FILE"] = home.appendingPathComponent("sync-key.json").path
            e["HITOP_SYNC_SERVER"] = server
            e["NO_COLOR"] = "1"
            e["HITOP_SYNC_RUNNER"] = nil
            p.environment = e
            let out = Pipe(), err = Pipe()
            p.standardOutput = out
            p.standardError = err
            try p.run()
            let o = out.fileHandleForReading.readDataToEndOfFile()
            let r = err.fileHandleForReading.readDataToEndOfFile()
            p.waitUntilExit()
            return (p.terminationStatus, String(decoding: o, as: UTF8.self), String(decoding: r, as: UTF8.self))
        }

        func ok(_ args: [String], file: StaticString = #filePath, line: UInt = #line) throws -> String {
            let r = try run(args)
            XCTAssertEqual(r.code, 0, "hitop \(args.joined(separator: " "))\n\(r.out)\n\(r.err)", file: file, line: line)
            return r.out + r.err
        }

        func json(_ rel: String) throws -> JSONAny {
            try JSONDecoder().decode(JSONAny.self, from: Data(contentsOf: home.appendingPathComponent(rel)))
        }
    }

    /// The request file whose YAML names `name`.
    func requestFile(_ folder: URL, _ name: String) throws -> URL {
        let dir = folder.appendingPathComponent("requests")
        for f in try FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil) where f.pathExtension == "yaml" {
            if (try String(contentsOf: f, encoding: .utf8)).contains("name: \(name)\n") { return f }
        }
        throw XCTSkip("no request file for \(name) in \(dir.path)")
    }

    func edit(_ file: URL, _ from: String, _ to: String) throws {
        let t = try String(contentsOf: file, encoding: .utf8)
        XCTAssertTrue(t.contains(from), t)
        try t.replacingOccurrences(of: from, with: to).write(to: file, atomically: true, encoding: .utf8)
    }

    func testASharedCollectionWithTheCLI() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let bin = env["HITOP_BIN"], !bin.isEmpty else {
            if env["HITOP_E2E_REQUIRED"] != nil { XCTFail("HITOP_BIN (the hitop CLI) must be set") }
            throw XCTSkip("HITOP_BIN not set")
        }
        try await startServer()
        let ada = device("HITOP on iPhone")
        let shared = SharedSync(cloud: ada.cloud, model: ada.model, store: MemorySharedSyncStore())
        shared.pushDelay = .seconds(3600)
        let c = ada.model.addCollection(name: "Shop")
        var list = ada.model.newRequest(in: c.id)
        list.name = "List orders"
        list.url = "https://internal.example.test/orders"
        ada.model.save(list)
        var create = ada.model.newRequest(in: c.id)
        create.name = "Create order"
        create.method = "POST"
        create.url = "https://internal.example.test/orders"
        create.body = "{\"item\": 1}"
        ada.model.save(create)
        try await signIn(ada, "ada@example.com")
        try await shared.share(c.id)
        let ws = try XCTUnwrap(shared.link(c.id)?.workspace)

        // Grace signs in with the CLI.
        let home = dir!.appendingPathComponent("grace")
        try FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let grace = CLI(bin: bin, home: home, server: server)
        let g = "grace@example.com"
        let before = log.value.components(separatedBy: "mail to \(g):").count - 1
        _ = try grace.ok(["sync", "login", "--email", g, "--code"])
        let signedIn = try grace.ok(["sync", "login", "--email", g, "--code", try await mailedCode(g, after: before)])
        let graceFP = try XCTUnwrap(signedIn.split(separator: "\n").compactMap { $0.components(separatedBy: "key fingerprint ").dropFirst().first }.first).trimmingCharacters(in: .whitespaces)

        // Ada invites her; the code names Ada's device; Grace accepts; Ada checks and adds her.
        let invite = try await shared.invite(c.id, email: g, role: "editor")
        XCTAssertTrue(invite.code.hasSuffix(SyncCore.normalize(fingerprint: shared.fingerprint!)))
        let accepted = try grace.ok(["sync", "accept", invite.code])
        XCTAssertTrue(accepted.contains(shared.fingerprint!), accepted)
        let m1 = try await shared.members(c.id)
        let waiting = try XCTUnwrap(m1.invites.first { $0.acceptedBy != nil }?.acceptedBy)
        XCTAssertEqual(waiting.keys.map(\.fingerprint), [graceFP])
        shared.trust(waiting.keys)
        let m2 = try await shared.members(c.id)
        let keys = try XCTUnwrap(m2.invites.first { $0.acceptedBy != nil }?.acceptedBy?.keys)
        try await shared.addMember(c.id, email: g, role: "editor", keys: keys)

        // Grace links a folder: what Ada's phone wrote, readable by the CLI.
        let folder = home.appendingPathComponent("shop")
        let linked = try grace.ok(["sync", "link", ws, "-c", folder.path])
        XCTAssertTrue(linked.contains("trusted device"), linked)
        XCTAssertTrue(try String(contentsOf: folder.appendingPathComponent("hitop.yaml"), encoding: .utf8).contains("Shop"))
        let listFile = try requestFile(folder, "List orders")
        _ = try requestFile(folder, "Create order")

        // Nothing to push on either side: both write the folder the same way.
        let quiet = try await shared.syncNow(c.id)
        XCTAssertEqual(quiet.pushed, [])
        XCTAssertTrue(try grace.ok(["sync", "push", "-c", folder.path]).contains("Up to date"))

        // Grace's edit reaches the phone; the phone's reaches her.
        try edit(listFile, "url: https://internal.example.test/orders", "url: https://internal.example.test/v2/orders")
        XCTAssertTrue(try grace.ok(["sync", "push", "-c", folder.path]).contains("pushed"))
        try await shared.syncNow(c.id)
        XCTAssertEqual(ada.model.collection(c.id)?.requests.first { $0.name == "List orders" }?.url, "https://internal.example.test/v2/orders")
        var cr = try XCTUnwrap(ada.model.collection(c.id)?.requests.first { $0.name == "Create order" })
        cr.body = "{\"item\": 2}"
        ada.model.save(cr)
        let pushed = try await shared.syncNow(c.id)
        XCTAssertEqual(pushed.pushed.count, 1)
        _ = try grace.ok(["sync", "pull", "-c", folder.path])
        XCTAssertTrue(try String(contentsOf: try requestFile(folder, "Create order"), encoding: .utf8).contains("item"), "the body came")

        // Both change List orders, the CLI second: a conflict there; she takes theirs (Ada's).
        var lo = try XCTUnwrap(ada.model.collection(c.id)?.requests.first { $0.name == "List orders" })
        lo.url = "https://ada.example.test/orders"
        ada.model.save(lo)
        try await shared.syncNow(c.id)
        try edit(listFile, "url: https://internal.example.test/v2/orders", "url: https://grace.example.test/orders")
        let conflict = try grace.run(["sync", "push", "-c", folder.path])
        XCTAssertEqual(conflict.code, 1, conflict.out + conflict.err)
        XCTAssertTrue(conflict.out.contains("CONFLICT"), conflict.out)
        _ = try grace.ok(["sync", "resolve", "requests/\(listFile.lastPathComponent)", "--theirs", "-c", folder.path])
        XCTAssertTrue(try String(contentsOf: listFile, encoding: .utf8).contains("https://ada.example.test/orders"))

        // The other way round: the phone second, keeping mine.
        try edit(listFile, "url: https://ada.example.test/orders", "url: https://grace2.example.test/orders")
        _ = try grace.ok(["sync", "push", "-c", folder.path])
        lo = try XCTUnwrap(ada.model.collection(c.id)?.requests.first { $0.name == "List orders" })
        lo.url = "https://ada2.example.test/orders"
        ada.model.save(lo)
        let r = try await shared.syncNow(c.id)
        XCTAssertEqual(r.conflicts.count, 1)
        let k = try XCTUnwrap(shared.conflicts[c.id]?.first)
        XCTAssertTrue(k.theirs?.contains("grace2") ?? false)
        try await shared.resolve(c.id, path: k.path, .mine)
        _ = try grace.ok(["sync", "pull", "-c", folder.path])
        XCTAssertTrue(try String(contentsOf: listFile, encoding: .utf8).contains("https://ada2.example.test/orders"))

        // Grace's key for epoch 1, as her device holds it (her session and device secret from the
        // CLI's key file), before she is removed.
        let secret = try grace.json("sync-key.json")
        // The CLI's config folder: XDG_CONFIG_HOME on Linux, Application Support on macOS.
        let mac = "Library/Application Support/hitop/sync/account.json"
        let account = try grace.json(FileManager.default.fileExists(atPath: home.appendingPathComponent(mac).path) ? mac : "config/hitop/sync/account.json")
        let graceKey = try XCTUnwrap(account["keyId"]?.string)
        let asGrace = CloudRequest(method: "GET", url: URL(string: "\(server)/v1/workspaces/\(ws)/wraps?keyId=\(graceKey)")!,
                                   headers: ["Authorization": "Bearer \(secret["session"]?.string ?? "")", "X-Hitop-Device": graceKey], body: nil)
        let wraps = try await URLSessionCloudTransport().send(asGrace).json ?? .null
        let a = try XCTUnwrap(ada.cloud.account)
        let pin = try SyncCore.publicKey(secret: ada.cloud.credentials.load(server)!.secret)
        let ring = try SyncCore.acceptWraps(secret: try XCTUnwrap(secret["secret"]?.string), keyId: graceKey, workspace: ws, currentEpoch: 1,
                                            wraps: (wraps["wraps"]?.array ?? []).map { (epoch: $0["epoch"]!.int!, wrapped: $0["wrapped"]!.string!) },
                                            pinned: [a.keyId: pin], listed: [:], memory: .object([:]))
        let old = try XCTUnwrap(ring.keys[1])

        // Ada removes her: a new key; every file sealed again; Grace's old key opens none of them.
        let info = try await shared.members(c.id)
        let graceID = try XCTUnwrap(info.members.first { $0.email == g }?.userId)
        let epoch = try await shared.removeMember(c.id, userId: graceID)
        XCTAssertEqual(epoch, 2)
        cr = try XCTUnwrap(ada.model.collection(c.id)?.requests.first { $0.name == "Create order" })
        cr.body = "{\"after removal\": true}"
        ada.model.save(cr)
        try await shared.syncNow(c.id)
        let changes = try await ada.cloud.call("GET", "/v1/workspaces/\(ws)/changes?since=0").json ?? .null
        var opened = 0
        for ch in changes["changes"]?.array ?? [] where !(ch["deleted"]?.bool ?? false) {
            let id = ch["id"]!.string!
            let b = try await ada.cloud.call("GET", "/v1/workspaces/\(ws)/blobs/\(id)").json!
            let envelope = b["content"]!.string!
            XCTAssertEqual(try SyncCore.blobEpoch(envelope), 2)
            XCTAssertThrowsError(try SyncCore.open(key: old, workspace: ws, blob: id, version: b["version"]!.int!, envelope: envelope))
            opened += 1
        }
        XCTAssertGreaterThanOrEqual(opened, 3)
        let refused = try grace.run(["sync", "pull", "-c", folder.path])
        XCTAssertNotEqual(refused.code, 0, refused.out + refused.err)
        XCTAssertFalse(try String(contentsOf: try requestFile(folder, "Create order"), encoding: .utf8).contains("after removal"))
    }
}
#endif
