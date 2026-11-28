import Foundation
import HitopKit
import XCTest
@testable import HITOP

/// Scheduled runs, what another app shared, and files that can't be used while nobody looks.
@MainActor
final class BackgroundTests: XCTestCase {
    private var root: URL!
    private var model: AppModel!
    private var vault: BrokenVault!
    private var runs: RunCenter!

    override func setUp() async throws {
        StubURLProtocol.enabled = true
        root = FileManager.default.temporaryDirectory.appendingPathComponent("background-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        vault = BrokenVault()
        model = AppModel(store: MemoryLibraryStore(), vault: vault, sender: Sender(configuration: StubURLProtocol.configuration))
        model.load()
        model.seed()
        runs = RunCenter(model: model, store: MemoryRunStore())
    }

    override func tearDown() async throws { try? FileManager.default.removeItem(at: root) }

    private var shop: UInt64 { model.collections.first { $0.name == "shop-api" }!.id }
    private func schedules() -> Schedules { Schedules(store: FileScheduleStore(root: root), runs: runs) }

    func testADueRunGoesOnceAndIsKept() async throws {
        let s = schedules()
        let start = Date(timeIntervalSince1970: 1_000_000)
        try s.set(shop, environmentID: "env-staging", everyMinutes: 60, now: start)
        XCTAssertEqual(schedules().list, s.list)
        let early = await s.runDue(now: start.addingTimeInterval(3599))
        XCTAssertTrue(early.isEmpty)
        let late = start.addingTimeInterval(3600)
        let left = await s.runDue(now: late, fits: { _ in false })
        XCTAssertTrue(left.isEmpty)
        let done = await s.runDue(now: late)
        XCTAssertEqual(done.map(\.environmentName), ["Staging"])
        XCTAssertEqual(schedules().next, late.addingTimeInterval(3600))
        let again = await s.runDue(now: late)
        XCTAssertTrue(again.isEmpty)
        try s.set(shop, environmentID: nil, everyMinutes: nil)
        XCTAssertNil(schedules().next)
    }

    func testARunNeverGoesWithOtherValues() async throws {
        let s = schedules()
        let start = Date(timeIntervalSince1970: 1_000_000)
        try s.set(shop, environmentID: "env-gone", everyMinutes: 60, now: start)
        var r = await s.runDue(now: start.addingTimeInterval(7200))
        XCTAssertTrue(r.isEmpty)
        XCTAssertTrue(s.schedule(of: shop)!.note!.contains("environment"))
        try s.set(shop, environmentID: "env-staging", everyMinutes: 60, now: start)
        vault.broken = true
        r = await s.runDue(now: start.addingTimeInterval(7200))
        XCTAssertTrue(r.isEmpty)
        XCTAssertTrue(s.schedule(of: shop)!.note!.contains("Keychain"))
        XCTAssertTrue(runs.runs.isEmpty)
    }

    func testACollectionThatIsGoneLosesItsSchedule() async throws {
        let s = schedules()
        try s.set(42, environmentID: nil, everyMinutes: 60, now: .distantPast)
        let r = await s.runDue()
        XCTAssertTrue(r.isEmpty)
        XCTAssertTrue(s.list.isEmpty)
    }

    func testAStoppedRunIsNotARecord() async throws {
        let s = schedules()
        let id = shop
        try s.set(id, environmentID: "env-staging", everyMinutes: 60, now: .distantPast)
        var stopped = false
        var work: Task<[RunRecord], Never>?
        runs.onEvent = { e in
            if case .started = e { work?.cancel() }
            if case .stopped = e { stopped = true }
        }
        work = Task { await s.runDue() }
        let r = await work!.value
        XCTAssertTrue(r.isEmpty)
        XCTAssertTrue(stopped)
        XCTAssertFalse(runs.isRunning)
        XCTAssertTrue(runs.runs.isEmpty)
        XCTAssertNotNil(s.schedule(of: id)!.note)
        XCTAssertGreaterThan(s.schedule(of: id)!.due, Date())
    }

    func testADamagedScheduleFileIsReplacedByTheNextChange() throws {
        try Data("{".utf8).write(to: root.appendingPathComponent("schedules.json"))
        let s = schedules()
        XCTAssertTrue(s.list.isEmpty)
        try s.set(shop, environmentID: nil, everyMinutes: 60)
        XCTAssertEqual(schedules().list.count, 1)
    }

    // MARK: Shared to HITOP

    func testSharedItemsAreTakenOnceOldestFirst() throws {
        let inbox = SharedInbox(root: root)
        XCTAssertNil(inbox.take())
        try inbox.add(text: "  curl https://api.example/x \n")
        let f = root.appendingPathComponent("a.json")
        try Data("[]".utf8).write(to: f)
        Thread.sleep(forTimeInterval: 0.005)
        try inbox.add(file: f, name: "../../b.json")
        XCTAssertEqual(inbox.take(), .text("curl https://api.example/x"))
        guard case let .file(url)? = inbox.take() else { return XCTFail("no file") }
        XCTAssertEqual(url.lastPathComponent, "..-..-b.json")
        XCTAssertEqual(try String(contentsOf: url, encoding: .utf8), "[]")
        XCTAssertNil(inbox.take())
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.path))
        try? FileManager.default.removeItem(at: url.deletingLastPathComponent().deletingLastPathComponent())
    }

    func testWhatCannotBeShared() throws {
        let inbox = SharedInbox(root: root)
        XCTAssertThrowsError(try inbox.add(text: " \n"))
        XCTAssertThrowsError(try inbox.add(file: root))
        XCTAssertThrowsError(try inbox.add(text: String(repeating: "x", count: SharedInbox.maxBytes + 1)))
        XCTAssertEqual(SharedInbox.safe(".."), "file")
        XCTAssertEqual(SharedInbox.safe("a/b\\c:d"), "a-b-c-d")
        for i in 0..<SharedInbox.maxItems { try inbox.add(text: "\(i)") }
        XCTAssertThrowsError(try inbox.add(text: "one more"))
        // Taken in the order shared, also when several came in one millisecond.
        for i in 0..<SharedInbox.maxItems { XCTAssertEqual(inbox.take(), .text("\(i)")) }
        XCTAssertNil(inbox.take())
    }

    func testSharedTextBecomesWhatItIs() {
        let nav = Navigator()
        let before = model.collections.flatMap(\.requests).count
        nav.receive(.text("https://api.example/users?page=2"), model)
        XCTAssertEqual(model.collections.flatMap(\.requests).count, before + 1)
        XCTAssertEqual(model.collections.flatMap(\.requests).first { $0.url == "https://api.example/users?page=2" }?.method, "GET")
        nav.receive(.text("curl https://api.example/x"), model)
        XCTAssertTrue(nav.showRequestFromText)
        XCTAssertEqual(nav.requestText, "curl https://api.example/x")
        nav.receive(.text("{\"openapi\": \"3.0.0\"}"), model)
        XCTAssertEqual(nav.importing?.text, "{\"openapi\": \"3.0.0\"}")
        nav.receive(.text("file:///etc/passwd"), model)
        XCTAssertEqual(nav.requestText, "file:///etc/passwd")
        XCTAssertEqual(model.collections.flatMap(\.requests).count, before + 1, "nothing but the address became a request")
    }
}

