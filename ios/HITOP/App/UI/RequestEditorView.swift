import SwiftUI
import UIKit

enum EditorTab: String, Hashable {
    case params = "Params", headers = "Headers", body = "Body", auth = "Auth", scripts = "Scripts", tests = "Tests"
    /// I4: a WebSocket request's saved messages and subprotocols; a gRPC request's call.
    case messages = "Messages", call = "Call"
}

/// The iPhone request screen: editor, Save + Send at the bottom, the response as a sheet.
struct RequestScreen: View {
    var id: UInt64
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var showResponse = false
    @State private var detent: PresentationDetent = .fraction(0.86)
    @State private var naming: NameTarget?
    @State private var moving: UInt64?
    @State private var deleting: NameTarget?
    @State private var keyboardUp = false

    var body: some View {
        if let editor = model.editor(for: id) {
            content(editor)
        } else {
            ContentUnavailableView("Request deleted", systemImage: "trash", description: Text("This request is no longer in your library."))
        }
    }

    private var readOnly: Bool { model.isReadOnly(model.collectionOf(request: id)?.id) }

    private func content(_ editor: RequestEditor) -> some View {
        ScrollView {
            RequestEditorView(editor: editor, wide: false)
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 110)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Theme.ground.ignoresSafeArea())
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 1) {
                    Text(editor.draft.name).font(Typo.title).foregroundStyle(Theme.text).lineLimit(1)
                    Text(readOnly ? "View only · \(editor.breadcrumb)" : editor.isDirty ? "Unsaved changes" : editor.breadcrumb)
                        .font(Typo.geist(12, relativeTo: .caption))
                        .foregroundStyle(editor.isDirty ? Theme.warning : Theme.textMuted)
                        .lineLimit(1)
                        .accessibilityIdentifier("save-state")
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    if !readOnly {
                        Button("Rename", systemImage: "pencil") { naming = .request(id) }
                        Button("Duplicate", systemImage: "plus.square.on.square") { model.duplicateRequest(id) }
                        Button("Move to…", systemImage: "folder") { moving = id }
                    }
                    Button(model.isPinned(request: id) ? "Unpin" : "Pin", systemImage: "pin") { model.togglePin(request: id) }
                    if editor.isDirty {
                        Button("Discard changes", systemImage: "arrow.uturn.backward") { editor.revert() }
                    }
                    if !readOnly {
                        Button("Delete", systemImage: "trash", role: .destructive) {
                            model.deleteRequest(id)
                            dismiss()
                        }
                    }
                } label: {
                    Image(systemName: "ellipsis")
                }
                .accessibilityLabel("More")
            }
        }
        // I5 (I4 review): the floating Save + Send bar covered the field being typed in (GraphQL
        // variables, the body, scripts) when the keyboard was up. It steps aside while the keyboard
        // shows; the keyboard's own bar has Done, Save and Send instead.
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in keyboardUp = true }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in keyboardUp = false }
        .toolbar {
            ToolbarItemGroup(placement: .keyboard) {
                Button("Done") { Self.endEditing() }
                    .accessibilityIdentifier("keyboard-done")
                Spacer()
                if !readOnly {
                    Button(editor.isDirty ? "Save" : "Saved") { editor.save() }
                        .disabled(!editor.isDirty)
                        .accessibilityIdentifier("save")
                }
                Button {
                    Self.endEditing()
                    let opens = !(editor.draft.isWebSocket && editor.socket.status != .closed) && !editor.isStreaming
                    editor.primaryAction()
                    if opens { showResponse = true }
                } label: {
                    Label(editor.sendTitle, systemImage: editor.sendIcon).labelStyle(.titleAndIcon).font(Typo.geist(15, .semibold))
                }
                .disabled(!editor.sendEnabled)
                .accessibilityLabel(editor.sendTitle)
                .accessibilityIdentifier("send")
            }
        }
        .safeAreaBar(edge: .bottom) {
            if !keyboardUp { bottomBar(editor) }
        }
        .sheet(isPresented: $showResponse) {
            ResponseSheet(editor: editor)
                .presentationDetents([.medium, .fraction(0.86), .large], selection: $detent)
                .presentationDragIndicator(.visible)
                .presentationBackgroundInteraction(.enabled(upThrough: .medium))
        }
        .modifier(LibraryActions(naming: $naming, moving: $moving, deleting: $deleting))
    }

    static func endEditing() {
        UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
    }

    private func bottomBar(_ editor: RequestEditor) -> some View {
            GlassEffectContainer(spacing: 10) {
                HStack(spacing: 10) {
                    if readOnly {
                        Label("View only", systemImage: "eye")
                            .font(Typo.geist(13, .semibold))
                            .foregroundStyle(Theme.textSoft)
                            .padding(.horizontal, 14)
                            .frame(height: 50)
                            .glassEffect(.regular, in: Capsule())
                            .accessibilityIdentifier("view-only")
                    } else {
                        Button { editor.save() } label: {
                            Image(systemName: editor.isDirty ? "square.and.arrow.down.fill" : "square.and.arrow.down")
                                .font(.system(size: 21, weight: .medium))
                                .foregroundStyle(editor.isDirty ? Theme.accentText : Theme.text)
                                .frame(width: 50, height: 50)
                                .overlay(alignment: .topTrailing) {
                                    if editor.isDirty { Circle().fill(Theme.warning).frame(width: 9, height: 9).offset(x: -6, y: 6) }
                                }
                        }
                        .buttonStyle(.glass)
                        .buttonBorderShape(.circle)
                        .accessibilityLabel(editor.isDirty ? "Save, unsaved changes" : "Saved")
                        .accessibilityIdentifier("save")
                    }
                    SendButton(title: editor.sendTitle, icon: editor.sendIcon, sending: editor.sendBusy) {
                        let opens = !(editor.draft.isWebSocket && editor.socket.status != .closed) && !editor.isStreaming
                        editor.primaryAction()
                        if opens { showResponse = true }
                    }
                    .disabled(!editor.sendEnabled)
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 8)
    }
}

