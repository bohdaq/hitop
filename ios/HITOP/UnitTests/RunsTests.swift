import Foundation
import XCTest
import HitopKit
@testable import HITOP

/// Collection runs (I3): the runner on the stubbed shop-api, report.json in HitopKit's shape, the
/// run history on disk, the widget snapshot, shared-collection notices in words.
@MainActor
final class RunsTests: XCTestCase {
    var model: AppModel!
    var root: URL!
    var center: RunCenter!

    override func setUp() async throws {
        StubURLProtocol.enabled = true
        root = FileManager.default.temporaryDirectory.appendingPathComponent("hitop-runs-\(UUID().uuidString)")
        model = AppModel(store: MemoryLibraryStore(), vault: MemoryVault(), sender: Sender(configuration: StubURLProtocol.configuration))
        model.load()
        model.seed()
        center = RunCenter(model: model, store: FileRunStore(root: root))
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: root)
    }

    var shop: SavedCollection { model.collections.first { $0.name == "shop-api" }! }

    /// The secret values the seed keeps in the vault.
    let secretValues = Seed.secrets.map(\.2)

    func runShop(_ settings: RunSettings) async throws -> RunRecord {
        let r = await center.start(collectionID: shop.id, settings: settings)
        return try XCTUnwrap(r)
    }

    func staging(bail: Bool = false, iterations: Int = 1) -> RunSettings {
        RunSettings(environmentID: "env-staging", bail: bail, iterations: iterations)
    }

    /// The shop-api in the run screen's numbers: 13 requests, the WebSocket one left out, Cancel
    /// order answered 409.
    func testRunningShopAPIInStaging() async throws {
        var events: [String] = []
        center.onEvent = { e in
            switch e {
            case .started: events.append("started")
            case .progress: events.append("progress")
            case .finished: events.append("finished")
            }
        }
        let record = try await runShop(staging())
        XCTAssertEqual(record.requestsTotal, 12)
        XCTAssertEqual(record.requestsPassed, 11)
        XCTAssertFalse(record.passed)
        XCTAssertFalse(record.bailed)
        XCTAssertEqual(record.environmentName, "Staging")
        XCTAssertEqual(record.firstFailure, "Cancel order: HTTP 409")
        XCTAssertEqual(events.first, "started")
        XCTAssertEqual(events.last, "finished")
        XCTAssertEqual(events.filter { $0 == "progress" }.count, 12)
        XCTAssertNil(center.active)
        XCTAssertEqual(center.runs.first, record)
        XCTAssertEqual(center.failing.map(\.collectionID), [shop.id])

        let report = try XCTUnwrap(record.report)
        XCTAssertEqual(report.reportVersion, 1)
        XCTAssertEqual(report.tool.name, "hitop")
        XCTAssertEqual(report.summary.requests, RunReport.Counts(total: 12, passed: 11, failed: 1))
        XCTAssertEqual(report.collections.map(\.environment), ["Staging"])
        XCTAssertFalse(report.requests.map(\.name).contains("Order events"))
        let cancel = try XCTUnwrap(report.requests.first { $0.name == "Cancel order" })
        XCTAssertEqual(cancel.status, 409)
        XCTAssertEqual(cancel.failure, "HTTP 409")
        XCTAssertNil(cancel.file, "not a folder collection")
        XCTAssertNotNil(report.p95)
        #if canImport(JavaScriptCore)
        // Scripts run (JavaScriptCore): Create order's four tests and List orders' one.
        XCTAssertEqual(report.summary.assertions.total, 5)
        XCTAssertEqual(report.requests.first { $0.name == "Create order" }?.assertions.count, 4)
        #else
        XCTAssertFalse(report.requests.first { $0.name == "Create order" }?.notes?.isEmpty ?? true, "no script engine here: noted")
        #endif
    }

    /// report.json is HitopKit's text unchanged (what `hitop run --reporter json` writes), with the
    /// keys of spec/run-report.md.
    func testTheReportIsHitopKitsReportJSON() async throws {
        let record = try await runShop(staging())
        let any = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(record.reportJSON.utf8)) as? [String: Any])
        XCTAssertEqual(Set(any.keys), ["reportVersion", "tool", "startedAt", "durationMs", "passed", "bailed", "summary", "collections"])
        XCTAssertEqual(any["reportVersion"] as? Int, 1)
        XCTAssertTrue(record.reportJSON.hasSuffix("\n"))
        let collections = try XCTUnwrap(any["collections"] as? [[String: Any]])
        let iterations = try XCTUnwrap(collections.first?["iterations"] as? [[String: Any]])
        let req = try XCTUnwrap((iterations.first?["requests"] as? [[String: Any]])?.first)
        for key in ["name", "method", "url", "passed", "status", "responseTimeMs", "durationMs", "error", "failure", "assertions", "file"] {
            XCTAssertTrue(req.keys.contains(key), key)
        }
        // The reader and the text agree.
        XCTAssertEqual(record.report?.summary.requests.total, record.requestsTotal)
    }

    /// The reader takes the compatibility suite's run report (spec/compat/v1/run-report/report.json).
    func testTheReaderTakesTheSpecsRunReport() throws {
        let report = try RunReport.decode(try Self.specReport())
        XCTAssertEqual(report.reportVersion, 1)
        XCTAssertTrue(report.bailed)
        XCTAssertEqual(report.summary.collections, 2)
        XCTAssertEqual(report.summary.requests.total, 6)
        XCTAssertEqual(report.requests.count, 6)
    }

    static func specReport() throws -> String {
        var places: [URL] = []
        if let d = ProcessInfo.processInfo.environment["HITOP_RUN_REPORT_DIR"], !d.isEmpty { places.append(URL(fileURLWithPath: d)) }
        for b in Bundle.allBundles + [Bundle(for: RunsTests.self)] {
            if let u = b.url(forResource: "run-report", withExtension: nil) { places.append(u) }
        }
        let here = URL(fileURLWithPath: #filePath).resolvingSymlinksInPath()
        places.append(here.deletingLastPathComponent().appendingPathComponent("../../../spec/compat/v1/run-report").standardizedFileURL)
        for p in places {
            if let t = try? String(contentsOf: p.appendingPathComponent("report.json"), encoding: .utf8) { return t }
        }
        struct Missing: Error {}
        XCTFail("no spec/compat/v1/run-report/report.json: set HITOP_RUN_REPORT_DIR (core-tests.sh does) or bundle it")
        throw Missing()
    }

    func testNoSecretValueInTheHistoryOrTheReport() async throws {
        let record = try await runShop(staging())
        let file = try String(contentsOf: root.appendingPathComponent("runs.json"), encoding: .utf8)
        for v in secretValues {
            XCTAssertFalse(record.reportJSON.contains(v), v)
            XCTAssertFalse(file.contains(v), v)
        }
    }

    func testBailStopsAtTheFirstFailure() async throws {
        let record = try await runShop(staging(bail: true))
        XCTAssertTrue(record.bailed)
        XCTAssertEqual(record.requestsTotal, 7, "Log in … Cancel order (the WebSocket one left out)")
        XCTAssertEqual(record.report?.requests.last?.name, "Cancel order")
    }

    func testIterations() async throws {
        var totals: Set<Int> = []
        center.onEvent = { e in if case let .progress(r) = e { totals.insert(r.progress.total) } }
        let record = try await runShop(staging(iterations: 2))
        XCTAssertEqual(totals, [24])
        XCTAssertEqual(record.iterations, 2)
        XCTAssertEqual(record.requestsTotal, 24)
        XCTAssertEqual(record.report?.collections.first?.iterations.map(\.iteration), [1, 2])
    }

    func testWithoutAnEnvironmentTheRequestsFail() async throws {
        let record = try await runShop(RunSettings(environmentID: nil))
        XCTAssertNil(record.environmentName)
        XCTAssertEqual(record.requestsPassed, 0)
        XCTAssertTrue(record.report?.requests.first?.error?.contains("baseUrl") ?? false, "\(String(describing: record.report?.requests.first))")
    }

    /// A viewer of a shared collection can run it; nothing of the collection changes.
    func testAViewerCanRun() async throws {
        model.sharedRoles[shop.id] = "viewer"
        let before = shop
        let record = await center.start(collectionID: shop.id, settings: staging())
        XCTAssertNotNil(record)
        XCTAssertEqual(shop, before)
    }

    func testHistoryIsCappedAndReloads() async throws {
        let one = try await runShop(staging(bail: true))
        var runs = Array(repeating: one, count: FileRunStore.limit + 5)
        for i in runs.indices { runs[i].id = UInt64(i + 1) }
        try FileRunStore(root: root).save(runs)
        let again = RunCenter(model: model, store: FileRunStore(root: root))
        XCTAssertEqual(again.runs.count, FileRunStore.limit)
        XCTAssertEqual(again.runs.first?.reportJSON, one.reportJSON)
        again.delete(again.runs[0].id)
        XCTAssertEqual(RunCenter(model: model, store: FileRunStore(root: root)).runs.count, FileRunStore.limit - 1)
        again.clear()
        XCTAssertEqual(FileRunStore(root: root).load(), [])
    }

    func testOneRunAtATime() async throws {
        let shopID = shop.id
        let first = Task { await center.start(collectionID: shopID, settings: staging()) }
        while center.active == nil { await Task.yield() }
        let second = await center.start(collectionID: shopID, settings: staging())
        let done = await first.value
        XCTAssertNotNil(done)
        XCTAssertNil(second)
    }

    // MARK: Widget snapshot

    func testWidgetSnapshot() async throws {
        let record = try await runShop(staging())
        let snap = WidgetSnapshot.make(model: model, runs: center.runs, now: Date(timeIntervalSince1970: 1_000))
        XCTAssertEqual(snap.activeEnvironment?.name, "Staging")
        XCTAssertEqual(snap.environments.map(\.name), ["Staging", "Production", "Local"])
        XCTAssertEqual(snap.pinned.map(\.name), ["Create order"])
        XCTAssertEqual(snap.pinned.first?.method, "POST")
        XCTAssertEqual(snap.pinned.first?.lastStatus, 201, "the seeded history's last send")
        XCTAssertEqual(snap.lastRun?.collectionID, String(shop.id))
        XCTAssertEqual(snap.lastRun?.summary, "11 of 12 passed")
        XCTAssertEqual(snap.failing.map(\.name), ["shop-api"])
        XCTAssertEqual(snap.collections.first { $0.name == "shop-api" }?.lastRun, WidgetSnapshot.Run(record))
        XCTAssertEqual(snap.requests.first { $0.name == "Order events" }?.method, "WS")

        let dir = root.appendingPathComponent("group")
        try SharedContainer.write(snap, to: dir)
        XCTAssertEqual(SharedContainer.read(from: dir), snap)
        let text = try String(contentsOf: dir.appendingPathComponent(SharedContainer.snapshotFile), encoding: .utf8)
        for v in secretValues + ["staging.shop.example"] { XCTAssertFalse(text.contains(v), v) }
    }

    func testNoContainerNoSnapshot() {
        #if !canImport(Darwin)
        XCTAssertNil(SharedContainer.directory())
        #endif
        XCTAssertNil(SharedContainer.read(from: root.appendingPathComponent("missing")))
    }

    // MARK: Shared notices

    func testKeyNoticesNameTheDeviceNeverItsKeyID() {
        let names = ["key6": "Grace's Mac"]
        let pinned = SharedSync.describe(SyncCore.KeyNotice(kind: "senderPinned", senderKeyId: "key6", fingerprint: "61e8 0c2a",
                                                            message: "trusted device key6 (fingerprint 61e8 0c2a): it is the one your invite code named"), names: names)
        XCTAssertEqual(pinned, SharedNotice("Trusted Grace's Mac — the device your invite code named", detail: "Fingerprint 61e8 0c2a"))
        for kind in ["senderPinned", "senderKeyChanged", "knownKeyFromUntrusted", "untrustedIgnored", "legacyAccepted", "legacyIgnored", "somethingNew"] {
            for n in [names, [:]] {
                let d = SharedSync.describe(SyncCore.KeyNotice(kind: kind, epoch: 2, senderKeyId: "key6", fingerprint: "61e8", message: "device key6 did a thing"), names: n)
                XCTAssertFalse(d.text.contains("key6"), "\(kind): \(d.text)")
            }
        }
    }
}
