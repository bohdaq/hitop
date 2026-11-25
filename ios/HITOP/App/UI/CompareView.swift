import SwiftUI

/// Two responses of one request side by side in words: what was added, removed, changed.
struct CompareSheet: View {
    var sources: [ComparedResponse]
    @Environment(\.dismiss) private var dismiss
    @State private var left = ""
    @State private var right = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(spacing: 0) {
                        picker("From", $left, "compare-from")
                        Divider().overlay(Theme.separator)
                        picker("To", $right, "compare-to")
                    }
                    .padding(.horizontal, 14)
                    .card(radius: 18)
                    if let a = sources.first(where: { $0.id == left }), let b = sources.first(where: { $0.id == right }) {
                        CompareResult(result: ResponseDiff.compare(a, b))
                    }
                }
                .padding(16)
            }
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle("Compare responses")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { dismiss() }.accessibilityIdentifier("compare-close") } }
        }
        .onAppear {
            // What came before, to what is here now.
            if left.isEmpty, sources.count > 1 {
                left = sources[1].id
                right = sources[0].id
            }
        }
    }

    private func picker(_ title: String, _ id: Binding<String>, _ identifier: String) -> some View {
        Picker(title, selection: id) {
            ForEach(sources) { s in Text(s.label).tag(s.id) }
        }
        .pickerStyle(.navigationLink)
        .font(Typo.row)
        .frame(minHeight: 48)
        .accessibilityIdentifier(identifier)
    }
}

private struct CompareResult: View {
    var result: ResponseDiff.Result

    var body: some View {
        if result.same {
            Label("No differences", systemImage: "checkmark.circle").font(Typo.row).foregroundStyle(Theme.success)
                .accessibilityIdentifier("compare-same")
        }
        if let s = result.status { section("STATUS", [s]) }
        if !result.headers.isEmpty { section("HEADERS", result.headers) }
        let body = result.json ? result.body : result.body.filter { $0.kind != .same }
        if !body.isEmpty { section("BODY", body) }
    }

    private func section(_ title: String, _ changes: [ResponseDiff.Change]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title).font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint).accessibilityAddTraits(.isHeader)
            LazyVStack(alignment: .leading, spacing: 10) {
                ForEach(Array(changes.enumerated()), id: \.offset) { _, c in row(c) }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card(radius: 18, fill: Theme.sheetCode)
        }
    }

    private func row(_ c: ResponseDiff.Change) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            if !c.path.isEmpty { Text(c.path).font(Typo.mono(12, .medium)).foregroundStyle(Theme.textSoft) }
            if let b = c.before { Text("− \(b)").font(Typo.mono(12.5)).foregroundStyle(Theme.danger).accessibilityLabel("Before: \(b)") }
            if let a = c.after { Text("+ \(a)").font(Typo.mono(12.5)).foregroundStyle(Theme.success).accessibilityLabel("After: \(a)") }
        }
        .textSelection(.enabled)
    }
}

/// "Deleted … Undo" over the bottom of the screen for a while.
struct UndoBar: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            if let offer = model.undoOffer {
                HStack(spacing: 12) {
                    Text(offer.label).font(Typo.row).foregroundStyle(Theme.text).lineLimit(2)
                    Button("Undo") { model.undo() }
                        .font(Typo.geist(16, .semibold))
                        .foregroundStyle(Theme.accentText)
                        .frame(minWidth: 44, minHeight: 44)
                        .accessibilityIdentifier("undo")
                }
                .padding(.leading, 16)
                .padding(.trailing, 10)
                .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Theme.surface))
                .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(Theme.separator))
                .padding(.horizontal, 16)
                .padding(.bottom, 92)
                .transition(.opacity)
                .task(id: offer.id) {
                    AccessibilityNotification.Announcement("\(offer.label). Undo is at the bottom of the screen.").post()
                    try? await Task.sleep(for: .seconds(8))
                    if !Task.isCancelled { model.dismissUndo(offer.id) }
                }
            }
        }
        .animation(.default, value: model.undoOffer?.id)
    }
}
