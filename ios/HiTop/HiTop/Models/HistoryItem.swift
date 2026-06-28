import Foundation

struct HistoryItem: Codable, Identifiable {
    var id: UInt64
    var timestamp: UInt64
    var url: String
    var method: String
    var statusCode: UInt16
    var success: Bool
    var collectionName: String?
    var requestName: String?
    var durationMs: UInt64
    // Full request data for reload
    var headers: [Header]
    var body: String
    var preRequestScript: String
    var postRequestScript: String

    var date: Date {
        Date(timeIntervalSince1970: TimeInterval(timestamp) / 1000)
    }
}
