import SwiftUI

/// What the account screens share: the web sign-in (ASWebAuthenticationSession, or the UI tests'
/// stub) and which sheet shows.
@MainActor
@Observable
final class CloudUI {
    @ObservationIgnored let web: WebSignIn
    var showSignIn = false

    init(web: WebSignIn) { self.web = web }
}

/// The organization pill (Collections header, as the drafts): the current organization signed in,
/// HITOP signed out. Opens the Account screen.
struct AccountPill: View {
    @Environment(HitopCloud.self) private var cloud
    @Environment(Navigator.self) private var nav

    private var label: String {
        guard cloud.signedIn else { return "HITOP" }
        guard let org = cloud.currentOrg else { return "Personal" }
        return org.personal ? "Personal" : org.name
    }

    var body: some View {
        Button { nav.showAccount = true } label: {
            HStack(spacing: 8) {
                Text(cloud.signedIn ? String((cloud.account?.email.first ?? "H").uppercased()) : "H")
                    .font(Typo.geist(11, .bold)).foregroundStyle(.white)
                    .frame(width: 24, height: 24).background(Circle().fill(Theme.logoGradient))
                Text(label).font(Typo.geist(14, .medium)).foregroundStyle(Theme.text).lineLimit(1)
                if cloud.signedIn {
                    Image(systemName: "chevron.down").font(.system(size: 10, weight: .bold)).foregroundStyle(Theme.textMuted)
                }
            }
            .padding(.horizontal, 4)
        }
        .accessibilityLabel(cloud.signedIn ? "Organization: \(label)" : "Account and sign in")
        .accessibilityIdentifier("account-pill")
    }
}

/// The collection card's second line: "13 requests · synced 2 min ago" / "· on this iPhone".
struct SyncLine: View {
    @Environment(LibrarySync.self) private var sync
    var collection: SavedCollection

    var body: some View {
        TimelineView(.periodic(from: .now, by: 30)) { context in
            Text(Self.describe(collection, sync, now: context.date))
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
        }
    }

    /// "13 requests · synced 2 min ago" (also the card's VoiceOver label).
    @MainActor
    static func describe(_ c: SavedCollection, _ sync: LibrarySync, now: Date = Date()) -> String {
        let where_: String = switch sync.badges[c.id] ?? .local {
        case .local: "on \(sync.deviceLabel)"
        case .waiting: sync.status.saving ? "saving…" : "waiting to sync"
        case .refused: "not in HITOP Cloud"
        case .synced: sync.status.lastSync.map { "synced \(LibrarySync.ago($0, now: now))" } ?? "synced"
        }
        return "\(c.requests.count) \(c.requests.count == 1 ? "request" : "requests") · \(where_)"
    }
}

/// The Account screen: signed out a Sign in button; signed in the account, its sync, organization,
/// plan usage (neutral words, nothing to buy: store mode) and devices.
struct AccountView: View {
    @Environment(HitopCloud.self) private var cloud
    @Environment(LibrarySync.self) private var sync
    @Environment(CloudUI.self) private var ui
    @Environment(\.dismiss) private var dismiss
    @State private var devices: [CloudDevice]?
    @State private var usage: CloudUsage?
    @State private var loadError: String?
    @State private var renaming: CloudDevice?
    @State private var newName = ""
    @State private var removing: CloudDevice?
    @State private var confirmSignOut = false
    @State private var about = false
    @State private var busy = false

