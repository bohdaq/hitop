import SwiftUI
import UIKit

// Shared collections (I2b): the card's badge, the Share sheet (share, invite, members, conflicts),
// joining with an invite code (the trust prompt), and the Account screen's part. Store mode: no
// plan, price or upgrade link anywhere; a limit the server answers with reads as it says, without
// the link (`StoreText.withoutPlanLink`).

/// The capsule under a shared collection's name: "Shared with Acme", "View only", "1 conflict".
struct SharedBadge: View {
    var collectionID: UInt64
    var name: String
    @Environment(SharedSync.self) private var shared
    @Environment(AppModel.self) private var model

    var body: some View {
        if let text = shared.badge(collectionID) {
            HStack(spacing: 6) {
                Label(text, systemImage: "person.2.fill")
                    .labelStyle(.titleAndIcon)
                    .font(Typo.geist(11.5, .semibold, relativeTo: .caption))
                    .foregroundStyle(Theme.accentText)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(Capsule().fill(Theme.accent.opacity(0.16)))
                    .accessibilityIdentifier("shared-badge-\(name)")
                if model.isReadOnly(collectionID) {
                    Text("View only").font(Typo.geist(11.5, .medium, relativeTo: .caption)).foregroundStyle(Theme.textMuted)
                }
                let n = shared.conflicts[collectionID]?.count ?? 0
                if n > 0 {
                    Text(n == 1 ? "1 conflict" : "\(n) conflicts").font(Typo.geist(11.5, .semibold, relativeTo: .caption)).foregroundStyle(Theme.warning)
                        .accessibilityIdentifier("conflict-count-\(name)")
                }
            }
        }
    }

    /// For the card's VoiceOver label.
    @MainActor
    static func describe(_ id: UInt64, _ shared: SharedSync, _ model: AppModel) -> String {
        guard let b = shared.badge(id) else { return "" }
        var parts = [b]
        if model.isReadOnly(id) { parts.append("view only") }
        let n = shared.conflicts[id]?.count ?? 0
        if n > 0 { parts.append(n == 1 ? "1 conflict" : "\(n) conflicts") }
        return parts.joined(separator: ", ")
    }
}

/// "3 requests · synced just now" for a shared collection.
struct SharedSyncLine: View {
    var collection: SavedCollection
    @Environment(SharedSync.self) private var shared

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { context in
            Text(Self.describe(collection, shared, now: context.date))
                .font(Typo.caption).foregroundStyle(shared.statuses[collection.id]?.error == nil ? Theme.textMuted : Theme.warning)
                .lineLimit(2)
        }
    }

    @MainActor
    static func describe(_ c: SavedCollection, _ shared: SharedSync, now: Date = Date()) -> String {
        let n = "\(c.requests.count) \(c.requests.count == 1 ? "request" : "requests")"
        if shared.link(c.id)?.gone ?? false { return "\(n) · on this device only" }
        let s = shared.statuses[c.id]
        if s?.syncing ?? false { return "\(n) · syncing…" }
        if let e = s?.error { return "\(n) · \(e)" }
        if let at = s?.lastSync { return "\(n) · synced \(LibrarySync.ago(at, now: now))" }
        return "\(n) · end-to-end encrypted"
    }
}

/// Which sheet the shared screens show.
struct ShareTarget: Identifiable, Hashable {
    var id: UInt64
}

// MARK: - Share sheet

/// Share a collection (a new workspace in its organization), its members and their devices, the
/// invite form, removal (the key rotates), and its open conflicts.
struct ShareSheet: View {
    var collectionID: UInt64
    @Environment(SharedSync.self) private var shared
    @Environment(AppModel.self) private var model
    @Environment(HitopCloud.self) private var cloud
    @Environment(\.dismiss) private var dismiss
    @State private var info: WorkspaceInfo?
    @State private var ownDevices: [MemberKey] = []
    @State private var error: String?
    @State private var note: String?
    @State private var busy = false
    @State private var email = ""
    @State private var role = "editor"
    @State private var code: String?
    @State private var removing: WorkspaceMember?
    @State private var trusting: MemberKey?

    private var name: String { model.collection(collectionID)?.name ?? "Collection" }
    private var link: SharedLink? { shared.link(collectionID) }
    private var owner: Bool { info?.role == "owner" }

