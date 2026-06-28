import Foundation

final class StorageService {
    static let shared = StorageService()

    private let maxHistory = 50

    private var configDir: URL {
        get throws {
            let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
            let dir = base.appendingPathComponent("hitop")
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            return dir
        }
    }

    func loadCollections() -> [Collection] {
        guard let dir = try? configDir else { return [] }
        let url = dir.appendingPathComponent("collections.json")
        guard let data = try? Data(contentsOf: url) else { return [] }
        return (try? JSONDecoder().decode([Collection].self, from: data)) ?? []
    }

    func saveCollections(_ collections: [Collection]) {
        guard let dir = try? configDir else { return }
        let url = dir.appendingPathComponent("collections.json")
        let encoder = JSONEncoder()
        encoder.outputFormatting = .prettyPrinted
        guard let data = try? encoder.encode(collections) else { return }
        try? data.write(to: url, options: .atomic)
    }

    func loadHistory() -> [HistoryItem] {
        guard let dir = try? configDir else { return [] }
        let url = dir.appendingPathComponent("history.json")
        guard let data = try? Data(contentsOf: url) else { return [] }
        return (try? JSONDecoder().decode([HistoryItem].self, from: data)) ?? []
    }

    func saveHistory(_ history: [HistoryItem]) {
        guard let dir = try? configDir else { return }
        let url = dir.appendingPathComponent("history.json")
        let trimmed = history.suffix(maxHistory)
        let encoder = JSONEncoder()
        encoder.outputFormatting = .prettyPrinted
        guard let data = try? encoder.encode(Array(trimmed)) else { return }
        try? data.write(to: url, options: .atomic)
    }

    func loadContexts() -> [String: [String: String]] {
        guard let dir = try? configDir else { return [:] }
        let url = dir.appendingPathComponent("contexts.json")
        guard let data = try? Data(contentsOf: url) else { return [:] }
        return (try? JSONDecoder().decode([String: [String: String]].self, from: data)) ?? [:]
    }

    func saveContexts(_ contexts: [String: [String: String]]) {
        guard let dir = try? configDir else { return }
        let url = dir.appendingPathComponent("contexts.json")
        let encoder = JSONEncoder()
        encoder.outputFormatting = .prettyPrinted
        guard let data = try? encoder.encode(contexts) else { return }
        try? data.write(to: url, options: .atomic)
    }
}
