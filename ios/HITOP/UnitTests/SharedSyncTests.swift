import Foundation
import HitopKit
import XCTest
@testable import HITOP

/// Shared collections (SharedSync) between two people's devices against `FakeCloud`'s workspaces:
/// share, invite with the fingerprint, accept, add, join, both edit, conflicts (mine, theirs,
/// both), viewer read-only, removal with rotation, a forged key, team environments, and that a
/// shared collection never reaches the server-readable personal library.
@MainActor
final class SharedSyncTests: XCTestCase {
    /// One person's phone: its own files, Keychain and HITOP Cloud client.
    @MainActor
    final class Phone {
        let email: String
        let model: AppModel
        let cloud: HitopCloud
        let library: LibrarySync
        let shared: SharedSync

        init(_ email: String, fake: FakeCloud) {
            self.email = email
            model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault())
            model.load()
            cloud = HitopCloud(transport: fake, credentials: CloudCredentials(store: MemorySecretStore()), settingsStore: MemoryCloudSettingsStore(CloudSettings(server: FakeCloud.server)))
            library = LibrarySync(cloud: cloud, model: model, stateStore: MemoryLibrarySyncStateStore())
            shared = SharedSync(cloud: cloud, model: model, store: MemorySharedSyncStore())
            cloud.onAccountChange = nil
            library.pushDelay = .seconds(3600)
            shared.pushDelay = .seconds(3600)
        }

