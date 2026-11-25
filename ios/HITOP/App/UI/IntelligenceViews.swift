import SwiftUI
import UIKit

// I5: Apple Intelligence screens (the drafts' "Apple Intelligence" page): Explain response with a
// suggested test, and a new request from plain text or a curl command. The model runs on this
// device only; every screen says so in a line.

enum OnDevice {
    static var device: String { UIDevice.current.userInterfaceIdiom == .pad ? "iPad" : "iPhone" }
    static var note: String { "On device · nothing leaves your \(device)" }
}

/// The response's Explain button: a round glass button (iPhone sheet header).
struct ExplainButton: View {
    var requestID: UInt64
    @Environment(Navigator.self) private var nav

    var body: some View {
        Button { nav.explaining = ExplainTarget(id: requestID) } label: {
            Image(systemName: "sparkles").font(.system(size: 16, weight: .medium)).foregroundStyle(Theme.accentText)
                .frame(width: 44, height: 44)
        }
        .buttonStyle(.glass)
        .buttonBorderShape(.circle)
        .accessibilityLabel("Explain with Apple Intelligence")
        .accessibilityHint("Explains this response on this device")
        .accessibilityIdentifier("explain-response")
    }
}

/// The iPad response column's full-width Explain button (the drafts' iPad page), or why it is off.
struct ExplainWideButton: View {
    var requestID: UInt64
    @Environment(Navigator.self) private var nav
    @Environment(Intelligence.self) private var ai

    var body: some View {
        let availability = ai.availability
        VStack(alignment: .leading, spacing: 6) {
            Button { nav.explaining = ExplainTarget(id: requestID) } label: {
                Label("Explain response", systemImage: "sparkles")
                    .font(Typo.geist(14, .semibold, relativeTo: .subheadline))
                    .foregroundStyle(availability.isAvailable ? Theme.text : Theme.textMuted)
                    .frame(maxWidth: .infinity, minHeight: 46)
            }
            .buttonStyle(.glass)
            .disabled(!availability.isAvailable)
            .hoverEffect(.highlight)
            .accessibilityHint(availability.reason ?? "Explains this response on this device, Shift Command E")
            .accessibilityIdentifier("explain-response")
            Text(availability.reason ?? OnDevice.note)
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
                .frame(maxWidth: .infinity)
                .accessibilityIdentifier("explain-availability")
        }
    }
}

/// Presents the Explain sheet for `nav.explaining` (attached where a sheet can be presented: the
/// iPhone response sheet, the iPad split view).
struct ExplainHost: ViewModifier {
    @Environment(Navigator.self) private var nav

    func body(content: Content) -> some View {
        @Bindable var nav = nav
        content.sheet(item: $nav.explaining) { ExplainSheet(target: $0) }
    }
}

struct ExplainSheet: View {
    var target: ExplainTarget
    @Environment(AppModel.self) private var model
    @Environment(Intelligence.self) private var ai
    @Environment(\.dismiss) private var dismiss
    @State private var session: ExplainSession?
    @State private var added = false
    /// Starts at half height; a suggested test opens it to full height, so the script isn't cut
    /// at the sheet's edge.
    @State private var detent: PresentationDetent = .medium