/// The request editor itself (iPhone and the iPad middle column).
struct RequestEditorView: View {
    @Bindable var editor: RequestEditor
    var wide: Bool
    @Environment(AppModel.self) private var model
    @State private var tab: EditorTab = .params
    @State private var didPickTab = false

    private var tabs: [EditorTab] {
        if editor.draft.isWebSocket { return [.messages, .params, .headers, .auth] }
        if editor.draft.isGrpc { return [.call, .headers, .auth] }
        return wide ? [.params, .headers, .body, .auth, .scripts, .tests] : [.params, .headers, .body, .auth, .tests]
    }

    private var defaultTab: EditorTab {
        if editor.draft.isWebSocket { return .messages }
        if editor.draft.isGrpc { return .call }
        return editor.draft.bodyKind != .none ? .body : .params
    }

    /// A viewer's shared collection: everything shows, nothing edits (Send still works).
    private var readOnly: Bool { model.isReadOnly(model.collectionOf(request: editor.id)?.id) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if !wide { requestCard }
            tabBar
            switch tab {
            // Their own fields are read-only for a viewer; connecting, sending and loading the
            // schema still work.
            case .messages: WebSocketFieldsEditor(editor: editor, readOnly: readOnly)
            case .call: GrpcFieldsEditor(editor: editor, readOnly: readOnly)
            default:
                Group {
                    switch tab {
                    case .params: ParamsEditor(editor: editor)
                    case .headers: HeadersEditor(editor: editor, metadata: editor.draft.isGrpc)
                    case .body: BodyEditor(editor: editor, context: context, readOnly: readOnly)
                    case .auth: AuthEditor(editor: editor, collectionAuth: model.collectionOf(request: editor.id)?.auth)
                    case .scripts: ScriptsEditor(editor: editor, pre: true)
                    default: ScriptsEditor(editor: editor, pre: !wide ? nil : false)
                    }
                }
                .disabled(readOnly && tab != .body)
            }
        }
        .animation(nil, value: tab)
        .onAppear {
            guard !didPickTab else { return }
            didPickTab = true
            tab = defaultTab
        }
        .onChange(of: editor.draft.kind) { _, _ in
            // A request that changes kind opens on its own first tab (Messages, Call, or Body/Params).
            tab = defaultTab
        }
    }

    private var context: VariableContext { model.context(for: editor.collectionID) }

    @ViewBuilder
    private var tabBar: some View {
        let items = tabs.map { t -> (EditorTab, String) in
            switch t {
            case .headers where editor.draft.isGrpc: (t, "Metadata")
            case .headers where wide && !editor.draft.headers.isEmpty: (t, "Headers \(editor.draft.headers.count)")
            default: (t, t.rawValue)
            }
        }
        if wide {
            HStack(spacing: 18) {
                ForEach(items, id: \.0) { item in
                    let on = item.0 == tab
                    Button { tab = item.0 } label: {
                        Text(item.1)
                            .font(Typo.geist(14, on ? .semibold : .regular, relativeTo: .subheadline))
                            .foregroundStyle(on ? Color.white : Theme.textTab)
                            .padding(.bottom, 6)
                            .overlay(alignment: .bottom) { if on { Rectangle().fill(Theme.accent).frame(height: 2) } }
                            .frame(minHeight: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(on ? .isSelected : [])
                    .accessibilityIdentifier("tab-\(item.0.rawValue)")
                }
                Spacer()
            }
            .padding(.horizontal, 6)
        } else {
            SegmentTabs(items: items, selection: $tab, label: "Request parts")
        }
    }

    private var requestCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                MethodPicker(editor: editor).disabled(readOnly)
                EnvironmentMenu(compact: true)
                Spacer()
            }
            URLField(url: $editor.draft.url, context: context).disabled(readOnly)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .card(radius: 20)
    }
}