    var body: some View {
        NavigationStack {
            List {
                if let error {
                    Text(error).font(Typo.caption).foregroundStyle(Theme.warning).accessibilityIdentifier("share-error")
                        .listRowBackground(Theme.surface)
                }
                if let note {
                    Text(note).font(Typo.caption).foregroundStyle(Theme.textSoft).accessibilityIdentifier("share-note")
                        .listRowBackground(Theme.surface)
                }
                if link == nil {
                    notShared
                } else {
                    ConflictsSection(collectionID: collectionID)
                    if let info {
                        if owner { inviteSection }
                        membersSection(info)
                        if owner { invitesSection(info) }
                    } else if link?.gone ?? false {
                        Text("This collection is no longer shared with you: its copy stays on this device.").font(Typo.caption).foregroundStyle(Theme.textMuted)
                            .listRowBackground(Theme.surface)
                    } else {
                        ProgressView().frame(maxWidth: .infinity, minHeight: 44).listRowBackground(Theme.surface)
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle(link == nil ? "Share \(name)" : name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.accessibilityIdentifier("share-done") } }
            .task { await reload() }
            .refreshable { await reload() }
            .confirmationDialog("Remove \(removing?.email ?? "")?", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }), titleVisibility: .visible) {
                Button("Remove", role: .destructive) { remove() }.accessibilityIdentifier("confirm-remove")
                Button("Cancel", role: .cancel) { removing = nil }
            } message: {
                Text("The collection gets a new key, and every file is sealed again with it: they can't read what changes from now on. What they already saw stays with them.")
            }
            .alert("Trust this device?", isPresented: Binding(get: { trusting != nil }, set: { if !$0 { trusting = nil } })) {
                Button("They match, trust it") {
                    if let k = trusting { shared.trust([k]) }
                    trusting = nil
                    Task { await reload() }
                }
                .accessibilityIdentifier("trust-device")
                Button("Cancel", role: .cancel) { trusting = nil }
            } message: {
                Text("Fingerprint \(trusting?.fingerprint ?? ""). Compare it with the device's owner (in person, on a call, or in a chat you trust) before you say yes: the server hands out keys, and a wrong one would let someone else read the collection.")
            }
        }
        .presentationDetents([.large])
    }

    // MARK: Not shared yet

