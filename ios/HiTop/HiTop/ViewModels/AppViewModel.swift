import Foundation
import Observation
import SwiftUI

@MainActor
@Observable
final class AppViewModel {
    // MARK: - Collections
    var collections: [Collection] = []

    // MARK: - Active request state
    var activeRequestId: UInt64? = nil
    var activeCollectionId: UInt64? = nil
    var activeURL: String = ""
    var activeMethod: String = "GET"
    var activeHeaders: [Header] = [Header()]
    var activeBody: String = ""
    var activePreScript: String = ""
    var activePostScript: String = ""

    // MARK: - Response state
    var response: HTTPResponse? = nil
    var responseError: String? = nil
    var isLoading: Bool = false
    // Incremented each time a response arrives so views can observe it
    var responseReceived: Int = 0

    // MARK: - History
    var history: [HistoryItem] = []

    // MARK: - Context (per-collection runtime key-value)
    var contexts: [String: [String: String]] = [:]

    // MARK: - Navigation
    var selectedRequestId: UInt64? = nil
    var selectedCollectionId: UInt64? = nil

    // MARK: - Run collection
    var runResults: [RunResult] = []
    var isRunning: Bool = false

    private let storage = StorageService.shared
    private let http = HTTPService.shared
    private let scripts = ScriptService.shared

    // MARK: - Load / save

    func loadData() {
        let saved = storage.loadCollections()
        if saved.isEmpty {
            let defaultCollection = Collection(name: "Default", requests: [HTTPRequest()])
            collections = [defaultCollection]
        } else {
            collections = saved
        }
        history = storage.loadHistory()
        contexts = storage.loadContexts()

        // Auto-select first request so the detail pane is not empty on launch
        if let first = collections.first, let req = first.requests.first {
            loadRequest(req, collectionId: first.id)
        }
    }

    func persistCollections() {
        storage.saveCollections(collections)
    }

    func persistHistory() {
        storage.saveHistory(history)
    }

    func persistContexts() {
        storage.saveContexts(contexts)
    }

    // MARK: - Collection CRUD

    func addCollection(name: String) {
        let col = Collection(name: name.isEmpty ? "New Collection" : name)
        collections.append(col)
        persistCollections()
    }

    func renameCollection(id: UInt64, name: String) {
        guard let idx = collections.firstIndex(where: { $0.id == id }) else { return }
        collections[idx].name = name
        persistCollections()
    }

    func deleteCollection(id: UInt64) {
        collections.removeAll { $0.id == id }
        if activeCollectionId == id { clearActiveRequest() }
        persistCollections()
    }

    func duplicateCollection(id: UInt64) {
        guard var col = collections.first(where: { $0.id == id }) else { return }
        col.id = IDGenerator.next()
        col.name = col.name + " (Copy)"
        col.requests = col.requests.map { req in
            var r = req; r.id = IDGenerator.next(); return r
        }
        if let idx = collections.firstIndex(where: { $0.id == id }) {
            collections.insert(col, at: idx + 1)
        } else {
            collections.append(col)
        }
        persistCollections()
    }

    // MARK: - Request CRUD

    func addRequest(to collectionId: UInt64) {
        guard let idx = collections.firstIndex(where: { $0.id == collectionId }) else { return }
        let req = HTTPRequest()
        collections[idx].requests.append(req)
        persistCollections()
        loadRequest(req, collectionId: collectionId)
    }

    func deleteRequest(id: UInt64, collectionId: UInt64) {
        guard let cIdx = collections.firstIndex(where: { $0.id == collectionId }) else { return }
        collections[cIdx].requests.removeAll { $0.id == id }
        if activeRequestId == id { clearActiveRequest() }
        persistCollections()
    }

    func renameRequest(id: UInt64, collectionId: UInt64, name: String) {
        guard let cIdx = collections.firstIndex(where: { $0.id == collectionId }),
              let rIdx = collections[cIdx].requests.firstIndex(where: { $0.id == id }) else { return }
        collections[cIdx].requests[rIdx].name = name
        persistCollections()
    }

    func moveRequest(collectionId: UInt64, fromOffsets: IndexSet, toOffset: Int) {
        guard let idx = collections.firstIndex(where: { $0.id == collectionId }) else { return }
        collections[idx].requests.move(fromOffsets: fromOffsets, toOffset: toOffset)
        persistCollections()
    }

