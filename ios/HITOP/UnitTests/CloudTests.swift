import Foundation
import HitopKit
import XCTest
@testable import HITOP

/// HITOP Cloud sign-in, devices and plan usage against `FakeCloud` (the shapes of sync-server;
/// `CloudEndToEndTests` runs the same client against the real one).
@MainActor
final class CloudTests: XCTestCase {
    var fake: FakeCloud!
    var secrets: MemorySecretStore!
    var settings: MemoryCloudSettingsStore!
    var cloud: HitopCloud!

    override func setUp() async throws {
        fake = FakeCloud()
        secrets = MemorySecretStore()
        settings = MemoryCloudSettingsStore(CloudSettings(server: FakeCloud.server))
        cloud = makeCloud()
    }

    func makeCloud() -> HitopCloud {
        HitopCloud(transport: fake, credentials: CloudCredentials(store: secrets), settingsStore: settings, deviceName: "HITOP on iPhone")
    }

    /// Continue with Google, the return address handing the code back (what ASWebAuthenticationSession does).
    func signIn(_ kind: String = "google") async throws {
        let start = try await cloud.startBrowserSignIn(kind, server: FakeCloud.server)
        XCTAssertFalse(start.paste)
        let code = URLComponents(url: start.url, resolvingAgainstBaseURL: false)!.queryItems!.first { $0.name == "code" }!.value!
        try await cloud.finishBrowserSignIn(HitopCloud.code(from: "hitop://sign-in?sso=\(code)")!)
    }