struct MethodPicker: View {
    @Bindable var editor: RequestEditor
    var height: CGFloat = 30
    static let methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"]

    var body: some View {
        let label = methodLabel(editor.draft)
        Menu {
            MethodMenuItems(editor: editor)
        } label: {
            HStack(spacing: 4) {
                Text(label).font(Typo.mono(13, .semibold))
                Image(systemName: "chevron.down").font(.system(size: 9, weight: .bold))
            }
            .fixedSize()
            .foregroundStyle(Theme.method(label))
            .padding(.horizontal, 10)
            .frame(height: height)
            .background(RoundedRectangle(cornerRadius: 9, style: .continuous).fill(Theme.method(label).opacity(0.14)))
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .accessibilityLabel("Method \(label == "WS" ? "WebSocket" : label == "GRPC" ? "gRPC" : label == "SSE" ? "Server-Sent Events" : editor.draft.method)")
        .accessibilityIdentifier("method")
    }
}

/// The URL: `${name}` chips you can tap to peek at the value; tap elsewhere to edit.
struct URLField: View {
    @Binding var url: String
    var context: VariableContext
    var size: CGFloat = 15
    var singleLine = false
    @State private var editing = false
    @State private var peek: PeekName?
    @State private var peekedAt = Date.distantPast
    @FocusState private var focused: Bool

    struct PeekName: Identifiable { var id: String }