    @ViewBuilder
    private var notShared: some View {
        Section {
            VStack(alignment: .leading, spacing: 10) {
                Text("Share \(name) with people").font(Typo.title)
                Text("Members see it in HITOP on every platform and edit it with you. It is end-to-end encrypted: the server never sees a request, a file name or a key. Secret values never leave your devices.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted)
                Button {
                    run("share") {
                        try await shared.share(collectionID)
                        note = "Shared. Invite people below."
                    }
                } label: {
                    Text(shareLabel).font(Typo.geist(16, .semibold)).frame(maxWidth: .infinity, minHeight: 44)
                }
                .buttonStyle(.glassProminent)
                .tint(Theme.accent)
                .disabled(busy || !ownDevices.isEmpty || !cloud.signedIn)
                .accessibilityIdentifier("share-collection")
            }
            .padding(.vertical, 6)
        }
        .listRowBackground(Theme.surface)
        if !ownDevices.isEmpty {
            Section {
                ForEach(ownDevices) { k in keyRow(k, owner: "you") }
            } header: {
                header("Your other devices")
            } footer: {
                Text("Every device of yours gets the collection's key: compare each one's fingerprint (shown in its Account screen) and trust it first.").font(Typo.caption)
            }
            .listRowBackground(Theme.surface)
        }
    }

    private var shareLabel: String {
        let org = model.collection(collectionID)?.org ?? cloud.orgForNew
        if let o = cloud.settings.orgs.first(where: { $0.id == org }), !o.personal { return "Share in \(o.name)" }
        return "Share"
    }

    // MARK: Shared

    private var inviteSection: some View {
        Section {
            TextField("Email", text: $email)
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .frame(minHeight: 44)
                .accessibilityIdentifier("invite-email")
            Picker("Role", selection: $role) {
                Text("Editor").tag("editor")
                Text("Viewer").tag("viewer")
                Text("Owner").tag("owner")
            }
            .frame(minHeight: 44)
            .accessibilityIdentifier("invite-role")
            Button("Invite") {
                run("invite") {
                    let r = try await shared.invite(collectionID, email: email, role: role)
                    code = r.code
                    note = r.mailed ? "Invite mailed to \(email). Also send them this code yourself (in a chat, in person): it carries your device's fingerprint, so their HITOP takes the key only from you." : "Send them this code privately: it carries your device's fingerprint."
                    email = ""
                }
            }
            .frame(minHeight: 44)
            .disabled(busy)
            .accessibilityIdentifier("send-invite")
            if let code {
                VStack(alignment: .leading, spacing: 8) {
                    Text(code).font(Typo.mono(12.5)).textSelection(.enabled).accessibilityIdentifier("invite-code-text")
                    HStack {
                        Button("Copy code", systemImage: "doc.on.doc") { UIPasteboard.general.string = code }
                        Spacer()
                        ShareLink(item: code) { Label("Send", systemImage: "square.and.arrow.up") }
                    }
                    .font(Typo.geist(14, .medium))
                }
                .padding(.vertical, 4)
            }
        } header: {
            header("Invite")
        }
        .listRowBackground(Theme.surface)
    }

    private func membersSection(_ info: WorkspaceInfo) -> some View {
        Section {
            ForEach(info.members) { m in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(m.email).font(Typo.row).foregroundStyle(Theme.textRow)
                        Spacer()
                        Text(m.role.capitalized).font(Typo.geist(12, .semibold)).foregroundStyle(Theme.textMuted)
                    }
                    ForEach(m.keys) { k in keyRow(k, owner: m.email) }
                }
                .padding(.vertical, 4)
                .accessibilityIdentifier("member-\(m.email)")
                .swipeActions(edge: .trailing) {
                    if owner && m.email != cloud.account?.email {
                        Button("Remove", systemImage: "person.badge.minus", role: .destructive) { removing = m }
                    }
                }
                .contextMenu {
                    if owner && m.email != cloud.account?.email {
                        Button("Remove", systemImage: "person.badge.minus", role: .destructive) { removing = m }
                    }
                }
            }
        } header: {
            header("Members")
        } footer: {
            Text(owner ? "Swipe a member to remove them: the collection gets a new key." : "Owners invite and remove members.").font(Typo.caption)
        }
        .listRowBackground(Theme.surface)
    }

    @ViewBuilder
    private func invitesSection(_ info: WorkspaceInfo) -> some View {
        if !info.invites.isEmpty {
            Section {
                ForEach(info.invites) { i in
                    if let a = i.acceptedBy {
                        VStack(alignment: .leading, spacing: 6) {
                            Text("\(a.email) accepted (\(i.role)). Compare their fingerprint, trust it, then add them.").font(Typo.caption).foregroundStyle(Theme.textSoft)
                            ForEach(a.keys) { k in keyRow(k, owner: a.email) }
                            Button("Add \(a.email)") {
                                run("add") {
                                    try await shared.addMember(collectionID, email: a.email, role: i.role, keys: a.keys)
                                    note = "Added \(a.email) as \(i.role)."
                                }
                            }
                            .disabled(busy || !a.keys.allSatisfy { $0.trust == .trusted })
                            .frame(minHeight: 44)
                            .accessibilityIdentifier("add-\(a.email)")
                        }
                    } else {
                        Text("Open invite: \(i.email.isEmpty ? "code" : i.email) (\(i.role))").font(Typo.caption).foregroundStyle(Theme.textMuted).frame(minHeight: 44)
                    }
                }
            } header: {
                header("Invites")
            }
            .listRowBackground(Theme.surface)
        }
    }

    private func keyRow(_ k: MemberKey, owner: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: k.trust == .trusted ? "checkmark.shield" : k.trust == .changed ? "exclamationmark.triangle" : "questionmark.circle")
                .foregroundStyle(k.trust == .trusted ? Theme.success : Theme.warning).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(k.name.isEmpty ? "Device" : k.name).font(Typo.geist(13, .medium)).foregroundStyle(Theme.textSoft)
                Text(k.fingerprint).font(Typo.mono(11.5)).foregroundStyle(Theme.textMuted)
                Text(trustText(k)).font(Typo.caption).foregroundStyle(k.trust == .changed ? Theme.danger : Theme.textFaint)
            }
            Spacer()
            if k.trust == .new {
                Button("Trust") { trusting = k }.font(Typo.geist(13, .semibold)).buttonStyle(.borderless).frame(minHeight: 44)
                    .accessibilityIdentifier("trust-\(k.name)")
            }
        }
        .accessibilityElement(children: .contain)
    }

    private func trustText(_ k: MemberKey) -> String {
        let t: String = switch k.trust {
        case .trusted: "trusted"
        case .changed: "KEY CHANGED: not trusted, nothing is shared with it"
        case .new: "not checked yet"
        }
        return k.hasKey ? t : "\(t), waiting for the key"
    }

    private func header(_ s: String) -> some View {
        Text(s.uppercased()).font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
    }

    private func reload() async {
        error = nil
        do {
            if link == nil {
                ownDevices = cloud.signedIn ? try await shared.ownUntrustedDevices() : []
                info = nil
            } else if !(link?.gone ?? true) {
                info = try await shared.members(collectionID)
                if owner || info?.role == "editor" { _ = try? await shared.shareKeysWithNewDevices(collectionID) }
            }
        } catch {
            self.error = StoreText.withoutPlanLink(error.localizedDescription)
        }
    }

    private func remove() {
        guard let m = removing else { return }
        removing = nil
        run("remove") {
            let epoch = try await shared.removeMember(collectionID, userId: m.userId)
            note = "Removed \(m.email). The collection's key is new (epoch \(epoch))."
        }
    }

    private func run(_ what: String, _ f: @escaping @MainActor () async throws -> Void) {
        busy = true
        error = nil
        Task {
            do { try await f() } catch { self.error = StoreText.withoutPlanLink(error.localizedDescription) }
            busy = false
            await reload()
        }
    }
}

