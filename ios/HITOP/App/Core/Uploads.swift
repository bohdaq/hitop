import Foundation
import HitopKit

/// The files picked for multipart rows: copies in the app's own storage (`uploads/<token>`,
/// `uploads.json`). A row names its file by a token (`fileToken`), and a file goes out only with the
/// request it was picked for, so a collection (imported, synced, shared) can't name one.
final class Uploads: @unchecked Sendable {
    static let maxBytes = 50 * 1024 * 1024

    private struct Entry: Codable {
        var request: UInt64
        var name: String
        var type: String
    }

    struct Failure: LocalizedError, CustomStringConvertible {
        let message: String
        var errorDescription: String? { message }
        var description: String { message }
    }

    private let root: URL
    private let fm = FileManager.default
    private let lock = NSLock()
    /// nil: the index is there but can't be read (the phone is locked); nothing is written over it.
    private var entries: [String: Entry]?

    init(root: URL) { self.root = root }

    private var index: URL { root.appendingPathComponent("uploads.json") }
    private func file(_ token: String) -> URL { root.appendingPathComponent("uploads/\(token)") }

    private func locked<T>(_ f: () throws -> T) rethrows -> T {
        lock.lock()
        defer { lock.unlock() }
        return try f()
    }

    private func load() -> [String: Entry]? {
        if let entries { return entries }
        if !fm.fileExists(atPath: index.path) {
            entries = [:]
        } else if let data = try? Data(contentsOf: index) {
            entries = try? JSONDecoder().decode([String: Entry].self, from: data)
            if entries == nil {
                // Damaged: set aside, its files stay (a row asks for its file again).
                let aside = root.appendingPathComponent("uploads.json.damaged-\(Int(Date().timeIntervalSince1970 * 1000))")
                if (try? fm.moveItem(at: index, to: aside)) != nil { entries = [:] }
            }
        }
        return entries
    }

    private func store(_ all: [String: Entry]) throws {
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        try JSONEncoder().encode(all).write(to: index, options: [.atomic])
        entries = all
    }

    /// Keeps a copy of `source` for a row of `request`; the row's token.
    func add(_ source: URL, name: String, type: String, request: UInt64, max: Int = Uploads.maxBytes) throws -> String {
        try locked {
            guard var all = load() else { throw Failure(message: "The list of picked files can't be read.") }
            let size = { (u: URL) throws -> Int in ((try FileManager.default.attributesOfItem(atPath: u.path))[.size] as? NSNumber)?.intValue ?? 0 }
            let tooLarge = Failure(message: "The file is larger than \(max / 1024 / 1024) MB.")
            if try size(source) > max { throw tooLarge }
            let token = UUID().uuidString.lowercased()
            let to = file(token)
            try fm.createDirectory(at: to.deletingLastPathComponent(), withIntermediateDirectories: true)
            do {
                try fm.copyItem(at: source, to: to)
                if try size(to) > max { throw tooLarge }
                // A header value can't hold line breaks; HitopKit quotes the rest of the name.
                let mime = type.range(of: "^[A-Za-z0-9.+-]+/[A-Za-z0-9.+-]+$", options: .regularExpression) != nil && !type.contains("\n")
                all[token] = Entry(request: request, name: name, type: mime ? type : "application/octet-stream")
                try store(all)
            } catch {
                try? fm.removeItem(at: to)
                throw error
            }
            return token
        }
    }

    /// The row's file is here (picked for this request).
    func has(_ token: String, request: UInt64) -> Bool {
        locked { load()?[token]?.request == request && fm.fileExists(atPath: file(token).path) }
    }

    /// The file of a row, or nil for a row without one.
    private func read(_ token: String, request: UInt64) throws -> FormFile? {
        let entry: Entry? = locked { load()?[token] }
        guard let e = entry, e.request == request else { return nil }
        do {
            return FormFile(filename: e.name, contentType: e.type, data: try Data(contentsOf: file(token)))
        } catch {
            throw Failure(message: "\(e.name): \(error.localizedDescription)")
        }
    }

    /// What HitopKit asks for a multipart row's file.
    static func source(_ uploads: Uploads?) -> @Sendable (HitopKit.Request, FormField) throws -> FormFile? {
        { r, f in
            guard let token = f.fileToken else { return nil }
            if let file = try uploads?.read(token, request: r.id) { return file }
            throw Failure(message: "\(f.fileName ?? "The file") isn't on this device: pick it again for the field \"\(f.key)\"")
        }
    }

    /// Drops files no request names any more.
    func prune(keeping used: Set<String>) {
        locked {
            guard let all = load() else { return }
            let gone = all.keys.filter { !used.contains($0) }
            guard !gone.isEmpty, (try? store(all.filter { used.contains($0.key) })) != nil else { return }
            gone.forEach { try? fm.removeItem(at: file($0)) }
        }
    }
}

extension AppModel {
    /// At the app's start, and only when the whole library could be read.
    func pruneUploads() {
        guard let uploads = sender.uploads, store.unreadableFiles.isEmpty, store.damagedFiles.isEmpty else { return }
        let requests = collections.flatMap(\.requests) + store.loadDrafts()
        uploads.prune(keeping: Set(requests.flatMap { $0.formFields.compactMap(\.fileToken) }))
    }
}
