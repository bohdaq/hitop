import Foundation

/// The old App Store app (ios/HiTop, "HiTop HTTP Client" 1.0) kept its data in Application
/// Support/hitop: `collections.json` (`[{id, name, requests: [{id, name, url, method, headers:
/// [{name, value}], body, preRequestScript, postRequestScript}], variables}]`), `history.json`
/// (`[{id, timestamp, url, method, statusCode, success, collectionName, requestName, durationMs,
/// headers, body, preRequestScript, postRequestScript}]`) and `contexts.json` (script-set values
/// by collection id). This app is its update (same bundle id, same container), so on first launch
/// it reads them once. The old files are left as they are.
enum LegacyImport {
    struct Result: Equatable {
        var collections: [SavedCollection] = []
        var history: [HistoryEntry] = []
    }

    static func defaultDirectory() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return base.appendingPathComponent("hitop", isDirectory: true)
    }

    private struct OldHistoryItem: Decodable {
        var id: UInt64?
        var timestamp: UInt64?
        var url: String?
        var method: String?
        var statusCode: Int?
        var success: Bool?
        var collectionName: String?
        var requestName: String?
        var durationMs: Int?
        var headers: [KeyValue]?
        var body: String?
        var preRequestScript: String?
        var postRequestScript: String?
    }

    static func read(from dir: URL) -> Result {
        var out = Result()
        let dec = JSONDecoder()
        if let data = try? Data(contentsOf: dir.appendingPathComponent("collections.json")),
           var collections = try? dec.decode([SavedCollection].self, from: data) {
            // Script-set values (pm.variables) the old app kept per collection become collection
            // variables where the collection doesn't define them.
            if let ctxData = try? Data(contentsOf: dir.appendingPathComponent("contexts.json")),
               let contexts = try? dec.decode([String: [String: String]].self, from: ctxData) {
                for i in collections.indices {
                    for (k, v) in contexts[String(collections[i].id)] ?? [:] where collections[i].variables[k] == nil {
                        collections[i].variables[k] = v
                    }
                }
            }
            out.collections = collections
        }
        if let data = try? Data(contentsOf: dir.appendingPathComponent("history.json")),
           let items = try? dec.decode([OldHistoryItem].self, from: data) {
            // The old app appended (oldest first); this app keeps newest first.
            out.history = items.reversed().map { o in
                let method = (o.method ?? "GET").uppercased()
                let url = o.url ?? ""
                let request = SavedRequest(name: o.requestName ?? AutoName.name(method: method, url: url), url: url, method: method,
                                           headers: (o.headers ?? []).filter { !$0.name.isEmpty }, body: o.body ?? "",
                                           preRequestScript: o.preRequestScript ?? "", postRequestScript: o.postRequestScript ?? "")
                let response = o.statusCode.map { ResponseSnapshot(status: $0, headers: [], body: "", truncated: false, durationMs: o.durationMs ?? 0, size: 0, tests: []) }
                return HistoryEntry(id: o.id ?? IDs.next(), timestamp: o.timestamp ?? 0, collectionName: o.collectionName, request: request,
                                    response: response?.status == 0 ? nil : response, error: o.success == false && (o.statusCode ?? 0) == 0 ? "The request failed" : nil)
            }
        }
        return out
    }
}
