import SwiftUI
import UIKit

// I4: WebSocket, Server-Sent Events, gRPC and GraphQL schema screens. The request's own fields
// are tabs of the request editor; the live parts (the socket's log, the event stream, the call's
// events) take the response's place: the sheet on iPhone, the right column on iPad.

extension RequestEditor {
    /// What the Send button says and does for this kind of request.
    var sendTitle: String {
        if draft.isWebSocket { return socket.status == .closed ? "Connect" : socket.status == .connecting ? "Cancel" : "Disconnect" }
        if draft.isGrpc { return grpcSession.calling ? "Cancel" : "Call" }
        return isStreaming ? "Stop" : "Send"
    }

    var sendIcon: String {
        if draft.isWebSocket { return socket.status == .closed ? "bolt.horizontal.fill" : "xmark" }
        if draft.isGrpc { return grpcSession.calling ? "xmark" : "arrow.right.circle.fill" }
        return isStreaming ? "stop.fill" : "paperplane.fill"
    }

    /// The spinner state of the Send button (a live stream shows Stop instead).
    var sendBusy: Bool { isSending && !isStreaming && !draft.isWebSocket && !draft.isGrpc }

    var sendEnabled: Bool {
        if draft.isWebSocket || draft.isGrpc || isStreaming { return !draft.url.trimmingCharacters(in: .whitespaces).isEmpty }
        return canSend || isSending
    }

    /// Send, Connect/Disconnect, Call/Cancel or Stop.
    func primaryAction() {
        if isStreaming { stopStream() } else { send() }
    }
}

/// "10:42:07.318": when a log line or event came.
func clockText(_ d: Date) -> String {
    let c = Calendar.current.dateComponents([.hour, .minute, .second, .nanosecond], from: d)
    return String(format: "%02d:%02d:%02d.%03d", c.hour ?? 0, c.minute ?? 0, c.second ?? 0, (c.nanosecond ?? 0) / 1_000_000)
}

// MARK: - Method menu

struct MethodMenuItems: View {
    @Bindable var editor: RequestEditor

    var body: some View {
        Picker("Method", selection: Binding(get: { editor.draft.mode }, set: { editor.draft.setMode($0) })) {
            ForEach(MethodPicker.methods, id: \.self) { m in Text(m).tag(SavedRequest.Mode.http(m)) }
        }
        Divider()
        Picker("Protocol", selection: Binding(get: { editor.draft.mode }, set: { editor.draft.setMode($0) })) {
            Label("Server-Sent Events", systemImage: "dot.radiowaves.left.and.right").tag(SavedRequest.Mode.sse)
            Label("WebSocket", systemImage: "bolt.horizontal").tag(SavedRequest.Mode.websocket)
            Label("gRPC", systemImage: "point.3.connected.trianglepath.dotted").tag(SavedRequest.Mode.grpc)
        }
    }
}

// MARK: - WebSocket: the request's fields