    // MARK: - Load request into editor

    func loadRequest(_ request: HTTPRequest, collectionId: UInt64) {
        // Auto-save current request before switching
        saveActiveRequest()

        activeRequestId = request.id
        activeCollectionId = collectionId
        activeURL = request.url
        activeMethod = request.method
        activeHeaders = request.headers.isEmpty ? [Header()] : request.headers
        activeBody = request.body
        activePreScript = request.preRequestScript
        activePostScript = request.postRequestScript
        response = nil
        responseError = nil
        selectedRequestId = request.id
        selectedCollectionId = collectionId
    }

    func clearActiveRequest() {
        saveActiveRequest()
        activeRequestId = nil
        activeCollectionId = nil
        activeURL = ""
        activeMethod = "GET"
        activeHeaders = [Header()]
        activeBody = ""
        activePreScript = ""
        activePostScript = ""
        response = nil
        responseError = nil
        selectedRequestId = nil
    }

    // MARK: - Save active request back to collection

    func saveActiveRequest() {
        guard let cId = activeCollectionId,
              let rId = activeRequestId,
              let cIdx = collections.firstIndex(where: { $0.id == cId }),
              let rIdx = collections[cIdx].requests.firstIndex(where: { $0.id == rId }) else { return }

        collections[cIdx].requests[rIdx].url = activeURL
        collections[cIdx].requests[rIdx].method = activeMethod
        collections[cIdx].requests[rIdx].headers = activeHeaders.filter { !$0.name.isEmpty }
        collections[cIdx].requests[rIdx].body = activeBody
        collections[cIdx].requests[rIdx].preRequestScript = activePreScript
        collections[cIdx].requests[rIdx].postRequestScript = activePostScript
        persistCollections()
    }

    // MARK: - Variables

    func variables(for collectionId: UInt64?) -> [String: String] {
        guard let id = collectionId,
              let col = collections.first(where: { $0.id == id }) else { return [:] }
        return col.variables
    }

    func updateVariables(_ vars: [String: String], for collectionId: UInt64) {
        guard let idx = collections.firstIndex(where: { $0.id == collectionId }) else { return }
        collections[idx].variables = vars
        persistCollections()
    }

    // MARK: - Context

    func context(for collectionId: UInt64?) -> [String: String] {
        guard let id = collectionId else { return [:] }
        return contexts[String(id)] ?? [:]
    }

    func updateContext(_ updates: [String: String], for collectionId: UInt64) {
        let key = String(collectionId)
        var ctx = contexts[key] ?? [:]
        for (k, v) in updates { ctx[k] = v }
        contexts[key] = ctx
        persistContexts()
    }

    // MARK: - Send request

    func sendRequest() async {
        guard !activeURL.isEmpty else { return }

        let vars = variables(for: activeCollectionId)
        let ctx = context(for: activeCollectionId)

        var finalURL = InterpolationService.interpolate(activeURL, variables: vars)
        var finalHeaders = InterpolationService.interpolateHeaders(activeHeaders, variables: vars)
        var finalBody = InterpolationService.interpolate(activeBody, variables: vars)

        // Pre-request script
        if !activePreScript.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            do {
                let result = try scripts.executePreRequest(
                    script: activePreScript,
                    url: finalURL,
                    method: activeMethod,
                    headers: finalHeaders,
                    body: finalBody,
                    variables: vars,
                    context: ctx
                )
                finalURL = result.url
                finalHeaders = result.headers
                finalBody = result.body
                if let id = activeCollectionId {
                    updateContext(result.contextUpdates, for: id)
                }
            } catch {
                responseError = "Pre-request script error: \(error.localizedDescription)"
                return
            }
        }

        isLoading = true
        response = nil
        responseError = nil

        do {
            let result = try await http.send(
                url: finalURL,
                method: activeMethod,
                headers: finalHeaders,
                body: finalBody
            )

            // Post-request script
            if !activePostScript.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                let currentCtx = context(for: activeCollectionId)
                if let updates = try? scripts.executePostRequest(
                    script: activePostScript,
                    responseBody: result.body,
                    responseHeaders: result.headers,
                    statusCode: result.statusCode,
                    context: currentCtx
                ), let id = activeCollectionId {
                    updateContext(updates, for: id)
                }
            }

