import SwiftUI
import UIKit

enum ResponseTab: Hashable { case events, body, headers, tests, cookies }

/// The iPhone response sheet.
struct ResponseSheet: View {
    var editor: RequestEditor
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ScrollView {
            ResponsePanel(editor: editor, style: .sheet, close: { dismiss() })
                .padding(.horizontal, 16)
                .padding(.top, 18)
                .padding(.bottom, 24)
        }
        // Explain opens over the response sheet (a sheet can only present from the top one).
        .modifier(ExplainHost())
    }
}

/// Status, timing, then Body / Headers / Tests / Cookies; or the error explained.
struct ResponsePanel: View {
    enum Style { case sheet, column }
    var editor: RequestEditor
    var style: Style
    var close: (() -> Void)?

    var body: some View {
        Group {
            if editor.draft.isWebSocket {
                SocketPanel(editor: editor, close: close)
            } else if editor.draft.isGrpc {
                GrpcPanel(editor: editor, close: close)
            } else if editor.isStreaming {
                LiveEventsView(editor: editor, close: close)
            } else if editor.isSending {
                VStack(spacing: 14) {
                    ProgressView().controlSize(.large)
                    Text("Sending \(editor.draft.method) \(AutoName.urlWhere(editor.draft.url))").font(Typo.caption).foregroundStyle(Theme.textMuted)
                    Button("Cancel") { editor.cancel() }.buttonStyle(.glass)
                }
                .frame(maxWidth: .infinity, minHeight: 220)
            } else if let outcome = editor.outcome {
                switch outcome {
                case let .success(o): ResponseContent(outcome: o, style: style, close: close, requestID: editor.id)
                case let .failure(f): ErrorCard(failure: f, close: close)
                }
            } else {
                ContentUnavailableView("No response yet", systemImage: "paperplane",
                                       description: Text("Send the request to see its status, timing, body, headers, tests and cookies here."))
                    .frame(minHeight: 260)
            }
        }
    }
}

struct ResponseContent: View {
    var outcome: SendOutcome
    var style: ResponsePanel.Style
    var close: (() -> Void)?
    /// The request it answers (Explain needs it).
    var requestID: UInt64?
    @State private var tab: ResponseTab
    @State private var raw = false
    @Environment(Intelligence.self) private var ai
    @Environment(\.dynamicTypeSize) private var typeSize

    init(outcome: SendOutcome, style: ResponsePanel.Style, close: (() -> Void)?, requestID: UInt64? = nil) {
        self.outcome = outcome
        self.style = style
        self.close = close
        self.requestID = requestID
        // An event stream opens on its events.
        _tab = State(initialValue: outcome.events != nil ? .events : .body)
    }

    private var tabItems: [(ResponseTab, String, String?)] {
        var items: [(ResponseTab, String, String?)] = []
        if let events = outcome.events { items.append((.events, "Events", "\(events.count)")) }
        items.append((.body, "Body", nil))
        items.append((.headers, "Headers", "\(outcome.headers.count)"))
        items.append((.tests, "Tests", outcome.tests.isEmpty ? nil : "\(passed)/\(outcome.tests.count)"))
        items.append((.cookies, "Cookies", outcome.cookies.isEmpty ? nil : "\(outcome.cookies.count)"))
        return items
    }

