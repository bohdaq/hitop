import SwiftUI

/// The Runs tab: the run going on, run history (newest first) and Run for every collection.
struct RunsScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(RunCenter.self) private var runs
    @Environment(Navigator.self) private var nav
    @State private var confirmClear = false

    var body: some View {
        List {
            if let active = runs.active {
                Section {
                    Button { nav.runSheet = RunTarget(id: active.collectionID) } label: {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text("Running \(active.collectionName)").font(Typo.title).foregroundStyle(Theme.text)
                                Spacer()
                                Text("\(active.progress.done)/\(active.progress.total)").font(Typo.mono(14, .semibold)).foregroundStyle(Theme.textSoft)
                            }
                            ProgressBar(fraction: active.progress.fraction, height: 6)
                        }
                        .padding(.vertical, 6)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("runs-active")
                }
                .listRowBackground(Theme.surface)
            }

            Section {
                if runs.runs.isEmpty {
                    Text("Run a collection to check every request and its tests in one go. Runs stay here, the last \(FileRunStore.limit) of them.")
                        .font(Typo.caption).foregroundStyle(Theme.textMuted).frame(minHeight: 44)
                }
                ForEach(runs.runs) { r in
                    NavigationLink(value: RunRoute.report(r.id)) { RunRow(record: r) }
                        .accessibilityIdentifier("run-\(r.collectionName)")
                        .swipeActions { Button("Delete", systemImage: "trash", role: .destructive) { runs.delete(r.id) } }
                }
            } header: {
                Text("RECENT RUNS").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
            }
            .listRowBackground(Theme.surface)

            Section {
                ForEach(model.visibleCollections) { c in
                    Button { nav.runSheet = RunTarget(id: c.id) } label: {
                        HStack(spacing: 12) {
                            CollectionIcon(name: c.name)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(c.name).font(Typo.row).foregroundStyle(Theme.textRow)
                                Text("\(Sender.runnable(c).count) requests to run").font(Typo.caption).foregroundStyle(Theme.textMuted)
                            }
                            Spacer()
                            Image(systemName: "play.fill").foregroundStyle(Theme.accent)
                        }
                        .frame(minHeight: 44)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .disabled(Sender.runnable(c).isEmpty)
                    .accessibilityLabel("Run \(c.name)")
                    .accessibilityIdentifier("run-collection-\(c.name)")
                }
            } header: {
                Text("RUN A COLLECTION").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
            }
            .listRowBackground(Theme.surface)
        }
        .scrollContentBackground(.hidden)
        .background(ScreenBackground())
        .navigationTitle("Runs")
        .navigationDestination(for: RunRoute.self) { route in
            switch route {
            case let .report(id): RunReportView(runID: id)
            }
        }
        .toolbar {
            if !runs.runs.isEmpty {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Clear", systemImage: "trash") { confirmClear = true }.accessibilityLabel("Clear run history")
                }
            }
        }
        .confirmationDialog("Clear the run history?", isPresented: $confirmClear, titleVisibility: .visible) {
            Button("Clear", role: .destructive) { runs.clear() }
            Button("Cancel", role: .cancel) {}
        }
    }
}


struct RunRow: View {
    var record: RunRecord

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: record.passed ? "checkmark.circle.fill" : "xmark.octagon.fill")
                .font(.system(size: 20)).foregroundStyle(record.passed ? Theme.success : Theme.danger)
            VStack(alignment: .leading, spacing: 2) {
                Text(record.collectionName).font(Typo.row).foregroundStyle(Theme.textRow)
                Text([record.environmentName ?? "No environment", record.date.formatted(.relative(presentation: .named))].joined(separator: " · "))
                    .font(Typo.caption).foregroundStyle(Theme.textMuted)
                if let f = record.firstFailure {
                    Text(f).font(Typo.caption).foregroundStyle(Theme.danger.opacity(0.85)).lineLimit(1)
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                Text("\(record.requestsPassed)/\(record.requestsTotal)").font(Typo.mono(14, .semibold)).foregroundStyle(record.passed ? Theme.success : Theme.text)
                Text(RunActivityAttributes.seconds(record.durationMs)).font(Typo.mono(11.5)).foregroundStyle(Theme.textMuted)
            }
        }
        .frame(minHeight: 52)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(record.collectionName), \(record.requestsPassed) of \(record.requestsTotal) passed, \(record.environmentName ?? "no environment")")
    }
}