// MARK: - Conflicts

/// A shared collection's open conflicts: keep mine, take theirs, keep both, and both texts.
struct ConflictsSection: View {
    var collectionID: UInt64
    @Environment(SharedSync.self) private var shared
    @State private var shown: String?
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        let list = shared.conflicts[collectionID] ?? []
        if !list.isEmpty {
            Section {
                ForEach(list) { c in row(c) }
                if let error { Text(error).font(Typo.caption).foregroundStyle(Theme.warning) }
            } header: {
                Text("CHANGED HERE AND BY SOMEONE ELSE").font(Typo.label).tracking(0.9).foregroundStyle(Theme.warning)
            } footer: {
                Text("A file in conflict isn't sent until you choose.").font(Typo.caption)
            }
            .listRowBackground(Theme.surface)
        }
    }

    private func row(_ c: SharedConflict) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(c.name, systemImage: "exclamationmark.triangle.fill").font(Typo.row).foregroundStyle(Theme.text)
                .accessibilityIdentifier("conflict-\(c.name)")
            Text(c.path).font(Typo.mono(11.5)).foregroundStyle(Theme.textMuted)
            HStack(spacing: 8) {
                choice("Keep mine", "keep-mine", c, .mine)
                choice("Take theirs", "take-theirs", c, .theirs)
                if c.canKeepBoth { choice("Keep both", "keep-both", c, .both) }
            }
            Button(shown == c.path ? "Hide" : "View both") { shown = shown == c.path ? nil : c.path }
                .font(Typo.geist(13, .medium)).buttonStyle(.borderless).frame(minHeight: 44)
                .accessibilityIdentifier("view-both")
            if shown == c.path {
                side("Mine (this device)", c.mine)
                side("Theirs", c.theirs)
                side("Before both changes", c.base)
            }
        }
        .padding(.vertical, 6)
    }

    private func choice(_ title: String, _ id: String, _ c: SharedConflict, _ ch: SharedSync.Choice) -> some View {
        Button(title) {
            busy = true
            Task {
                do { try await shared.resolve(collectionID, path: c.path, ch) } catch { self.error = error.localizedDescription }
                busy = false
            }
        }
        .font(Typo.geist(13, .semibold))
        .buttonStyle(.bordered)
        .disabled(busy)
        .frame(minHeight: 44)
        .accessibilityIdentifier(id)
    }

    private func side(_ title: String, _ text: String?) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased()).font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
            Text(text ?? "(deleted)").font(Typo.mono(11.5)).foregroundStyle(Theme.textSoft).textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
                .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Theme.fill))
        }
    }
}

