import SwiftUI

/// Collection runs come in I3 (runner, report, Live Activity).
struct RunsScreen: View {
    var body: some View {
        ContentUnavailableView {
            Label("Collection runs", systemImage: "play.circle")
        } description: {
            Text("Running a whole collection with its tests arrives in a coming update. Until then, `hitop run` and the desktop app run collections.")
        }
        .background(ScreenBackground())
        .navigationTitle("Runs")
    }
}

struct SearchScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(Navigator.self) private var nav

    var body: some View {
        let hits = model.search(nav.searchText)
        List {
            if nav.searchText.trimmingCharacters(in: .whitespaces).isEmpty {
                let pinned = model.state.pinnedRequests.compactMap { model.request($0) }
                if !pinned.isEmpty {
                    Section("Pinned") {
                        ForEach(pinned) { r in requestHit(r.id, r.name, model.collectionOf(request: r.id)?.name ?? "", methodLabel(r)) }
                    }
                    .listRowBackground(Theme.surface)
                }
                Section("Recent") {
                    ForEach(model.history.prefix(8)) { h in
                        Button { open(h) } label: { HistoryRow(entry: h) }.buttonStyle(.plain)
                    }
                }
                .listRowBackground(Theme.surface)
            } else if hits.isEmpty {
                ContentUnavailableView.search(text: nav.searchText).listRowBackground(Color.clear)
            } else {
                ForEach(hits) { hit in
                    switch hit.kind {
                    case .request:
                        requestHit(hit.requestID!, hit.title, hit.subtitle, hit.method ?? "GET")
                    case .collection:
                        Button { nav.tab = .collections } label: {
                            Label { Text(hit.title).font(Typo.row) } icon: { Image(systemName: "folder") }.frame(minHeight: 44)
                        }
                    case .history:
                        if let h = model.history.first(where: { $0.id == hit.historyID }) {
                            Button { open(h) } label: { HistoryRow(entry: h) }.buttonStyle(.plain)
                        }
                    }
                }
                .listRowBackground(Theme.surface)
            }
        }
        .scrollContentBackground(.hidden)
        .background(ScreenBackground())
        .navigationTitle("Search")
    }

    private func requestHit(_ id: UInt64, _ title: String, _ subtitle: String, _ method: String) -> some View {
        Button { nav.open(request: id) } label: {
            HStack(spacing: 12) {
                MethodTag(method: method)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(Typo.row).foregroundStyle(Theme.textRow)
                    Text(subtitle).font(Typo.caption).foregroundStyle(Theme.textMuted)
                }
                Spacer()
            }
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func open(_ h: HistoryEntry) {
        if let r = model.request(h.request.id) { nav.open(request: r.id) } else { nav.open(request: model.openAsNewRequest(h).id) }
    }
}

struct AboutView: View {
    @Environment(\.dismiss) private var dismiss

    private var version: String {
        let v = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
        let b = Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? ""
        return "\(v) (\(b))"
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(spacing: 10) {
                        Text("H").font(Typo.geist(30, .bold)).foregroundStyle(.white)
                            .frame(width: 72, height: 72)
                            .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Theme.logoGradient))
                            .accessibilityHidden(true)
                        Text("HITOP").font(Typo.geist(24, .bold))
                        Text("The API client your terminal, your CI and your AI agent can all drive.")
                            .font(Typo.caption).foregroundStyle(Theme.textMuted).multilineTextAlignment(.center)
                        Text("Version \(version)").font(Typo.mono(12)).foregroundStyle(Theme.textFaint)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                }
                .listRowBackground(Color.clear)
                Section {
                    Link("hitop.rws8.tech", destination: URL(string: "https://hitop.rws8.tech")!)
                    Link("Privacy", destination: URL(string: "https://hitop.rws8.tech/privacy.html")!)
                    NavigationLink("Acknowledgements") { AcknowledgementsView() }
                }
                .listRowBackground(Theme.surface)
            }
            .scrollContentBackground(.hidden)
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle("About")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}

struct AcknowledgementsView: View {
    private var ofl: String {
        guard let url = Bundle.main.url(forResource: "OFL-Geist", withExtension: "txt"), let text = try? String(contentsOf: url, encoding: .utf8) else { return "" }
        return text
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                item("Geist and Geist Mono", "Typefaces by Vercel with basement.studio, under the SIL Open Font License 1.1.", ofl)
                item("Yams", "YAML for Swift, by JP Simard and contributors, under the MIT License.", nil)
                item("HitopKit", "HITOP's collection format, variables, scripts and runner for Swift, under the MIT License.", nil)
            }
            .padding(16)
        }
        .background(Theme.ground.ignoresSafeArea())
        .navigationTitle("Acknowledgements")
    }

    private func item(_ title: String, _ text: String, _ license: String?) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(Typo.title)
            Text(text).font(Typo.caption).foregroundStyle(Theme.textSoft)
            if let license, !license.isEmpty {
                Text(license).font(Typo.mono(11)).foregroundStyle(Theme.textMuted).padding(12).card(radius: 14)
            }
        }
    }
}