/// A run of one collection (the drafts' "Collection run"): options, then the ring with live
/// progress, the failures and every request; Report, Share report.json, Run again.
struct RunSheet: View {
    var target: RunTarget
    @Environment(AppModel.self) private var model
    @Environment(RunCenter.self) private var runs
    @Environment(\.dismiss) private var dismiss
    @State private var settings = RunSettings()
    @State private var record: RunRecord?
    @State private var didSetUp = false
    @AppStorage(SystemBridge.liveActivityKey) private var liveActivity = true

    private var collection: SavedCollection? { model.collection(target.id) }
    private var active: ActiveRun? { runs.active?.collectionID == target.id ? runs.active : nil }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 18) {
                    if let active {
                        progress(active)
                    } else if let record {
                        result(record)
                    } else {
                        options
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 100)
            }
            .background(RunBackground(passed: record.map(\.passed)))
            .safeAreaInset(edge: .bottom) { bottomBar }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: RunRoute.self) { route in
                switch route {
                case let .report(id): RunReportView(runID: id)
                }
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close", systemImage: "xmark") { dismiss() }.accessibilityIdentifier("run-close")
                }
                if record != nil && active == nil {
                    ToolbarItem(placement: .primaryAction) {
                        Button("Run again", systemImage: "arrow.clockwise") { start() }.accessibilityIdentifier("run-again")
                    }
                }
            }
        }
        .onAppear {
            guard !didSetUp else { return }
            didSetUp = true
            settings = runs.defaultSettings(for: target.id)
            if target.showLast { record = runs.lastRun(of: target.id) }
        }
    }

    private var title: String { "Run \(collection?.name ?? "collection")" }

    private var envName: String { model.environments.first { $0.id == settings.environmentID }?.name ?? "No environment" }

    // MARK: Options

    private var options: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text(collection?.name ?? "").font(Typo.geist(26, .bold, relativeTo: .title))
                let n = collection.map { Sender.runnable($0).count } ?? 0
                let skipped = (collection?.requests.count ?? 0) - n
                Text(skipped > 0 ? "\(n) requests run in order; \(skipped) WebSocket or gRPC left out, as `hitop run` does." : "\(n) requests run in order, with their scripts and tests.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted)
            }
            VStack(spacing: 0) {
                Picker(selection: Binding(get: { settings.environmentID ?? "" }, set: { settings.environmentID = $0.isEmpty ? nil : $0 })) {
                    Text("No environment").tag("")
                    ForEach(model.environments) { e in Text(e.extras["team"] == nil ? e.name : "\(e.name) (team)").tag(e.id) }
                } label: {
                    Label("Environment", systemImage: "globe").foregroundStyle(Theme.text)
                }
                .frame(minHeight: 48)
                .accessibilityIdentifier("run-environment")
                Divider().overlay(Theme.separator)
                Toggle(isOn: $settings.bail) {
                    VStack(alignment: .leading, spacing: 1) {
                        Label("Stop at the first failure", systemImage: "stop.circle").foregroundStyle(Theme.text)
                    }
                }
                .frame(minHeight: 48)
                .accessibilityIdentifier("run-bail")
                Divider().overlay(Theme.separator)
                Stepper(value: $settings.iterations, in: 1...20) {
                    Label(settings.iterations == 1 ? "Once" : "\(settings.iterations) times", systemImage: "repeat").foregroundStyle(Theme.text)
                }
                .frame(minHeight: 48)
                .accessibilityIdentifier("run-iterations")
                Divider().overlay(Theme.separator)
                Toggle(isOn: $liveActivity) {
                    Label("Live Activity on the Lock Screen", systemImage: "rectangle.inset.filled.and.person.filled").foregroundStyle(Theme.text)
                }
                .frame(minHeight: 48)
            }
            .font(Typo.row)
            .tint(Theme.success)
            .padding(.horizontal, 14)
            .card(radius: 18)
            if let last = runs.lastRun(of: target.id) {
                NavigationLink(value: RunRoute.report(last.id)) {
                    HStack { Text("Last run").foregroundStyle(Theme.textSoft); Spacer(); RunRow(record: last) }
                }
                .padding(.horizontal, 14)
                .card(radius: 18)
            }
        }
    }

    // MARK: Running

    private func progress(_ run: ActiveRun) -> some View {
        VStack(spacing: 18) {
            RunRing(done: run.progress.done, total: run.progress.total, fraction: run.progress.fraction,
                    color: run.progress.failed > 0 ? Theme.warning : Theme.success, label: "of \(run.progress.total)")
            Text("\(run.environmentName ?? "No environment") · \(run.progress.current ?? "starting…")").font(Typo.caption).foregroundStyle(Theme.textMuted)
                .accessibilityIdentifier("run-progress")
            stepList(run.steps)
        }
    }

    // MARK: Result

    @ViewBuilder
    private func result(_ r: RunRecord) -> some View {
        let report = r.report
        VStack(spacing: 18) {
            RunRing(done: r.requestsPassed, total: r.requestsTotal, fraction: r.requestsTotal == 0 ? 0 : Double(r.requestsPassed) / Double(r.requestsTotal),
                    color: r.passed ? Theme.success : Theme.danger, label: "passed")
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("\(r.requestsPassed) of \(r.requestsTotal) passed")
                .accessibilityIdentifier("run-summary")
            Text([r.environmentName ?? "No environment", r.iterations > 1 ? "\(r.iterations) iterations" : nil, r.bailed ? "stopped at the first failure" : nil]
                .compactMap { $0 }.joined(separator: " · "))
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
            HStack(spacing: 10) {
                stat(r.assertionsTotal == 0 ? "–" : "\(r.assertionsPassed)/\(r.assertionsTotal)", "", "checks")
                stat(report?.p95.map(String.init) ?? "–", report?.p95 == nil ? "" : " ms", "p95")
                stat(r.durationMs < 1000 ? "\(r.durationMs)" : String(format: "%.1f", Double(r.durationMs) / 1000), r.durationMs < 1000 ? " ms" : " s", "total")
            }
            if let report {
                ForEach(Array(report.requests.enumerated().filter { !$0.element.passed }.prefix(3)), id: \.offset) { _, f in
                    FailureCard(request: f)
                }
                stepList(report.requests.enumerated().map { RunStepRow(id: $0.offset, name: $0.element.name, method: $0.element.method, passed: $0.element.passed, status: $0.element.status, durationMs: $0.element.responseTimeMs ?? $0.element.durationMs) })
            }
        }
    }

    private func stat(_ value: String, _ unit: String, _ label: String) -> some View {
        VStack(spacing: 2) {
            (Text(value).font(Typo.mono(20, .semibold)) + Text(unit).font(Typo.mono(13)).foregroundStyle(Theme.textMuted))
                .lineLimit(1).minimumScaleFactor(0.6)
            Text(label).font(Typo.caption).foregroundStyle(Theme.textMuted)
        }
        .frame(maxWidth: .infinity)
        .padding(12)
        .card(radius: 18)
        .accessibilityElement(children: .combine)
    }

    private func stepList(_ rows: [RunStepRow]) -> some View {
        VStack(spacing: 0) {
            ForEach(rows) { row in
                HStack(spacing: 10) {
                    Image(systemName: row.passed ? "checkmark" : "xmark").font(.system(size: 13, weight: .bold))
                        .foregroundStyle(row.passed ? Theme.success : Theme.danger).frame(width: 16)
                    Text(row.method).font(Typo.tag).foregroundStyle(Theme.method(row.method)).frame(width: 52, alignment: .leading)
                    Text(row.name).font(Typo.row).foregroundStyle(Theme.textRow).lineLimit(1)
                    Spacer(minLength: 4)
                    if let s = row.status, !row.passed { Text("\(s)").font(Typo.mono(12.5, .semibold)).foregroundStyle(Theme.status(s)) }
                    Text("\(row.durationMs) ms").font(Typo.mono(12.5)).foregroundStyle(Theme.textMuted)
                }
                .frame(minHeight: 42)
                .padding(.horizontal, 14)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("run-step-\(row.name)")
                if row.id != rows.last?.id { Divider().overlay(Theme.separator) }
            }
        }
        .card(radius: 18)
    }

    // MARK: Bottom bar

    @ViewBuilder
    private var bottomBar: some View {
        GlassEffectContainer {
            HStack(spacing: 10) {
                if active != nil {
                    Label("Running…", systemImage: "hourglass").font(Typo.title).frame(maxWidth: .infinity, minHeight: 46)
                        .glassEffect(.regular, in: Capsule())
                } else if let record {
                    NavigationLink(value: RunRoute.report(record.id)) {
                        Label("Report", systemImage: "list.bullet.rectangle").font(Typo.title).frame(maxWidth: .infinity, minHeight: 30)
                    }
                    .buttonStyle(.glass)
                    .controlSize(.large)
                    .accessibilityIdentifier("run-report")
                    ReportShareButton(record: record)
                } else {
                    Button(action: start) {
                        Label("Run", systemImage: "play.fill").font(Typo.geist(18, .bold, relativeTo: .headline)).foregroundStyle(Theme.onAccent)
                            .frame(maxWidth: .infinity, minHeight: 34)
                    }
                    .buttonStyle(.glassProminent)
                    .tint(Theme.accent)
                    .controlSize(.large)
                    .disabled(collection.map { Sender.runnable($0).isEmpty } ?? true || runs.isRunning)
                    .accessibilityIdentifier("run-start")
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 8)
        }
    }

    private func start() {
        record = nil
        let id = target.id
        let s = settings
        Task { record = await runs.start(collectionID: id, settings: s) }
    }
}

