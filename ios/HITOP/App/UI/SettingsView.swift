import SwiftUI
import UniformTypeIdentifiers

/// Requests' time limit and certificates (this device only).
struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var failure: String?
    @State private var pickingCA = false
    @State private var pickingP12 = false
    @State private var p12: Data?
    @State private var asking = false
    @State private var password = ""
    @State private var host = ""

    var body: some View {
        List {
            if let store = model.sender.settings { content(store) }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.ground.ignoresSafeArea())
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func attempt(_ f: () throws -> Void) {
        failure = nil
        do { try f() } catch { failure = error.localizedDescription }
    }

    @ViewBuilder
    private func content(_ store: SettingsStore) -> some View {
        if let problem = store.problem {
            Section {
                Text(problem).font(Typo.caption).foregroundStyle(Theme.danger)
                Button("Start over with the default settings", role: .destructive) { attempt { try store.reset() } }
            }
            .listRowBackground(Theme.surface)
        }
        if let failure {
            Section { Text(failure).font(Typo.caption).foregroundStyle(Theme.danger).accessibilityIdentifier("settings-error") }
                .listRowBackground(Theme.surface)
        }
        Section("Requests") {
            Picker("Time limit", selection: Binding(get: { store.shown.timeout }, set: { t in attempt { try store.update { $0.timeout = t } } })) {
                ForEach(Array(Set(AppSettings.timeouts + [store.shown.timeout])).sorted(), id: \.self) { t in
                    Text(t < 120 ? "\(t) seconds" : "\(t / 60) minutes").tag(t)
                }
            }
            .accessibilityIdentifier("settings-timeout")
        }
        .listRowBackground(Theme.surface)
        Section {
            Toggle("Sync in the background", isOn: Binding(get: { store.shown.backgroundSync }, set: { on in attempt { try store.update { $0.backgroundSync = on } } }))
                .tint(Theme.success)
                .accessibilityIdentifier("settings-background-sync")
        } header: {
            Text("HITOP Cloud")
        } footer: {
            Text("While you are signed in. iOS decides when, often a few times a day.")
        }
        .listRowBackground(Theme.surface)
        Section {
            ForEach(store.shown.authorities) { a in
                row(a.name, nil) { attempt { try store.update { $0.authorities.removeAll { $0.id == a.id } } } }
            }
            Button("Add an authority…") { pickingCA = true }
                .accessibilityIdentifier("settings-add-ca")
                .fileImporter(isPresented: $pickingCA, allowedContentTypes: [.data]) { r in
                    attempt {
                        let (data, name) = try read(r)
                        let found = try SettingsStore.authorities(in: data, name: name)
                        try store.update { s in s.authorities += found.filter { f in !s.authorities.contains { $0.der == f.der } } }
                    }
                }
        } header: {
            Text("Certificate authorities")
        } footer: {
            Text("PEM or DER files, trusted for your requests beside the system's. Not for HITOP Cloud. The system's own settings also apply, except to gRPC.")
        }
        .listRowBackground(Theme.surface)
        Section {
            ForEach(store.shown.clientCertificates) { c in
                row(c.name, c.host) {
                    attempt {
                        try store.update { $0.clientCertificates.removeAll { $0.id == c.id } }
                        ClientIdentity.remove(c.reference)
                    }
                }
            }
            Button("Add a client certificate…") { pickingP12 = true }
                .accessibilityIdentifier("settings-add-p12")
                .fileImporter(isPresented: $pickingP12, allowedContentTypes: [.pkcs12]) { r in
                    attempt {
                        p12 = try read(r).0
                        password = ""
                        host = ""
                        asking = true
                    }
                }
        } header: {
            Text("Client certificates")
        } footer: {
            Text("A .p12 file, kept in the Keychain and sent only to the server you name. HTTP and WebSocket; gRPC doesn't use it on iOS.")
        }
        .listRowBackground(Theme.surface)
        .alert("Client certificate", isPresented: $asking) {
            SecureField("Password of the file", text: $password)
            TextField("Server (api.example.com)", text: $host)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Cancel", role: .cancel) { forget() }
            Button("Add") { attempt { try addIdentity(store) } }
        } message: {
            Text("The server it is for, and the file's password (not kept).")
        }
    }

    private func forget() {
        p12 = nil
        password = ""
    }

    private func addIdentity(_ store: SettingsStore) throws {
        defer { forget() }
        guard let data = p12 else { return }
        guard let server = AppSettings.host(host) else { throw Uploads.Failure(message: "Name the server the certificate is for.") }
        if store.shown.clientCertificates.contains(where: { $0.host == server }) {
            throw Uploads.Failure(message: "\(server) has a certificate already: remove it first.")
        }
        let (name, reference) = try ClientIdentity.add(data, password: password)
        do {
            try store.update { $0.clientCertificates.append(.init(name: name, host: server, reference: reference)) }
        } catch {
            ClientIdentity.remove(reference)
            throw error
        }
    }

    private func read(_ r: Result<URL, any Error>) throws -> (Data, String) {
        let url = try r.get()
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        if let n = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize, n > 1024 * 1024 {
            throw Uploads.Failure(message: "\(url.lastPathComponent) is too large for a certificate.")
        }
        return (try Data(contentsOf: url), url.lastPathComponent)
    }

    private func row(_ title: String, _ subtitle: String?, remove: @escaping () -> Void) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(Typo.row).foregroundStyle(Theme.text)
                if let subtitle { Text(subtitle).font(Typo.caption).foregroundStyle(Theme.textMuted) }
            }
            Spacer()
            Button(role: .destructive, action: remove) {
                Image(systemName: "minus.circle.fill").foregroundStyle(Theme.danger).frame(width: 44, height: 44).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove \(title)")
        }
    }
}
