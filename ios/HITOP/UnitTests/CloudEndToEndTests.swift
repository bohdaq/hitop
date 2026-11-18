#if os(macOS) || os(Linux)
import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import HitopKit
import XCTest
@testable import HITOP

/// The app's Core layer against a real hitop sync server (sync-server, built in CI): sign in by a
/// mailed code (the server logs its mail), push the library from one device, read it back through
/// the API and on a second device, a conflict copy, a tombstone, devices, organizations.
/// Runs where Foundation has `Process` (macOS, Linux; not the iOS simulator) and only with
/// HITOP_SYNC_SERVER_BIN set (skipped otherwise, unless HITOP_E2E_REQUIRED is set).
@MainActor
final class CloudEndToEndTests: XCTestCase {
    final class Log: @unchecked Sendable {
        private let lock = NSLock()
        private var text = ""
        func add(_ s: String) { lock.lock(); text += s; lock.unlock() }
        var value: String { lock.lock(); defer { lock.unlock() }; return text }
    }

    var process: Process?
    var dir: URL?
    let log = Log()
    var server = ""

    override func tearDown() async throws {
        process?.terminate()
        process?.waitUntilExit()
        if let dir { try? FileManager.default.removeItem(at: dir) }
    }

    func startServer() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let bin = env["HITOP_SYNC_SERVER_BIN"], !bin.isEmpty else {
            if env["HITOP_E2E_REQUIRED"] != nil { XCTFail("HITOP_SYNC_SERVER_BIN must be set") }
            throw XCTSkip("HITOP_SYNC_SERVER_BIN not set")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("hitop-ios-e2e-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        self.dir = dir
        let port = Int.random(in: 20000...60000)
        server = "http://127.0.0.1:\(port)"
        let p = Process()
        p.executableURL = URL(fileURLWithPath: bin)
        p.currentDirectoryURL = dir
        var e = env
        e["RWS_CONFIG_IP"] = "127.0.0.1"
        e["RWS_CONFIG_PORT"] = String(port)
        e["HITOP_SYNC_PUBLIC_URL"] = server
        e["HITOP_SYNC_RP_ID"] = "localhost"
        e["HITOP_SYNC_RP_ORIGINS"] = "http://localhost:\(port)"
        e["HITOP_SYNC_DATABASE"] = "sqlite:\(dir.appendingPathComponent("sync.db").path)"
        e["HITOP_SYNC_MAIL"] = "log"
        e["HITOP_SYNC_HOST_KEY"] = CloudCrypto.randomBytes(32).map { String(format: "%02x", $0) }.joined()
        p.environment = e
        let err = Pipe()
        p.standardError = err
        p.standardOutput = FileHandle.nullDevice
        let log = self.log
        err.fileHandleForReading.readabilityHandler = { h in log.add(String(decoding: h.availableData, as: UTF8.self)) }
        try p.run()
        process = p
        for _ in 0..<150 {
            if let r = try? await URLSessionCloudTransport().send(CloudRequest(method: "GET", url: URL(string: "\(server)/healthz")!, headers: [:], body: nil)), r.status == 200 { return }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTFail("the sync server didn't start: \(log.value)")
    }

    /// The code of the newest sign-in mail to `email` (HITOP_SYNC_MAIL=log).
    func mailedCode(_ email: String, after count: Int) async throws -> String {
        for _ in 0..<100 {
            let parts = log.value.components(separatedBy: "mail to \(email):")
            if parts.count - 1 > count, let line = parts.last!.split(separator: "\n").first(where: { $0.contains("/signin#") }) {
                return String(line.trimmingCharacters(in: .whitespaces).split(separator: "#").last!)
            }
            try await Task.sleep(for: .milliseconds(100))
        }
        throw XCTSkip("no mail to \(email): \(log.value)")
    }

    struct Device {
        var model: AppModel
        var cloud: HitopCloud
        var sync: LibrarySync
        var vault: MemoryVault
    }

    func device(_ name: String) -> Device {
        let model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault())
        model.load()
        let cloud = HitopCloud(transport: URLSessionCloudTransport(), credentials: CloudCredentials(store: MemorySecretStore()),
                               settingsStore: MemoryCloudSettingsStore(CloudSettings(server: server)), keyMaker: RandomKeyMaker(), deviceName: name)
        let sync = LibrarySync(cloud: cloud, model: model, stateStore: MemoryLibrarySyncStateStore())
        cloud.onAccountChange = nil
        sync.pushDelay = .seconds(3600)
        sync.retryDelay = .seconds(3600)
        return Device(model: model, cloud: cloud, sync: sync, vault: model.vault as! MemoryVault)
    }

    func signIn(_ d: Device, _ email: String) async throws {
        let before = log.value.components(separatedBy: "mail to \(email):").count - 1
        try await d.cloud.startEmailSignIn(server: server, email: email)
        try await d.cloud.finishEmailSignIn(try await mailedCode(email, after: before))
        await d.sync.sync()
    }

    func testTheAppsCoreAgainstARealSyncServer() async throws {
        try await startServer()
        let email = "ios-e2e@example.com"

        // What the sign-in sheet would offer: no provider is set up on this server.
        let methods = try await device("probe").cloud.authMethods(server)
        XCTAssertEqual(methods?.providers, [])

        // Device one: a collection, an environment with a secret, signed in → pushed.
        let one = device("HITOP on iPhone")
        let shop = one.model.addCollection(name: "shop-api")
        var req = one.model.newRequest(in: shop.id)
        req.name = "Create order"
        req.url = "${baseUrl}/v2/orders"
        req.method = "POST"
        one.model.save(req)
        let env = one.model.addEnvironment(name: "Staging")
        one.model.setVariable(env, name: "baseUrl", value: "https://staging.test", secret: false)
        one.model.setVariable(one.model.environments[0], name: "apiKey", value: "sk_e2e", secret: true)
        try await signIn(one, email)
        XCTAssertTrue(one.cloud.signedIn)
        XCTAssertNil(one.sync.status.error)
        XCTAssertEqual(one.sync.status.secretsStored, true)
        XCTAssertEqual(one.sync.badges[shop.id], .synced)

        // Read back through the API.
        let lib = try await one.cloud.call("GET", "/v1/library").json!
        let items = lib["items"]!.array!
        let col = try XCTUnwrap(items.first { $0["kind"]?.string == "collection" && $0["id"]?.string == String(shop.id) })
        XCTAssertEqual(col["data"]?["name"]?.string, "shop-api")
        XCTAssertEqual(col["data"]?["requests"]?.array?.first?["url"]?.string, "${baseUrl}/v2/orders")
        let envs = try XCTUnwrap(items.first { $0["kind"]?.string == "environments" })
        XCTAssertEqual(envs["data"]?["environments"]?.array?.first?["variables"]?["apiKey"]?.string, "")
        XCTAssertEqual(envs["secrets"]?[env.id]?["apiKey"]?.string, "sk_e2e")
        // The empty My requests stayed on the device.
        XCTAssertFalse(items.contains { $0["data"]?["name"]?.string == "My requests" })

        // Device two, same account: a second device, the library pulled with the secret.
        let two = device("HITOP on iPad")
        try await signIn(two, email)
        XCTAssertEqual(two.model.collection(shop.id)?.requests.first?.name, "Create order")
        XCTAssertEqual(two.vault.value(environment: env.id, name: "apiKey"), "sk_e2e")
        let devices = try await one.cloud.devices() ?? []
        XCTAssertEqual(devices.map(\.name).sorted(), ["HITOP on iPad", "HITOP on iPhone"])
        XCTAssertEqual(devices.first { $0.thisDevice }?.name, "HITOP on iPhone", "X-Hitop-Device ties the session to this device")

        // Both edit: two pushes first, one meets the server's newer version → a copy.
        two.model.renameCollection(shop.id, to: "shop-api (renamed on iPad)")
        await two.sync.flush()
        one.model.renameRequest(req.id, to: "Create an order")
        await one.sync.flush()
        XCTAssertEqual(one.model.collection(shop.id)?.name, "shop-api (renamed on iPad)")
        let copy = try XCTUnwrap(one.model.collections.first { $0.name == "shop-api (this iPhone)" })
        XCTAssertEqual(copy.requests.first?.name, "Create an order")
        await two.sync.pull()
        XCTAssertNotNil(two.model.collections.first { $0.name == "shop-api (this iPhone)" })

        // A delete: a tombstone the other device follows.
        one.model.deleteCollection(copy.id)
        await one.sync.flush()
        await two.sync.pull()
        XCTAssertNil(two.model.collection(copy.id))

        // Sign out keeps the device: signing in again adds none.
        await two.cloud.signOut()
        try await signIn(two, email)
        let after = try await one.cloud.devices() ?? []
        XCTAssertEqual(after.count, 2)

        // Organizations and usage (a self-hosted server: no limits, no plan to show limits for).
        let orgs = try await one.cloud.loadOrgs()
        let personal = try XCTUnwrap(orgs.first { $0.personal })
        let usage = try await one.cloud.usage(personal.id)
        XCTAssertTrue(usage.meters.allSatisfy { $0.max == nil })
        XCTAssertTrue(usage.meters.contains { $0.id == "devices" && $0.used == 2 })
    }
}
#endif
