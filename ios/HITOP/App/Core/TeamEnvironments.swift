import Foundation

/// An organization's environments (sync-server teamenvs.rs), read and changed here. Secret values
/// never go: each member keeps theirs in their own Keychain. Only what was changed or deleted here
/// is sent (`dirty`, `deleted`), so a lost file never deletes or overwrites anything for the team.
@MainActor
final class TeamEnvironments {
    struct Known: Codable, Hashable, Sendable {
        var org: String
        var version: Int
        /// As the server has it, keys this app doesn't know included.
        var data: JSONAny
    }

    /// team-environments.json.
    struct State: Codable, Hashable, Sendable {
        var account = ""
        var known: [String: Known] = [:]
        var dirty: Set<String> = []
        var deleted: [String: Known] = [:]
    }

    private let cloud: HitopCloud
    private let model: AppModel
    private let file: URL?
    private let notice: (SharedNotice) -> Void
    private(set) var state = State()
    var pushDelay: Duration = .seconds(1)
    private var pushTask: Task<Void, Never>?
    private var last: Task<Void, Never>?

    init(cloud: HitopCloud, model: AppModel, file: URL?, notice: @escaping (SharedNotice) -> Void) {
        self.cloud = cloud
        self.model = model
        self.file = file
        self.notice = notice
        if let file { state = ((try? StateFileProblem.read(State.self, file)) ?? nil) ?? State() }
        model.onTeamChange = { [weak self] id in self?.changed(id) }
        model.onTeamGone = { [weak self] id in self?.gone(id) }
    }

    private func save() {
        guard let file, let data = try? JSONEncoder().encode(state) else { return }
        try? data.write(to: file, options: [.atomic])
    }

    private func changed(_ id: String) {
        claim()
        state.dirty.insert(id)
        state.deleted[id] = nil
        save()
        schedule()
    }

    private func gone(_ id: String) {
        claim()
        state.dirty.remove(id)
        if let k = state.known.removeValue(forKey: id) { state.deleted[id] = k }
        save()
        schedule()
    }

    /// What was noted for another account is never sent with this one's.
    private func claim() {
        guard state.account != cloud.accountKey else { return }
        state = State(account: cloud.accountKey)
    }

    private func schedule() {
        guard cloud.signedIn else { return }
        pushTask?.cancel()
        pushTask = Task { [weak self, pushDelay] in
            try? await Task.sleep(for: pushDelay)
            guard !Task.isCancelled else { return }
            await self?.one { await self?.trySend() }
        }
    }

    /// One round at a time.
    private func one(_ f: @escaping @MainActor () async -> Void) async {
        let before = last
        let t = Task { @MainActor in
            await before?.value
            await f()
        }
        last = t
        await t.value
    }

    /// Sends what waits (tests, Sync now).
    func flush() async {
        pushTask?.cancel()
        await one { await self.trySend() }
    }

    private func trySend() async {
        do { try await send() } catch {
            notice(SharedNotice("Team environments aren't saved yet: \(StoreText.withoutPlanLink(error.localizedDescription)) They go with the next sync."))
        }
    }

    // MARK: Data

    /// What the server keeps of `e`: its secret variables empty.
    static func data(of e: SavedEnvironment, over known: JSONAny?) -> JSONAny {
        var o = known?.object ?? [:]
        var variables = e.variables.mapValues(JSONAny.string)
        for n in e.secrets { variables[n] = .string("") }
        o["name"] = .string(e.name)
        o["variables"] = .object(variables)
        o["secrets"] = .array(e.secrets.map(JSONAny.string))
        return .object(o)
    }

    static func environment(_ id: String, _ org: String, _ data: JSONAny, by: String?) -> SavedEnvironment? {
        guard var e = try? data.decode(SavedEnvironment.self) else { return nil }
        e.id = id
        e.numericID = false
        for n in e.secrets { e.variables[n] = nil }
        e.extras["team"] = .string(org)
        e.extras["org"] = .string(org)
        if let by, !by.isEmpty { e.extras["updatedBy"] = .string(by) }
        return e
    }

    private func here(_ id: String) -> SavedEnvironment? { model.environments.first { $0.id == id && $0.extras["team"] != nil } }

    // MARK: Sending