/// The ring of the drafts: passed of total.
struct RunRing: View {
    var done: Int
    var total: Int
    var fraction: Double
    var color: Color
    var label: String

    var body: some View {
        ZStack {
            Circle().stroke(Color.white.opacity(0.08), lineWidth: 12)
            Circle().trim(from: 0, to: max(0.001, fraction)).stroke(color, style: StrokeStyle(lineWidth: 12, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .animation(.snappy, value: fraction)
            VStack(spacing: 4) {
                (Text("\(done)") + Text("/\(total)").foregroundStyle(Theme.textFaint)).font(Typo.geist(40, .bold, relativeTo: .largeTitle))
                Text(label).font(Typo.geist(13)).foregroundStyle(Theme.textMuted)
            }
        }
        .frame(width: 168, height: 168)
        .padding(.top, 8)
    }
}

/// The run screen's background: graphite with a green (or red) glow, as the draft.
struct RunBackground: View {
    var passed: Bool?

    var body: some View {
        ZStack(alignment: .top) {
            Theme.ground
            RadialGradient(colors: [(passed == false ? Theme.danger : Theme.success).opacity(0.16), .clear], center: .center, startRadius: 0, endRadius: 190)
                .frame(width: 380, height: 380)
                .offset(y: -20)
                .accessibilityHidden(true)
        }
        .ignoresSafeArea()
    }
}

/// A failed request: method, name, status, the failure line.
struct FailureCard: View {
    var request: RunReport.Request

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Text(request.method).font(Typo.tag).foregroundStyle(Theme.danger)
                Text(request.name).font(Typo.geist(14, .semibold))
                Spacer()
                if let s = request.status { Text("\(s)").font(Typo.mono(13, .semibold)).foregroundStyle(Theme.danger) }
            }
            Text(request.failure ?? "failed").font(Typo.geist(13)).foregroundStyle(Color(hex: 0xE8C2C2))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Theme.danger.opacity(0.10)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Theme.danger.opacity(0.28)))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("failure-\(request.name)")
    }
}

