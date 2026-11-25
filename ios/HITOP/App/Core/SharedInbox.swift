import Foundation

/// What another app shared to HITOP: the share extension leaves it in the app group's folder, the
/// app takes it when it comes to the screen. Self-contained (compiled into the extension too).
struct SharedInbox: Sendable {
    static let maxBytes = 25 * 1024 * 1024
    /// Nobody opened HITOP after sharing this many times.
    static let maxItems = 20

    enum Item: Equatable, Sendable {
        case text(String)
        /// A copy outside the inbox, its own folder, the name as shared.
        case file(URL)
    }

    struct Failure: Error, LocalizedError, Equatable {
        var message: String
        var errorDescription: String? { message }
    }

    let folder: URL

    init(root: URL) { folder = root.appendingPathComponent("inbox", isDirectory: true) }

    /// A name that stays in its folder.
    static func safe(_ name: String) -> String {
        let n = name.components(separatedBy: CharacterSet(charactersIn: "/\\:\0").union(.controlCharacters)).joined(separator: "-")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let cut = String(n.suffix(100))
        return cut.isEmpty || cut.allSatisfy({ $0 == "." }) ? "file" : cut
    }

    private func entries() -> [URL] {
        let all = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        return all.filter { !$0.lastPathComponent.hasPrefix(".") }.sorted { $0.lastPathComponent < $1.lastPathComponent }
    }

    /// Written beside and moved in, so a half-written item is never taken.
    private func put(_ kind: String, _ name: String, _ write: (URL) throws -> Void) throws {
        let fm = FileManager.default
        guard entries().count < Self.maxItems else { throw Failure(message: "HITOP has \(Self.maxItems) shared items waiting. Open it first.") }
        try fm.createDirectory(at: folder, withIntermediateDirectories: true)
        let id = String(format: "%015.0f", Date().timeIntervalSince1970 * 1000) + "-" + UUID().uuidString
        let part = folder.appendingPathComponent(".\(id)", isDirectory: true)
        let content = part.appendingPathComponent(kind, isDirectory: true)
        do {
            try fm.createDirectory(at: content, withIntermediateDirectories: true)
            try write(content.appendingPathComponent(name))
            try fm.moveItem(at: part, to: folder.appendingPathComponent(id, isDirectory: true))
        } catch {
            try? fm.removeItem(at: part)
            throw error
        }
    }

    func add(text: String) throws {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty else { throw Failure(message: "There is no text in it.") }
        guard t.utf8.count <= Self.maxBytes else { throw Failure(message: "It is larger than 25 MB.") }
        try put("text", "text") { try Data(t.utf8).write(to: $0) }
    }

    func add(file: URL, name: String? = nil) throws {
        let v = try file.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
        guard v.isRegularFile == true, let size = v.fileSize else { throw Failure(message: "HITOP takes files, not folders.") }
        guard size <= Self.maxBytes else { throw Failure(message: "The file is larger than 25 MB.") }
        try put("file", Self.safe(name ?? file.lastPathComponent)) { try FileManager.default.copyItem(at: file, to: $0) }
    }

    /// The oldest item, taken out (an unusable one is dropped); nil when there is none.
    func take() -> Item? {
        let fm = FileManager.default
        for e in entries() {
            defer { try? fm.removeItem(at: e) }
            if let data = try? Data(contentsOf: e.appendingPathComponent("text/text")) { return .text(String(decoding: data, as: UTF8.self)) }
            guard let f = (try? fm.contentsOfDirectory(at: e.appendingPathComponent("file"), includingPropertiesForKeys: nil))?.first else { continue }
            let to = fm.temporaryDirectory.appendingPathComponent("shared-\(UUID().uuidString)/Inbox", isDirectory: true)
            guard (try? fm.createDirectory(at: to, withIntermediateDirectories: true)) != nil else { continue }
            let copy = to.appendingPathComponent(f.lastPathComponent)
            if (try? fm.moveItem(at: f, to: copy)) != nil { return .file(copy) }
        }
        return nil
    }
}