    private var passed: Int { outcome.tests.filter(\.pass).count }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            // At accessibility text sizes the numbers get a line of their own, the buttons the next.
            let stacked = typeSize.isAccessibilitySize
            let header = stacked ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8)) : AnyLayout(HStackLayout(spacing: 8))
            let numbers = stacked ? AnyLayout(VStackLayout(alignment: .leading, spacing: 6)) : AnyLayout(HStackLayout(spacing: 8))
            header {
            numbers {
                StatusPill(status: outcome.status, size: style == .sheet ? 15 : 14)
                Text("\(outcome.durationMs) ms").font(Typo.mono(14, .medium)).foregroundStyle(Theme.codeText)
                    .fixedSize()
                    .accessibilityLabel("\(outcome.durationMs) milliseconds")
                Text(ByteText.text(outcome.size)).font(Typo.mono(14, .medium)).foregroundStyle(Theme.textMuted)
                    .lineLimit(1)
                    .minimumScaleFactor(0.7)
                    .layoutPriority(1)
            }
            // First claim on the row's width: the buttons' Spacer otherwise takes half of it and the
            // size ("285 B") was cut to "28…" next to Explain (I6 store screenshots).
            .layoutPriority(1)
            HStack(spacing: 8) {
                // The iOS 26 sheet floats inset from the screen edges: nothing in this row may push it wider.
                Spacer(minLength: 0)
                if style == .column, !outcome.tests.isEmpty {
                    Text("\(passed)/\(outcome.tests.count) tests")
                        .font(Typo.geist(12, .semibold))
                        .foregroundStyle(passed == outcome.tests.count ? Theme.success : Theme.danger)
                        .padding(.horizontal, 9).padding(.vertical, 4)
                        .background(RoundedRectangle(cornerRadius: 10).fill((passed == outcome.tests.count ? Theme.success : Theme.danger).opacity(0.12)))
                }
                if explainInHeader, let requestID {
                    // The drafts: Explain takes Share's place in the sheet's header; Share moves
                    // to the body's tools.
                    ExplainButton(requestID: requestID)
                } else {
                    shareButton
                }
                if let close {
                    Button(action: close) {
                        Image(systemName: "xmark").font(.system(size: 14, weight: .semibold)).foregroundStyle(Theme.text).frame(width: 44, height: 44)
                    }
                    .buttonStyle(.glass)
                    .buttonBorderShape(.circle)
                    .accessibilityLabel("Close")
                    .accessibilityIdentifier("close-response")
                }
            }
            }
            Text("\(outcome.sentURL)")
                .font(Typo.mono(11.5))
                .foregroundStyle(Theme.textFaint)
                .lineLimit(1)
                .truncationMode(.middle)
                .accessibilityLabel("Sent to \(outcome.sentURL)")
                .accessibilityIdentifier("sent-url")
            if let timing = outcome.timing, timing.total > 0 {
                // The phase labels are one row: at accessibility sizes they are read from the bar (VoiceOver) instead.
                TimingBar(timing: timing, showLabels: style == .sheet && !typeSize.isAccessibilitySize, height: style == .sheet ? 8 : 6)
            }
            CapsuleTabs(items: tabItems, selection: $tab)
            switch tab {
            case .events:
                if outcome.stopped {
                    Label("Stopped: the stream is what came until Stop.", systemImage: "stop.circle").font(Typo.caption).foregroundStyle(Theme.textMuted)
                        .accessibilityIdentifier("stream-stopped")
                }
                EventsList(events: outcome.events ?? [])
            case .body: bodyView
            case .headers: headersView
            case .tests: TestsList(tests: outcome.tests)
            case .cookies: cookiesView
            }
            ForEach(outcome.notes, id: \.self) { n in
                Label(n, systemImage: "info.circle").font(Typo.caption).foregroundStyle(Theme.textMuted)
            }
            if style == .column, let requestID {
                ExplainWideButton(requestID: requestID).padding(.top, 4)
            }
        }
    }

    /// iPhone sheet: Explain in the header when Apple Intelligence is on (else Share stays there).
    private var explainInHeader: Bool { style == .sheet && ai.availability.isAvailable }

    private var shareButton: some View {
        ShareLink(item: outcome.text) {
            Image(systemName: "square.and.arrow.up").font(.system(size: 16, weight: .medium)).foregroundStyle(Theme.text)
                .frame(width: 44, height: 44)
        }
        .buttonStyle(.glass)
        .buttonBorderShape(.circle)
        .accessibilityLabel("Share the response body")
        .accessibilityIdentifier("share-response")
    }

    /// Pretty/Raw (JSON), Copy, and Share when Explain took its place in the header.
    private func bodyTools(json: Bool) -> some View {
        let stacked = typeSize.isAccessibilitySize
        let layout = stacked ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4)) : AnyLayout(HStackLayout())
        return layout {
            if json {
                Picker("View", selection: $raw) {
                    Text("Pretty").tag(false)
                    Text("Raw").tag(true)
                }
                .pickerStyle(.segmented)
                .frame(width: stacked ? nil : 160)
            }
            if !stacked { Spacer() }
            Button {
                UIPasteboard.general.string = outcome.text
            } label: { Label("Copy", systemImage: "doc.on.doc").font(Typo.caption).frame(minHeight: 44).contentShape(Rectangle()) }
            .frame(minHeight: 44)
            .hoverEffect(.highlight)
            .accessibilityLabel("Copy the response body")
            if explainInHeader {
                ShareLink(item: outcome.text) { Label("Share", systemImage: "square.and.arrow.up").font(Typo.caption).frame(minHeight: 44).contentShape(Rectangle()) }
                    .frame(minHeight: 44)
                    .hoverEffect(.highlight)
                    .accessibilityLabel("Share the response body")
                    .accessibilityIdentifier("share-response")
            }
        }
    }

    @ViewBuilder
    private var bodyView: some View {
        if outcome.isImage, let image = UIImage(data: outcome.body) {
            Image(uiImage: image)
                .resizable()
                .scaledToFit()
                .frame(maxWidth: .infinity, maxHeight: 360)
                .padding(14)
                .card(radius: 18, fill: Theme.sheetCode)
                .accessibilityLabel("Image response, \(Int(image.size.width)) by \(Int(image.size.height))")
        } else if let json = JSONOutline.box(outcome.text) {
            bodyTools(json: true)
            if raw {
                CodeBlock(text: outcome.text, json: false, fill: Theme.sheetCode)
            } else {
                JSONTreeView(value: json, size: style == .sheet ? 13.5 : 13)
            }
        } else if outcome.text.isEmpty {
            Text("No body").font(Typo.caption).foregroundStyle(Theme.textMuted).frame(maxWidth: .infinity, minHeight: 80)
        } else {
            bodyTools(json: false)
            CodeBlock(text: String(outcome.text.prefix(200_000)), json: false, fill: Theme.sheetCode)
        }
    }

    private var headersView: some View {
        VStack(spacing: 0) {
            ForEach(Array(outcome.headers.enumerated()), id: \.offset) { i, h in
                VStack(alignment: .leading, spacing: 2) {
                    Text(h.first ?? "").font(Typo.mono(12.5, .semibold)).foregroundStyle(Theme.codeKey)
                    Text(h.last ?? "").font(Typo.mono(13)).foregroundStyle(Theme.codeText).textSelection(.enabled)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 14).padding(.vertical, 9)
                .accessibilityElement(children: .combine)
                if i < outcome.headers.count - 1 { Divider().overlay(Theme.separator) }
            }
        }
        .card(radius: 18)
    }

    @ViewBuilder
    private var cookiesView: some View {
        if outcome.cookies.isEmpty {
            Text("The response set no cookies.").font(Typo.caption).foregroundStyle(Theme.textMuted).frame(maxWidth: .infinity, minHeight: 80)
        } else {
            VStack(spacing: 0) {
                ForEach(outcome.cookies) { c in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(c.name).font(Typo.mono(13, .semibold)).foregroundStyle(Theme.codeKey)
                            Spacer()
                            Text([c.secure ? "Secure" : nil, c.httpOnly ? "HttpOnly" : nil].compactMap { $0 }.joined(separator: " · "))
                                .font(Typo.caption).foregroundStyle(Theme.textMuted)
                        }
                        Text(c.value).font(Typo.mono(13)).foregroundStyle(Theme.codeText).textSelection(.enabled)
                        Text("\(c.domain)\(c.path)").font(Typo.caption).foregroundStyle(Theme.textMuted)
                    }
                    .padding(.horizontal, 14).padding(.vertical, 9)
                    .accessibilityElement(children: .combine)
                }
            }
            .card(radius: 18)
        }
    }
}