struct WebSocketFieldsEditor: View {
    @Bindable var editor: RequestEditor
    var readOnly: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("SAVED MESSAGES").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint).accessibilityAddTraits(.isHeader)
            VStack(spacing: 0) {
                ForEach(Array(editor.draft.websocket.messages.enumerated()), id: \.offset) { i, m in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            TextField("Name", text: messageBinding(i, \.name))
                                .font(Typo.geist(14, .semibold))
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                                .disabled(readOnly)
                            Spacer()
                            Button {
                                Task { await editor.sendMessage(m.content) }
                            } label: {
                                Label("Send", systemImage: "paperplane").font(Typo.geist(13, .semibold))
                            }
                            .buttonStyle(.glass)
                            .disabled(editor.socket.status != .open)
                            .accessibilityLabel("Send \(m.name)")
                            .accessibilityIdentifier("ws-send-saved-\(m.name)")
                            if !readOnly {
                                Button(role: .destructive) {
                                    editor.draft.websocket.messages.remove(at: i)
                                } label: { Image(systemName: "minus.circle").frame(width: 44, height: 44).contentShape(Rectangle()) }
                                .frame(width: 44, height: 44)
                                .accessibilityLabel("Delete \(m.name)")
                            }
                        }
                        TextField("Message", text: messageBinding(i, \.content), axis: .vertical)
                            .font(Typo.mono(13.5))
                            .foregroundStyle(Theme.codeText)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .lineLimit(1...6)
                            .disabled(readOnly)
                            .frame(minHeight: 44)
                            .accessibilityIdentifier("ws-saved-content-\(i)")
                    }
                    .padding(.horizontal, 14).padding(.vertical, 10)
                    if i < editor.draft.websocket.messages.count - 1 { Divider().overlay(Theme.separator) }
                }
                if editor.draft.websocket.messages.isEmpty {
                    Text("Messages you send often, with ${variables}. They are saved with the request.")
                        .font(Typo.caption).foregroundStyle(Theme.textMuted).padding(14).frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .card(radius: 18)
            if !readOnly {
                SmallChipButton(title: "Add message") {
                    editor.draft.websocket.messages.append(WSSavedMessage(name: "message \(editor.draft.websocket.messages.count + 1)", content: ""))
                }
                .accessibilityIdentifier("ws-add-message")
            }
            Text("SUBPROTOCOLS").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint).padding(.top, 6).accessibilityAddTraits(.isHeader)
            VStack(spacing: 0) {
                ForEach(Array(editor.draft.websocket.subprotocols.enumerated()), id: \.offset) { i, _ in
                    HStack {
                        TextField("chat.v1", text: Binding(get: {
                            editor.draft.websocket.subprotocols.indices.contains(i) ? editor.draft.websocket.subprotocols[i] : ""
                        }, set: { v in
                            guard editor.draft.websocket.subprotocols.indices.contains(i) else { return }
                            editor.draft.websocket.subprotocols[i] = v
                        }))
                        .font(Typo.mono(14))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .disabled(readOnly)
                        if !readOnly {
                            Button(role: .destructive) { editor.draft.websocket.subprotocols.remove(at: i) } label: { Image(systemName: "minus.circle").frame(width: 44, height: 44).contentShape(Rectangle()) }
                                .frame(width: 44, height: 44)
                                .accessibilityLabel("Remove subprotocol")
                        }
                    }
                    .padding(.horizontal, 14).frame(minHeight: 46)
                    if i < editor.draft.websocket.subprotocols.count - 1 { Divider().overlay(Theme.separator) }
                }
                if editor.draft.websocket.subprotocols.isEmpty {
                    Text("Offered in Sec-WebSocket-Protocol, in order; the server picks one.")
                        .font(Typo.caption).foregroundStyle(Theme.textMuted).padding(14).frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .card(radius: 18)
            if !readOnly {
                SmallChipButton(title: "Add subprotocol") { editor.draft.websocket.subprotocols.append("") }
            }
            Text("The URL, params, headers and auth go with the opening handshake.").font(Typo.caption).foregroundStyle(Theme.textMuted)
        }
    }

    private func messageBinding(_ i: Int, _ kp: WritableKeyPath<WSSavedMessage, String>) -> Binding<String> {
        Binding(get: {
            editor.draft.websocket.messages.indices.contains(i) ? editor.draft.websocket.messages[i][keyPath: kp] : ""
        }, set: { v in
            guard editor.draft.websocket.messages.indices.contains(i) else { return }
            editor.draft.websocket.messages[i][keyPath: kp] = v
        })
    }
}

// MARK: - WebSocket: the connection

struct SocketPanel: View {
    @Bindable var editor: RequestEditor
    var close: (() -> Void)?

    private var s: SocketSession { editor.socket }

    private var statusText: String {
        switch s.status {
        case .closed: "Not connected"
        case .connecting: "Connecting…"
        case .open: s.negotiated.map { "Connected · \($0)" } ?? "Connected"
        }
    }