    var body: some View {
        Group {
            if editing || focused || url.isEmpty {
                TextField("https://api.example.com/users", text: $url, axis: singleLine ? .horizontal : .vertical)
                    .font(Typo.mono(size, .medium))
                    .foregroundStyle(Theme.text)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .focused($focused)
                    .submitLabel(.done)
                    .onSubmit { editing = false }
                    .onChange(of: focused) { _, f in editing = f }
                    .accessibilityLabel("URL")
                    .accessibilityIdentifier("url")
            } else {
                Text(attributed)
                    .lineLimit(singleLine ? 1 : nil)
                    .tint(Theme.accentText)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                    .environment(\.openURL, OpenURLAction { link in
                        if link.scheme == "hitop-var",
                           let name = URLComponents(url: link, resolvingAgainstBaseURL: false)?.queryItems?.first(where: { $0.name == "name" })?.value {
                            peekedAt = Date()
                            peek = PeekName(id: name)
                        }
                        return .handled
                    })
                    .simultaneousGesture(TapGesture().onEnded {
                        // A tap on a chip opens the peek (the link above); anywhere else edits.
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                            if Date().timeIntervalSince(peekedAt) > 0.5 {
                                editing = true
                                focused = true
                            }
                        }
                    })
                    .accessibilityLabel("URL \(url)")
                    .accessibilityHint("Double-tap to edit")
                    .accessibilityAddTraits(.isButton)
                    .accessibilityIdentifier("url-display")
                    .accessibilityActions {
                        ForEach(VariableText.names(url), id: \.self) { name in
                            Button("Show \(name)") { peek = PeekName(id: name) }
                        }
                    }
            }
        }
        .popover(item: $peek) { p in
            VariablePeek(name: p.id, context: context)
                .presentationCompactAdaptation(.popover)
        }
    }

    private var attributed: AttributedString {
        var out = AttributedString()
        var inQuery = false
        for seg in VariableText.segments(url) {
            switch seg {
            case let .variable(name):
                var a = AttributedString("${\(name)}")
                a.font = Typo.mono(size, .medium)
                a.backgroundColor = Theme.chipFill
                a.foregroundColor = context.source(name).map { $0.missing ? Theme.danger : Theme.accentText } ?? Theme.danger
                var comps = URLComponents()
                comps.scheme = "hitop-var"
                comps.path = "peek"
                comps.queryItems = [URLQueryItem(name: "name", value: name)]
                a.link = comps.url
                out += a
            case let .text(t):
                for ch in t {
                    var a = AttributedString(String(ch))
                    a.font = Typo.mono(size, .medium)
                    if ch == "?" { inQuery = true }
                    let muted = inQuery && (ch == "?" || ch == "&" || ch == "=")
                    a.foregroundColor = muted ? Theme.textFaint : Theme.text
                    out += a
                }
            }
        }
        return out
    }
}

/// What a `${name}` is: its value (secrets masked until revealed) and where it comes from.
struct VariablePeek: View {
    var name: String
    var context: VariableContext
    @State private var reveal = false

    var body: some View {
        let s = context.source(name)
        VStack(alignment: .leading, spacing: 8) {
            Text("${\(name)}").font(Typo.mono(14, .semibold)).foregroundStyle(Theme.accentText)
            if let s, !s.missing {
                HStack {
                    Text(s.secret && !reveal ? "••••••" : (s.value.isEmpty ? "(empty)" : s.value))
                        .font(Typo.mono(14))
                        .foregroundStyle(Theme.text)
                        .textSelection(.enabled)
                        .accessibilityIdentifier("peek-value")
                    if s.secret {
                        Button(reveal ? "Hide" : "Show") { reveal.toggle() }.font(Typo.caption)
                    }
                }
                Text("From \(s.scope)\(s.secret ? " · secret, in the Keychain" : "")").font(Typo.caption).foregroundStyle(Theme.textMuted)
            } else {
                Text(s?.secret == true ? "This secret has no value on this device." : "No value: define it in an environment, the collection or the globals.")
                    .font(Typo.caption).foregroundStyle(Theme.warning)
            }
        }
        .padding(16)
        .frame(minWidth: 240, alignment: .leading)
    }
}

// MARK: - Tabs

struct ParamsEditor: View {
    @Bindable var editor: RequestEditor