// MARK: - Join with an invite code

/// Paste the code → the trust prompt with the inviter's fingerprint → accepted; once the owner
/// adds this account, the collection opens here.
struct JoinSheet: View {
    @Environment(SharedSync.self) private var shared
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var parsed: InviteCode?
    @State private var accepted: (workspace: String, role: String)?
    @State private var opened: String?
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        NavigationStack {
            List {
                if let error {
                    Text(error).font(Typo.caption).foregroundStyle(Theme.warning).accessibilityIdentifier("join-error").listRowBackground(Theme.surface)
                }
                if let opened {
                    Section {
                        Label("\(opened) is open here.", systemImage: "checkmark.circle.fill").foregroundStyle(Theme.success).accessibilityIdentifier("join-opened")
                    }
                    .listRowBackground(Theme.surface)
                } else if let accepted {
                    waiting(accepted)
                } else if let parsed {
                    trust(parsed)
                } else {
                    paste
                }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle("Join a shared collection")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.accessibilityIdentifier("join-done") } }
        }
    }

    private var paste: some View {
        Section {
            TextField("Invite code", text: $text, axis: .vertical)
                .font(Typo.mono(13))
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .frame(minHeight: 44)
                .accessibilityIdentifier("invite-code")
            Button("Continue") {
                do {
                    parsed = try InviteCode.parse(text)
                    error = nil
                } catch {
                    self.error = error.localizedDescription
                }
            }
            .frame(minHeight: 44)
            .disabled(text.trimmingCharacters(in: .whitespaces).isEmpty)
            .accessibilityIdentifier("invite-continue")
        } footer: {
            Text("The code someone sent you to share a collection with you (also the part after # of an invite link).").font(Typo.caption)
        }
        .listRowBackground(Theme.surface)
    }

    private func trust(_ code: InviteCode) -> some View {
        Section {
            VStack(alignment: .leading, spacing: 10) {
                if let fp = code.fingerprint {
                    Text("The collection's key will come from the device with this fingerprint:").font(Typo.caption).foregroundStyle(Theme.textSoft)
                    Text(SyncCore.grouped(fp)).font(Typo.mono(15, .semibold)).foregroundStyle(Theme.text).accessibilityIdentifier("inviter-fingerprint")
                    Text("Compare it with the person who invited you (in person, on a call, or in a chat you trust). Only if it matches, accept: HITOP then takes the key from that device only, and refuses a changed one.")
                        .font(Typo.caption).foregroundStyle(Theme.textMuted)
                } else {
                    Text("This code doesn't name the inviter's key. When the key arrives, HITOP shows the fingerprint of the device that sent it, for you to compare.").font(Typo.caption).foregroundStyle(Theme.textMuted)
                }
                HStack {
                    Button("Cancel") { parsed = nil }.buttonStyle(.bordered)
                    Spacer()
                    Button(code.fingerprint == nil ? "Accept invite" : "They match, accept") { accept(code) }
                        .buttonStyle(.glassProminent).tint(Theme.accent).disabled(busy)
                        .accessibilityIdentifier("trust-accept")
                }
                .frame(minHeight: 44)
            }
            .padding(.vertical, 6)
        } header: {
            Text("CHECK WHO SENDS THE KEY").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
        }
        .listRowBackground(Theme.surface)
    }

    private func waiting(_ a: (workspace: String, role: String)) -> some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text("Invite accepted (\(a.role)).").font(Typo.title).accessibilityIdentifier("join-status")
                Text("Tell the owner this device's fingerprint so they can check it before adding you:").font(Typo.caption).foregroundStyle(Theme.textSoft)
                Text(shared.fingerprint ?? "").font(Typo.mono(13.5, .semibold))
                HStack(spacing: 8) {
                    ProgressView()
                    Text("Waiting for the owner to add you…").font(Typo.caption).foregroundStyle(Theme.textMuted)
                }
            }
            .padding(.vertical, 6)
        }
        .listRowBackground(Theme.surface)
        .task(id: a.workspace) { await poll(a.workspace) }
    }

    private func accept(_ code: InviteCode) {
        busy = true
        Task {
            do {
                accepted = try await shared.accept(code)
            } catch {
                self.error = StoreText.withoutPlanLink(error.localizedDescription)
            }
            busy = false
        }
    }

    /// Until the owner adds this account (or two minutes), then opens the collection.
    private func poll(_ ws: String) async {
        for _ in 0..<120 {
            if (try? await shared.refreshWorkspaces()) != nil, shared.joinable.contains(where: { $0.id == ws }) {
                do {
                    let id = try await shared.join(ws)
                    opened = shared.model.collection(id)?.name ?? "The collection"
                } catch {
                    self.error = StoreText.withoutPlanLink(error.localizedDescription)
                }
                return
            }
            try? await Task.sleep(for: .seconds(1))
        }
    }
}