        func signIn() async throws {
            try await cloud.startEmailSignIn(server: FakeCloud.server, email: email)
            try await cloud.finishEmailSignIn("mail-\(email)")
            await library.sync()
            await shared.syncAll()
        }
    }

    var fake: FakeCloud!
    var ada: Phone!
    var grace: Phone!

    override func setUp() async throws {
        fake = FakeCloud()
        ada = Phone("ada@example.com", fake: fake)
        grace = Phone("grace@example.com", fake: fake)
    }

    func shop(on p: Phone) -> SavedCollection {
        let c = p.model.addCollection(name: "Shop API")
        var r = p.model.newRequest(in: c.id)
        r.name = "List orders"
        r.url = "https://api.example.com/orders"
        p.model.save(r)
        var r2 = p.model.newRequest(in: c.id)
        r2.name = "Create order"
        r2.method = "POST"
        r2.url = "https://api.example.com/orders"
        r2.body = "{\"item\": 1}"
        p.model.save(r2)
        return p.model.collection(c.id)!
    }

    /// Ada shares Shop API and adds Grace with `role` through an invite code: Grace's collection id.
    func shareWithGrace(_ role: String = "editor", fingerprint: Bool = true) async throws -> (ada: UInt64, grace: UInt64, ws: String) {
        let c = shop(on: ada)
        try await ada.signIn()
        try await grace.signIn()
        try await ada.shared.share(c.id)
        let ws = try XCTUnwrap(ada.shared.link(c.id)?.workspace)
        let invite = try await ada.shared.invite(c.id, email: grace.email, role: role)
        XCTAssertTrue(invite.mailed)
        XCTAssertTrue(invite.code.hasSuffix(SyncCore.normalize(fingerprint: ada.shared.fingerprint!)))
        let code = try InviteCode.parse(fingerprint ? invite.code : String(invite.code.split(separator: ".")[0]))
        let accepted = try await grace.shared.accept(code)
        XCTAssertEqual(accepted.workspace, ws)
        // Ada checks Grace's device fingerprint (out of band), trusts it, adds her.
        let info = try await ada.shared.members(c.id)
        let waiting = try XCTUnwrap(info.invites.first { $0.acceptedBy != nil }?.acceptedBy)
        XCTAssertEqual(waiting.keys.map(\.fingerprint), [grace.shared.fingerprint!])
        XCTAssertEqual(waiting.keys.first?.trust, .new)
        do {
            try await ada.shared.addMember(c.id, email: grace.email, role: role, keys: waiting.keys)
            XCTFail("an untrusted key must not get the workspace key")
        } catch {}
        ada.shared.trust(waiting.keys)
        let again = try await ada.shared.members(c.id)
        try await ada.shared.addMember(c.id, email: grace.email, role: role, keys: again.invites.first!.acceptedBy!.keys)
        try await grace.shared.refreshWorkspaces()
        XCTAssertEqual(grace.shared.joinable.map(\.id), [ws])
        if !fingerprint {
            return (c.id, 0, ws)
        }
        let gid = try await grace.shared.join(ws)
        return (c.id, gid, ws)
    }

    func request(_ p: Phone, _ cid: UInt64, _ name: String) -> SavedRequest? { p.model.collection(cid)?.requests.first { $0.name == name } }

    // MARK: Share, join, edit, conflicts, removal

    func testShareInviteJoinBothEditConflictAndRemove() async throws {
        let (aid, gid, ws) = try await shareWithGrace()
        XCTAssertEqual(ada.shared.badge(aid), "Shared")
        XCTAssertEqual(grace.model.collection(gid)?.name, "Shop API")
        XCTAssertEqual(grace.model.collection(gid)?.requests.map(\.name), ["List orders", "Create order"])
        XCTAssertEqual(grace.model.sharedRoles[gid], "editor")
        // The invite's device was trusted: said with its name, the fingerprint as secondary text,
        // never an internal key id.
        let trusted = try XCTUnwrap(grace.shared.notices.first { $0.text.hasPrefix("Trusted ") }, "\(grace.shared.notices)")
        XCTAssertEqual(trusted.text, "Trusted HITOP on iPhone — the device your invite code named")
        XCTAssertEqual(trusted.detail, "Fingerprint \(ada.shared.fingerprint!)")
        let adaKey = try XCTUnwrap(ada.cloud.settings.account?.keyId)
        for n in grace.shared.notices { XCTAssertFalse(n.text.contains(adaKey) || (n.detail ?? "").contains(adaKey), n.text) }
        // The server never saw the collection's text.
        for e in fake.envelopes(ws) { XCTAssertFalse(String(decoding: CloudCrypto.unbase64url(e) ?? [], as: UTF8.self).contains("orders")) }
        // Joining with no edits pushes nothing.
        let first = try await grace.shared.syncNow(gid)
        XCTAssertEqual(first.pushed, [])
        let quiet = try await ada.shared.syncNow(aid)
        XCTAssertEqual(quiet.pushed, [])

        // Grace edits one request, Ada another: both arrive.
        var g = try XCTUnwrap(request(grace, gid, "List orders"))
        g.url = "https://api.example.com/orders?page=2"
        grace.model.save(g)
        try await grace.shared.syncNow(gid)
        var a = try XCTUnwrap(request(ada, aid, "Create order"))
        a.body = "{\"item\": 2}"
        ada.model.save(a)
        let r = try await ada.shared.syncNow(aid)
        XCTAssertEqual(r.pushed.count, 1)
        XCTAssertEqual(request(ada, aid, "List orders")?.url, "https://api.example.com/orders?page=2")
        try await grace.shared.syncNow(gid)
        XCTAssertEqual(request(grace, gid, "Create order")?.body, "{\"item\": 2}")

        // Both change the same request: a conflict on the second, never pushed until resolved.
        a = try XCTUnwrap(request(ada, aid, "List orders"))
        a.url = "https://api.example.com/orders?ada"
        ada.model.save(a)
        try await ada.shared.syncNow(aid)
        g = try XCTUnwrap(request(grace, gid, "List orders"))
        g.url = "https://api.example.com/orders?grace"
        grace.model.save(g)
        let c = try await grace.shared.syncNow(gid)
        XCTAssertEqual(c.conflicts.count, 1)
        let conflict = try XCTUnwrap(grace.shared.conflicts[gid]?.first)
        XCTAssertTrue(conflict.mine?.contains("grace") ?? false)
        XCTAssertTrue(conflict.theirs?.contains("ada") ?? false)
        XCTAssertTrue(conflict.canKeepBoth)
        // Keep both: theirs stays, mine comes back as "List orders (mine)", both go to Ada.
        try await grace.shared.resolve(gid, path: conflict.path, .both)
        XCTAssertEqual(grace.shared.conflicts[gid], [])
        XCTAssertEqual(request(grace, gid, "List orders")?.url, "https://api.example.com/orders?ada")
        XCTAssertEqual(request(grace, gid, "List orders (mine)")?.url, "https://api.example.com/orders?grace")
        try await ada.shared.syncNow(aid)
        XCTAssertEqual(request(ada, aid, "List orders (mine)")?.url, "https://api.example.com/orders?grace")

        // Again, keeping mine this time.
        a = try XCTUnwrap(request(ada, aid, "Create order"))
        a.method = "PUT"
        ada.model.save(a)
        try await ada.shared.syncNow(aid)
        g = try XCTUnwrap(request(grace, gid, "Create order"))
        g.method = "PATCH"
        grace.model.save(g)
        try await grace.shared.syncNow(gid)
        let path = try XCTUnwrap(grace.shared.conflicts[gid]?.first?.path)
        try await grace.shared.resolve(gid, path: path, .mine)
        try await ada.shared.syncNow(aid)
        XCTAssertEqual(request(ada, aid, "Create order")?.method, "PATCH")

        // Ada removes Grace: the key rotates; Grace's old key can't read what comes next.
        let graceKeys = try await grace.shared.workspaceKeys(ws)
        let info = try await ada.shared.members(aid)
        let graceID = try XCTUnwrap(info.members.first { $0.email == grace.email }?.userId)
        let epoch = try await ada.shared.removeMember(aid, userId: graceID)
        XCTAssertEqual(epoch, 2)
        XCTAssertEqual(fake.epoch(ws), 2)
        XCTAssertNil(fake.role(ws, grace.email))
        a = try XCTUnwrap(request(ada, aid, "Create order"))
        a.body = "{\"secret plan\": true}"
        ada.model.save(a)
        try await ada.shared.syncNow(aid)
        for e in fake.envelopes(ws) {
            XCTAssertEqual(try SyncCore.blobEpoch(e), 2, "every file sealed again under the new key")
            XCTAssertThrowsError(try SyncCore.open(key: graceKeys.keys[1]!, workspace: ws, blob: "x", version: 1, envelope: e))
        }
        do {
            try await grace.shared.syncNow(gid)
            XCTFail("a removed member can't sync")
        } catch {
            XCTAssertEqual((error as? CloudError)?.status, 404)
        }
        try await grace.shared.refreshWorkspaces()
        XCTAssertEqual(grace.shared.badge(gid), "No longer shared")
        XCTAssertNotNil(grace.model.collection(gid), "the copy stays on her phone")
        await grace.library.flush()
        XCTAssertFalse(fake.items(grace.email).contains { $0.kind == "collection" && $0.data["name"]?.string == "Shop API" }, "never into her library")
    }

    func testASharedCollectionNeverReachesThePersonalLibrary() async throws {
        let (aid, gid, _) = try await shareWithGrace()
        await ada.library.flush()
        await grace.library.flush()
        XCTAssertFalse(fake.items(grace.email).contains { $0.kind == "collection" && !$0.deleted && $0.data["name"]?.string == "Shop API" })
        XCTAssertNil(grace.library.desired()["collection/\(gid)"])
        XCTAssertNil(ada.library.desired()["collection/\(aid)"])
        // Ada's copy was in her library before she shared it: it leaves the library.
        XCTAssertTrue(fake.item(ada.email, kind: "collection", id: String(aid))?.deleted ?? true)
        // An edit afterwards goes to the workspace only.
        fake.clearLog()
        var r = try XCTUnwrap(request(ada, aid, "List orders"))
        r.url = "https://api.example.com/v2/orders"
        ada.model.save(r)
        await ada.library.flush()
        try await ada.shared.syncNow(aid)
        XCTAssertFalse(fake.log.contains { $0.hasPrefix("PUT /v1/library/collection/") })
        XCTAssertTrue(fake.log.contains { $0.hasPrefix("PUT /v1/workspaces/") })
    }

    func testAViewerCanOnlyRead() async throws {
        let (aid, gid, _) = try await shareWithGrace("viewer")
        XCTAssertTrue(grace.model.isReadOnly(gid))
        var r = try XCTUnwrap(request(grace, gid, "List orders"))
        r.url = "https://evil.example.com"
        grace.model.save(r)
        grace.model.renameCollection(gid, to: "Mine now")
        grace.model.deleteRequest(r.id)
        grace.model.addFolder("x", in: gid)
        XCTAssertEqual(request(grace, gid, "List orders")?.url, "https://api.example.com/orders")
        XCTAssertEqual(grace.model.collection(gid)?.name, "Shop API")
        // A new request asked for there goes to My requests.
        let new = grace.model.newRequest(in: gid)
        XCTAssertFalse(grace.model.collection(gid)!.requests.contains { $0.id == new.id })
        XCTAssertFalse(grace.model.moveTargets.contains { $0.collection.id == gid })
        // Syncing only pulls.
        var a = try XCTUnwrap(request(ada, aid, "List orders"))
        a.url = "https://api.example.com/orders?new"
        ada.model.save(a)
        try await ada.shared.syncNow(aid)
        let s = try await grace.shared.syncNow(gid)
        XCTAssertEqual(s.pushed, [])
        XCTAssertEqual(request(grace, gid, "List orders")?.url, "https://api.example.com/orders?new")
    }

    // MARK: Keys

    func testABareInviteAsksToCompareTheSendersFingerprint() async throws {
        let (_, _, ws) = try await shareWithGrace(fingerprint: false)
        do {
            try await grace.shared.join(ws)
            XCTFail("a key from a device nobody trusted")
        } catch let e as SyncCore.KeyRefusal {
            XCTAssertEqual(e.kind, "untrustedSender")
            XCTAssertEqual(e.fingerprint, ada.shared.fingerprint)
        }
        let pending = try XCTUnwrap(grace.shared.pendingTrust)
        XCTAssertEqual(pending.fingerprint, ada.shared.fingerprint)
        try await grace.shared.trustSender(pending)
        XCTAssertNil(grace.shared.pendingTrust)
        let gid = try await grace.shared.join(ws)
        XCTAssertEqual(grace.model.collection(gid)?.name, "Shop API")
    }

    func testADamagedInviteCodeIsRefused() throws {
        XCTAssertThrowsError(try InviteCode.parse("abc.1234"))
        XCTAssertEqual(try InviteCode.parse("https://cloud.hitop.test/invite#tok.\(String(repeating: "ab", count: 16))").token, "tok")
        XCTAssertNil(try InviteCode.parse("tok").fingerprint)
        XCTAssertEqual(try InviteCode.parse(" tok.62BD A406 8c54 0000 1111 2222 3333 4444 ").fingerprint, "62bda4068c5400001111222233334444")
    }

    func testAForgedOrSwappedKeyIsRefused() async throws {
        let (_, gid, ws) = try await shareWithGrace()
        let graceKey = try XCTUnwrap(fake.deviceKeys(grace.email).first)
        let adaKey = try XCTUnwrap(fake.deviceKeys(ada.email).first)
        // The server makes its own key pair and a wrap that claims to come from Ada's device.
        let server = SyncCore.keygen()
        let pk = try XCTUnwrap(fake.devicePublicKey(grace.email))
        let forged = try SyncCore.sealWrap(secret: server.secret, sender: adaKey, recipient: graceKey, recipientPublicKey: pk, workspace: ws, epoch: 1, head: 1, key: try SyncCore.newKey())
        fake.replaceWrap(ws, keyId: graceKey, epoch: 1, wrapped: forged)
        do {
            try await grace.shared.syncNow(gid)
            XCTFail("a forged key")
        } catch let e as SyncCore.KeyRefusal {
            XCTAssertEqual(e.kind, "forged")
            XCTAssertTrue(e.localizedDescription.hasPrefix("Security warning"))
        }
        XCTAssertNotNil(grace.shared.statuses[gid]?.error)
    }

    // MARK: Team environments

    func testTeamEnvironmentsOfTheCurrentOrganization() async throws {
        try await ada.signIn()
        fake.teamEnvironments = [.object(["id": .string("team-staging"), "version": .number(3), "updatedBy": .string("grace@example.com"), "updatedAt": .number(0),
                                          "data": .object(["name": .string("Acme staging"), "variables": .object(["baseUrl": .string("https://staging.acme.test"), "token": .string("")]),
                                                           "secrets": .array([.string("token")])])])]
        try await ada.cloud.loadOrgs()
        await ada.shared.loadTeamEnvironments()
        XCTAssertFalse(ada.model.environments.contains { $0.name == "Acme staging" }, "personal org: none")
        ada.cloud.selectOrg("org-acme")
        await ada.shared.loadTeamEnvironments()
        let e = try XCTUnwrap(ada.model.environments.first { $0.id == "team-staging" })
        XCTAssertEqual(e.variables, ["baseUrl": "https://staging.acme.test"])
        XCTAssertEqual(e.secrets, ["token"])
        XCTAssertEqual(e.extras["team"], .string("org-acme"))
        // Picked in the pill, never in the personal library.
        ada.model.setActiveEnvironment(e.id)
        XCTAssertFalse((ada.library.environmentsItem.data["environments"]?.array ?? []).contains { $0["id"]?.string == "team-staging" })
        // Gone for the team: gone here.
        fake.teamEnvironments = []
        await ada.shared.loadTeamEnvironments()
        XCTAssertNil(ada.model.environments.first { $0.id == "team-staging" })
        XCTAssertNil(ada.model.state.activeEnvironmentID)
    }

    // MARK: Files other clients wrote, limits, re-added members

    func testFilesAnotherClientWroteKeepTheirBytes() async throws {
        let (aid, gid, ws) = try await shareWithGrace()
        // Grace's CLI-like edit: a hand-written request without an id, as .yml.
        var files = grace.shared.store.files(ws)
        let hand = "name: Hand made\nmethod: GET\nurl: https://hand.example.test\n"
        files["requests/hand.yml"] = hand
        try grace.shared.applyFiles(gid, files)
        let pushed = try await grace.shared.syncNow(gid)
        XCTAssertEqual(pushed.pushed, ["requests/hand.yml"])
        // Ada pulls it and has nothing to send back, twice.
        try await ada.shared.syncNow(aid)
        XCTAssertNotNil(request(ada, aid, "Hand made"))
        let again = try await ada.shared.syncNow(aid)
        XCTAssertEqual(again.pushed, [])
        XCTAssertEqual(ada.shared.store.files(ws)["requests/hand.yml"], hand)
        XCTAssertNil(ada.shared.store.files(ws)["requests/hand.yaml"])
        // Ada edits it: written back where it was.
        var r = try XCTUnwrap(request(ada, aid, "Hand made"))
        r.url = "https://hand.example.test/v2"
        ada.model.save(r)
        let edited = try await ada.shared.syncNow(aid)
        XCTAssertEqual(edited.pushed, ["requests/hand.yml"])
        try await grace.shared.syncNow(gid)
        XCTAssertEqual(request(grace, gid, "Hand made")?.url, "https://hand.example.test/v2")
    }

    func testALimitReadsNeutrally() async throws {
        let c = shop(on: ada)
        try await ada.signIn()
        fake.sharingLimit = true
        do {
            try await ada.shared.share(c.id)
            XCTFail("over the limit")
        } catch {
            let text = error.localizedDescription
            XCTAssertTrue(text.contains("allows 0 shared collections"), text)
            for word in ["See or change the plan", "http", "upgrade", "pricing"] { XCTAssertFalse(text.contains(word), text) }
        }
        XCTAssertNil(ada.shared.link(c.id))
    }

    func testAReAddedMemberSyncsAgain() async throws {
        let (aid, gid, ws) = try await shareWithGrace()
        let info = try await ada.shared.members(aid)
        let graceID = try XCTUnwrap(info.members.first { $0.email == grace.email }?.userId)
        try await ada.shared.removeMember(aid, userId: graceID)
        try await grace.shared.refreshWorkspaces()
        XCTAssertEqual(grace.shared.badge(gid), "No longer shared")
        // Invited and added again.
        let code = try await ada.shared.invite(aid, email: grace.email, role: "editor").code
        _ = try await grace.shared.accept(try InviteCode.parse(code))
        let m = try await ada.shared.members(aid)
        // Her devices were unpinned when she was removed: compared and trusted again.
        ada.shared.trust(try XCTUnwrap(m.invites.first { $0.acceptedBy != nil }?.acceptedBy?.keys))
        let m2 = try await ada.shared.members(aid)
        let keys = try XCTUnwrap(m2.invites.first { $0.acceptedBy != nil }?.acceptedBy?.keys)
        try await ada.shared.addMember(aid, email: grace.email, role: "editor", keys: keys)
        try await grace.shared.refreshWorkspaces()
        XCTAssertEqual(grace.shared.badge(gid), "Shared")
        XCTAssertEqual(grace.shared.link(gid)?.workspace, ws)
        try await grace.shared.syncNow(gid)
        XCTAssertEqual(grace.model.collection(gid)?.name, "Shop API")
    }

    func testOnlyTheCurrentOrganizationsTeamEnvironments() async throws {
        try await ada.signIn()
        fake.teamEnvironments = [.object(["id": .string("team-a"), "version": .number(1), "data": .object(["name": .string("Acme staging"), "variables": .object([:])])])]
        try await ada.cloud.loadOrgs()
        ada.cloud.selectOrg("org-acme")
        await ada.shared.loadTeamEnvironments()
        XCTAssertNotNil(ada.model.environments.first { $0.id == "team-a" })
        ada.model.setActiveEnvironment("team-a")
        ada.cloud.selectOrg(ada.cloud.personalOrg?.id)
        await ada.shared.loadTeamEnvironments()
        XCTAssertNil(ada.model.environments.first { $0.id == "team-a" })
        XCTAssertNil(ada.model.state.activeEnvironmentID)
    }

    func testALibraryConflictNeverReplacesASharedCollection() async throws {
        let (aid, _, _) = try await shareWithGrace()
        // The library answers 409 on the delete that follows sharing: the shared one stays.
        let before = ada.model.collection(aid)
        ada.library.conflict(LibrarySync.Item(kind: "collection", id: String(aid), data: .null, secrets: [:]),
                             current: .object(["kind": .string("collection"), "id": .string(String(aid)), "version": .number(9), "deleted": .bool(false),
                                               "data": .object(["id": .number(Double(aid)), "name": .string("Library copy"), "requests": .array([])])]))
        XCTAssertEqual(ada.model.collection(aid), before)
        XCTAssertFalse(ada.model.collections.contains { $0.name.contains("(this iPhone)") })
    }
}