    var body: some View {
        let rows = editor.params
        VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { i, _ in
                KeyValueRow(key: binding(i, \.key), value: binding(i, \.value), enabled: binding(i, \.enabled),
                            keyPlaceholder: "Key", valuePlaceholder: "Value") {
                    var all = editor.params
                    all.remove(at: i)
                    editor.setParams(all)
                }
                if i < rows.count - 1 { Divider().overlay(Theme.separator) }
            }
            if rows.isEmpty {
                Text("Query parameters come from the URL; add one here or type it after a ?.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted).padding(14).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .card(radius: 18)
        SmallChipButton(title: "Add parameter") {
            editor.setParams(editor.params + [.init(id: -1, key: "key", value: "", enabled: true)])
        }
        .accessibilityIdentifier("add-param")
    }

    private func binding<T>(_ i: Int, _ kp: WritableKeyPath<RequestEditor.ParamItem, T>) -> Binding<T> {
        let fallback = editor.params.indices.contains(i) ? editor.params[i][keyPath: kp] : RequestEditor.ParamItem(id: i, key: "", value: "", enabled: true)[keyPath: kp]
        return Binding(get: {
            let rows = editor.params
            return rows.indices.contains(i) ? rows[i][keyPath: kp] : fallback
        }, set: { v in
            var all = editor.params
            guard i < all.count else { return }
            all[i][keyPath: kp] = v
            editor.setParams(all)
        })
    }
}

struct HeadersEditor: View {
    @Bindable var editor: RequestEditor
    /// A gRPC request's headers are its metadata.
    var metadata = false

    var body: some View {
        VStack(spacing: 0) {
            ForEach($editor.draft.headers) { $h in
                KeyValueRow(key: $h.name, value: $h.value, enabled: $h.enabled, keyPlaceholder: "Header", valuePlaceholder: "Value") {
                    editor.draft.headers.removeAll { $0.id == h.id }
                }
                Divider().overlay(Theme.separator)
            }
            if editor.draft.headers.isEmpty {
                Text(metadata ? "No metadata. Each header is sent as gRPC metadata (a name ending in -bin takes a base64 value); the auth too."
                     : "No headers. Content-Type is added for JSON, form and GraphQL bodies when you send.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted).padding(14).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .card(radius: 18)
        SmallChipButton(title: "Add header") { editor.draft.headers.append(KeyValue("", "")) }
            .accessibilityIdentifier("add-header")
    }
}

struct BodyEditor: View {
    @Bindable var editor: RequestEditor
    var context: VariableContext
    /// A viewer: the body doesn't change, but the GraphQL schema still loads.
    var readOnly = false
    @State private var editing = false

    var body: some View {
        let kind = editor.draft.bodyKind
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Menu {
                    Picker("Body", selection: Binding(get: { editor.draft.bodyKind }, set: { editor.draft.bodyKind = $0 })) {
                        ForEach(BodyKind.allCases) { k in Text(k.label).tag(k) }
                    }
                } label: {
                    HStack(spacing: 4) {
                        Text(kind.label.uppercased()).font(Typo.mono(11, .semibold))
                        Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold))
                    }
                    .foregroundStyle(Theme.textSoft)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(RoundedRectangle(cornerRadius: 7, style: .continuous).fill(Theme.fill))
                    .frame(minHeight: 44)
                    .contentShape(Rectangle())
                }
                .accessibilityLabel("Body type \(kind.label)")
                .accessibilityIdentifier("body-kind")
                .disabled(readOnly)
                if kind == .json {
                    let check = editor.jsonCheck
                    Text(check.valid ? "Valid" : "Not valid JSON")
                        .font(Typo.geist(12.5, relativeTo: .caption))
                        .foregroundStyle(check.valid ? Theme.success : Theme.danger)
                        .accessibilityIdentifier("json-validity")
                }
                Spacer()
                if editing {
                    Button("Done") { editing = false }.font(Typo.geist(14, .semibold))
                }
            }
            switch kind {
            case .none:
                Text("This request has no body. Pick JSON, Raw, Form or GraphQL above.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted)
            case .json, .text:
                if editing {
                    CodeEditor(text: $editor.draft.body, placeholder: kind == .json ? "{ }" : "Body", minHeight: 220, identifier: "body-editor")
                } else {
                    CodeBlock(text: editor.draft.body, json: kind == .json, placeholder: "Tap to write the body")
                        .onTapGesture { if !readOnly { editing = true } }
                        .accessibilityAddTraits(.isButton)
                        .accessibilityHint("Double-tap to edit")
                        .accessibilityIdentifier("body-preview")
                }
                if kind == .json { symbolRow.disabled(readOnly) }
            case .form, .multipart:
                FormRowsEditor(rows: $editor.draft.formFields).disabled(readOnly)
            case .graphql:
                GraphQLSchemaBar(editor: editor)
                Text("QUERY").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
                GraphQLQueryEditor(editor: editor).disabled(readOnly)
                Text("VARIABLES").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
                CodeEditor(text: $editor.draft.graphqlVariables, placeholder: "{ \"id\": 1 }", minHeight: 90, identifier: "graphql-variables")
                    .disabled(readOnly)
                TextField("Operation name (optional)", text: $editor.draft.graphqlOperationName)
                    .font(Typo.mono(14))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .padding(12)
                    .card(radius: 14)
                    .disabled(readOnly)
            }
        }
    }

    /// The drafts' helper row: characters that are awkward on the iPhone keyboard, and Format.
    private var symbolRow: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(["{ }", "\"", ":", "[ ]"], id: \.self) { s in
                    SmallChipButton(title: s, mono: true) { insert(s) }.accessibilityLabel("Insert \(s)")
                }
                SmallChipButton(title: "${ }", mono: true, tint: Theme.accentText, fill: Theme.accent.opacity(0.16)) { insert("${}") }
                    .accessibilityLabel("Insert variable")
                SmallChipButton(title: "Format") { editor.formatBody() }
                    .accessibilityIdentifier("format")
            }
        }
    }

    private func insert(_ s: String) {
        editor.draft.body += s.replacingOccurrences(of: " ", with: "")
        editing = true
    }
}