    private var statusColor: Color {
        switch s.status {
        case .closed: Theme.textMuted
        case .connecting: Theme.warning
        case .open: Theme.success
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Circle().fill(statusColor).frame(width: 9, height: 9)
                Text(statusText).font(Typo.geist(15, .semibold)).foregroundStyle(Theme.text)
                    .lineLimit(1)
                    .accessibilityIdentifier("socket-status")
                Spacer(minLength: 0)
                Button(s.status == .closed ? "Connect" : "Disconnect") { editor.send() }
                    .buttonStyle(.glass)
                    .accessibilityIdentifier("socket-toggle")
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
            if !editor.draft.websocket.messages.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(Array(editor.draft.websocket.messages.enumerated()), id: \.offset) { _, m in
                            SmallChipButton(title: m.name.isEmpty ? m.content : m.name, tint: Theme.accentText, fill: Theme.accent.opacity(0.14)) {
                                Task { await editor.sendMessage(m.content) }
                            }
                            .disabled(s.status != .open)
                            .accessibilityLabel("Send saved message \(m.name)")
                            .accessibilityIdentifier("ws-saved-\(m.name)")
                        }
                    }
                }
            }
            HStack(spacing: 8) {
                TextField("Message", text: Bindable(s).draft, axis: .vertical)
                    .font(Typo.mono(14))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .lineLimit(1...4)
                    .padding(.horizontal, 12).padding(.vertical, 10)
                    .card(radius: 16, fill: Theme.codeGround)
                    .frame(minHeight: 44)
                    .accessibilityIdentifier("ws-composer")
                Button {
                    let text = s.draft
                    Task {
                        await editor.sendMessage(text)
                        if s.log.last?.kind == .sent { s.draft = "" }
                    }
                } label: {
                    Image(systemName: "paperplane.fill").font(.system(size: 16, weight: .semibold)).frame(width: 44, height: 44)
                }
                .buttonStyle(.glassProminent)
                .buttonBorderShape(.circle)
                .tint(Theme.accent)
                .disabled(s.status != .open || s.draft.isEmpty)
                .accessibilityLabel("Send message")
                .accessibilityIdentifier("ws-send")
            }
            HStack {
                Text("\(s.log.count) in the log").font(Typo.caption).foregroundStyle(Theme.textMuted)
                Spacer()
                if !s.log.isEmpty { Button { s.clear() } label: { Text("Clear").frame(minWidth: 44, minHeight: 44).contentShape(Rectangle()) }.font(Typo.caption) }
            }
            if s.log.isEmpty {
                Text("Connect to open the socket; what you send and what arrives is listed here with its time.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted).frame(maxWidth: .infinity, minHeight: 80)
            } else {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(s.log.reversed()) { e in
                        SocketLogRow(entry: e)
                        Divider().overlay(Theme.separator)
                    }
                }
                .card(radius: 18, fill: Theme.sheetCode)
            }
        }
    }
}

struct SocketLogRow: View {
    var entry: WSLogEntry

    private var icon: (String, Color, String) {
        switch entry.kind {
        case .sent: ("arrow.up", Theme.accentText, "Sent")
        case .received: ("arrow.down", Theme.success, "Received")
        case .error: ("exclamationmark.triangle", Theme.danger, "Error")
        case .system: ("info.circle", Theme.textMuted, "")
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: icon.0).font(.system(size: 11, weight: .bold)).foregroundStyle(icon.1).frame(width: 16).padding(.top, 2)
            VStack(alignment: .leading, spacing: 2) {
                Text(clockText(entry.time)).font(Typo.mono(10.5)).foregroundStyle(Theme.textFaint)
                Text(entry.display)
                    .font(entry.kind == .system ? Typo.caption : Typo.mono(13))
                    .foregroundStyle(entry.kind == .error ? Theme.danger : entry.kind == .system ? Theme.textMuted : Theme.codeText)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(icon.2.isEmpty ? "" : icon.2 + ": ")\(entry.display)")
        .accessibilityIdentifier("ws-log-\(entry.kind.rawValue)")
    }
}

// MARK: - Server-Sent Events

struct EventsList: View {
    var events: [SSEEvent]

