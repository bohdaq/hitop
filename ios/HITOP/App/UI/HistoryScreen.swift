import SwiftUI

struct HistoryRow: View {
    var entry: HistoryEntry

    var body: some View {
        HStack(spacing: 12) {
            MethodTag(method: methodLabel(entry.request), width: 52)
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.request.name).font(Typo.row).foregroundStyle(Theme.textRow).lineLimit(1)
                Text(entry.request.url).font(Typo.mono(11.5)).foregroundStyle(Theme.textMuted).lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: 6)
            VStack(alignment: .trailing, spacing: 2) {
                if let r = entry.response {
                    Text("\(r.status)").font(Typo.mono(13, .semibold)).foregroundStyle(Theme.status(r.status))
                    Text("\(r.durationMs) ms").font(Typo.mono(11.5)).foregroundStyle(Theme.textMuted)
                } else {
                    Text("Failed").font(Typo.geist(12.5, .semibold)).foregroundStyle(Theme.danger)
                }
            }
        }
        .frame(minHeight: 50)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("history-\(entry.request.name)")
    }
}

struct HistoryScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(Navigator.self) private var nav
    @State private var confirmClear = false

    private var groups: [(String, [HistoryEntry])] {
        let cal = Calendar.current
        var out: [(String, [HistoryEntry])] = []
        for e in model.history {
            let title: String
            if cal.isDateInToday(e.date) { title = "Today" } else if cal.isDateInYesterday(e.date) { title = "Yesterday" } else {
                title = e.date.formatted(date: .abbreviated, time: .omitted)
            }
            if out.last?.0 == title { out[out.count - 1].1.append(e) } else { out.append((title, [e])) }
        }
        return out
    }

    var body: some View {
        List {
            if model.history.isEmpty {
                ContentUnavailableView("No history yet", systemImage: "clock", description: Text("Every request you send is kept here with its response."))
                    .listRowBackground(Color.clear)
            }
            ForEach(groups, id: \.0) { group in
                Section {
                    ForEach(group.1) { e in
                        NavigationLink(value: Route.history(e.id)) { HistoryRow(entry: e) }
                            .swipeActions(edge: .trailing) {
                                Button("Delete", systemImage: "trash", role: .destructive) { model.deleteHistory(e.id) }
                            }
                            .swipeActions(edge: .leading) {
                                Button("Open", systemImage: "arrow.up.forward.square") { open(e) }.tint(Theme.accentDeep)
                            }
                            .contextMenu {
                                Button("Open as new request", systemImage: "arrow.up.forward.square") { open(e) }
                                Button("Delete", systemImage: "trash", role: .destructive) { model.deleteHistory(e.id) }
                            }
                    }
                } header: {
                    Text(group.0.uppercased()).font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
                }
                .listRowBackground(Theme.surface)
                .listRowSeparatorTint(Theme.separator)
            }
        }
        .listStyle(.insetGrouped)
        .listSectionSpacing(12)
        .scrollContentBackground(.hidden)
        .background(ScreenBackground())
        .navigationTitle("History")
        .toolbar {
            if !model.history.isEmpty {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Clear", role: .destructive) { confirmClear = true }
                }
            }
        }
        .confirmationDialog("Clear the history?", isPresented: $confirmClear, titleVisibility: .visible) {
            Button("Clear history", role: .destructive) { model.clearHistory() }
        }
    }

    private func open(_ e: HistoryEntry) {
        let r = model.openAsNewRequest(e)
        nav.open(request: r.id)
    }
}

struct HistoryDetail: View {
    var id: UInt64
    @Environment(AppModel.self) private var model
    @Environment(Navigator.self) private var nav

    var body: some View {
        if let e = model.history.first(where: { $0.id == id }) {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(spacing: 8) {
                            MethodTag(method: methodLabel(e.request), width: nil, size: 13)
                            Text(e.collectionName ?? "").font(Typo.caption).foregroundStyle(Theme.textMuted)
                            Spacer()
                            Text(e.date.formatted(date: .abbreviated, time: .shortened)).font(Typo.caption).foregroundStyle(Theme.textMuted)
                        }
                        Text(e.request.url).font(Typo.mono(14, .medium)).foregroundStyle(Theme.text).textSelection(.enabled)
                    }
                    .padding(14)
                    .card(radius: 20)
                    if let r = e.response {
                        HStack(spacing: 10) {
                            StatusPill(status: r.status)
                            Text("\(r.durationMs) ms").font(Typo.mono(14, .medium)).foregroundStyle(Theme.codeText)
                            if r.size > 0 { Text(ByteText.text(r.size)).font(Typo.mono(14, .medium)).foregroundStyle(Theme.textMuted) }
                        }
                        if !r.tests.isEmpty { TestsList(tests: r.tests) }
                        if !r.body.isEmpty {
                            if let box = JSONOutline.box(r.body) {
                                JSONTreeView(value: box, size: 13)
                            } else {
                                CodeBlock(text: r.body, json: false, fill: Theme.sheetCode)
                            }
                            if r.truncated { Text("The first 100 kB are kept.").font(Typo.caption).foregroundStyle(Theme.textMuted) }
                        }
                    } else if let err = e.error {
                        ErrorCard(failure: SendFailure(message: err))
                    }
                }
                .padding(16)
                .padding(.bottom, 90)
            }
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle(e.request.name)
            .navigationBarTitleDisplayMode(.inline)
            .safeAreaInset(edge: .bottom) {
                Button {
                    let r = model.openAsNewRequest(e)
                    nav.historyPath = []
                    nav.open(request: r.id)
                } label: {
                    Label("Open as new request", systemImage: "arrow.up.forward.square")
                        .font(Typo.geist(16, .semibold))
                        .frame(maxWidth: .infinity, minHeight: 36)
                }
                .buttonStyle(.glassProminent)
                .tint(Theme.accent)
                .foregroundStyle(Theme.onAccent)
                .padding(.horizontal, 16)
                .padding(.bottom, 8)
                .accessibilityIdentifier("open-as-new")
            }
        } else {
            ContentUnavailableView("Not in the history", systemImage: "clock")
        }
    }
}
