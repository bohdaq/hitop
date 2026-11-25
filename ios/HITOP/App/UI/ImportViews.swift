import SwiftUI
import UniformTypeIdentifiers

/// Import: a file, pasted text, or an OpenAPI address; what it holds and what doesn't come over are
/// shown before anything is added.
struct ImportSheet: View {
    var target: ImportTarget
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var picking = false
    @State private var reading = false
    @State private var plan: Importing.Plan?
    @State private var failure: String?
    @FocusState private var focused: Bool

    private static let types: [UTType] = [.json, .yaml, .zip, UTType("tech.rws8.hitop.har") ?? .data, .plainText]
    private var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if let plan { found(plan) } else { form }
                    if let failure {
                        Label(failure, systemImage: "exclamationmark.triangle").font(Typo.caption).foregroundStyle(Theme.warning)
                            .accessibilityIdentifier("import-failure")
                    }
                }
                .padding(16)
            }
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle("Import")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.accessibilityIdentifier("import-cancel") }
                ToolbarItem(placement: .confirmationAction) {
                    if let plan {
                        Button("Add") {
                            model.apply(plan)
                            dismiss()
                        }
                        .disabled(plan.summary.isEmpty)
                        .accessibilityIdentifier("import-add")
                    } else {
                        Button("Read") { readText() }
                            .disabled(trimmed.isEmpty || reading)
                            .accessibilityIdentifier("import-read")
                    }
                }
            }
        }
        .fileImporter(isPresented: $picking, allowedContentTypes: Self.types) { result in
            switch result {
            case let .success(url): readFile(url)
            case let .failure(e): failure = e.localizedDescription
            }
        }
        .task { if let t = target.text, plan == nil { text = t; readText() } }
        .task { if let f = target.file, plan == nil { readFile(f) } }
    }

    @ViewBuilder
    private var form: some View {
        Text("Postman, Insomnia, Bruno, HAR, OpenAPI and HITOP files, or a collection folder's .zip. Or paste one here, or the address of an OpenAPI document.")
            .font(Typo.body).foregroundStyle(Theme.textSoft)
        Button { picking = true } label: { Label("Choose a file", systemImage: "doc").frame(maxWidth: .infinity, minHeight: 44) }
            .buttonStyle(.bordered)
            .accessibilityIdentifier("import-file")
        TextEditor(text: $text)
            .font(Typo.code)
            .foregroundStyle(Theme.codeText)
            .scrollContentBackground(.hidden)
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
            .focused($focused)
            .padding(10)
            .frame(minHeight: 150)
            .card(radius: 18, fill: Theme.codeGround)
            .accessibilityLabel("Text or address to import")
            .accessibilityIdentifier("import-text")
        if reading { ProgressView().frame(maxWidth: .infinity) }
    }

    @ViewBuilder
    private func found(_ p: Importing.Plan) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(p.title).font(Typo.title).foregroundStyle(Theme.text)
            ForEach(p.collections) { c in Text(c.name).font(Typo.row).foregroundStyle(Theme.textRow) }
            Text(p.summary.isEmpty ? "Nothing to import in it." : p.summary).font(Typo.caption).foregroundStyle(Theme.textMuted)
                .accessibilityIdentifier("import-summary")
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .card(radius: 18)
        if !p.issues.isEmpty {
            Text("Check after import").font(Typo.title).foregroundStyle(Theme.text).accessibilityAddTraits(.isHeader)
            LazyVStack(alignment: .leading, spacing: 8) {
                ForEach(Array(p.issues.enumerated()), id: \.offset) { _, i in
                    Text(i).font(Typo.caption).foregroundStyle(Theme.textSoft)
                }
            }
            .accessibilityIdentifier("import-issues")
        }
    }

    private func readText() {
        focused = false
        let t = trimmed
        let isAddress = !t.contains(where: \.isWhitespace) && (t.hasPrefix("https://") || t.hasPrefix("http://"))
        guard isAddress, let url = URL(string: t) else {
            read { try Importing.read(text: t) }
            return
        }
        reading = true
        failure = nil
        Task {
            defer { reading = false }
            do {
                let (data, response) = try await URLSession.shared.data(from: url)
                let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                guard (200..<300).contains(status) else { throw Importing.Failure(message: "The address answered \(status).") }
                plan = try await Task.detached { try Importing.read(data, source: t) }.value
            } catch {
                failure = error.localizedDescription
            }
        }
    }

    private func readFile(_ url: URL) {
        read {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            // A file another app handed over is a copy in this app's Inbox: not kept.
            defer { if url.deletingLastPathComponent().lastPathComponent == "Inbox" { try? FileManager.default.removeItem(at: url) } }
            return try Importing.read(Data(contentsOf: url), name: url.deletingPathExtension().lastPathComponent)
        }
    }

    private func read(_ work: @escaping @Sendable () throws -> Importing.Plan) {
        reading = true
        failure = nil
        Task {
            defer { reading = false }
            do { plan = try await Task.detached { try work() }.value } catch { failure = error.localizedDescription }
        }
    }
}

/// A file made when it is shared, so nothing is written before that.
struct ExportedFile: Transferable {
    var collection: SavedCollection
    var format: Importing.Format

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .data) { f in
            let (name, data) = try Importing.export(f.collection, as: f.format)
            let dir = FileManager.default.temporaryDirectory.appendingPathComponent("export-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let url = dir.appendingPathComponent(name)
            try data.write(to: url, options: [.atomic, .completeFileProtection])
            return SentTransferredFile(url)
        }
    }
}

/// "Export" in a collection's menu: one share button per format.
struct ExportMenu: View {
    var collection: SavedCollection

    var body: some View {
        Menu("Export", systemImage: "square.and.arrow.up") {
            ForEach(Importing.Format.allCases) { f in
                ShareLink(f.label, item: ExportedFile(collection: collection, format: f), preview: SharePreview(collection.name))
            }
        }
    }
}
