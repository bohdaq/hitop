#if canImport(CryptoKit)
import CryptoKit
#endif
import Foundation
import XCTest
@testable import HITOP

/// The published hitop sync test vectors (cli/crates/hitop-sync-crypto/vectors/v1.json, v2.json),
/// the key acceptance policy (`accept_wraps`, cases of hitop-sync-crypto's tests_v2.rs) and the
/// sync plan, all through `SyncCore`: the ops the app itself calls, so the glue is checked too.
/// Runs in the app's unit tests (simulator) and in core-tests.sh (Linux, macOS).
final class SyncCoreTests: XCTestCase {
    static let ws = "3f6c1a9e0b7d4c2a8e5f1b3d7a9c0e24"

    // MARK: Helpers

    static func vectors(_ name: String) throws -> JSONAny {
        var places: [URL] = []
        if let d = ProcessInfo.processInfo.environment["HITOP_SYNC_VECTORS"], !d.isEmpty { places.append(URL(fileURLWithPath: d)) }
        for b in Bundle.allBundles + [Bundle(for: SyncCoreTests.self)] {
            if let u = b.url(forResource: "vectors", withExtension: nil) { places.append(u) }
        }
        let here = URL(fileURLWithPath: #filePath).resolvingSymlinksInPath()
        places.append(here.deletingLastPathComponent().appendingPathComponent("../../../cli/crates/hitop-sync-crypto/vectors").standardizedFileURL)
        for p in places {
            if let data = try? Data(contentsOf: p.appendingPathComponent(name)) { return try JSONDecoder().decode(JSONAny.self, from: data) }
        }
        struct Missing: Error {}
        XCTFail("no \(name): set HITOP_SYNC_VECTORS (core-tests.sh does) or bundle cli/crates/hitop-sync-crypto/vectors")
        throw Missing()
    }

    static func b64(_ hex: String) -> String {
        var bytes: [UInt8] = []
        var i = hex.startIndex
        while i < hex.endIndex {
            let j = hex.index(i, offsetBy: 2)
            bytes.append(UInt8(hex[i..<j], radix: 16)!)
            i = j
        }
        return CloudCrypto.base64url(bytes)
    }

    static func hex(_ b64: String) -> String { (CloudCrypto.unbase64url(b64) ?? []).map { String(format: "%02x", $0) }.joined() }

    struct Dev {
        let id: String
        let key: DeviceKey
        init(_ id: String, key: DeviceKey = SyncCore.keygen()) {
            self.id = id
            self.key = key
        }

        func wrap(for to: Dev, epoch: Int, head: Int, key k: String) throws -> String {
            try SyncCore.sealWrap(secret: key.secret, sender: id, recipient: to.id, recipientPublicKey: to.key.publicKey, workspace: SyncCoreTests.ws, epoch: epoch, head: head, key: k)
        }
    }

    func pins(_ devs: [Dev]) -> [String: String] { Dictionary(uniqueKeysWithValues: devs.map { ($0.id, $0.key.publicKey) }) }

    func accept(_ me: Dev, _ current: Int, _ wraps: [(Int, String)], pinned: [String: String] = [:], listed: [String: String] = [:], memory: inout JSONAny) throws -> SyncCore.Keyring {
        let ring = try SyncCore.acceptWraps(secret: me.key.secret, keyId: me.id, workspace: Self.ws, currentEpoch: current,
                                            wraps: wraps.map { (epoch: $0.0, wrapped: $0.1) }, pinned: pinned, listed: listed, memory: memory)
        memory = ring.memory
        return ring
    }

    func refusal(_ f: () throws -> Any) -> SyncCore.KeyRefusal? {
        do {
            _ = try f()
            return nil
        } catch let e as SyncCore.KeyRefusal {
            return e
        } catch {
            XCTFail("\(error)")
            return nil
        }
    }

    // MARK: Published vectors

    func testVectorsV1Blobs() throws {
        let v = try Self.vectors("v1.json")
        var n = 0
        for b in v["blobs"]?.array ?? [] {
            let s = { (k: String) in b[k]?.string ?? "" }
            let args: [String: JSONAny] = ["key": .string(Self.b64(s("key"))), "workspace": .string(s("workspaceId")), "blob": .string(s("blobId")),
                                           "version": b["version"]!, "epoch": b["epoch"]!]
            var enc = args
            enc["plaintext"] = .string(Self.b64(s("plaintext")))
            enc["nonce"] = .string(Self.b64(s("nonce")))
            XCTAssertEqual(Self.hex(try SyncCore.string("encryptBlob", enc)), s("envelope"), s("name"))
            XCTAssertEqual(Self.hex(try SyncCore.string("blobAad", args)), s("aad"), s("name"))
            var dec = args
            dec["envelope"] = .string(Self.b64(s("envelope")))
            XCTAssertEqual(Self.hex(try SyncCore.string("decryptBlob", dec)), s("plaintext"), s("name"))
            XCTAssertEqual(try SyncCore.blobEpoch(Self.b64(s("envelope"))), b["epoch"]?.int)
            n += 1
        }
        for b in v["blobFailures"]?.array ?? [] {
            let s = { (k: String) in b[k]?.string ?? "" }
            XCTAssertThrowsError(try SyncCore.call("decryptBlob", ["key": .string(Self.b64(s("key"))), "workspace": .string(s("workspaceId")), "blob": .string(s("blobId")),
                                                                   "version": b["version"]!, "epoch": b["epoch"]!, "envelope": .string(Self.b64(s("envelope")))]), s("name"))
            n += 1
        }
        XCTAssertGreaterThanOrEqual(n, 7)
    }

    func testVectorsV1Wraps() throws {
        let v = try Self.vectors("v1.json")
        for w in v["wraps"]?.array ?? [] {
            let s = { (k: String) in w[k]?.string ?? "" }
            let secret = Self.b64(s("recipientSecret"))
            XCTAssertEqual(try SyncCore.publicKey(secret: secret), Self.b64(s("recipientPublic")), s("name"))
            XCTAssertEqual(Self.hex(try SyncCore.string("wrapInfoLegacy", ["workspace": .string(s("workspaceId")), "epoch": w["epoch"]!])), s("info"))
            let key = try SyncCore.string("unwrapLegacy", ["secret": .string(secret), "workspace": .string(s("workspaceId")), "epoch": w["epoch"]!, "wrapped": .string(Self.b64(s("wrapped")))])
            XCTAssertEqual(Self.hex(key), s("workspaceKey"), s("name"))
            XCTAssertEqual(try SyncCore.wrapHeader(Self.b64(s("wrapped"))).legacy, true)
        }
        for w in v["wrapFailures"]?.array ?? [] {
            let s = { (k: String) in w[k]?.string ?? "" }
            XCTAssertThrowsError(try SyncCore.call("unwrapLegacy", ["secret": .string(Self.b64(s("recipientSecret"))), "workspace": .string(s("workspaceId")),
                                                                    "epoch": w["epoch"]!, "wrapped": .string(Self.b64(s("wrapped")))]), s("name"))
        }
        XCTAssertEqual(v["wraps"]?.array?.count, 2)
    }

    func testVectorsV2() throws {
        let v = try Self.vectors("v2.json")
        for w in v["wraps"]?.array ?? [] {
            let s = { (k: String) in w[k]?.string ?? "" }
            let secret = Self.b64(s("recipientSecret"))
            XCTAssertEqual(try SyncCore.publicKey(secret: secret), Self.b64(s("recipientPublic")))
            XCTAssertEqual(Self.hex(try SyncCore.string("wrapInfo", ["workspace": .string(s("workspaceId")), "epoch": w["epoch"]!, "head": w["head"]!,
                                                                     "senderKeyId": .string(s("senderKeyId")), "recipientKeyId": .string(s("recipientKeyId"))])), s("info"), s("name"))
            let o = try SyncCore.call("openWrap", ["secret": .string(secret), "recipientKeyId": .string(s("recipientKeyId")), "senderPublicKey": .string(Self.b64(s("senderPublic"))),
                                                   "workspace": .string(s("workspaceId")), "epoch": w["epoch"]!, "wrapped": .string(Self.b64(s("wrapped")))])
            XCTAssertEqual(Self.hex(o["key"]?.string ?? ""), s("workspaceKey"), s("name"))
            XCTAssertEqual(o["sender"]?.string, s("senderKeyId"))
            XCTAssertEqual(o["head"]?.int, w["head"]?.int)
            XCTAssertEqual(try SyncCore.wrapHeader(Self.b64(s("wrapped"))), SyncCore.WrapHeader(legacy: false, sender: s("senderKeyId"), head: w["head"]?.int))
            // accept_wraps takes it from the pinned sender, and only from it.
            var memory = JSONAny.object(["strict": .bool(true)])
            let me = Dev(s("recipientKeyId"), key: DeviceKey(secret: secret, publicKey: Self.b64(s("recipientPublic"))))
            let epoch = w["epoch"]!.int!
            if epoch == w["head"]!.int! {
                let ring = try accept(me, epoch, [(epoch, Self.b64(s("wrapped")))], pinned: [s("senderKeyId"): Self.b64(s("senderPublic"))], memory: &memory)
                XCTAssertEqual(Self.hex(ring.keys[epoch]!), s("workspaceKey"))
            }
        }
        for w in v["wrapFailures"]?.array ?? [] {
            let s = { (k: String) in w[k]?.string ?? "" }
            XCTAssertThrowsError(try SyncCore.call("openWrap", ["secret": .string(Self.b64(s("recipientSecret"))), "recipientKeyId": .string(s("recipientKeyId")),
                                                                "senderPublicKey": .string(Self.b64(s("senderPublic"))), "workspace": .string(s("workspaceId")),
                                                                "epoch": w["epoch"]!, "wrapped": .string(Self.b64(s("wrapped")))]), s("name"))
        }
        XCTAssertEqual(v["wrapFailures"]?.array?.count, 10)
        for c in v["commitments"]?.array ?? [] {
            XCTAssertEqual(try SyncCore.string("keyCommitment", ["workspace": c["workspaceId"]!, "epoch": c["epoch"]!, "key": .string(Self.b64(c["workspaceKey"]!.string!))]),
                           c["commitment"]?.string)
        }
    }

    // MARK: accept_wraps (tests_v2.rs)

    func testAPinnedSendersWrapIsAcceptedAndRemembered() throws {
        let (owner, bob) = (Dev("o"), Dev("b"))
        let (k1, k2) = (try SyncCore.newKey(), try SyncCore.newKey())
        var mem = JSONAny.object([:])
        let wraps = [(1, try owner.wrap(for: bob, epoch: 1, head: 2, key: k1)), (2, try owner.wrap(for: bob, epoch: 2, head: 2, key: k2))]
        let ring = try accept(bob, 2, wraps, pinned: pins([owner]), memory: &mem)
        XCTAssertEqual(ring.keys, [1: k1, 2: k2])
        XCTAssertEqual(ring.current, 2)
        XCTAssertTrue(ring.notices.isEmpty)
        XCTAssertFalse(ring.legacy)
        XCTAssertEqual(mem["strict"]?.bool, true)
        XCTAssertEqual(mem["maxEpoch"]?.int, 2)
        XCTAssertEqual(mem["commitments"]?["1"]?.string, try SyncCore.string("keyCommitment", ["workspace": .string(Self.ws), "epoch": .number(1), "key": .string(k1)]))
        let before = mem
        _ = try accept(bob, 2, wraps, pinned: pins([owner]), memory: &mem)
        XCTAssertEqual(mem, before)
    }

    func testAForgedWrapNamingAPinnedSenderIsRefused() throws {
        let (owner, bob, server) = (Dev("o"), Dev("b"), Dev("o"))
        let forged = try server.wrap(for: bob, epoch: 1, head: 1, key: try SyncCore.newKey())
        var mem = JSONAny.object([:])
        let e = refusal { try self.accept(bob, 1, [(1, forged)], pinned: self.pins([owner]), memory: &mem) }
        XCTAssertEqual(e?.kind, "forged")
        XCTAssertEqual(e?.senderKeyId, "o")
        XCTAssertTrue(e?.security ?? false)
        XCTAssertEqual(mem, .object([:]), "nothing remembered from a refusal")
    }

    func testAnUntrustedSenderIsRefusedWithItsFingerprint() throws {
        let (owner, bob, server) = (Dev("o"), Dev("b"), Dev("s"))
        let w = try server.wrap(for: bob, epoch: 1, head: 1, key: try SyncCore.newKey())
        var mem = JSONAny.object([:])
        let e = refusal { try self.accept(bob, 1, [(1, w)], pinned: self.pins([owner]), listed: self.pins([owner, server]), memory: &mem) }
        XCTAssertEqual(e?.kind, "untrustedSender")
        XCTAssertEqual(e?.senderKeyId, "s")
        XCTAssertEqual(e?.fingerprint, try SyncCore.fingerprint(server.key.publicKey))
        let unlisted = refusal { try self.accept(bob, 1, [(1, w)], pinned: self.pins([owner]), memory: &mem) }
        XCTAssertEqual(unlisted?.kind, "untrustedSender")
        XCTAssertNil(unlisted?.fingerprint)
    }

    func testAnInviteFingerprintPinsTheInviterAndNobodyElse() throws {
        let (owner, bob, server) = (Dev("o"), Dev("b"), Dev("s"))
        let k = try SyncCore.newKey()
        let fp = try SyncCore.fingerprint(owner.key.publicKey)
        var mem = JSONAny.object(["strict": .bool(true), "expectedSenders": .array([.string(fp.uppercased())])])
        let ring = try accept(bob, 1, [(1, try owner.wrap(for: bob, epoch: 1, head: 1, key: k))], listed: pins([owner, server]), memory: &mem)
        XCTAssertEqual(ring.newPins, ["o": owner.key.publicKey])
        XCTAssertTrue(ring.notices.contains { $0.contains(fp) })
        // The server swaps its own key in under the owner's key id: not the invite's fingerprint.
        let fake = Dev("o", key: server.key)
        var fresh = JSONAny.object(["strict": .bool(true), "expectedSenders": .array([.string(fp)])])
        let e = refusal { try self.accept(bob, 1, [(1, try fake.wrap(for: bob, epoch: 1, head: 1, key: k))], listed: ["o": server.key.publicKey], memory: &fresh) }
        XCTAssertEqual(e?.kind, "untrustedSender")
    }

    func testASwappedKeyForAKnownEpochIsRefused() throws {
        let (owner, editor, bob) = (Dev("o"), Dev("e"), Dev("b"))
        let (k1, other) = (try SyncCore.newKey(), try SyncCore.newKey())
        var mem = JSONAny.object([:])
        _ = try accept(bob, 1, [(1, try owner.wrap(for: bob, epoch: 1, head: 1, key: k1))], pinned: pins([owner, editor]), memory: &mem)
        let e = refusal { try self.accept(bob, 1, [(1, try editor.wrap(for: bob, epoch: 1, head: 1, key: other))], pinned: self.pins([owner, editor]), memory: &mem) }
        XCTAssertEqual(e?.kind, "keyChanged")
    }

    func testAHiddenRotationOrAReplayIsRefused() throws {
        let (owner, bob) = (Dev("o"), Dev("b"))
        let (k1, k2) = (try SyncCore.newKey(), try SyncCore.newKey())
        let p = pins([owner])
        let w1 = try owner.wrap(for: bob, epoch: 1, head: 1, key: k1)
        var mem = JSONAny.object([:])
        _ = try accept(bob, 1, [(1, w1)], pinned: p, memory: &mem)
        _ = try accept(bob, 2, [(1, w1), (2, try owner.wrap(for: bob, epoch: 2, head: 2, key: k2))], pinned: p, memory: &mem)
        XCTAssertEqual(refusal { try self.accept(bob, 1, [(1, w1)], pinned: p, memory: &mem) }?.kind, "rollback")
        XCTAssertEqual(refusal { try self.accept(bob, 2, [(2, w1)], pinned: p, memory: &mem) }?.kind, "forged")
        let carol = Dev("c")
        var fresh = JSONAny.object([:])
        XCTAssertEqual(refusal { try self.accept(carol, 1, [(1, try owner.wrap(for: carol, epoch: 1, head: 2, key: k1))], pinned: p, memory: &fresh) }?.kind, "rollback")
        var m = JSONAny.object([:])
        XCTAssertEqual(refusal { try self.accept(carol, 3, [(1, try owner.wrap(for: carol, epoch: 1, head: 1, key: k1))], pinned: p, memory: &m) }?.kind, "missingKey")
    }

    func testLegacyWrapsOnlyWithANoticeAndNeverOnceStrict() throws {
        let (owner, bob) = (Dev("o"), Dev("b"))
        let k1 = try SyncCore.newKey()
        let legacy = try SyncCore.string("wrapLegacy", ["publicKey": .string(bob.key.publicKey), "workspace": .string(Self.ws), "epoch": .number(1), "key": .string(k1)])
        var mem = JSONAny.object([:])
        let ring = try accept(bob, 1, [(1, legacy)], memory: &mem)
        XCTAssertTrue(ring.legacy)
        XCTAssertTrue(ring.notices.first?.contains("without authentication") ?? false)
        var strict = JSONAny.object(["strict": .bool(true)])
        XCTAssertEqual(refusal { try self.accept(bob, 1, [(1, legacy)], memory: &strict) }?.kind, "legacyRefused")
        let k2 = try SyncCore.newKey()
        let r2 = try accept(bob, 2, [(1, legacy), (2, try owner.wrap(for: bob, epoch: 2, head: 2, key: k2))], pinned: pins([owner]), memory: &mem)
        XCTAssertEqual(mem["strict"]?.bool, true)
        XCTAssertNil(r2.keys[1])
    }

    func testAChangedKeyForAKnownDeviceIsAnnounced() throws {
        let (owner, bob, server) = (Dev("o"), Dev("b"), Dev("x"))
        var mem = JSONAny.object([:])
        let ring = try accept(bob, 1, [(1, try owner.wrap(for: bob, epoch: 1, head: 1, key: try SyncCore.newKey()))], pinned: pins([owner]), listed: ["o": server.key.publicKey], memory: &mem)
        XCTAssertTrue(ring.notices.first?.hasPrefix("WARNING") ?? false, "\(ring.notices)")
    }

    // MARK: Device keys

    func testFingerprintsAndKeys() throws {
        let k = SyncCore.keygen()
        XCTAssertEqual(try SyncCore.publicKey(secret: k.secret), k.publicKey)
        let fp = try SyncCore.fingerprint(k.publicKey)
        XCTAssertEqual(fp.split(separator: " ").count, 8)
        XCTAssertEqual(SyncCore.normalize(fingerprint: fp.uppercased()), fp.replacingOccurrences(of: " ", with: ""))
        XCTAssertEqual(SyncCore.grouped(fp.replacingOccurrences(of: " ", with: "")), fp)
        XCTAssertThrowsError(try SyncCore.call("nope"))
    }

    #if canImport(CryptoKit)
    /// I2's device keys (CryptoKit raw X25519, base64url) are hitop-sync-crypto's: the same public
    /// key from the secret, and a wrap sealed to it opens with it.
    func testCryptoKitDeviceKeysAreSyncCryptoKeys() throws {
        let k = Curve25519.KeyAgreement.PrivateKey()
        let me = Dev("ios", key: DeviceKey(secret: CloudCrypto.base64url([UInt8](k.rawRepresentation)), publicKey: CloudCrypto.base64url([UInt8](k.publicKey.rawRepresentation))))
        XCTAssertEqual(try SyncCore.publicKey(secret: me.key.secret), me.key.publicKey)
        let owner = Dev("o")
        let key = try SyncCore.newKey()
        var mem = JSONAny.object([:])
        let ring = try accept(me, 1, [(1, try owner.wrap(for: me, epoch: 1, head: 1, key: key))], pinned: pins([owner]), memory: &mem)
        XCTAssertEqual(ring.keys[1], key)
        // And the other way: a wrap this device makes opens for another.
        var m2 = JSONAny.object([:])
        XCTAssertEqual(try accept(owner, 1, [(1, try me.wrap(for: owner, epoch: 1, head: 1, key: key))], pinned: pins([me]), memory: &m2).keys[1], key)
    }
    #endif

    // MARK: Blobs

    func testBlobsAreBoundToTheirContext() throws {
        let k = try SyncCore.newKey()
        let env = try SyncCore.seal(key: k, workspace: Self.ws, blob: "b1", version: 2, epoch: 1, path: "requests/a.yaml", text: "name: A\n")
        let o = try SyncCore.open(key: k, workspace: Self.ws, blob: "b1", version: 2, envelope: env)
        XCTAssertEqual(o.path, "requests/a.yaml")
        XCTAssertEqual(o.text, "name: A\n")
        XCTAssertThrowsError(try SyncCore.open(key: k, workspace: Self.ws, blob: "b1", version: 3, envelope: env), "an old version served as newer")
        XCTAssertThrowsError(try SyncCore.open(key: k, workspace: Self.ws, blob: "b2", version: 2, envelope: env), "moved to another blob")
        XCTAssertThrowsError(try SyncCore.open(key: try SyncCore.newKey(), workspace: Self.ws, blob: "b1", version: 2, envelope: env))
    }

    // MARK: The plan (hitop-core sync.rs)

    static let manifest = "format: 1\nname: Shop\norder:\n- a\nvariables:\n  host: api\n"

    func remote(_ blob: String, _ version: Int, _ vv: [String: Int], _ path: String?, _ text: String?) -> JSONAny {
        .object(["blob": .string(blob), "version": .number(Double(version)), "vv": .object(vv.mapValues { .number(Double($0)) }),
                 "deleted": .bool(text == nil), "path": path.map(JSONAny.string) ?? .null, "text": text.map(JSONAny.string) ?? .null])
    }

    func plan(_ local: [String: String], _ state: JSONAny, _ remote: [JSONAny]) throws -> JSONAny {
        try SyncCore.call("plan", ["local": .object(local.mapValues(JSONAny.string)), "state": state, "remote": .array(remote), "me": .string("me")])
    }

    func testThePlanPullsMergesAndKeepsConflicts() throws {
        let empty = JSONAny.object(["seq": .number(0), "entries": .object([:]), "conflicts": .object([:]), "losers": .object([:])])
        let first = try plan([:], empty, [remote("b1", 1, ["a": 1], "hitop.yaml", Self.manifest), remote("b2", 1, ["a": 1], "requests/a.yaml", "name: A\nmethod: GET\nurl: /a\n")])
        XCTAssertEqual(first["write"]?.object?.count, 2)
        XCTAssertEqual(first["push"]?.array?.count, 0)
        let state = first["state"]!
        // Both changed the request: a conflict, never pushed.
        var local = first["write"]!.object!.compactMapValues(\.string)
        local["requests/a.yaml"] = "name: A\nmethod: GET\nurl: /mine\n"
        let both = try plan(local, state, [remote("b2", 2, ["a": 2], "requests/a.yaml", "name: A\nmethod: GET\nurl: /theirs\n")])
        XCTAssertEqual(both["conflicts"]?.array, [.string("requests/a.yaml")])
        XCTAssertEqual(both["push"]?.array?.count, 0)
        XCTAssertEqual(both["state"]?["conflicts"]?["requests/a.yaml"]?["theirs"]?.string, "name: A\nmethod: GET\nurl: /theirs\n")
        // Resolving: theirs is written, mine would be pushed.
        let theirs = try SyncCore.call("resolve", ["state": both["state"]!, "path": .string("requests/a.yaml"), "keepMine": .bool(false)])
        XCTAssertEqual(theirs["resolution"]?["action"]?.string, "write")
        let mine = try SyncCore.call("resolve", ["state": both["state"]!, "path": .string("requests/a.yaml"), "keepMine": .bool(true)])
        XCTAssertEqual(mine["resolution"]?["action"]?.string, "keepMine")
        let push = try plan(local, mine["state"]!, [])
        XCTAssertEqual(push["push"]?.array?.first?["path"]?.string, "requests/a.yaml")
        XCTAssertEqual(push["push"]?.array?.first?["version"]?.int, 3)
        // hitop.yaml changed on both sides in different keys: merged key by key.
        var l2 = first["write"]!.object!.compactMapValues(\.string)
        l2["hitop.yaml"] = Self.manifest + "  port: '80'\n"
        let merged = try plan(l2, state, [remote("b1", 2, ["a": 2], "hitop.yaml", Self.manifest.replacingOccurrences(of: "name: Shop", with: "name: Shop 2"))])
        XCTAssertEqual(merged["merged"]?.array, [.string("hitop.yaml")])
        let text = merged["write"]?["hitop.yaml"]?.string ?? ""
        XCTAssertTrue(text.contains("Shop 2") && text.contains("port"), text)
        // A path outside a collection folder is never written.
        let evil = try plan([:], empty, [remote("b9", 1, ["a": 1], "../../etc/passwd.yaml", "x"), remote("b8", 1, ["a": 1], ".hidden/x.yaml", "x")])
        XCTAssertEqual(evil["write"]?.object?.count, 0)
        XCTAssertEqual(evil["skipped"]?.array?.count, 2)
        XCTAssertFalse(SyncCore.isSyncedPath("../x.yaml"))
        XCTAssertTrue(SyncCore.isSyncedPath("requests/orders/create.yaml"))
    }

    // MARK: Folders

    func testACollectionRoundTripsThroughItsFilesUnchanged() throws {
        var c = SavedCollection(id: 4_242_424_242_424, name: "Shop API", variables: ["baseUrl": "https://api.example.com"])
        c.requests = [SavedRequest(id: 11, name: "List orders", url: "${baseUrl}/orders", method: "GET", headers: [KeyValue("Accept", "application/json")]),
                      SavedRequest(id: 12, name: "Create order", url: "${baseUrl}/orders", method: "POST", body: "{\"a\": 1}", bodyType: "json", file: "orders/create")]
        let files = try SyncCore.toFiles(c)
        XCTAssertNotNil(files["hitop.yaml"])
        XCTAssertNotNil(files["requests/orders/create.yaml"])
        let back = try SyncCore.readFiles(files, name: "x")
        XCTAssertEqual(back.id, c.id)
        XCTAssertEqual(back.requests.map(\.name), ["List orders", "Create order"])
        // What came from the files goes back as the same files: nothing to push.
        XCTAssertEqual(try SyncCore.toFiles(back), files)
        // A .yml request file keeps its extension.
        let p = try SyncCore.planFolder(existing: ["hitop.yaml", "requests/list-orders.yml"], files: files)
        XCTAssertNil(p.write["requests/list-orders.yaml"])
        XCTAssertEqual(p.remove, [])
    }
}