            addToHistory(result: result, url: finalURL)
            response = result
            isLoading = false
            responseReceived += 1
        } catch {
            responseError = error.localizedDescription
            isLoading = false
            responseReceived += 1
        }
    }

    private func addToHistory(result: HTTPResponse, url: String) {
        let item = HistoryItem(
            id: IDGenerator.next(),
            timestamp: UInt64(Date().timeIntervalSince1970 * 1000),
            url: url,
            method: activeMethod,
            statusCode: UInt16(result.statusCode),
            success: (200..<300).contains(result.statusCode),
            collectionName: collections.first(where: { $0.id == activeCollectionId })?.name,
            requestName: collections.first(where: { $0.id == activeCollectionId })?.requests.first(where: { $0.id == activeRequestId })?.name,
            durationMs: result.durationMs,
            headers: activeHeaders,
            body: activeBody,
            preRequestScript: activePreScript,
            postRequestScript: activePostScript
        )
        history.insert(item, at: 0)
        if history.count > 50 { history = Array(history.prefix(50)) }
        persistHistory()
    }

    func loadFromHistory(_ item: HistoryItem) {
        saveActiveRequest()
        activeRequestId = nil
        activeCollectionId = nil
        activeURL = item.url
        activeMethod = item.method
        activeHeaders = item.headers.isEmpty ? [Header()] : item.headers
        activeBody = item.body
        activePreScript = item.preRequestScript
        activePostScript = item.postRequestScript
        response = nil
        responseError = nil
        selectedRequestId = nil
        responseReceived += 1  // switch view to response tab if one exists
    }

    func clearHistory() {
        history = []
        persistHistory()
    }

    // MARK: - Run collection

    struct RunResult: Identifiable {
        let id = UUID()
        let requestName: String
        let method: String
        let url: String
        let statusCode: String
        let success: Bool
        let durationMs: UInt64
    }

    func runCollection(_ collection: Collection) async {
        guard !collection.requests.isEmpty else { return }
        runResults = []
        isRunning = true

        let vars = collection.variables
        let ctx = context(for: collection.id)

        for req in collection.requests {
            var url = InterpolationService.interpolate(req.url, variables: vars)
            var headers = InterpolationService.interpolateHeaders(req.headers, variables: vars)
            var body = InterpolationService.interpolate(req.body, variables: vars)

            if !req.preRequestScript.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
               let scriptResult = try? scripts.executePreRequest(
                script: req.preRequestScript, url: url, method: req.method,
                headers: headers, body: body, variables: vars, context: ctx
               ) {
                url = scriptResult.url
                headers = scriptResult.headers
                body = scriptResult.body
                updateContext(scriptResult.contextUpdates, for: collection.id)
            }

            do {
                let result = try await http.send(url: url, method: req.method, headers: headers, body: body)
                runResults.append(RunResult(
                    requestName: req.name, method: req.method, url: url,
                    statusCode: String(result.statusCode),
                    success: (200..<300).contains(result.statusCode),
                    durationMs: result.durationMs
                ))
                if !(200..<300).contains(result.statusCode) { break }
            } catch {
                runResults.append(RunResult(
                    requestName: req.name, method: req.method, url: url,
                    statusCode: "Error", success: false, durationMs: 0
                ))
                break
            }
        }

        isRunning = false
    }

    // MARK: - Import / Export

    func exportJSON() -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .prettyPrinted
        guard let data = try? encoder.encode(collections),
              let str = String(data: data, encoding: .utf8) else { return "[]" }
        return str
    }

    func importJSON(_ json: String) throws {
        guard let data = json.data(using: .utf8) else { throw ImportError.invalidData }
        let imported = try JSONDecoder().decode([Collection].self, from: data)
        collections = imported
        persistCollections()
        // Load first request after import
        if let first = imported.first, let req = first.requests.first {
            loadRequest(req, collectionId: first.id)
        }
    }

    enum ImportError: LocalizedError {
        case invalidData
        var errorDescription: String? { "Invalid JSON data" }
    }
}