struct TestsList: View {
    var tests: [TestOutcome]

    var body: some View {
        if tests.isEmpty {
            Text("No tests. Add pm.test(…) in the request's Tests tab.")
                .font(Typo.caption).foregroundStyle(Theme.textMuted).frame(maxWidth: .infinity, minHeight: 80)
        } else {
            VStack(spacing: 0) {
                ForEach(Array(tests.enumerated()), id: \.offset) { i, t in
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 10) {
                            Image(systemName: t.pass ? "checkmark" : "xmark")
                                .font(.system(size: 13, weight: .bold))
                                .foregroundStyle(t.pass ? Theme.success : Theme.danger)
                            Text(t.name).font(Typo.geist(14, relativeTo: .body)).foregroundStyle(Theme.text)
                            Spacer()
                        }
                        if !t.pass, !t.detail.isEmpty {
                            Text(t.detail).font(Typo.caption).foregroundStyle(Color(hex: 0xE8C2C2)).padding(.leading, 23)
                        }
                    }
                    .frame(minHeight: 42)
                    .padding(.horizontal, 14)
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("\(t.pass ? "Passed" : "Failed"): \(t.name)")
                    .accessibilityIdentifier("test-\(t.pass ? "pass" : "fail")-\(t.name)")
                    if i < tests.count - 1 { Divider().overlay(Theme.separator) }
                }
            }
            .card(radius: 18)
        }
    }
}

