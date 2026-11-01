import HitopKit
import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @Environment(AppModel.self) private var model
    @State private var picking = false

    var body: some View {
        NavigationStack {
            List {
                if model.folders.isEmpty {
                    ContentUnavailableView("No collections", systemImage: "folder",
                                           description: Text("Open a hitop collection folder (with a hitop.yaml) from Files or iCloud Drive."))
                }
                ForEach(model.folders) { saved in
                    NavigationLink(value: saved.id) {
                        FolderRow(saved: saved, status: model.statuses[saved.id])
                    }
                    .swipeActions {
                        Button("Remove", role: .destructive) { model.remove(saved.id) }
                        Button(saved.pinned ? "Unpin" : "Pin") { model.togglePin(saved.id) }.tint(.orange)
                    }
                }
                if let e = model.readErrors["new"] {
                    Text(e).foregroundStyle(.red).font(.footnote)
                }
            }
            .navigationTitle("HITOP")
            .navigationDestination(for: String.self) { id in CollectionView(id: id) }
            .toolbar {
                Button { picking = true } label: { Label("Open folder", systemImage: "folder.badge.plus") }
            }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
                if case let .success(url) = result { model.add(url) }
            }
        }
    }
}

struct FolderRow: View {
    let saved: SavedFolder
    let status: RunStatus?

    var body: some View {
        HStack {
            VStack(alignment: .leading) {
                Text(saved.name).font(.headline)
                Text(saved.environment.map { "Environment: \($0)" } ?? "No environment").font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if saved.pinned { Image(systemName: "pin.fill").foregroundStyle(.orange) }
            if let status { StatusBadge(passed: status.passed, text: status.summary) }
        }
    }
}

struct StatusBadge: View {
    let passed: Bool
    let text: String

    var body: some View {
        Label(text, systemImage: passed ? "checkmark.circle.fill" : "xmark.circle.fill")
            .font(.caption.bold())
            .foregroundStyle(passed ? .green : .red)
    }
}

struct CollectionView: View {
    @Environment(AppModel.self) private var model
    let id: String
    @State private var showSecrets = false

    var body: some View {
        let saved = model.folders.first { $0.id == id }
        let folder = model.loaded[id]
        List {
            if let e = model.readErrors[id] {
                Section { Text(e).foregroundStyle(.red) }
            }
            if let folder, let saved {
                Section("Environment") {
                    Picker("Environment", selection: Binding(get: { saved.environment ?? "" },
                                                             set: { model.setEnvironment($0.isEmpty ? nil : $0, for: id) })) {
                        Text("None").tag("")
                        ForEach(folder.environments) { Text($0.name).tag($0.name) }
                    }
                    if !model.secrets(for: id).isEmpty {
                        Button("Secrets…") { showSecrets = true }
                    }
                }
                Section {
                    Button {
                        Task { await model.run(id) }
                    } label: {
                        HStack {
                            Label("Run collection", systemImage: "play.fill")
                            if model.running.contains(id) { Spacer(); ProgressView() }
                        }
                    }
                    .disabled(model.running.contains(id))
                    if let e = model.runErrors[id] { Text(e).foregroundStyle(.red).font(.footnote) }
                    if let report = model.reports[id] { NavigationLink("Last run: \(report.requests.passed)/\(report.requests.total) passed") { ReportView(report: report) } }
                }
                Section("Requests") {
                    ForEach(folder.collection.requests) { r in RequestRow(folderID: id, request: r) }
                }
            }
        }
        .navigationTitle(folder?.collection.name ?? saved?.name ?? "")
        .refreshable { if let saved { model.load(saved) } }
        .task { if let saved, model.loaded[id] == nil { model.load(saved) } }
        .sheet(isPresented: $showSecrets) { SecretsView(folderID: id) }
    }
}

struct RequestRow: View {
    @Environment(AppModel.self) private var model
    let folderID: String
    let request: Request