    var body: some View {
        @Bindable var ui = ui
        NavigationStack {
            List {
                if cloud.signedIn { signedIn } else { signedOut }
                Section {
                    Button { about = true } label: { Label("About HITOP", systemImage: "info.circle").foregroundStyle(Theme.text) }
                        .frame(minHeight: 44)
                }
                .listRowBackground(Theme.surface)
            }
            .scrollContentBackground(.hidden)
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle(cloud.signedIn ? "Account" : "HITOP Cloud")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() }.accessibilityIdentifier("account-done") } }
            .refreshable { await reload() }
            .task(id: cloud.accountKey) { await reload() }
            .sheet(isPresented: $ui.showSignIn) { SignInSheet() }
            .sheet(isPresented: $about) { AboutView() }
            .alert("Rename device", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
                TextField("Name", text: $newName).accessibilityIdentifier("device-name-field")
                Button("Cancel", role: .cancel) { renaming = nil }
                Button("Save") { rename() }
            }
            .confirmationDialog(removeTitle, isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }), titleVisibility: .visible) {
                Button("Remove", role: .destructive) { remove() }
                Button("Cancel", role: .cancel) { removing = nil }
            } message: {
                Text(removing.map { $0.workspaces > 0 ? "It holds keys to shared collections: they get a new key, and it can't open them any more." : "It is signed out of HITOP Cloud." } ?? "")
            }
            .confirmationDialog("Sign out of HITOP Cloud?", isPresented: $confirmSignOut, titleVisibility: .visible) {
                Button("Sign out", role: .destructive) { Task { await cloud.signOut() } }.accessibilityIdentifier("confirm-sign-out")
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Your collections stay on this \(sync.deviceLabel.replacingOccurrences(of: "this ", with: "")). Signing in again is the same device.")
            }
        }
        .presentationDetents([.large])
    }

    // MARK: Signed out

    @ViewBuilder
    private var signedOut: some View {
        Section {
            VStack(spacing: 12) {
                Image(systemName: "icloud.and.arrow.up")
                    .font(.system(size: 34, weight: .medium)).foregroundStyle(Theme.accent)
                    .frame(width: 72, height: 72)
                    .background(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Theme.chipFill))
                    .accessibilityHidden(true)
                Text("Your collections on every device").font(Typo.geist(20, .bold)).multilineTextAlignment(.center)
                Text("Sign in to keep collections, environments and globals in HITOP Cloud, the same in the web app, the desktop app and on your other devices. Free to create; everything keeps working without an account.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted).multilineTextAlignment(.center)
                Button { ui.showSignIn = true } label: {
                    Text("Sign in").font(Typo.geist(16, .semibold)).frame(maxWidth: .infinity, minHeight: 44)
                }
                .buttonStyle(.glassProminent)
                .tint(Theme.accent)
                .accessibilityIdentifier("sign-in")
                if let e = cloud.lastError {
                    Text(e).font(Typo.caption).foregroundStyle(Theme.warning).multilineTextAlignment(.center)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 10)
        }
        .listRowBackground(Color.clear)
    }

    // MARK: Signed in

    @ViewBuilder
    private var signedIn: some View {
        if let a = cloud.account {
            Section {
                HStack(spacing: 12) {
                    Text(String(a.email.first ?? "?").uppercased())
                        .font(Typo.geist(18, .bold)).foregroundStyle(.white)
                        .frame(width: 44, height: 44).background(Circle().fill(Theme.logoGradient))
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(a.email).font(Typo.title).accessibilityIdentifier("account-email")
                        Text(URL(string: a.server)?.host ?? a.server).font(Typo.caption).foregroundStyle(Theme.textMuted)
                    }
                }
                .padding(.vertical, 4)
                HStack {
                    syncStatus
                    Spacer()
                    Button("Sync now") { Task { await sync.sync() } }
                        .font(Typo.geist(14, .medium))
                        .disabled(sync.status.saving || sync.status.pulling)
                        .accessibilityIdentifier("sync-now")
                }
                .frame(minHeight: 44)
                ForEach(sync.notices, id: \.self) { n in
                    Text(n).font(Typo.caption).foregroundStyle(Theme.textSoft)
                }
                if sync.status.secretsStored == false {
                    Text("This server doesn't keep secret values: they stay in this device's Keychain.")
                        .font(Typo.caption).foregroundStyle(Theme.textMuted)
                }
            }
            .listRowBackground(Theme.surface)

            if cloud.settings.orgs.count > 1 {
                Section {
                    ForEach(cloud.settings.orgs) { org in
                        Button { cloud.selectOrg(org.id) } label: {
                            HStack {
                                Image(systemName: org.personal ? "person" : "person.3").foregroundStyle(Theme.textMuted).frame(width: 24)
                                Text(org.personal ? "Personal" : org.name).foregroundStyle(Theme.text)
                                Spacer()
                                if cloud.currentOrg?.id == org.id { Image(systemName: "checkmark").foregroundStyle(Theme.accent) }
                            }
                            .frame(minHeight: 44)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("org-\(org.personal ? "Personal" : org.name)")
                        .accessibilityAddTraits(cloud.currentOrg?.id == org.id ? .isSelected : [])
                    }
                } header: {
                    header("Organization")
                } footer: {
                    Text("Collections shows the organization's collections; new ones are made in it.").font(Typo.caption)
                }
                .listRowBackground(Theme.surface)
            }

            if let usage {
                Section {
                    HStack {
                        Text("Plan").foregroundStyle(Theme.textSoft)
                        Spacer()
                        Text(usage.planName).font(Typo.geist(14, .semibold)).foregroundStyle(Theme.accentText).accessibilityIdentifier("plan-name")
                    }
                    .frame(minHeight: 44)
                    if usage.ended {
                        Text("This plan has ended: Free limits apply. Nothing is deleted.").font(Typo.caption).foregroundStyle(Theme.warning)
                    }
                    ForEach(usage.meters) { m in meter(m) }
                } header: {
                    header("Plan & usage")
                }
                .listRowBackground(Theme.surface)
            }

            Section {
                if let devices {
                    ForEach(devices) { d in deviceRow(d) }
                } else if let loadError {
                    Text(loadError).font(Typo.caption).foregroundStyle(Theme.warning)
                } else {
                    ProgressView().frame(maxWidth: .infinity, minHeight: 44)
                }
            } header: {
                header("Devices")
            }
            .listRowBackground(Theme.surface)

            Section {
                Button(role: .destructive) { confirmSignOut = true } label: {
                    Text("Sign out").frame(maxWidth: .infinity, minHeight: 44)
                }
                .accessibilityIdentifier("sign-out")
            }
            .listRowBackground(Theme.surface)
        }
    }

    private var syncStatus: some View {
        let s = sync.status
        let text: String
        let color: Color
        if s.saving || s.pulling {
            text = "Syncing…"
            color = Theme.textMuted
        } else if let e = s.error {
            text = e
            color = Theme.warning
        } else if let at = s.lastSync {
            text = "Synced \(LibrarySync.ago(at))"
            color = Theme.success
        } else {
            text = "Not synced yet"
            color = Theme.textMuted
        }
        return Label {
            Text(text).font(Typo.caption).foregroundStyle(color).lineLimit(3)
        } icon: {
            Image(systemName: s.error == nil ? "checkmark.icloud" : "exclamationmark.icloud").foregroundStyle(color)
        }
        .accessibilityIdentifier("sync-status")
    }

    private func header(_ s: String) -> some View {
        Text(s.uppercased()).font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
    }

    private func meter(_ m: UsageMeter) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(m.label).font(Typo.geist(14)).foregroundStyle(Theme.textSoft)
                Spacer()
                Text(m.text).font(Typo.mono(12.5)).foregroundStyle(Theme.text)
            }
            if m.max != nil {
                ProgressView(value: m.fraction)
                    .tint(m.level == "full" ? Theme.danger : m.level == "warn" ? Theme.warning : Theme.accent)
            }
            if !m.note.isEmpty {
                Text(m.note).font(Typo.caption).foregroundStyle(m.level == "full" ? Theme.danger : Theme.warning)
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("meter-\(m.id)")
    }

    private func deviceRow(_ d: CloudDevice) -> some View {
        HStack(spacing: 12) {
            Image(systemName: d.thisDevice ? "iphone" : "laptopcomputer.and.iphone").foregroundStyle(Theme.textMuted).frame(width: 24)
            VStack(alignment: .leading, spacing: 2) {
                Text(d.name.isEmpty ? "Device" : d.name).font(Typo.row).foregroundStyle(Theme.textRow)
                Text(detail(d)).font(Typo.caption).foregroundStyle(Theme.textMuted)
            }
            Spacer()
        }
        .frame(minHeight: 44)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("device-\(d.name)")
        .contextMenu {
            Button("Rename", systemImage: "pencil") { newName = d.name; renaming = d }
            if !d.thisDevice { Button("Remove", systemImage: "trash", role: .destructive) { removing = d } }
        }
        .swipeActions(edge: .trailing) {
            if !d.thisDevice { Button("Remove", systemImage: "trash", role: .destructive) { removing = d } }
            Button("Rename", systemImage: "pencil") { newName = d.name; renaming = d }.tint(Theme.accentDeep)
        }
    }

    private func detail(_ d: CloudDevice) -> String {
        if d.thisDevice { return "This device" }
        guard let last = d.lastUsedAt else { return "No recent use" }
        return "Used \(LibrarySync.ago(Date(timeIntervalSince1970: TimeInterval(last))))"
    }

    private var removeTitle: String { "Remove \(removing?.name ?? "this device")?" }

    private func reload() async {
        guard cloud.signedIn else { return }
        do {
            devices = try await cloud.devices() ?? []
            let orgs = try await cloud.loadOrgs()
            if let org = cloud.currentOrg ?? orgs.first {
                usage = try? await cloud.usage(org.id)
            }
            loadError = nil
        } catch {
            loadError = error.localizedDescription
        }
    }

    private func rename() {
        guard let d = renaming else { return }
        renaming = nil
        Task {
            do { try await cloud.renameDevice(d.id, to: newName) } catch { loadError = error.localizedDescription }
            await reload()
        }
    }

    private func remove() {
        guard let d = removing else { return }
        removing = nil
        Task {
            do { try await cloud.removeDevice(d.id, force: d.workspaces > 0) } catch { loadError = error.localizedDescription }
            await reload()
        }
    }
}