    private func send() async throws {
        guard cloud.signedIn, state.account == cloud.accountKey, model.store.unreadableFiles.isEmpty else { return }
        for id in state.dirty.sorted() {
            guard let e = here(id), let org = e.extras["team"]?.string else {
                state.dirty.remove(id)
                continue
            }
            let k = state.known[id].flatMap { $0.org == org ? $0 : nil }
            let data = Self.data(of: e, over: k?.data)
            let r = try await cloud.call("PUT", "/v1/orgs/\(org)/environments/\(id)", body: .object(["data": data, "baseVersion": .number(Double(k?.version ?? 0))]),
                                         allow: [400, 402, 403, 404, 409, 413])
            if r.status == 200 {
                state.known[id] = Known(org: org, version: r.json?["version"]?.int ?? (k?.version ?? 0) + 1, data: data)
                // Changed again while it was on its way: that goes too.
                if here(id).map({ Self.data(of: $0, over: k?.data) }) == data { state.dirty.remove(id) }
            } else if r.status == 409 {
                state.dirty.remove(id)
                if let cur = r.json?["current"], !cur.isNull, let theirs = cur["data"] {
                    let by = cur["updatedBy"]?.string
                    state.known[id] = Known(org: org, version: cur["version"]?.int ?? 0, data: theirs)
                    model.replaceTeam(id, with: Self.environment(id, org, theirs, by: by))
                    notice(SharedNotice("\(e.name) was changed by \(by.flatMap { $0.isEmpty ? nil : $0 } ?? "someone in the organization"): showing theirs."))
                } else {
                    state.known[id] = nil
                    model.replaceTeam(id, with: nil)
                    notice(SharedNotice("\(e.name) was deleted by someone in the organization."))
                }
            } else {
                // Refused: the team's stays, a new one stays this person's own.
                state.dirty.remove(id)
                if let k {
                    model.replaceTeam(id, with: Self.environment(id, org, k.data, by: nil))
                } else {
                    var own = e
                    own.extras["team"] = nil
                    own.extras["org"] = nil
                    model.replaceTeam(id, with: own)
                }
                notice(SharedNotice("\(e.name) wasn't saved for the organization: \(StoreText.withoutPlanLink(HitopCloud.error(r).localizedDescription))"))
            }
            save()
        }
        for (id, k) in state.deleted.sorted(by: { $0.key < $1.key }) {
            let r = try await cloud.call("DELETE", "/v1/orgs/\(k.org)/environments/\(id)?baseVersion=\(k.version)", allow: [400, 403, 404, 409])
            state.deleted[id] = nil
            save()
            if r.status == 409 || r.status == 403 {
                notice(SharedNotice("\(k.data["name"]?.string ?? "An environment") wasn't deleted for the organization: \(r.status == 409 ? "someone changed it meanwhile" : "it isn't allowed")."))
            }
        }
        save()
    }

    // MARK: Loading

    /// The current organization's: what was changed here first, then the server's.
    func load() async { await one { await self.loadNow() } }

    private func loadNow() async {
        let org = cloud.signedIn ? cloud.currentOrg.flatMap { $0.personal ? nil : $0.id } : nil
        claim()
        // Not sent yet (offline): what is here stays.
        do { try await send() } catch { if !state.dirty.isEmpty || !state.deleted.isEmpty { return } }
        model.dropTeamEnvironments(except: org)
        state.known = state.known.filter { $0.value.org == org }
        save()
        guard let org else { return }
        guard let r = try? await cloud.call("GET", "/v1/orgs/\(org)/environments", allow: [400, 403, 404]), r.status == 200, let j = r.json else { return }
        // Changed here, or another organization chosen, while the answer was on its way: next time.
        guard state.account == cloud.accountKey, cloud.currentOrg?.id == org, state.dirty.isEmpty, state.deleted.isEmpty else { return schedule() }
        var list: [SavedEnvironment] = []
        state.known = [:]
        for e in j["environments"]?.array ?? [] {
            guard let id = e["id"]?.string, let data = e["data"], let env = Self.environment(id, org, data, by: e["updatedBy"]?.string) else { continue }
            state.known[id] = Known(org: org, version: e["version"]?.int ?? 0, data: data)
            list.append(env)
        }
        model.applyTeam(environments: list, org: org)
        save()
    }
}