    private var editor: RequestEditor? { model.editor(for: target.id) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    header
                    if let session {
                        explanation(session)
                        suggestion(session)
                    } else {
                        Text("Send the request first: there is no response to explain.").font(Typo.body).foregroundStyle(Theme.textMuted)
                    }
                }
                .padding(16)
            }
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle(editor?.draft.name ?? "Explain")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }.accessibilityIdentifier("explain-done")
                }
            }
        }
        .presentationDetents([.medium, .large], selection: $detent)
        .presentationDragIndicator(.visible)
        .onChange(of: session?.suggestion != nil) { _, suggested in
            if suggested { withAnimation(.hitop) { detent = .large } }
        }
        .task {
            guard session == nil, let editor, case let .success(o)? = editor.outcome else { return }
            let s = ExplainSession(assistant: ai.assistant, request: editor.draft, facts: ResponseFacts(o),
                                   secrets: model.secretValues(for: editor.collectionID))
            session = s
            await s.explain()
        }
    }

    private var header: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "sparkles").font(.system(size: 18, weight: .semibold)).foregroundStyle(Theme.accentText)
                .frame(width: 36, height: 36)
                .background(Circle().fill(Theme.accent.opacity(0.16)))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text("Explain response").font(Typo.geist(17, .semibold, relativeTo: .headline)).foregroundStyle(Theme.text)
                    .accessibilityAddTraits(.isHeader)
                Text(OnDevice.note).font(Typo.caption).foregroundStyle(Theme.textMuted)
                    .accessibilityIdentifier("on-device-note")
            }
        }
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private func explanation(_ s: ExplainSession) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            if let failure = s.failure {
                Label(failure, systemImage: s.availability.isAvailable ? "exclamationmark.triangle" : "sparkles")
                    .font(Typo.body).foregroundStyle(s.availability.isAvailable ? Theme.warning : Theme.textSoft)
                    .accessibilityIdentifier("explain-failure")
                if s.availability.isAvailable {
                    Button("Try again") { Task { await s.explain() } }.font(Typo.geist(14, .semibold)).frame(minHeight: 44)
                }
            } else if s.text.isEmpty {
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Reading the response…").font(Typo.body).foregroundStyle(Theme.textMuted)
                }
                .accessibilityElement(children: .combine)
            } else {
                Text(s.text)
                    .font(Typo.geist(15, relativeTo: .body))
                    .foregroundStyle(Theme.text)
                    .lineSpacing(3)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("explain-text")
                if s.isExplaining {
                    ProgressView().accessibilityLabel("Still writing")
                }
            }
        }
        .padding(14)
        .card(radius: 18)
    }

    @ViewBuilder
    private func suggestion(_ s: ExplainSession) -> some View {
        let readOnly = model.isReadOnly(editor?.collectionID)
        if s.availability.isAvailable {
            VStack(alignment: .leading, spacing: 10) {
                Text("SUGGESTED TEST").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint).accessibilityAddTraits(.isHeader)
                if let suggestion = s.suggestion {
                    CodeBlock(text: suggestion.script, json: false, fill: Theme.sheetCode)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel("Suggested test script")
                        .accessibilityValue(suggestion.checks.map { $0.name.isEmpty ? TestScript.defaultName($0) : $0.name }.joined(separator: ", "))
                        .accessibilityIdentifier("suggested-test")
                    Text("\(suggestion.checks.count == 1 ? "1 check" : "\(suggestion.checks.count) checks"), each checked against this response\(suggestion.dropped > 0 ? " (\(suggestion.dropped) that it fails left out)" : "").")
                        .font(Typo.caption).foregroundStyle(Theme.textMuted)
                    HStack(spacing: 10) {
                        Button {
                            UIPasteboard.general.string = suggestion.script
                        } label: {
                            Label("Copy", systemImage: "doc.on.doc").frame(minHeight: 30)
                        }
                        .buttonStyle(.glass)
                        .accessibilityIdentifier("copy-test")
                        Button {
                            guard let editor else { return }
                            editor.draft.postRequestScript = TestScript.appending(suggestion.script, to: editor.draft.postRequestScript)
                            added = true
                        } label: {
                            Label(added ? "Added to Tests" : "Add test to request", systemImage: added ? "checkmark" : "plus")
                                .foregroundStyle(Theme.onAccent)
                                .frame(maxWidth: .infinity, minHeight: 30)
                        }
                        .buttonStyle(.glassProminent)
                        .tint(Theme.accent)
                        .disabled(added || readOnly)
                        .accessibilityIdentifier("add-test")
                    }
                    if added {
                        Text("Added to the request's Tests. Save the request to keep it.").font(Typo.caption).foregroundStyle(Theme.success)
                            .accessibilityIdentifier("test-added")
                    } else if readOnly {
                        Text("View only: copy the test instead.").font(Typo.caption).foregroundStyle(Theme.textMuted)
                    }
                } else if s.isSuggesting {
                    HStack(spacing: 10) {
                        ProgressView()
                        Text("Writing a test…").font(Typo.body).foregroundStyle(Theme.textMuted)
                    }
                    .accessibilityElement(children: .combine)
                } else {
                    if let f = s.suggestFailure {
                        Text(f).font(Typo.caption).foregroundStyle(Theme.warning).accessibilityIdentifier("suggest-failure")
                    }
                    Button { Task { await s.suggest() } } label: {
                        Label("Suggest a test", systemImage: "checklist").frame(maxWidth: .infinity, minHeight: 30)
                    }
                    .buttonStyle(.glass)
                    .disabled(s.isExplaining)
                    .accessibilityHint("Writes pm.test checks that this response passes")
                    .accessibilityIdentifier("suggest-test")
                }
            }
        }
    }
}