    var body: some View {
        if events.isEmpty {
            Text("No events yet.").font(Typo.caption).foregroundStyle(Theme.textMuted).frame(maxWidth: .infinity, minHeight: 80)
                .accessibilityIdentifier("events-empty")
        } else {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(events.reversed()) { e in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 8) {
                            Text(e.event).font(Typo.mono(11.5, .semibold)).foregroundStyle(Theme.method("SSE"))
                                .padding(.horizontal, 7).padding(.vertical, 2)
                                .background(RoundedRectangle(cornerRadius: 6).fill(Theme.method("SSE").opacity(0.14)))
                            if let id = e.lastEventID { Text("id \(id)").font(Typo.mono(11)).foregroundStyle(Theme.textMuted) }
                            Spacer()
                            Text(clockText(e.at)).font(Typo.mono(10.5)).foregroundStyle(Theme.textFaint)
                        }
                        Text(e.data).font(Typo.mono(13)).foregroundStyle(Theme.codeText).textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .padding(.horizontal, 12).padding(.vertical, 9)
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("Event \(e.event)\(e.lastEventID.map { ", id \($0)" } ?? ""): \(e.data)")
                    .accessibilityIdentifier("event-\(e.id)")
                    Divider().overlay(Theme.separator)
                }
            }
            .card(radius: 18, fill: Theme.sheetCode)
        }
    }
}

/// An event stream being read: status, how many events, Stop, the events as they arrive.
struct LiveEventsView: View {
    var editor: RequestEditor
    var close: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 8) {
                StatusPill(status: editor.streamOpen?.status ?? 200)
                HStack(spacing: 6) {
                    Circle().fill(Theme.success).frame(width: 8, height: 8)
                    Text("Live · \(editor.liveEvents.count) \(editor.liveEvents.count == 1 ? "event" : "events")")
                        .font(Typo.geist(14, .semibold)).foregroundStyle(Theme.text)
                        .accessibilityIdentifier("events-count")
                }
                Spacer(minLength: 0)
                Button {
                    editor.stopStream()
                } label: { Label("Stop", systemImage: "stop.fill") }
                    .buttonStyle(.glass)
                    .accessibilityIdentifier("stop-stream")
                if let close {
                    Button(action: close) {
                        Image(systemName: "xmark").font(.system(size: 14, weight: .semibold)).foregroundStyle(Theme.text).frame(width: 44, height: 44)
                    }
                    .buttonStyle(.glass)
                    .buttonBorderShape(.circle)
                    .accessibilityLabel("Close")
                }
            }
            Text("An event stream: events show as they arrive, until the server ends it or you stop it.")
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
            EventsList(events: editor.liveEvents)
        }
    }
}

// MARK: - gRPC: the request's fields

struct GrpcFieldsEditor: View {
    @Bindable var editor: RequestEditor
    var readOnly: Bool

