import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
#if canImport(Security)
import Security
#endif
import HitopKit
import XCTest
@testable import HITOP

@MainActor
final class SettingsTests: XCTestCase {
    private var root: URL!
    private var caPEM: String { SettingsFixtures.ca }

    override func setUp() async throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("settings-\(UUID().uuidString)")
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: root)
        _ = try? SyncCore.call("networkRoots", ["roots": .array([])])
    }

    func testSettingsAreKeptAndAWrongFileStopsRequests() async throws {
        let s = SettingsStore(root: root)
        XCTAssertEqual(s.value.timeout, 60)
        let found = try SettingsStore.authorities(in: Data(caPEM.utf8), name: "ca.pem")
        XCTAssertEqual(found.count, 1)
        try s.update { $0.timeout = 10; $0.authorities = found }
        XCTAssertEqual(SettingsStore(root: root).value, s.value)
        var sender = Sender(configuration: StubURLProtocol.configuration)
        sender.settings = s
        XCTAssertEqual(sender.limit, 10)

        try Data("{".utf8).write(to: root.appendingPathComponent("settings.json"))
        let damaged = SettingsStore(root: root)
        XCTAssertNotNil(damaged.problem)
        XCTAssertThrowsError(try damaged.update { $0.timeout = 30 })
        sender.settings = damaged
        let r = await sender.send(SavedRequest(name: "x", url: "https://api.shop.example/x", method: "GET"), collection: nil, context: VariableContext())
        guard case let .failure(f) = r.outcome else { return XCTFail("was sent") }
        XCTAssertTrue(f.message.contains("settings can't be read"), f.message)
        try damaged.reset()
        XCTAssertNil(damaged.problem)
        XCTAssertNil(SettingsStore(root: root).problem)
    }

    func testOnlyCertificatesAreTaken() throws {
        for bad in ["nothing", "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----", "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----"] {
            XCTAssertThrowsError(try SettingsStore.authorities(in: Data(bad.utf8), name: "f"), bad)
        }
        let s = SettingsStore(root: root)
        XCTAssertThrowsError(try s.update { $0.authorities = [.init(name: "x", der: "AAAA")] })
        XCTAssertEqual(s.value, AppSettings())
        XCTAssertEqual(try SettingsStore.authorities(in: Data((caPEM + "\n" + caPEM).utf8), name: "f").count, 2)
    }

    func testHosts() {
        XCTAssertEqual(AppSettings.host(" https://API.example:8443/x "), "api.example")
        XCTAssertEqual(AppSettings.host("api.example"), "api.example")
        XCTAssertNil(AppSettings.host("  "))
    }

    #if canImport(Security)
    private func trust(_ host: String) throws -> SecTrust {
        func cert(_ pem: String) throws -> SecCertificate {
            let b64 = pem.split(separator: "\n").filter { !$0.hasPrefix("-----") }.joined()
            return try XCTUnwrap(SecCertificateCreateWithData(nil, XCTUnwrap(Data(base64Encoded: b64)) as CFData))
        }
        var t: SecTrust?
        SecTrustCreateWithCertificates([try cert(SettingsFixtures.leaf)] as CFArray, SecPolicyCreateSSL(true, host as CFString), &t)
        let trust = try XCTUnwrap(t)
        SecTrustSetVerifyDate(trust, SettingsFixtures.validAt as CFDate)
        return trust
    }

    func testAnAddedAuthorityIsTrustedForItsNamesOnly() throws {
        let ca = try SettingsStore.authorities(in: Data(caPEM.utf8), name: "ca")
        XCTAssertTrue(TLSDelegate.trusts(try trust("tls.test"), ca))
        XCTAssertFalse(TLSDelegate.trusts(try trust("other.test"), ca), "the name is still checked")
        XCTAssertFalse(TLSDelegate.trusts(try trust("tls.test"), []))
        let other = try SettingsStore.authorities(in: Data(SettingsFixtures.otherCA.utf8), name: "other")
        XCTAssertFalse(TLSDelegate.trusts(try trust("tls.test"), other))
    }
    #endif
}
