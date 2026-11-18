import Foundation
import HitopKit
import XCTest
@testable import HITOP

/// The UI tests' shared-collection scenarios (`SharedScenario`) run here first, without a
/// simulator: what the UI tests then click through.
@MainActor
final class SharedScenarioTests: XCTestCase {
    func phone(_ fake: FakeCloud) -> (HitopCloud, AppModel, SharedSync) {
        let model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault())
        model.load()
        let cloud = HitopCloud(transport: fake, credentials: CloudCredentials(store: MemorySecretStore()), settingsStore: MemoryCloudSettingsStore(CloudSettings(server: FakeCloud.server)))
        let shared = SharedSync(cloud: cloud, model: model, store: MemorySharedSyncStore())
        shared.pushDelay = .seconds(3600)
        return (cloud, model, shared)
    }

    func testTheInviteCodeNamesGracesFixedKey() throws {
        let code = try InviteCode.parse(SharedScenario.inviteCode)
        XCTAssertEqual(code.token, "TEAMINVITE")
        XCTAssertEqual(code.fingerprint, SyncCore.normalize(fingerprint: SharedScenario.peerFingerprint))
        // The UI test types it: it must not change.
        XCTAssertEqual(SyncCore.grouped(code.fingerprint ?? ""), SharedScenario.peerFingerprint)
        print("peer fingerprint: \(SharedScenario.peerFingerprint)")
    }

    func testInviteThenAcceptThenOpen() async throws {
        let fake = FakeCloud()
        let (cloud, model, shared) = phone(fake)
        try await SharedScenario.invite(app: cloud, fake: fake)
        _ = try await shared.accept(try InviteCode.parse(SharedScenario.inviteCode))
        var ws: SharedWorkspace?
        for _ in 0..<40 where ws == nil {
            try await Task.sleep(for: .milliseconds(250))
            try await shared.refreshWorkspaces()
            ws = shared.joinable.first
        }
        let id = try await shared.join(try XCTUnwrap(ws).id)
        XCTAssertEqual(model.collection(id)?.name, "Team API")
        XCTAssertEqual(shared.badge(id), "Shared with Acme")
        XCTAssertTrue(model.visibleCollections.isEmpty || model.orgScope == nil || model.visibleCollections.contains { $0.id == id })
    }

    func testJoinedEditorMeetsAConflictAndViewerReadsOnly() async throws {
        let fake = FakeCloud()
        let (cloud, model, shared) = phone(fake)
        try await SharedScenario.joined(app: cloud, shared: shared, fake: fake)
        let team = try XCTUnwrap(model.collections.first { $0.name == "Team API" })
        let partner = try XCTUnwrap(model.collections.first { $0.name == "Partner API" })
        XCTAssertEqual(model.sharedRoles[team.id], "editor")
        XCTAssertTrue(model.isReadOnly(partner.id))
        XCTAssertEqual(shared.badge(team.id), "Shared with Acme")
        // Ada's launch sync sees nothing new (Grace's change is held back).
        await shared.syncAll()
        XCTAssertEqual(model.collection(team.id)?.requests.first { $0.name == "List orders" }?.url, "${baseUrl}/orders")
        var r = try XCTUnwrap(model.collection(team.id)?.requests.first { $0.name == "List orders" })
        r.method = "PUT"
        model.save(r)
        try await shared.syncNow(team.id)
        let c = try XCTUnwrap(shared.conflicts[team.id]?.first)
        XCTAssertTrue(c.theirs?.contains("open") ?? false, c.theirs ?? "")
        try await shared.resolve(team.id, path: c.path, .theirs)
        XCTAssertEqual(model.collection(team.id)?.requests.first { $0.name == "List orders" }?.method, "GET")
        XCTAssertEqual(shared.conflicts[team.id], [])
    }
}