    private var g: GrpcSession { editor.grpcSession }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Toggle(isOn: $editor.draft.grpc.plaintext) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Plaintext").font(Typo.geist(14, .medium)).foregroundStyle(Theme.text)
                        Text("HTTP/2 without TLS for a host:port address").font(Typo.caption).foregroundStyle(Theme.textMuted)
                    }
                }
                .tint(Theme.accent)
                .disabled(readOnly)
                .accessibilityIdentifier("grpc-plaintext")
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .card(radius: 18)

            HStack(spacing: 10) {
                Button {
                    Task { await editor.loadGrpcSchema() }
                } label: {
                    if g.loadingSchema { ProgressView() } else { Label(g.listing == nil ? "Load services" : "Reload", systemImage: "arrow.down.circle") }
                }
                .buttonStyle(.glass)
                .accessibilityIdentifier("grpc-load")
                if let listing = g.listing {
                    Text("\(listing.services.count) \(listing.services.count == 1 ? "service" : "services") · \(listing.source)")
                        .font(Typo.caption).foregroundStyle(Theme.textMuted)
                        .accessibilityIdentifier("grpc-source")
                }
                Spacer()
            }
            if let e = g.schemaError {
                Label(e, systemImage: g.noReflection ? "info.circle" : "exclamationmark.triangle")
                    .font(Typo.caption).foregroundStyle(g.noReflection ? Theme.textSoft : Theme.danger)
                    .accessibilityIdentifier(g.noReflection ? "grpc-no-reflection" : "grpc-schema-error")
            }

            VStack(spacing: 0) {
                row("Service") {
                    if let listing = g.listing, !readOnly {
                        Menu {
                            ForEach(listing.services) { s in
                                Button(s.name) {
                                    editor.draft.grpc.service = s.name
                                    if !(s.methods.contains { $0.name == editor.draft.grpc.method }) { editor.draft.grpc.method = s.methods.first?.name ?? "" }
                                }
                            }
                        } label: { menuLabel(editor.draft.grpc.service, placeholder: "Pick a service") }
                        .accessibilityIdentifier("grpc-service")
                    } else {
                        field("package.Service", $editor.draft.grpc.service).accessibilityIdentifier("grpc-service-field")
                    }
                }
                Divider().overlay(Theme.separator)
                row("Method") {
                    if let service = g.listing?.services.first(where: { $0.name == editor.draft.grpc.service }), !readOnly {
                        Menu {
                            ForEach(service.methods) { m in
                                Button {
                                    editor.draft.grpc.method = m.name
                                } label: {
                                    Text("\(m.name) · \(m.shape)")
                                }
                            }
                        } label: { menuLabel(editor.draft.grpc.method, placeholder: "Pick a method") }
                        .accessibilityIdentifier("grpc-method")
                    } else {
                        field("Method", $editor.draft.grpc.method).accessibilityIdentifier("grpc-method-field")
                    }
                }
            }
            .card(radius: 18)
            if let m = currentMethod, m.clientStreaming {
                Label("\(m.name) is \(m.shape): write its messages as JSON one after another.", systemImage: "info.circle")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted)
            }

            HStack {
                Text("MESSAGE").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint).accessibilityAddTraits(.isHeader)
                Spacer()
                if !readOnly {
                    SmallChipButton(title: "Template") { Task { await editor.useGrpcTemplate() } }
                        .disabled(editor.draft.grpc.service.isEmpty || editor.draft.grpc.method.isEmpty)
                        .accessibilityIdentifier("grpc-template")
                }
            }
            CodeEditor(text: $editor.draft.grpc.message, placeholder: "{ }", minHeight: 140, identifier: "grpc-message")
                .disabled(readOnly)
            Text("JSON in the proto3 mapping; ${variables} work anywhere in it. Headers are sent as metadata, the auth too.")
                .font(Typo.caption).foregroundStyle(Theme.textMuted)
        }
    }

    private var currentMethod: GrpcMethodInfo? {
        g.listing?.services.first { $0.name == editor.draft.grpc.service }?.methods.first { $0.name == editor.draft.grpc.method }
    }

    private func row(_ label: String, @ViewBuilder content: () -> some View) -> some View {
        LabeledRow(label: label, width: 74) {
            HStack {
                content()
                Spacer(minLength: 0)
            }
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 48)
    }

    private func field(_ placeholder: String, _ text: Binding<String>) -> some View {
        TextField(placeholder, text: text)
            .font(Typo.mono(14))
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .disabled(readOnly)
    }

    private func menuLabel(_ value: String, placeholder: String) -> some View {
        HStack(spacing: 4) {
            Text(value.isEmpty ? placeholder : value).font(Typo.mono(13.5, .medium)).foregroundStyle(value.isEmpty ? Theme.textMuted : Theme.text)
                .lineLimit(1).truncationMode(.middle)
            Image(systemName: "chevron.up.chevron.down").font(.system(size: 10, weight: .semibold)).foregroundStyle(Theme.textMuted)
        }
        .frame(minHeight: 44)
        .contentShape(Rectangle())
    }
}