// MARK: - Account screen part

/// Shared collections on the Account screen: this device's fingerprint, joining with a code, the
/// workspaces waiting to be opened here, a key from a device not trusted yet.
struct SharedAccountSection: View {
    @Environment(SharedSync.self) private var shared
    @State private var joining = false
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        Section {
            if let p = shared.pendingTrust { pendingTrust(p) }
            if let fp = shared.fingerprint {
                VStack(alignment: .leading, spacing: 2) {
                    Text("This device's fingerprint").font(Typo.geist(13, .medium)).foregroundStyle(Theme.textSoft)
                    Text(fp).font(Typo.mono(12.5)).foregroundStyle(Theme.text).textSelection(.enabled).accessibilityIdentifier("device-fingerprint")
                    Text("Others compare it before they share a collection's key with this device.").font(Typo.caption).foregroundStyle(Theme.textMuted)
                }
                .padding(.vertical, 4)
            }
            ForEach(shared.joinable) { w in
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(w.orgName.map { w.orgPersonal ? "Shared with you" : "Shared in \($0)" } ?? "Shared with you").font(Typo.row)
                        Text("You're \(w.role == "owner" ? "an owner" : "a" + (w.role == "editor" ? "n editor" : " viewer"))").font(Typo.caption).foregroundStyle(Theme.textMuted)
                    }
                    Spacer()
                    Button("Open here") {
                        busy = true
                        Task {
                            do { _ = try await shared.join(w.id) } catch { self.error = StoreText.withoutPlanLink(error.localizedDescription) }
                            busy = false
                        }
                    }
                    .disabled(busy)
                    .accessibilityIdentifier("open-shared-\(w.id)")
                }
                .frame(minHeight: 44)
            }
            Button { joining = true } label: { Label("Join with invite code…", systemImage: "person.badge.plus").foregroundStyle(Theme.text) }
                .frame(minHeight: 44)
                .accessibilityIdentifier("join-invite")
            ForEach(shared.notices, id: \.self) { n in Text(n).font(Typo.caption).foregroundStyle(Theme.textSoft) }
            if let error { Text(error).font(Typo.caption).foregroundStyle(Theme.warning) }
        } header: {
            Text("SHARED COLLECTIONS").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
        }
        .listRowBackground(Theme.surface)
        .sheet(isPresented: $joining) { JoinSheet() }
        .task { try? await shared.refreshWorkspaces() }
    }

    private func pendingTrust(_ p: SyncCore.KeyRefusal) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("A shared collection's key came from a device you haven't trusted, fingerprint:").font(Typo.caption).foregroundStyle(Theme.textSoft)
            Text(p.fingerprint ?? "").font(Typo.mono(13, .semibold)).accessibilityIdentifier("pending-fingerprint")
            Text("Compare it with the person who added you. If it isn't the one their device shows, someone (the server, for instance) is trying to give you a key they know: don't trust it.")
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
            Button("They match, trust it") {
                Task {
                    do {
                        try await shared.trustSender(p)
                        await shared.syncAll()
                    } catch { self.error = error.localizedDescription }
                }
            }
            .buttonStyle(.bordered)
            .frame(minHeight: 44)
            .accessibilityIdentifier("trust-sender")
        }
        .padding(.vertical, 4)
    }
}