/// A vault that stops answering (the Keychain while it can't be read).
final class BrokenVault: SecretVault, @unchecked Sendable {
    struct Locked: Error {}
    let inner = MemoryVault()
    var broken = false

    func value(environment: String, name: String) -> String? { broken ? nil : inner.value(environment: environment, name: name) }
    func read(environment: String, name: String) throws -> String? {
        if broken { throw Locked() }
        return inner.value(environment: environment, name: name)
    }
    func setValue(_ value: String?, environment: String, name: String) { inner.setValue(value, environment: environment, name: name) }
}

/// A sync's own files that can't be used: nothing is uploaded or lost by it.
@MainActor
final class SyncFileTests: XCTestCase {
    private var root: URL!
    private var fake: FakeCloud!
    private var library: MemoryLibraryStore!
    private var vault: BrokenVault!
    private var cloud: HitopCloud!
    private var model: AppModel!
    private var sync: LibrarySync!
    private var shared: SharedSync!
    private let keys = MemorySecretStore()
    private let settings = MemoryCloudSettingsStore(CloudSettings(server: FakeCloud.server))

    override func setUp() async throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("syncfiles-\(UUID().uuidString)")
        fake = FakeCloud()
        library = MemoryLibraryStore()
        vault = BrokenVault()
        launch()
    }

    override func tearDown() async throws { try? FileManager.default.removeItem(at: root) }

    private func launch() {
        model = AppModel(store: library, vault: vault)
        model.load()
        cloud = HitopCloud(transport: fake, credentials: CloudCredentials(store: keys), settingsStore: settings)
        sync = LibrarySync(cloud: cloud, model: model, stateStore: FileLibrarySyncStateStore(root: root))
        shared = SharedSync(cloud: cloud, model: model, store: FileSharedSyncStore(root: root))
        cloud.onAccountChange = nil
        sync.pushDelay = .seconds(3600)
        sync.retryDelay = .seconds(3600)
    }

    private func signIn() async throws {
        let start = try await cloud.startBrowserSignIn("google", server: FakeCloud.server)
        let code = URLComponents(url: start.url, resolvingAgainstBaseURL: false)!.queryItems!.first { $0.name == "code" }!.value!
        try await cloud.finishBrowserSignIn(code)
        await sync.sync()
    }

    private func collection(_ name: String) -> SavedCollection {
        var c = SavedCollection(name: name)
        c.requests = [SavedRequest(name: "x", url: "https://api.example/x", method: "GET")]
        return c
    }

    private var uploaded: [String] { sync.state.known.keys.filter { $0.hasPrefix("collection/") } }

    /// Two collections: one in the library, one shared (end-to-end encrypted).
    private func both() async throws -> (plain: UInt64, secret: UInt64) {
        let plain = collection("plain")
        try library.saveCollection(plain)
        launch()
        try await signIn()
        XCTAssertTrue(uploaded.contains("collection/\(plain.id)"))
        let secret = collection("secret")
        try library.saveCollection(secret)
        var state = try FileSharedSyncStore(root: root).load()
        state.account = cloud.accountKey
        state.links[String(secret.id)] = SharedLink(workspace: "ws", role: "owner")
        try FileSharedSyncStore(root: root).save(state)
        launch()
        XCTAssertTrue(model.isShared(secret.id))
        return (plain.id, secret.id)
    }

    func testADamagedListOfSharedCollectionsUploadsNone() async throws {
        let (plain, secret) = try await both()
        try Data("{".utf8).write(to: root.appendingPathComponent("shared-sync.json"))
        launch()
        await sync.push()
        XCTAssertFalse(uploaded.contains("collection/\(secret)"))
        XCTAssertTrue(uploaded.contains("collection/\(plain)"))
        XCTAssertTrue(sync.isLocalOnly(secret))
        XCTAssertFalse(sync.isLocalOnly(plain))
        XCTAssertNil(model.sharingProblem)
        XCTAssertFalse(shared.notices.isEmpty)
        launch()
        await sync.sync()
        XCTAssertFalse(uploaded.contains("collection/\(secret)"), "nor after another start")
        XCTAssertNotNil(model.collection(secret))
    }

    func testKeysAnotherVersionDidNotWrite() throws {
        let d = JSONDecoder()
        let s = try d.decode(SharedState.self, from: Data(#"{"links":{"1":{"workspace":"w","role":"viewer"}}}"#.utf8))
        XCTAssertEqual(s.links["1"], SharedLink(workspace: "w", role: "viewer"))
        XCTAssertThrowsError(try d.decode(SharedState.self, from: Data("{}".utf8)))
        XCTAssertThrowsError(try d.decode(SharedState.self, from: Data(#"{"links":[]}"#.utf8)))
        XCTAssertEqual(try d.decode(LibrarySync.State.self, from: Data(#"{"account":"a"}"#.utf8)), LibrarySync.State(account: "a"))
        XCTAssertThrowsError(try d.decode(LibrarySync.State.self, from: Data(#"{"account":"a","localOnly":1}"#.utf8)))
    }

    func testAListThatCannotBeOpenedWaits() async throws {
        let (_, secret) = try await both()
        let file = root.appendingPathComponent("shared-sync.json")
        let kept = try Data(contentsOf: file)
        // A folder in its place: there, but not to be read.
        try FileManager.default.removeItem(at: file)
        try FileManager.default.createDirectory(at: file, withIntermediateDirectories: true)
        launch()
        XCTAssertEqual(model.sharingProblem, .unreadable)
        await sync.sync()
        XCTAssertFalse(uploaded.contains("collection/\(secret)"))
        XCTAssertFalse(sync.isLocalOnly(secret))
        try FileManager.default.removeItem(at: file)
        try kept.write(to: file)
        await sync.sync()
        XCTAssertNil(model.sharingProblem)
        XCTAssertTrue(model.isShared(secret))
        XCTAssertFalse(uploaded.contains("collection/\(secret)"))
    }

    func testADamagedLibraryStateUploadsNothingByItself() async throws {
        let (plain, secret) = try await both()
        let mine = collection("only here")
        try library.saveCollection(mine)
        try Data("{".utf8).write(to: root.appendingPathComponent("library-sync.json"))
        launch()
        await sync.sync()
        XCTAssertTrue(uploaded.contains("collection/\(plain)"), "the library brought it back")
        XCTAssertFalse(uploaded.contains("collection/\(mine.id)"))
        XCTAssertFalse(uploaded.contains("collection/\(secret)"))
    }

    func testSecretsThatCannotBeReadAreNotPushedAsGone() async throws {
        var env = SavedEnvironment(id: "e1", name: "Prod")
        env.secrets = ["token"]
        try library.saveEnvironments([env])
        vault.setValue("s3cret-value", environment: "e1", name: "token")
        launch()
        try await signIn()
        let known = sync.state.known["environments/all"]
        XCTAssertNotNil(known)
        vault.broken = true
        await sync.push()
        XCTAssertEqual(sync.state.known["environments/all"], known)
        vault.broken = false
        await sync.push()
        XCTAssertEqual(sync.state.known["environments/all"], known)
    }
}