// MARK: - gRPC: the call

struct GrpcPanel: View {
    var editor: RequestEditor
    var close: (() -> Void)?

    private var g: GrpcSession { editor.grpcSession }

    private var statusLine: (String, Color) {
        if g.calling { return ("Calling \(editor.draft.grpc.method)…", Theme.warning) }
        switch g.lastStatus {
        case let .status(code, name, _, _)?: return (name, code == 0 ? Theme.success : Theme.danger)
        case .error?: return ("Not called", Theme.danger)
        default: return ("No call yet", Theme.textMuted)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Text(statusLine.0)
                    .font(Typo.mono(14, .semibold)).foregroundStyle(statusLine.1)
                    .padding(.horizontal, 10).padding(.vertical, 5)
                    .background(Capsule().fill(statusLine.1.opacity(0.14)))
                    .accessibilityIdentifier("grpc-status")
                Text("\(editor.draft.grpc.service.split(separator: ".").last.map(String.init) ?? "")/\(editor.draft.grpc.method)")
                    .font(Typo.mono(12)).foregroundStyle(Theme.textMuted).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 0)
                Button(g.calling ? "Cancel" : "Call") { editor.send() }
                    .buttonStyle(.glass)
                    .accessibilityIdentifier("grpc-call")
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
            HStack {
                Text("\(g.log.filter { if case .message = $0.event { true } else { false } }.count) messages · \(g.log.count) in the log")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted)
                    .accessibilityIdentifier("grpc-count")
                Spacer()
                if !g.log.isEmpty { Button { g.clear() } label: { Text("Clear").frame(minWidth: 44, minHeight: 44).contentShape(Rectangle()) }.font(Typo.caption) }
            }
            if g.log.isEmpty {
                Text("Call the method: the response headers, each message and the final status are listed here.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted).frame(maxWidth: .infinity, minHeight: 80)
            } else {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(g.log) { e in
                        GrpcLogRow(entry: e)
                        Divider().overlay(Theme.separator)
                    }
                }
                .card(radius: 18, fill: Theme.sheetCode)
            }
        }
    }
}

struct GrpcLogRow: View {
    var entry: GrpcLogEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            HStack {
                Text(title).font(Typo.label).tracking(0.8).foregroundStyle(color).accessibilityAddTraits(.isHeader)
                Spacer()
                Text(clockText(entry.time)).font(Typo.mono(10.5)).foregroundStyle(Theme.textFaint)
            }
            switch entry.event {
            case let .headers(pairs): pairList(pairs, empty: "No headers")
            case let .message(json): CodeBlock(text: json, json: true, fill: .clear)
            case let .status(_, _, message, trailers):
                if !message.isEmpty { Text(message).font(Typo.mono(13)).foregroundStyle(Theme.codeText).textSelection(.enabled) }
                if !trailers.isEmpty { pairList(trailers, empty: "") }
            case let .error(message):
                Text(message).font(Typo.mono(13)).foregroundStyle(Theme.danger).textSelection(.enabled)
            }
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("grpc-log-\(kind)")
    }

    private var kind: String {
        switch entry.event {
        case .headers: "headers"
        case .message: "message"
        case .status: "status"
        case .error: "error"
        }
    }

    private var title: String {
        switch entry.event {
        case .headers: "HEADERS"
        case .message: "MESSAGE"
        case let .status(code, name, _, _): "STATUS \(code) \(name)"
        case .error: "NOT CALLED"
        }
    }

    private var color: Color {
        switch entry.event {
        case .headers: Theme.textFaint
        case .message: Theme.accentText
        case let .status(code, _, _, _): code == 0 ? Theme.success : Theme.danger
        case .error: Theme.danger
        }
    }

    @ViewBuilder
    private func pairList(_ pairs: [[String]], empty: String) -> some View {
        if pairs.isEmpty {
            if !empty.isEmpty { Text(empty).font(Typo.caption).foregroundStyle(Theme.textMuted) }
        } else {
            ForEach(Array(pairs.enumerated()), id: \.offset) { _, p in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(p.first ?? "").font(Typo.mono(12.5, .semibold)).foregroundStyle(Theme.codeKey)
                    Text(p.last ?? "").font(Typo.mono(12.5)).foregroundStyle(Theme.codeText).textSelection(.enabled)
                }
            }
        }
    }
}

