import ActivityKit
import Foundation

// Compiled into the app and the widget extension (project.yml): the Live Activity of a collection
// run. The app starts, updates and ends it (`SystemBridge`); the extension draws it.

struct RunActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable, Sendable {
        var done: Int
        var total: Int
        var passed: Int
        var failed: Int
        /// The request done last.
        var current: String?
        var elapsedMs: Int
        /// The run is over (passed = done − failed).
        var finished: Bool

        var fraction: Double { total == 0 ? 0 : min(1, Double(done) / Double(total)) }

        /// "1.4 s".
        var elapsed: String { RunActivityAttributes.seconds(elapsedMs) }
    }

    var collectionID: String
    var collectionName: String
    var environment: String?

    static func seconds(_ ms: Int) -> String {
        ms < 1000 ? "\(ms) ms" : String(format: "%.1f s", Double(ms) / 1000)
    }

    /// The drafts' example: shop-api in Staging, 8 of 13.
    static let sample = RunActivityAttributes(collectionID: "1", collectionName: "shop-api", environment: "Staging")
    static let sampleState = ContentState(done: 8, total: 13, passed: 8, failed: 0, current: "Create order", elapsedMs: 1400, finished: false)
}