/// A JSON body with fold points (tap ▾ / ▸).
struct JSONTreeView: View {
    var value: JSONValueBox
    var size: CGFloat
    @State private var folded: Set<String> = []

    init(value: JSONValueBox, size: CGFloat) {
        self.value = value
        self.size = size
        _folded = State(initialValue: value.defaultFolds)
    }

    var body: some View {
        let lines = value.lines(folded: folded)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(lines.prefix(4000)) { line in
                HStack(alignment: .firstTextBaseline, spacing: 0) {
                    if let fold = line.fold {
                        Button {
                            if folded.contains(fold) { folded.remove(fold) } else { folded.insert(fold) }
                        } label: {
                            Text(line.folded ? "▸" : "▾").font(Typo.mono(size)).foregroundStyle(Theme.codeMuted)
                                .frame(width: 16, alignment: .leading)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(line.folded ? "Expand" : "Collapse")
                    } else {
                        Color.clear.frame(width: 16, height: 1)
                    }
                    Text(String(repeating: "  ", count: line.depth)).font(Typo.mono(size))
                    Text(Self.attributed(line.tokens, size: size))
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                }
                .padding(.vertical, size * 0.22)
                .contentShape(Rectangle())
                .onTapGesture {
                    guard let fold = line.fold else { return }
                    if folded.contains(fold) { folded.remove(fold) } else { folded.insert(fold) }
                }
            }
        }
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .card(radius: 18, fill: Theme.sheetCode)
        .accessibilityIdentifier("json-body")
    }

    static func attributed(_ tokens: [JSONOutline.Token], size: CGFloat) -> AttributedString {
        var out = AttributedString()
        for t in tokens {
            var a = AttributedString(t.text)
            a.font = Typo.mono(size)
            a.foregroundColor = switch t.role {
            case .punct: Theme.codeText
            case .key: Theme.codeKey
            case .string: Theme.codeString
            case .number: Theme.codeNumber
            case .literal: Theme.codeLiteral
            case .summary: Theme.codeMuted
            }
            out += a
        }
        return out
    }
}

/// An error as a headline, a next step and the raw text under Details (the web app's error card).
struct ErrorCard: View {
    var failure: SendFailure
    var close: (() -> Void)?
    @State private var details = false

    var body: some View {
        let e = failure.explained
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Theme.danger)
                Text(e.headline).font(Typo.geist(17, .semibold, relativeTo: .headline)).foregroundStyle(Theme.text)
                    .accessibilityIdentifier("error-headline")
                Spacer()
                if let close {
                    Button(action: close) {
                        Image(systemName: "xmark").font(.system(size: 14, weight: .semibold)).foregroundStyle(Theme.text).frame(width: 44, height: 44)
                    }
                    .buttonStyle(.glass)
                    .buttonBorderShape(.circle)
                    .accessibilityLabel("Close")
                    .accessibilityIdentifier("close-response")
                }
            }
            if !e.next.isEmpty {
                Text(e.next).font(Typo.body).foregroundStyle(Color(hex: 0xE8C2C2))
            }
            DisclosureGroup("Details", isExpanded: $details) {
                Text(e.raw).font(Typo.mono(12.5)).foregroundStyle(Theme.textSoft).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 6)
            }
            .font(Typo.caption)
            .tint(Theme.textMuted)
        }
        .padding(14)
        .background(RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Theme.danger.opacity(0.10)))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).stroke(Theme.danger.opacity(0.28)))
    }
}