/// "New request from text or curl": a pasted curl command becomes a request at once (no model);
/// plain text goes to Apple Intelligence when it is on.
struct RequestFromTextSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(Navigator.self) private var nav
    @Environment(Intelligence.self) private var ai
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var working = false
    @State private var failure: String?
    @State private var made: (id: UInt64, notes: [String])?
    @FocusState private var focused: Bool

    private var source: RequestFromText.Source { RequestFromText.source(text) }
    private var blank: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    private var canMake: Bool { !blank && !working && (source == .curl || ai.availability.isAvailable) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    Text("Describe the request (\"POST a JSON order to ${baseUrl}/v2/orders\") or paste a curl command.")
                        .font(Typo.body).foregroundStyle(Theme.textSoft)
                    ZStack(alignment: .topLeading) {
                        if text.isEmpty {
                            Text("curl https://… or a sentence").font(Typo.code).foregroundStyle(Theme.textFaint)
                                .padding(.horizontal, 19).padding(.vertical, 22).accessibilityHidden(true)
                        }
                        TextEditor(text: $text)
                            .font(Typo.code)
                            .foregroundStyle(Theme.codeText)
                            .scrollContentBackground(.hidden)
                            .autocorrectionDisabled()
                            .textInputAutocapitalization(.never)
                            .focused($focused)
                            .padding(10)
                            .accessibilityLabel("Request description or curl command")
                            .accessibilityIdentifier("request-text")
                    }
                    .frame(minHeight: 150)
                    .card(radius: 18, fill: Theme.codeGround)
                    sourceNote
                    if let failure {
                        Label(failure, systemImage: "exclamationmark.triangle").font(Typo.caption).foregroundStyle(Theme.warning)
                            .accessibilityIdentifier("request-text-failure")
                    }
                    if let made, !made.notes.isEmpty {
                        VStack(alignment: .leading, spacing: 6) {
                            ForEach(made.notes, id: \.self) { n in
                                Label(n, systemImage: "info.circle").font(Typo.caption).foregroundStyle(Theme.textMuted)
                            }
                        }
                        .accessibilityIdentifier("request-text-notes")
                    }
                }
                .padding(16)
            }
            .background(Theme.ground.ignoresSafeArea())
            .onAppear {
                if text.isEmpty { text = nav.requestText }
                nav.requestText = ""
            }
            .navigationTitle("New request from text")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if let made {
                        Button("Open") { open(made.id) }.accessibilityIdentifier("request-text-open")
                    } else {
                        Button("Create") { create() }
                            .disabled(!canMake)
                            .accessibilityIdentifier("request-text-create")
                    }
                }
            }
            .onAppear { focused = true }
        }
    }

    @ViewBuilder
    private var sourceNote: some View {
        if source == .curl && !blank {
            Label("A curl command: read on this device, no model needed.", systemImage: "terminal")
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
                .accessibilityIdentifier("request-text-source")
        } else if let reason = ai.availability.reason {
            Label("\(reason) A pasted curl command still works.", systemImage: "sparkles")
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
                .accessibilityIdentifier("request-text-source")
        } else {
            Label("Written by Apple Intelligence on this \(OnDevice.device); nothing leaves it. Check the request before you send it.", systemImage: "sparkles")
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
                .accessibilityIdentifier("request-text-source")
        }
    }

    private func create() {
        guard canMake else { return }
        working = true
        failure = nil
        let input = text
        let names = Array(model.context(for: nil).merged.keys)
        Task {
            defer { working = false }
            do {
                let result = try await RequestFromText.make(input, assistant: ai.assistant, variables: names)
                let r = model.newRequest(from: result.request)
                if result.notes.isEmpty {
                    open(r.id)
                } else {
                    made = (r.id, result.notes)
                }
            } catch {
                failure = error.localizedDescription
            }
        }
    }

    private func open(_ id: UInt64) {
        dismiss()
        nav.open(request: id)
    }
}