struct FormRowsEditor: View {
    @Binding var rows: [FormRow]

    var body: some View {
        VStack(spacing: 0) {
            ForEach($rows) { $r in
                KeyValueRow(key: $r.key, value: $r.value, enabled: $r.enabled, keyPlaceholder: "Field", valuePlaceholder: "Value") {
                    rows.removeAll { $0.id == r.id }
                }
                Divider().overlay(Theme.separator)
            }
            if rows.isEmpty {
                Text("No fields yet.").font(Typo.caption).foregroundStyle(Theme.textMuted).padding(14).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .card(radius: 18)
        SmallChipButton(title: "Add field") { rows.append(FormRow()) }
    }
}

struct AuthEditor: View {
    @Bindable var editor: RequestEditor
    var collectionAuth: AuthSpec?

    private enum Kind: String, CaseIterable { case inherit = "From collection", none = "None", bearer = "Bearer token", basic = "Basic", apiKey = "API key", other = "Set in another app" }

    private var isOther: Bool {
        if case .other? = editor.draft.auth { return true }
        return false
    }

    private var kind: Binding<Kind> {
        Binding(get: {
            switch editor.draft.auth {
            case nil: .inherit
            case .none?: .none
            case .bearer?: .bearer
            case .basic?: .basic
            case .apiKey?: .apiKey
            case .other?: .other
            }
        }, set: { k in
            switch k {
            case .other: break
            case .inherit: editor.draft.auth = nil
            case .none: editor.draft.auth = AuthSpec.none
            case .bearer: editor.draft.auth = .bearer(token: "")
            case .basic: editor.draft.auth = .basic(username: "", password: "")
            case .apiKey: editor.draft.auth = .apiKey(key: "X-API-Key", value: "", inQuery: false)
            }
        })
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Picker("Auth", selection: kind) {
                ForEach(Kind.allCases.filter { $0 != .other || isOther }, id: \.self) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.menu)
            .accessibilityIdentifier("auth-kind")
            VStack(spacing: 0) {
                switch editor.draft.auth {
                case nil:
                    note(collectionAuth.map { "The collection's \($0.kind == "apikey" ? "API key" : $0.kind) auth applies." } ?? "The collection has no auth.")
                case .none?:
                    note("No Authorization is sent, even when the collection has auth.")
                case let .other(type, _)?:
                    note("\(type) auth, set in the web or desktop app. It is kept as it is; this version sends the request without it.")
                case let .bearer(token)?:
                    field("Token", token) { editor.draft.auth = .bearer(token: $0) }
                case let .basic(u, p)?:
                    field("Username", u) { editor.draft.auth = .basic(username: $0, password: p) }
                    Divider().overlay(Theme.separator)
                    field("Password", p, secure: true) { editor.draft.auth = .basic(username: u, password: $0) }
                case let .apiKey(k, v, q)?:
                    field("Key", k) { editor.draft.auth = .apiKey(key: $0, value: v, inQuery: q) }
                    Divider().overlay(Theme.separator)
                    field("Value", v) { editor.draft.auth = .apiKey(key: k, value: $0, inQuery: q) }
                    Divider().overlay(Theme.separator)
                    Picker("Send in", selection: Binding(get: { q }, set: { editor.draft.auth = .apiKey(key: k, value: v, inQuery: $0) })) {
                        Text("Header").tag(false)
                        Text("Query").tag(true)
                    }
                    .pickerStyle(.segmented)
                    .padding(12)
                }
            }
            .card(radius: 18)
            Text("Values can use ${variables}; keep tokens in an environment secret.").font(Typo.caption).foregroundStyle(Theme.textMuted)
        }
    }

    private func note(_ s: String) -> some View {
        Text(s).font(Typo.caption).foregroundStyle(Theme.textMuted).padding(14).frame(maxWidth: .infinity, alignment: .leading)
    }

    private func field(_ label: String, _ value: String, secure: Bool = false, set: @escaping (String) -> Void) -> some View {
        HStack {
            Text(label).font(Typo.geist(14, .medium)).foregroundStyle(Theme.textSoft).frame(width: 90, alignment: .leading)
            Group {
                if secure && !value.hasPrefix("${") {
                    SecureField(label, text: Binding(get: { value }, set: set))
                } else {
                    TextField(label, text: Binding(get: { value }, set: set))
                }
            }
            .font(Typo.mono(14))
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .accessibilityIdentifier("auth-\(label.lowercased())")
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 48)
    }
}

/// Scripts: pre-request and tests (post-request), run in JavaScriptCore with the pm.* shim.
struct ScriptsEditor: View {
    @Bindable var editor: RequestEditor
    /// true: pre-request only; false: tests only; nil: both behind a switch (iPhone).
    var pre: Bool?
    @State private var showPre = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if pre == nil {
                Picker("Script", selection: $showPre) {
                    Text("Tests").tag(false)
                    Text("Pre-request").tag(true)
                }
                .pickerStyle(.segmented)
            }
            let isPre = pre ?? showPre
            if isPre {
                CodeEditor(text: $editor.draft.preRequestScript, placeholder: "pm.environment.set(\"ts\", Date.now());", minHeight: 200, identifier: "pre-script")
                Text("Runs before the request is sent; it can set variables and change headers.").font(Typo.caption).foregroundStyle(Theme.textMuted)
            } else {
                CodeEditor(text: $editor.draft.postRequestScript, placeholder: "pm.test(\"status is 200\", () => pm.response.to.have.status(200));", minHeight: 200, identifier: "tests-script")
                Text("Runs after the response arrives; each pm.test is a check in the response's Tests tab.").font(Typo.caption).foregroundStyle(Theme.textMuted)
            }
        }
    }
}