/// Sign in: Continue with Apple, Google or GitHub (what the server offers), through the system's
/// sign-in sheet and the server's one-time code; the paste step for a server that can't send the
/// code back to the app; the device limit's remove-one step (store mode: no plans, no prices).
struct SignInSheet: View {
    @Environment(HitopCloud.self) private var cloud
    @Environment(CloudUI.self) private var ui
    @Environment(\.dismiss) private var dismiss

    enum Step: Equatable { case providers, server, paste, finishing, devices }

    @State private var step: Step = .providers
    @State private var methods: AuthMethods?
    @State private var loading = true
    @State private var methodsError: String?
    @State private var error: String?
    @State private var kind = ""
    @State private var pageURL: URL?
    @State private var code = ""
    @State private var serverText = ""
    @State private var limit: DeviceLimit?
    @State private var deviceErrors: [String: String] = [:]
    @State private var busy = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    switch step {
                    case .providers: providers
                    case .server: serverStep
                    case .paste: pasteStep
                    case .finishing: finishing
                    case .devices: devicesStep
                    }
                }
                .padding(20)
            }
            .background(Theme.ground.ignoresSafeArea())
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.accessibilityIdentifier("sign-in-cancel") }
            }
            .task { await loadMethods() }
            .onChange(of: cloud.signedIn) { _, now in if now { dismiss() } }
        }
        .presentationDetents([.large])
    }

    // MARK: Steps

    @ViewBuilder
    private var providers: some View {
        VStack(alignment: .center, spacing: 10) {
            Text("H").font(Typo.geist(26, .bold)).foregroundStyle(.white)
                .frame(width: 60, height: 60)
                .background(RoundedRectangle(cornerRadius: 16, style: .continuous).fill(Theme.logoGradient))
                .accessibilityHidden(true)
            Text("Sign in to HITOP").font(Typo.geist(24, .bold))
            Text("Sync your collections across devices. New here? The same button creates your account.")
                .font(Typo.caption).foregroundStyle(Theme.textMuted).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.bottom, 6)
        if loading {
            ProgressView().frame(maxWidth: .infinity, minHeight: 120)
        } else if let methodsError {
            Text(methodsError).font(Typo.caption).foregroundStyle(Theme.warning)
            Button("Try again") { Task { await loadMethods() } }
        } else if let methods, !methods.providers.isEmpty {
            VStack(spacing: 10) {
                ForEach(methods.providers, id: \.self) { k in providerButton(k) }
            }
        } else {
            Text("\(host) offers no sign-in this app has (Apple, Google or GitHub). Company single sign-on comes in a later version; until then sign in with the web or desktop app.")
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
                .accessibilityIdentifier("no-methods")
        }
        if let error {
            Text(error).font(Typo.caption).foregroundStyle(Theme.danger).accessibilityIdentifier("sign-in-error")
        }
        Text("Requests, environments and history keep working without an account.")
            .font(Typo.caption).foregroundStyle(Theme.textMuted)
        HStack {
            Link("Privacy policy", destination: URL(string: "https://hitop.rws8.tech/privacy.html")!)
            Spacer()
            Button("Your own server…") {
                serverText = cloud.settings.server
                error = nil
                step = .server
            }
            .accessibilityIdentifier("own-server")
        }
        .font(Typo.geist(14, .medium))
    }

    private func providerButton(_ k: String) -> some View {
        Button { Task { await begin(k) } } label: {
            HStack(spacing: 10) {
                Image(systemName: k == "apple" ? "apple.logo" : k == "github" ? "chevron.left.forwardslash.chevron.right" : "g.circle")
                    .font(.system(size: 17, weight: .semibold))
                    .frame(width: 22)
                Text("Continue with \(AuthMethods.name(k))").font(Typo.geist(16, .semibold))
            }
            .frame(maxWidth: .infinity, minHeight: 48)
        }
        .buttonStyle(.glass)
        .disabled(busy)
        .accessibilityIdentifier("continue-\(k)")
    }

    @ViewBuilder
    private var serverStep: some View {
        Button("‹ Back") { step = .providers }.font(Typo.geist(14, .medium))
        Text("Your own server").font(Typo.geist(22, .bold))
        Text("Sign in on a HITOP sync server your team runs. Leave it empty for HITOP Cloud.")
            .font(Typo.caption).foregroundStyle(Theme.textMuted)
        TextField("https://sync.example.com", text: $serverText)
            .textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.URL)
            .font(Typo.mono(14))
            .padding(12).card(radius: 14)
            .accessibilityIdentifier("server-field")
        if let error { Text(error).font(Typo.caption).foregroundStyle(Theme.danger) }
        Button {
            do {
                try cloud.setServer(serverText)
                error = nil
                step = .providers
                Task { await loadMethods() }
            } catch {
                self.error = error.localizedDescription
            }
        } label: {
            Text("Use this server").frame(maxWidth: .infinity, minHeight: 44)
        }
        .buttonStyle(.glassProminent).tint(Theme.accent)
        .accessibilityIdentifier("use-server")
    }

    @ViewBuilder
    private var pasteStep: some View {
        Text("Finish in your browser").font(Typo.geist(22, .bold))
        Text("This server can't send the code back to the app. Sign in with \(AuthMethods.name(kind)) on the page, copy the code it shows, and paste it here.")
            .font(Typo.caption).foregroundStyle(Theme.textMuted)
        TextField("Code shown after signing in", text: $code)
            .textInputAutocapitalization(.never).autocorrectionDisabled()
            .font(Typo.mono(14))
            .padding(12).card(radius: 14)
            .accessibilityIdentifier("code-field")
        if let error { Text(error).font(Typo.caption).foregroundStyle(Theme.danger).accessibilityIdentifier("sign-in-error") }
        Button { Task { await finish(code) } } label: {
            Text("Sign in").frame(maxWidth: .infinity, minHeight: 44)
        }
        .buttonStyle(.glassProminent).tint(Theme.accent)
        .disabled(code.trimmingCharacters(in: .whitespaces).isEmpty || busy)
        .accessibilityIdentifier("paste-sign-in")
        if let pageURL {
            Button("Open the sign-in page again") { Task { _ = try? await ui.web.open(pageURL) } }
                .font(Typo.geist(14, .medium))
        }
    }

    private var finishing: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text("Signing in…").font(Typo.caption).foregroundStyle(Theme.textMuted)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
        .accessibilityIdentifier("signing-in")
    }

    @ViewBuilder
    private var devicesStep: some View {
        if let limit {
            Text("Remove a device to sign in here").font(Typo.geist(22, .bold)).accessibilityIdentifier("device-limit")
            Text("This account allows \(limit.max) \(limit.max == 1 ? "device" : "devices") and has \(limit.devices.count). Remove one you no longer use; nothing else on the account changes.")
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
            VStack(spacing: 0) {
                ForEach(Array(limit.devices.enumerated()), id: \.element.id) { i, d in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Image(systemName: "laptopcomputer.and.iphone").foregroundStyle(Theme.textMuted)
                            Text(d.name.isEmpty ? "Device \(i + 1)" : d.name).font(Typo.row)
                            Spacer()
                            Button("Remove") { Task { await removeDevice(d) } }
                                .buttonStyle(.glass)
                                .disabled(busy)
                                .accessibilityLabel("Remove \(d.name.isEmpty ? "device \(i + 1)" : d.name)")
                                .accessibilityIdentifier("remove-\(d.name)")
                        }
                        if let e = deviceErrors[d.id] { Text(e).font(Typo.caption).foregroundStyle(Theme.danger) }
                    }
                    .padding(12)
                    if i < limit.devices.count - 1 { Divider().overlay(Theme.separator) }
                }
            }
            .card(radius: 18)
            if let error { Text(error).font(Typo.caption).foregroundStyle(Theme.danger) }
        }
    }

    // MARK: Actions

    private var host: String { URL(string: cloud.settings.server)?.host ?? cloud.settings.server }

    private func loadMethods() async {
        loading = true
        methodsError = nil
        defer { loading = false }
        do {
            methods = try await cloud.authMethods(cloud.settings.server) ?? AuthMethods()
        } catch {
            methodsError = "Couldn't reach \(host). \(error.localizedDescription)"
        }
    }

    private func begin(_ k: String) async {
        kind = k
        error = nil
        busy = true
        defer { busy = false }
        do {
            let start = try await cloud.startBrowserSignIn(k, server: cloud.settings.server)
            pageURL = start.url
            if start.paste {
                step = .paste
                _ = try await ui.web.open(start.url)
                return
            }
            guard let back = try await ui.web.open(start.url) else { return } // closed: nothing happens
            guard let c = HitopCloud.code(from: back.absoluteString) else {
                error = "The sign-in didn't finish. Try again."
                return
            }
            await finish(c)
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func finish(_ c: String) async {
        let was = step
        step = .finishing
        busy = true
        defer { busy = false }
        do {
            try await cloud.finishBrowserSignIn(c)
        } catch let l as DeviceLimit {
            limit = l
            step = .devices
        } catch {
            self.error = error.localizedDescription
            step = was == .paste ? .paste : .providers
        }
    }

    private func removeDevice(_ d: CloudDevice) async {
        guard let l = limit else { return }
        busy = true
        defer { busy = false }
        deviceErrors = [:]
        do {
            let next = try await cloud.removeDevice(d.id, at: l)
            limit = next
            step = .finishing
            try await cloud.retry(next)
        } catch let again as DeviceLimit {
            limit = again
            step = .devices
        } catch let e as CloudError where e.code == "key_in_use" {
            deviceErrors[d.id] = "This device holds keys to shared collections, so it stays. Remove it from that device, or pick another."
            step = .devices
        } catch {
            self.error = error.localizedDescription
            step = .devices
        }
    }
}