    func testCrypto() {
        XCTAssertEqual(CloudCrypto.sha256(Array("abc".utf8)).map { String(format: "%02x", $0) }.joined(),
                       "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        XCTAssertEqual(CloudCrypto.sha256([]).map { String(format: "%02x", $0) }.joined(),
                       "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        let long = Array(String(repeating: "a", count: 1000).utf8)
        XCTAssertEqual(CloudCrypto.sha256(long).map { String(format: "%02x", $0) }.joined(),
                       "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3")
        // The challenge is base64url(SHA-256(verifier)) without padding (sync-server oidc.rs).
        XCTAssertEqual(CloudCrypto.challenge("abc"), "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0")
        let (v, c) = CloudCrypto.verifier()
        XCTAssertEqual(v.count, 64)
        XCTAssertEqual(CloudCrypto.unbase64url(c)?.count, 32)
    }

    func testCodeFromTheReturnAddressOrPasted() {
        XCTAssertEqual(HitopCloud.code(from: "hitop://sign-in?sso=abc123"), "abc123")
        XCTAssertEqual(HitopCloud.code(from: "https://app.example/x#sso=zz"), "zz")
        XCTAssertEqual(HitopCloud.code(from: "  pasted-code \n"), "pasted-code")
        XCTAssertNil(HitopCloud.code(from: "hitop://sign-in?error=x"))
        XCTAssertNil(HitopCloud.code(from: ""))
    }

    func testServerAddresses() throws {
        XCTAssertEqual(try HitopCloud.normalize(""), CloudSettings.defaultServer)
        XCTAssertEqual(try HitopCloud.normalize(" https://sync.example.com/ "), "https://sync.example.com")
        XCTAssertEqual(try HitopCloud.normalize("http://localhost:7881"), "http://localhost:7881")
        XCTAssertThrowsError(try HitopCloud.normalize("http://sync.example.com"))
        XCTAssertThrowsError(try HitopCloud.normalize("sync"))
    }

    func testStoreModeTakesOutPlanLinks() {
        XCTAssertEqual(StoreText.withoutPlanLink("plan limit: the pro plan of your personal account allows 2 devices; nothing changed. See or change the plan at https://cloud.hitop.rws8.tech/orgs/p/plan"),
                       "plan limit: the pro plan of your personal account allows 2 devices; nothing changed.")
        XCTAssertEqual(StoreText.withoutPlanLink("Mock servers come with Pro, Team and Business plans."), "Mock servers aren't part of this plan.")
        XCTAssertEqual(StoreText.withoutPlanLink("Full, https://cloud.hitop.rws8.tech/orgs/x/billing here."), "Full here.")
    }

    func testMethodsShowWhatTheServerOffers() async throws {
        fake.methods = (apple: false, google: true, github: true)
        let m = try await cloud.authMethods(FakeCloud.server)
        XCTAssertEqual(m?.providers, ["google", "github"])
    }

    func testSignInRegistersThisDeviceAndSendsItsKeyId() async throws {
        try await signIn()
        let a = try XCTUnwrap(cloud.account)
        XCTAssertEqual(a.email, "ada@example.com")
        XCTAssertEqual(a.server, FakeCloud.server)
        XCTAssertEqual(fake.devices("ada@example.com"), ["HITOP on iPhone"])
        // The session and the device's secret key are in the Keychain, not in cloud.json.
        let secret = try XCTUnwrap(CloudCredentials(store: secrets).load(FakeCloud.server))
        XCTAssertNotNil(secret.session)
        XCTAssertEqual(CloudCrypto.unbase64url(secret.secret)?.count, 32)
        XCTAssertNil(String(data: try JSONEncoder().encode(settings.load()), encoding: .utf8)!.range(of: secret.session!))
        // DD3: every request as this account names the device.
        _ = try await cloud.devices()
        XCTAssertEqual(fake.lastHeaders["X-Hitop-Device"], a.keyId)
        XCTAssertEqual(fake.lastHeaders["Authorization"], "Bearer \(secret.session!)")
    }

    func testSignOutKeepsTheDeviceKey() async throws {
        try await signIn()
        let keyId = cloud.account!.keyId
        await cloud.signOut()
        XCTAssertFalse(cloud.signedIn)
        XCTAssertEqual(settings.load().device?.keyId, keyId)
        let kept = CloudCredentials(store: secrets).load(FakeCloud.server)
        XCTAssertNil(kept?.session)
        XCTAssertNotNil(kept?.secret)
        XCTAssertTrue(fake.log.contains("POST /v1/auth/logout"))
        // Signing in to the same account again is the same device, not one more.
        try await signIn()
        XCTAssertEqual(cloud.account?.keyId, keyId)
        XCTAssertEqual(fake.devices("ada@example.com").count, 1)
        // Another account on this device: a new device for it.
        await cloud.signOut()
        fake.signInEmail = "bea@example.com"
        try await signIn()
        XCTAssertNotEqual(cloud.account?.keyId, keyId)
        XCTAssertEqual(fake.devices("bea@example.com").count, 1)
    }

    func testAServerWithoutTheAppReturnGetsThePastedCode() async throws {
        fake.refusesAppReturn = true
        let start = try await cloud.startBrowserSignIn("github", server: FakeCloud.server)
        XCTAssertTrue(start.paste)
        let code = URLComponents(url: start.url, resolvingAgainstBaseURL: false)!.queryItems!.first { $0.name == "code" }!.value!
        try await cloud.finishBrowserSignIn("  \(code)  ")
        XCTAssertTrue(cloud.signedIn)
        // A code used twice, or a code without this app's verifier, is refused.
        do {
            try await cloud.finishBrowserSignIn(code)
            XCTFail("finished twice")
        } catch {}
    }

    func testDeviceLimitListsTheDevicesAndRemovingOneFinishes() async throws {
        fake.deviceLimit = 2
        fake.addDevice("ada@example.com", name: "Old iPad")
        fake.addDevice("ada@example.com", name: "Work laptop", workspaces: 1)
        do {
            try await signIn()
            XCTFail("signed in past the limit")
        } catch let limit as DeviceLimit {
            XCTAssertFalse(cloud.signedIn)
            XCTAssertEqual(limit.max, 2)
            XCTAssertEqual(limit.devices.map(\.name), ["Old iPad", "Work laptop"])
            // One holding shared collections' keys stays.
            do {
                _ = try await cloud.removeDevice(limit.devices[1].id, at: limit)
                XCTFail("removed a device holding keys")
            } catch let e as CloudError {
                XCTAssertEqual(e.code, "key_in_use")
            }
            let next = try await cloud.removeDevice(limit.devices[0].id, at: limit)
            XCTAssertEqual(next.devices.map(\.name), ["Work laptop"])
            try await cloud.retry(next)
            XCTAssertTrue(cloud.signedIn)
            XCTAssertEqual(fake.devices("ada@example.com"), ["Work laptop", "HITOP on iPhone"])
        }
    }

    func testTheLimitAnswerNeverCarriesAPlanLink() async throws {
        // sync-server limits.rs `refused`: 402 {error: limit, message ... See or change the plan at <url>, upgradeUrl}.
        let body = #"{"error":"limit","message":"plan limit: the free plan of your personal account allows 100 MB storage; nothing changed. See or change the plan at https://cloud.hitop.rws8.tech/orgs/personal-u1/plan","limit":"storageBytes","plan":"free","org":"personal-u1","used":104857600,"max":104857600,"upgradeUrl":"https://cloud.hitop.rws8.tech/orgs/personal-u1/plan"}"#
        let e = HitopCloud.error(CloudResponse(status: 402, body: Data(body.utf8)))
        XCTAssertEqual(e.code, "limit")
        let text = e.localizedDescription
        XCTAssertFalse(text.contains("http"), text)
        XCTAssertFalse(text.lowercased().contains("see or change"), text)
    }

    func testDevicesRenameAndRemove() async throws {
        fake.addDevice("ada@example.com", name: "Old iPad")
        try await signIn()
        let first = try await cloud.devices()
        var list = try XCTUnwrap(first)
        XCTAssertEqual(list.map(\.name), ["Old iPad", "HITOP on iPhone"])
        XCTAssertEqual(list.map(\.thisDevice), [false, true])
        try await cloud.renameDevice(list[1].id, to: "Ada's iPhone")
        XCTAssertEqual(cloud.account?.deviceName, "Ada's iPhone")
        try await cloud.removeDevice(list[0].id)
        let second = try await cloud.devices()
        list = try XCTUnwrap(second)
        XCTAssertEqual(list.map(\.name), ["Ada's iPhone"])
        XCTAssertTrue(fake.log.contains { $0.hasPrefix("DELETE /v1/me/keys/") && $0.contains("untied=true") })
    }

    func testOrganizationsAndNeutralUsage() async throws {
        fake.deviceLimit = 3
        try await signIn()
        let orgs = try await cloud.loadOrgs()
        XCTAssertEqual(orgs.map(\.name), ["ada@example.com", "Acme"])
        XCTAssertEqual(cloud.currentOrg?.personal, true)
        cloud.selectOrg("org-acme")
        XCTAssertEqual(cloud.orgForNew, "org-acme")
        cloud.selectOrg(cloud.personalOrg?.id)
        XCTAssertNil(cloud.orgForNew)
        let usage = try await cloud.usage(orgs[0].id)
        XCTAssertEqual(usage.planName, "Pro")
        XCTAssertEqual(usage.meters.map(\.label), ["Seats", "Shared collections", "Storage", "Devices", "CI runners"])
        let devices = try XCTUnwrap(usage.meters.first { $0.id == "devices" })
        XCTAssertEqual(devices.text, "1 of 3")
        XCTAssertEqual(usage.meters.first { $0.id == "storage" }?.text, "46 MB of 2.0 GB")
        // Store mode: no word about buying, upgrading or prices anywhere in what the screen shows.
        let words = usage.meters.flatMap { [$0.label, $0.text, $0.note] }.joined(separator: " ").lowercased()
        for banned in ["upgrade", "price", "$", "buy", "plans", "billing"] { XCTAssertFalse(words.contains(banned), banned) }
    }

    func testAnEndedSessionSignsOutAndKeepsTheDevice() async throws {
        try await signIn()
        let a = cloud.account!
        cloud.sessionEnded()
        XCTAssertFalse(cloud.signedIn)
        XCTAssertEqual(settings.load().device, a)
        XCTAssertNotNil(cloud.lastError)
    }

    func testRelaunchReadsTheAccountAndAMissingSessionIsSignedOut() async throws {
        try await signIn()
        XCTAssertTrue(makeCloud().signedIn)
        CloudCredentials(store: secrets).save(FakeCloud.server, nil)
        XCTAssertFalse(makeCloud().signedIn)
    }
}