/// Shares report.json (HitopKit's text: what `hitop run --reporter json` writes).
struct ReportShareButton: View {
    var record: RunRecord

    private var file: URL? {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("hitop-report-\(record.id)", isDirectory: true)
        let url = dir.appendingPathComponent("report.json")
        do {
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            try Data(record.reportJSON.utf8).write(to: url, options: .atomic)
            return url
        } catch { return nil }
    }

    var body: some View {
        if let file {
            ShareLink(item: file, preview: SharePreview("report.json")) {
                Image(systemName: "square.and.arrow.up").font(.system(size: 17, weight: .semibold)).frame(width: 30, height: 30)
            }
            .buttonStyle(.glass)
            .buttonBorderShape(.circle)
            .controlSize(.large)
            .accessibilityLabel("Share report.json")
            .accessibilityIdentifier("share-report")
        }
    }
}

/// The run report: every request of every iteration with status, time, its tests; failures open.
struct RunReportView: View {
    var runID: UInt64
    @Environment(RunCenter.self) private var runs
    @State private var open: Set<String> = []

    var body: some View {
        if let record = runs.run(runID), let report = record.report {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Image(systemName: report.passed ? "checkmark.circle.fill" : "xmark.octagon.fill")
                                .foregroundStyle(report.passed ? Theme.success : Theme.danger)
                            Text(report.passed ? "Passed" : "Failed").font(Typo.title)
                            Spacer()
                            Text("\(report.summary.requests.passed)/\(report.summary.requests.total)").font(Typo.mono(15, .semibold))
                        }
                        Text([record.environmentName ?? "No environment", record.date.formatted(date: .abbreviated, time: .shortened),
                              RunActivityAttributes.seconds(report.durationMs)].joined(separator: " · "))
                            .font(Typo.caption).foregroundStyle(Theme.textMuted)
                        Text("Requests \(report.summary.requests.passed) of \(report.summary.requests.total) · tests \(report.summary.assertions.passed) of \(report.summary.assertions.total)\(report.bailed ? " · stopped at the first failure" : "")")
                            .font(Typo.caption).foregroundStyle(Theme.textSoft)
                    }
                    .padding(.vertical, 4)
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("report-summary")
                }
                .listRowBackground(Theme.surface)
                ForEach(Array(report.collections.enumerated()), id: \.offset) { _, c in
                    ForEach(c.iterations, id: \.iteration) { it in
                        Section {
                            ForEach(Array(it.requests.enumerated()), id: \.offset) { i, r in
                                requestRow(r, key: "\(it.iteration)/\(i)")
                            }
                        } header: {
                            Text(c.iterations.count > 1 ? "ITERATION \(it.iteration)" : "REQUESTS").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
                        }
                        .listRowBackground(Theme.surface)
                    }
                }
            }
            .scrollContentBackground(.hidden)
            .background(ScreenBackground())
            .navigationTitle(record.collectionName)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .primaryAction) { ReportShareButton(record: record) } }
            .onAppear {
                // Failures start open.
                for c in report.collections {
                    for it in c.iterations {
                        for (i, r) in it.requests.enumerated() where !r.passed { open.insert("\(it.iteration)/\(i)") }
                    }
                }
            }
        } else {
            ContentUnavailableView("Run not found", systemImage: "questionmark.circle")
        }
    }

    private func requestRow(_ r: RunReport.Request, key: String) -> some View {
        let isOpen = open.contains(key)
        return VStack(alignment: .leading, spacing: 8) {
            Button {
                withAnimation(.snappy) { if isOpen { open.remove(key) } else { open.insert(key) } }
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: r.passed ? "checkmark" : "xmark").font(.system(size: 13, weight: .bold))
                        .foregroundStyle(r.passed ? Theme.success : Theme.danger).frame(width: 16)
                    Text(r.method).font(Typo.tag).foregroundStyle(Theme.method(r.method)).frame(width: 52, alignment: .leading)
                    Text(r.name).font(Typo.row).foregroundStyle(Theme.textRow).lineLimit(1)
                    Spacer(minLength: 4)
                    if let s = r.status { Text("\(s)").font(Typo.mono(12.5, .semibold)).foregroundStyle(Theme.status(s)) }
                    Text("\(r.responseTimeMs ?? r.durationMs) ms").font(Typo.mono(12)).foregroundStyle(Theme.textMuted)
                    Image(systemName: "chevron.right").font(.system(size: 11, weight: .bold)).foregroundStyle(Theme.chevron)
                        .rotationEffect(.degrees(isOpen ? 90 : 0))
                }
                .frame(minHeight: 44)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityValue(isOpen ? "expanded" : "collapsed")
            .accessibilityIdentifier("report-request-\(r.name)")
            if isOpen {
                VStack(alignment: .leading, spacing: 6) {
                    Text(r.url).font(Typo.mono(11.5)).foregroundStyle(Theme.textMuted).lineLimit(2).textSelection(.enabled)
                    if let f = r.failure, !r.passed {
                        Text(f).font(Typo.geist(13)).foregroundStyle(Color(hex: 0xE8C2C2)).accessibilityIdentifier("report-failure-\(r.name)")
                    }
                    ForEach(Array(r.assertions.enumerated()), id: \.offset) { _, a in
                        HStack(alignment: .top, spacing: 8) {
                            Image(systemName: a.passed ? "checkmark.circle.fill" : "xmark.circle.fill")
                                .foregroundStyle(a.passed ? Theme.success : Theme.danger).font(.system(size: 13))
                            VStack(alignment: .leading, spacing: 2) {
                                Text(a.name).font(Typo.caption).foregroundStyle(Theme.textSoft)
                                if let m = a.message { Text(m).font(Typo.mono(11.5)).foregroundStyle(Theme.danger.opacity(0.9)) }
                            }
                        }
                        .accessibilityElement(children: .combine)
                    }
                    if r.assertions.isEmpty && r.passed { Text("No tests").font(Typo.caption).foregroundStyle(Theme.textFaint) }
                    ForEach(r.notes ?? [], id: \.self) { n in Text(n).font(Typo.caption).foregroundStyle(Theme.textFaint) }
                }
                .padding(.leading, 26)
                .padding(.bottom, 6)
            }
        }
    }
}
