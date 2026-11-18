import Foundation

extension WidgetSnapshot.Run {
    init(_ r: RunRecord) {
        self.init(collectionID: String(r.collectionID), collectionName: r.collectionName, environment: r.environmentName, passed: r.passed,
                  requestsPassed: r.requestsPassed, requestsTotal: r.requestsTotal, assertionsPassed: r.assertionsPassed,
                  assertionsTotal: r.assertionsTotal, durationMs: r.durationMs, finishedAt: r.date, firstFailure: r.firstFailure)
    }
}

extension WidgetSnapshot {
    /// What the app writes for its widgets and intents (names, counts and statuses only).
    @MainActor
    static func make(model: AppModel, runs: [RunRecord], now: Date = Date()) -> WidgetSnapshot {
        let env = model.activeEnvironment.map { Environment(id: $0.id, name: $0.name) }
        let lastSend: [UInt64: HistoryEntry] = model.history.reduce(into: [:]) { m, h in if m[h.request.id] == nil { m[h.request.id] = h } }
        func row(_ r: SavedRequest, _ c: SavedCollection) -> Request {
            let h = lastSend[r.id]
            return Request(id: String(r.id), name: r.name, method: Self.methodLabel(r), collection: c.name, lastStatus: h?.response?.status,
                           lastFailed: h.map { $0.error != nil || ($0.response?.status ?? 0) >= 400 } ?? false,
                           lastAt: h.map { Date(timeIntervalSince1970: Double($0.timestamp) / 1000) })
        }
        let collections = model.visibleCollections.map { c in
            Collection(id: String(c.id), name: c.name, requestCount: c.requests.count, lastRun: runs.first { $0.collectionID == c.id }.map(Run.init))
        }
        let pinned = model.state.pinnedRequests.compactMap { id -> Request? in
            guard let r = model.request(id), let c = model.collectionOf(request: id) else { return nil }
            return row(r, c)
        }
        let all = model.visibleCollections.flatMap { c in c.requests.map { row($0, c) } }
        return WidgetSnapshot(updatedAt: now, activeEnvironment: env,
                              environments: model.environments.map { Environment(id: $0.id, name: $0.name) },
                              collections: collections, pinned: pinned, requests: Array(all.prefix(requestLimit)),
                              lastRun: runs.first.map(Run.init))
    }

    /// The method label of a saved request, as the app's rows show it (WS, GQL...).
    static func methodLabel(_ r: SavedRequest) -> String {
        switch r.kind {
        case "websocket": "WS"
        case "grpc": "GRPC"
        default: r.bodyType == "graphql" ? "GQL" : r.method.uppercased()
        }
    }
}
