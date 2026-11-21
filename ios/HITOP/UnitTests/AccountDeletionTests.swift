import Foundation
import HitopKit
import XCTest
@testable import HITOP

/// Deleting the HITOP Cloud account (`HitopCloud.deleteAccount`, sync-server `DELETE /v1/me`)
/// against `FakeCloud`: what is sent, the account gone on the server, everything of it forgotten
/// here (Keychain entry, account, device, orgs) while collections stay; refusals leave all as it was.
@MainActor
final class AccountDeletionTests: XCTestCase {
    var fake: FakeCloud!
    var secrets: MemorySecretStore!
    var settings: MemoryCloudSettingsStore!
    var cloud: HitopCloud!

    override func setUp() async throws {
        fake = FakeCloud()
        secrets = MemorySecretStore()
        settings = MemoryCloudSettingsStore(CloudSettings(server: FakeCloud.server))
        cloud = HitopCloud(transport: fake, credentials: CloudCredentials(store: secrets), settingsStore: settings, deviceName: "HITOP on iPhone")
    }

    func signIn() async throws {
        let start = try await cloud.startBrowserSignIn("apple", server: FakeCloud.server)
        let code = URLComponents(url: start.url, resolvingAgainstBaseURL: false)!.queryItems!.first { $0.name == "code" }!.value!
        try await cloud.finishBrowserSignIn(code)
    }

    func testDeletesOnTheServerAndForgetsTheAccountHere() async throws {
        fake.signInEmail = "abc123@privaterelay.appleid.com"
        try await signIn()
        _ = try await cloud.loadOrgs()
        XCTAssertTrue(cloud.signedIn)
        XCTAssertNotNil(cloud.credentials.load(FakeCloud.server)?.secret)
        var changes = 0
        var deleted: CloudAccount?
        cloud.onAccountChange = { changes += 1 }
        cloud.onAccountDeleted = { deleted = $0 }
        fake.clearLog()

        try await cloud.deleteAccount()
        XCTAssertEqual(fake.log, ["DELETE /v1/me"])
        XCTAssertEqual(fake.lastHeaders["Content-Type"], "application/json")
        XCTAssertFalse(fake.hasAccount("abc123@privaterelay.appleid.com"))
        // Nothing of the account is left on this device: no session, no device secret, no
        // remembered device to sign in as again, no organizations.
        XCTAssertFalse(cloud.signedIn)
        XCTAssertNil(cloud.credentials.load(FakeCloud.server))
        XCTAssertNil(try secrets.get(account: CloudCredentials.account(FakeCloud.server)))
        XCTAssertNil(cloud.settings.device)
        XCTAssertTrue(cloud.settings.orgs.isEmpty)
        XCTAssertNil(settings.load().account)
        XCTAssertNil(settings.load().device)
        XCTAssertEqual(changes, 1)
        XCTAssertEqual(deleted?.email, "abc123@privaterelay.appleid.com")
        // The server is kept; signing in again makes a new account with a new device key.
        XCTAssertEqual(cloud.settings.server, FakeCloud.server)
        try await signIn()
        XCTAssertTrue(fake.hasAccount("abc123@privaterelay.appleid.com"))
        XCTAssertEqual(fake.devices("abc123@privaterelay.appleid.com").count, 1)
    }

    func testTheConfirmationIsTheAccountsAddress() async throws {
        try await signIn()
        let r = try await cloud.http(FakeCloud.server, "DELETE", "/v1/me", body: .object(["confirm": .string("someone@else.example")]),
                                     session: cloud.credentials.load(FakeCloud.server)!.session)
        XCTAssertEqual(r.status, 400)
        XCTAssertEqual(r.json?["error"]?.string, "confirm_mismatch")
        XCTAssertTrue(fake.hasAccount("ada@example.com"))
    }

    func testAnOnlyOwnerIsRefusedAndNothingChanges() async throws {
        try await signIn()
        fake.deleteBlockers = [.object(["kind": .string("org"), "id": .string("org-acme"), "name": .string("Acme"), "members": .number(3)])]
        do {
            try await cloud.deleteAccount()
            XCTFail("refused")
        } catch let CloudError.http(status, code, message) {
            XCTAssertEqual(status, 409)
            XCTAssertEqual(code, "transfer_ownership_first")
            XCTAssertTrue(message.contains("make another member an owner"), message)
        }
        XCTAssertTrue(cloud.signedIn)
        XCTAssertNotNil(cloud.credentials.load(FakeCloud.server)?.session)
        XCTAssertTrue(fake.hasAccount("ada@example.com"))
    }

    func testOfflineDeletesNothing() async throws {
        try await signIn()
        fake.offline = true
        do {
            try await cloud.deleteAccount()
            XCTFail("unreachable")
        } catch CloudError.unreachable {}
        XCTAssertTrue(cloud.signedIn)
        XCTAssertNotNil(cloud.credentials.load(FakeCloud.server)?.secret)
    }

    func testNotSignedIn() async {
        do {
            try await cloud.deleteAccount()
            XCTFail("not signed in")
        } catch CloudError.notSignedIn {
        } catch {
            XCTFail("\(error)")
        }
    }
}

extension SharedSyncTests {
    /// Grace deletes her account: her copy of the shared collection stays on her phone, never
    /// synced again, her device key unpinned; Ada's workspace no longer has her.
    func testDeletingTheAccountKeepsSharedCopiesHereUnsynced() async throws {
        let (_, g, _) = try await shareWithGrace()
        XCTAssertFalse(grace.shared.link(g)?.gone ?? true)
        try await grace.cloud.deleteAccount()
        XCTAssertTrue(grace.shared.link(g)?.gone ?? false)
        XCTAssertNotNil(grace.model.collection(g), "the collection stays on this device")
        XCTAssertNil(grace.shared.fingerprint, "no device key any more")
        XCTAssertFalse(fake.hasAccount(grace.email))
    }
}