// MARK: - GraphQL schema

/// Load the schema (introspection), its summary, Browse; the suggestions bar sits on the query editor.
struct GraphQLSchemaBar: View {
    @Bindable var editor: RequestEditor
    @State private var browsing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                Button {
                    Task { await editor.fetchSchema() }
                } label: {
                    if editor.schemaLoading { ProgressView() } else { Label(editor.schema == nil ? "Load schema" : "Reload", systemImage: "arrow.down.circle") }
                }
                .buttonStyle(.glass)
                .accessibilityIdentifier("graphql-load-schema")
                if let s = editor.schema {
                    Text("\(s.types.count) types").font(Typo.caption).foregroundStyle(Theme.textMuted)
                        .accessibilityIdentifier("graphql-schema-summary")
                    Button("Browse") { browsing = true }
                        .font(Typo.geist(14, .semibold))
                        .frame(minHeight: 44)
                        .accessibilityIdentifier("graphql-browse")
                }
                Spacer()
            }
            if let e = editor.schemaError {
                Label("Schema not loaded: \(e)", systemImage: "exclamationmark.triangle").font(Typo.caption).foregroundStyle(Theme.danger)
                    .accessibilityIdentifier("graphql-schema-error")
            } else if editor.schema == nil {
                Text("The schema comes from the server (introspection, sent like this request); then fields are suggested as you type.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted)
            }
        }
        .sheet(isPresented: $browsing) {
            if let s = editor.schema {
                NavigationStack { SchemaBrowser(schema: s) }
                    .presentationDetents([.large])
            }
        }
    }
}

/// The query editor with the cursor known, and the fields that fit there above the keyboard.
struct GraphQLQueryEditor: View {
    @Bindable var editor: RequestEditor
    @State private var selection: TextSelection?

    private var cursor: Int {
        let text = editor.draft.body
        guard let selection, case let .selection(range) = selection.indices, range.upperBound <= text.endIndex else { return text.count }
        return text.distance(from: text.startIndex, to: range.upperBound)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let schema = editor.schema {
                let items = schema.suggestions(in: editor.draft.body, at: cursor)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        if items.isEmpty {
                            Text("Fields show here inside a { selection }.").font(Typo.caption).foregroundStyle(Theme.textMuted).frame(minHeight: 44)
                        }
                        ForEach(items) { s in
                            Button {
                                let (text, at) = GraphQLSchema.apply(s.name, to: editor.draft.body, at: cursor)
                                editor.draft.body = text
                                selection = TextSelection(insertionPoint: text.index(text.startIndex, offsetBy: min(at, text.count)))
                            } label: {
                                HStack(spacing: 5) {
                                    Text(s.name).font(Typo.mono(13, .semibold)).foregroundStyle(Theme.accentText)
                                    Text(s.detail).font(Typo.mono(11)).foregroundStyle(Theme.textMuted)
                                }
                                .padding(.horizontal, 10).padding(.vertical, 7)
                                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.accent.opacity(0.14)))
                                .frame(minHeight: 44)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Insert \(s.name), \(s.detail)")
                            .accessibilityIdentifier("gql-suggestion-\(s.name)")
                        }
                    }
                }
            }
            ZStack(alignment: .topLeading) {
                if editor.draft.body.isEmpty {
                    Text("query { … }").font(Typo.code).foregroundStyle(Theme.textFaint).padding(.horizontal, 19).padding(.vertical, 22)
                        .accessibilityHidden(true)
                }
                TextEditor(text: $editor.draft.body, selection: $selection)
                    .font(Typo.code)
                    .foregroundStyle(Theme.codeText)
                    .scrollContentBackground(.hidden)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .keyboardType(.asciiCapable)
                    .padding(10)
                    .accessibilityLabel("GraphQL query")
                    .accessibilityIdentifier("graphql-query")
            }
            .frame(minHeight: 160)
            .card(radius: 18, fill: Theme.codeGround)
        }
    }
}

