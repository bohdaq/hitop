import XCTest
@testable import HITOP

/// hitop-core's gRPC client through SyncFFI runs here too (the simulator's iOS slice, Linux/macOS
/// in `core-tests.sh`), not only links: a closed port is a clean error, never a crash.
final class GrpcFFITests: XCTestCase {
    func testAClosedPortIsACleanError() {
        let server: JSONAny = .object(["address": .string("127.0.0.1:1"), "plaintext": .bool(true), "timeoutMs": .number(3000)])
        XCTAssertThrowsError(try SyncCore.call("grpcList", ["server": server])) { e in
            XCTAssertFalse((e as? SyncCore.Failure)?.message.isEmpty ?? true)
        }
    }
}