    var body: some View {
        let key = String(request.id)
        let unsupported = request.isWebSocket ? "WebSocket: not run on iOS" : request.isGRPC ? "gRPC: not run on iOS" : nil
        VStack(alignment: .leading) {
        HStack {
            Text(request.isWebSocket ? "WS" : request.isGRPC ? "gRPC" : request.method)
                .font(.caption.monospaced().bold()).frame(width: 52, alignment: .leading)
            VStack(alignment: .leading) {
                Text(request.name)
                Text(unsupported ?? request.path).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if model.running.contains(key) {
                ProgressView()
            } else if let r = model.reports[key]?.collections.first?.iterations.first?.requests.first {
                NavigationLink { RequestResultView(result: r) } label: { StatusBadge(passed: r.passed, text: r.status.map(String.init) ?? "error") }
                    .fixedSize()
            }
            if unsupported == nil {
                Button { Task { await model.run(folderID, request: request) } } label: { Image(systemName: "play.circle") }
                    .buttonStyle(.borderless)
            }
        }
        if let e = model.runErrors[key] { Text(e).foregroundStyle(.red).font(.footnote) }
        }
    }
}

struct ReportView: View {
    let report: Report

    var body: some View {
        List {
            Section {
                StatusBadge(passed: report.passed, text: "\(report.requests.passed)/\(report.requests.total) requests, \(report.assertions.passed)/\(report.assertions.total) assertions")
                Text("\(report.durationMs) ms, started \(report.startedAt)").font(.caption).foregroundStyle(.secondary)
            }
            ForEach(Array(report.collections.flatMap(\.iterations).flatMap(\.requests).enumerated()), id: \.offset) { _, r in
                NavigationLink { RequestResultView(result: r) } label: {
                    HStack {
                        Text(r.name)
                        Spacer()
                        StatusBadge(passed: r.passed, text: r.status.map(String.init) ?? "error")
                    }
                }
            }
            Section {
                ShareLink(item: report.jsonText, preview: SharePreview("report.json")) { Label("Share report.json", systemImage: "square.and.arrow.up") }
            }
        }
        .navigationTitle("Run")
    }
}

struct RequestResultView: View {
    let result: Report.RequestReport

    var body: some View {
        List {
            Section {
                LabeledContent("Method", value: result.method)
                Text(result.url).font(.caption.monospaced()).textSelection(.enabled)
                LabeledContent("Status", value: result.status.map(String.init) ?? "none")
                if let t = result.responseTimeMs { LabeledContent("Response time", value: "\(t) ms") }
                if let f = result.file { LabeledContent("File", value: f) }
            }
            if let e = result.error { Section("Error") { Text(e).foregroundStyle(.red) } }
            if !result.assertions.isEmpty {
                Section("Assertions") {
                    ForEach(Array(result.assertions.enumerated()), id: \.offset) { _, a in
                        VStack(alignment: .leading) {
                            Label(a.name, systemImage: a.passed ? "checkmark" : "xmark").foregroundStyle(a.passed ? .green : .red)
                            if let m = a.message { Text(m).font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                }
            }
            if !result.notes.isEmpty { Section("Not run") { ForEach(result.notes, id: \.self) { Text($0) } } }
        }
        .navigationTitle(result.name)
    }
}

/// Values for secret variables, kept in the keychain (never in the folder).
struct SecretsView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let folderID: String
    @State private var values: [String: String] = [:]
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                ForEach(model.secrets(for: folderID)) { s in
                    let key = s.id
                    Section("\(s.name) (\(s.environment))") {
                        SecureField(s.isSet ? "•••• (stored in the keychain)" : "Value", text: Binding(get: { values[key] ?? "" }, set: { values[key] = $0 }))
                    }
                }
                Text("Values stay in this device's keychain. Leave a field empty to keep its value.").font(.footnote).foregroundStyle(.secondary)
                if let error { Text(error).foregroundStyle(.red) }
            }
            .navigationTitle("Secrets")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        do {
                            for (key, value) in values where !value.isEmpty {
                                let parts = key.split(separator: "/", maxSplits: 1).map(String.init)
                                try model.setSecret(value, environment: parts[0], name: parts[1], folder: folderID)
                            }
                            dismiss()
                        } catch {
                            self.error = error.localizedDescription
                        }
                    }
                }
            }
        }
    }
}