struct SchemaBrowser: View {
    var schema: GraphQLSchema
    @State private var query = ""
    @Environment(\.dismiss) private var dismiss

    private var types: [GQLType] {
        let q = query.lowercased()
        guard !q.isEmpty else { return schema.browsable }
        return schema.browsable.filter { $0.name.lowercased().contains(q) || $0.fields.contains { $0.name.lowercased().contains(q) } }
    }

    var body: some View {
        List {
            ForEach(types) { t in
                NavigationLink {
                    SchemaTypeView(schema: schema, type: t)
                } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(t.name).font(Typo.mono(15, .semibold)).foregroundStyle(Theme.text)
                            if t.name == schema.queryType || t.name == schema.mutationType || t.name == schema.subscriptionType {
                                Text(t.name == schema.queryType ? "query root" : t.name == schema.mutationType ? "mutation root" : "subscription root")
                                    .font(Typo.label).foregroundStyle(Theme.accentText)
                            }
                        }
                        Text("\(t.kind.lowercased().replacingOccurrences(of: "_", with: " ")) · \(t.kind == "ENUM" ? t.enumValues.count : t.fields.count) \(t.kind == "ENUM" ? "values" : "fields")")
                            .font(Typo.caption).foregroundStyle(Theme.textMuted)
                    }
                    .accessibilityElement(children: .combine)
                }
                .listRowBackground(Theme.surface)
                .accessibilityIdentifier("schema-type-\(t.name)")
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.ground)
        .searchable(text: $query, prompt: "Types and fields")
        .navigationTitle("Schema")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
        }
    }
}

struct SchemaTypeView: View {
    var schema: GraphQLSchema
    var type: GQLType

    var body: some View {
        List {
            if let d = type.description { Text(d).font(Typo.caption).foregroundStyle(Theme.textMuted).listRowBackground(Theme.surface) }
            if type.kind == "ENUM" {
                ForEach(type.enumValues, id: \.self) { v in Text(v).font(Typo.mono(14)).listRowBackground(Theme.surface) }
            }
            ForEach(type.fields) { f in
                let target = schema.type(f.type.named)
                Group {
                    if let target, !target.fields.isEmpty || target.kind == "ENUM" {
                        NavigationLink { SchemaTypeView(schema: schema, type: target) } label: { fieldRow(f) }
                    } else {
                        fieldRow(f)
                    }
                }
                .listRowBackground(Theme.surface)
                .accessibilityIdentifier("schema-field-\(f.name)")
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.ground)
        .navigationTitle(type.name)
        .navigationBarTitleDisplayMode(.inline)
    }

    private func fieldRow(_ f: GQLField) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(f.signature).font(Typo.mono(13.5, .medium)).foregroundStyle(f.deprecated ? Theme.textMuted : Theme.text)
            if let d = f.description { Text(d).font(Typo.caption).foregroundStyle(Theme.textMuted) }
        }
        .accessibilityElement(children: .combine)
    }
}
