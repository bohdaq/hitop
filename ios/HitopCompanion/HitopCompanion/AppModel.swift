import Foundation
import HitopKit
import Observation
import WidgetKit

/// The app's state: saved folders, what was read from them, and run results.
/// Everything that isn't UI lives in HitopKit (reading, running, reports)
/// or Shared/Library.swift (bookmarks, the app group).
@MainActor
@Observable
final class AppModel {
    var folders: [SavedFolder] = FolderLibrary.all()
    /// Read folders by saved id.
    var loaded: [String: CollectionFolder] = [:]
    /// Why a folder couldn't be read, by saved id.
    var readErrors: [String: String] = [:]
    /// The last report per saved id (a whole run) or request id (one request).
    var reports: [String: Report] = [:]
    var running: Set<String> = []
    var runErrors: [String: String] = [:]
    var statuses: [String: RunStatus] = [:]

    init() { refreshStatuses() }

    func refreshStatuses() {
        statuses = Dictionary(AppGroup.statusStore.all().map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    /// A folder picked in Files: must hold a hitop.yaml.
    func add(_ url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let bookmark = try FolderLibrary.bookmark(for: url)
            var saved = SavedFolder(id: UUID().uuidString, name: url.lastPathComponent, bookmark: bookmark, pinned: false, environment: nil)
            let folder = try FolderLibrary.read(saved)
            saved.name = folder.collection.name
            FolderLibrary.update(saved)
            folders = FolderLibrary.all()
            loaded[saved.id] = folder
        } catch {
            readErrors["new"] = error.localizedDescription
        }
    }

    func remove(_ id: String) {
        FolderLibrary.remove(id)
        folders = FolderLibrary.all()
        loaded[id] = nil
        WidgetCenter.shared.reloadAllTimelines()
    }

    func load(_ saved: SavedFolder) {
        do {
            let folder = try FolderLibrary.read(saved)
            loaded[saved.id] = folder
            readErrors[saved.id] = nil
            if folder.collection.name != saved.name {
                var s = saved
                s.name = folder.collection.name
                FolderLibrary.update(s)
                folders = FolderLibrary.all()
            }
        } catch {
            readErrors[saved.id] = error.localizedDescription
        }
    }

    func setEnvironment(_ name: String?, for id: String) {
        guard var s = FolderLibrary.folder(id) else { return }
        s.environment = name
        FolderLibrary.update(s)
        folders = FolderLibrary.all()
    }

    func togglePin(_ id: String) {
        guard var s = FolderLibrary.folder(id) else { return }
        s.pinned.toggle()
        FolderLibrary.update(s)
        folders = FolderLibrary.all()
        WidgetCenter.shared.reloadAllTimelines()
    }

    /// Runs the whole collection (`request` nil) or one request.
    func run(_ id: String, request: Request? = nil) async {
        guard let saved = FolderLibrary.folder(id) else { return }
        let key = request.map { String($0.id) } ?? id
        running.insert(key)
        defer { running.remove(key) }
        do {
            let report = try await CompanionRunner.run(saved, only: request.map { [$0.id] })
            reports[key] = report
            runErrors[key] = nil
            if request == nil {
                refreshStatuses()
                WidgetCenter.shared.reloadAllTimelines()
            }
        } catch {
            runErrors[key] = error.localizedDescription
        }
    }

    // MARK: Secrets

    /// The secret names of the chosen environment and the globals, with
    /// whether the keychain has a value.
    func secrets(for id: String) -> [SecretSlot] {
        guard let folder = loaded[id], let saved = FolderLibrary.folder(id) else { return [] }
        let ws = folder.collection.workspaceID
        var out: [SecretSlot] = []
        if let envName = saved.environment, let env = folder.environment(named: envName) {
            for n in env.secrets {
                out.append(SecretSlot(environment: envName, name: n, isSet: (try? CompanionRunner.secretStore.get(account: SecretNames.account(workspace: ws, environment: envName, name: n))) != nil))
            }
        }
        for n in folder.globals?.secrets ?? [] {
            let g = SecretNames.globalsEnvironment
            out.append(SecretSlot(environment: g, name: n, isSet: (try? CompanionRunner.secretStore.get(account: SecretNames.account(workspace: ws, environment: g, name: n))) != nil))
        }
        return out
    }

    func setSecret(_ value: String, environment: String, name: String, folder id: String) throws {
        guard let folder = loaded[id] else { return }
        let account = SecretNames.account(workspace: folder.collection.workspaceID, environment: environment, name: name)
        if value.isEmpty {
            try CompanionRunner.secretStore.delete(account: account)
        } else {
            try CompanionRunner.secretStore.set(account: account, value: value)
        }
    }
}

/// A secret variable the app can store a value for.
struct SecretSlot: Identifiable, Hashable {
    var environment: String
    var name: String
    var isSet: Bool
    var id: String { "\(environment)/\(name)" }
}
