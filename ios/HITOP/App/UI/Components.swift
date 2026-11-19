import SwiftUI

// MARK: - Glass

extension View {
    /// A card: surface fill and hairline, stronger with Increase Contrast.
    func card(radius: CGFloat = 22, fill: Color = Theme.surface) -> some View {
        modifier(CardModifier(radius: radius, fill: fill))
    }
}

struct CardModifier: ViewModifier {
    var radius: CGFloat
    var fill: Color
    @Environment(\.colorSchemeContrast) private var contrast

    func body(content: Content) -> some View {
        content
            .background(RoundedRectangle(cornerRadius: radius, style: .continuous).fill(fill))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous)
                .stroke(Color.white.opacity(contrast == .increased ? 0.3 : 0.06), lineWidth: 1))
    }
}

/// A round glass button (44 pt), for toolbars the system doesn't draw.
struct GlassCircleButton: View {
    var systemImage: String
    var label: String
    var size: CGFloat = 44
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.system(size: size * 0.4, weight: .semibold))
                .foregroundStyle(Theme.text)
                .frame(width: size, height: size)
        }
        .buttonStyle(.glass)
        .buttonBorderShape(.circle)
        .accessibilityLabel(label)
    }
}

// MARK: - Send

/// The "accent prominent" Send button: glassProminent tinted with the accent, label on accent.
struct SendButton: View {
    var title = "Send"
    var icon = "paperplane.fill"
    var shortcut: String?
    var sending = false
    var height: CGFloat = 62
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                if sending {
                    ProgressView().tint(Theme.onAccent)
                } else {
                    Image(systemName: icon).font(.system(size: 18, weight: .semibold))
                }
                Text(sending ? "Sending" : title).font(Typo.geist(height > 56 ? 18 : 16, .bold, relativeTo: .headline))
                if let shortcut {
                    Text(shortcut).font(Typo.mono(12, .semibold)).opacity(0.7)
                }
            }
            .foregroundStyle(Theme.onAccent)
            .frame(maxWidth: .infinity, minHeight: height - 16)
            .padding(.horizontal, 6)
        }
        .buttonStyle(.glassProminent)
        .buttonBorderShape(.capsule)
        .tint(Theme.accent)
        .controlSize(.large)
        .accessibilityLabel(sending ? "Sending" : title)
        .accessibilityIdentifier("send")
    }
}

// MARK: - Method tag, chips

/// What a request's tag says: the method, or WS / GRPC / GQL for those kinds.
func methodLabel(_ r: SavedRequest) -> String {
    switch r.kind {
    case "websocket": return "WS"
    case "grpc": return "GRPC"
    default: return r.bodyType == "graphql" ? "GQL" : r.mode == .sse ? "SSE" : r.method.uppercased()
    }
}

struct MethodTag: View {
    var method: String
    var width: CGFloat? = 52
    var size: CGFloat = 11.5

    var body: some View {
        Text(method)
            .font(Typo.mono(size, .semibold, relativeTo: .caption))
            .foregroundStyle(Theme.method(method))
            .lineLimit(1)
            .frame(width: width, alignment: .leading)
            .accessibilityLabel(method == "WS" ? "WebSocket" : method == "GQL" ? "GraphQL" : method)
    }
}

/// A `${name}` reference.
struct VariableChip: View {
    var name: String
    var size: CGFloat = 15

    var body: some View {
        Text("${\(name)}")
            .font(Typo.mono(size, .medium))
            .foregroundStyle(Theme.accentText)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(RoundedRectangle(cornerRadius: 6, style: .continuous).fill(Theme.chipFill))
            .accessibilityLabel("variable \(name)")
    }
}

struct FilterChip: View {
    var title: String
    var selected: Bool
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(Typo.geist(13, selected ? .semibold : .medium, relativeTo: .subheadline))
                .foregroundStyle(selected ? Theme.accentText : Theme.textSoft)
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
                .background(Capsule().fill(selected ? Theme.accent.opacity(0.16) : Theme.fill))
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

struct EnvironmentDot: View {
    var name: String?
    var size: CGFloat = 8
    var body: some View {
        Circle().fill(Theme.environment(name)).frame(width: size, height: size).accessibilityHidden(true)
    }
}

/// Segmented tabs drawn as in the drafts (a filled track, the selected tab raised).
struct SegmentTabs<T: Hashable>: View {
    var items: [(T, String)]
    @Binding var selection: T
    var label: String

    var body: some View {
        HStack(spacing: 0) {
            ForEach(items, id: \.0) { item in
                let on = item.0 == selection
                Button { selection = item.0 } label: {
                    Text(item.1)
                        .font(Typo.geist(13, on ? .semibold : .medium, relativeTo: .subheadline))
                        .foregroundStyle(on ? Color.white : Theme.textTab)
                        .lineLimit(1)
                        .minimumScaleFactor(0.7)
                        .frame(maxWidth: .infinity, minHeight: 34)
                        .background(RoundedRectangle(cornerRadius: 11, style: .continuous).fill(on ? Theme.fillStrong : .clear))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .frame(minHeight: 44)
                .accessibilityAddTraits(on ? [.isSelected, .isButton] : .isButton)
                .accessibilityIdentifier("tab-\(item.1)")
            }
        }
        .padding(3)
        .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Theme.fill))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(label)
    }
}

/// Capsule tabs (response sheet).
struct CapsuleTabs<T: Hashable>: View {
    var items: [(T, String, String?)]
    @Binding var selection: T

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(items, id: \.0) { item in
                    let on = item.0 == selection
                    Button { selection = item.0 } label: {
                        HStack(spacing: 4) {
                            Text(item.1)
                            if let extra = item.2 { Text(extra).foregroundStyle(extraColor(extra, on)) }
                        }
                        .font(Typo.geist(13.5, on ? .semibold : .medium, relativeTo: .subheadline))
                        .foregroundStyle(on ? Color.white : Theme.textTab)
                        .padding(.horizontal, 14)
                        .frame(minHeight: 34)
                        .background(Capsule().fill(on ? Theme.fillStrong : .clear))
                        .frame(minHeight: 44)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(on ? .isSelected : [])
                    .accessibilityIdentifier("rtab-\(item.1)")
                }
            }
        }
    }

    private func extraColor(_ s: String, _ on: Bool) -> Color {
        if s.contains("/") {
            let parts = s.split(separator: "/")
            if parts.count == 2, parts[0] == parts[1] { return Theme.success }
            return Theme.danger
        }
        return on ? Theme.textSoft : Theme.textMuted
    }
}

// MARK: - Status, timing

struct StatusPill: View {
    var status: Int
    var size: CGFloat = 15

    var body: some View {
        let text = HTTPStatusText.text(status)
        Text(text.isEmpty ? "\(status)" : "\(status) \(text)")
            .font(Typo.mono(size, .bold, relativeTo: .headline))
            .lineLimit(1)
            .fixedSize()
            .foregroundStyle(Theme.status(status))
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Theme.status(status).opacity(0.14)))
            .accessibilityLabel("Status \(status) \(text)")
            .accessibilityIdentifier("status")
    }
}

struct TimingBar: View {
    var timing: TimingPhases
    var showLabels = true
    var height: CGFloat = 8

    private var parts: [(String, Double, Color)] {
        let connect: Double = timing.connect ?? 0
        let secure: Double = (timing.tls ?? 0) + connect
        let raw: [(String, Double?, Color)] = [
            ("DNS", timing.dns, Theme.info),
            ("TLS", secure == 0 ? nil : secure, Theme.violet),
            ("Waiting", timing.waiting, Theme.warning),
            ("Download", timing.download, Theme.success),
        ]
        return raw.compactMap { p in p.1.flatMap { $0 > 0 ? (p.0, $0, p.2) : nil } }
    }

    var body: some View {
        let segments = parts
        let total: Double = max(segments.map(\.1).reduce(0, +), 1)
        VStack(spacing: 6) {
            GeometryReader { geo in
                let usable: CGFloat = geo.size.width - CGFloat(segments.count - 1) * 2
                HStack(spacing: 2) {
                    ForEach(segments, id: \.0) { p in
                        let w: CGFloat = max(2, usable * CGFloat(p.1 / total))
                        Rectangle().fill(p.2).frame(width: w)
                    }
                }
            }
            .frame(height: height)
            .clipShape(RoundedRectangle(cornerRadius: height / 2))
            if showLabels {
                HStack {
                    ForEach(Array(parts.enumerated()), id: \.offset) { i, p in
                        if i > 0 { Spacer(minLength: 4) }
                        Text("\(p.0) \(Int(p.1.rounded()))\(i == parts.count - 1 ? " ms" : "")")
                    }
                }
                .font(Typo.geist(11.5, relativeTo: .caption2))
                .foregroundStyle(Theme.textMuted)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Timing: " + parts.map { "\($0.0) \(Int($0.1.rounded())) milliseconds" }.joined(separator: ", "))
    }
}

// MARK: - Code

/// Highlighted text for a body: JSON tokens coloured, `${name}` as chips; tolerant of text that
/// isn't JSON.
enum Highlight {
    static func attributed(_ text: String, json: Bool, size: CGFloat = 13.5) -> AttributedString {
        var out = AttributedString()
        let font = Typo.mono(size)
        func add(_ s: Substring, _ color: Color, chip: Bool = false) {
            var a = AttributedString(String(s))
            a.font = font
            a.foregroundColor = color
            if chip { a.backgroundColor = Theme.chipFill }
            out += a
        }
        let chars = Array(text)
        var i = 0
        var plain = ""
        func flush() {
            if !plain.isEmpty { add(Substring(plain), Theme.codeText); plain = "" }
        }
        while i < chars.count {
            let c = chars[i]
            if c == "$", i + 1 < chars.count, chars[i + 1] == "{", let end = chars[(i + 2)...].firstIndex(of: "}") {
                flush()
                add(Substring(String(chars[i...end])), Theme.accentText, chip: true)
                i = end + 1
            } else if json, c == "\"" {
                flush()
                var j = i + 1
                var segment = "\""
                var pieces: [(String, Bool)] = []
                while j < chars.count, chars[j] != "\"" {
                    if chars[j] == "\\", j + 1 < chars.count {
                        segment.append(chars[j]); segment.append(chars[j + 1]); j += 2; continue
                    }
                    if chars[j] == "$", j + 1 < chars.count, chars[j + 1] == "{", let end = chars[(j + 2)...].firstIndex(of: "}"), end < (chars[(j + 1)...].firstIndex(of: "\"") ?? chars.count) {
                        pieces.append((segment, false)); segment = ""
                        pieces.append((String(chars[j...end]), true))
                        j = end + 1; continue
                    }
                    segment.append(chars[j]); j += 1
                }
                if j < chars.count { segment.append("\""); j += 1 }
                pieces.append((segment, false))
                var k = j
                while k < chars.count, chars[k] == " " { k += 1 }
                let isKey = k < chars.count && chars[k] == ":"
                for (p, chip) in pieces where !p.isEmpty {
                    add(Substring(p), chip ? Theme.accentText : (isKey ? Theme.codeKey : Theme.codeString), chip: chip)
                }
                i = j
            } else if json, c == "-" || c.isNumber, plain.last.map({ !$0.isLetter }) ?? true {
                flush()
                var j = i
                while j < chars.count, chars[j].isNumber || ".eE+-".contains(chars[j]) { j += 1 }
                add(Substring(String(chars[i..<j])), Theme.codeNumber)
                i = j
            } else if json, let lit = ["true", "false", "null"].first(where: { String(chars[i..<min(chars.count, i + $0.count)]) == $0 }) {
                flush()
                add(Substring(lit), Theme.codeLiteral)
                i += lit.count
            } else {
                plain.append(c)
                i += 1
            }
        }
        flush()
        return out
    }
}

/// A read-only code block (request body preview, raw response).
struct CodeBlock: View {
    var text: String
    var json: Bool
    var size: CGFloat = 13.5
    var fill: Color = Theme.codeGround
    var placeholder: String = ""

    var body: some View {
        Group {
            if text.isEmpty {
                Text(placeholder).font(Typo.mono(size)).foregroundStyle(Theme.textFaint)
            } else {
                Text(Highlight.attributed(text, json: json, size: size))
                    .lineSpacing(size * 0.45)
                    .textSelection(.enabled)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .card(radius: 18, fill: fill)
    }
}

/// A multi-line code field: Geist Mono, no autocorrection.
struct CodeEditor: View {
    @Binding var text: String
    var placeholder: String
    var minHeight: CGFloat = 160
    var identifier: String

    var body: some View {
        ZStack(alignment: .topLeading) {
            if text.isEmpty {
                Text(placeholder).font(Typo.code).foregroundStyle(Theme.textFaint).padding(.horizontal, 19).padding(.vertical, 22)
                    .accessibilityHidden(true)
            }
            TextEditor(text: $text)
                .font(Typo.code)
                .foregroundStyle(Theme.codeText)
                .scrollContentBackground(.hidden)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .keyboardType(.asciiCapable)
                .padding(10)
                .accessibilityLabel(placeholder)
                .accessibilityIdentifier(identifier)
        }
        .frame(minHeight: minHeight)
        .card(radius: 18, fill: Theme.codeGround)
    }
}

/// Key/value rows with an on switch, for params, headers, form fields and variables.
struct KeyValueRow: View {
    @Binding var key: String
    @Binding var value: String
    @Binding var enabled: Bool
    var keyPlaceholder = "Name"
    var valuePlaceholder = "Value"
    var secure = false
    var onDelete: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Toggle("", isOn: $enabled).labelsHidden().tint(Theme.accent)
                .accessibilityLabel(enabled ? "On" : "Off")
            VStack(spacing: 6) {
                TextField(keyPlaceholder, text: $key)
                    .font(Typo.mono(14, .medium))
                    .foregroundStyle(enabled ? Theme.text : Theme.textMuted)
                Divider().overlay(Theme.separator)
                Group {
                    if secure { SecureField(valuePlaceholder, text: $value) } else { TextField(valuePlaceholder, text: $value) }
                }
                .font(Typo.mono(14))
                .foregroundStyle(enabled ? Theme.codeText : Theme.textMuted)
            }
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
            Button(role: .destructive, action: onDelete) {
                Image(systemName: "minus.circle.fill").foregroundStyle(Theme.danger).frame(width: 44, height: 44)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove \(key.isEmpty ? "row" : key)")
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
    }
}

/// A small text button in a fill (Format, add row...).
struct SmallChipButton: View {
    var title: String
    var mono = false
    var tint: Color = Theme.textSoft
    var fill: Color = Theme.fill
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(mono ? Typo.mono(13, .medium) : Typo.geist(13, .medium))
                .foregroundStyle(tint)
                .padding(.horizontal, 11)
                .padding(.vertical, 7)
                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(fill))
                .frame(minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// The gradient square of a collection.
struct CollectionIcon: View {
    var name: String
    var size: CGFloat = 34

    var body: some View {
        let first = name == AutoName.myRequests
        ZStack {
            if name == "shop-api" || (!first && derivedID(name) % 3 == 0) {
                RoundedRectangle(cornerRadius: size * 0.3, style: .continuous).fill(Theme.logoGradient)
                Image(systemName: "folder").font(.system(size: size * 0.48, weight: .semibold)).foregroundStyle(.white)
            } else {
                RoundedRectangle(cornerRadius: size * 0.3, style: .continuous).fill(Theme.surfaceRaised)
                Image(systemName: first ? "clock" : "waveform.path.ecg")
                    .font(.system(size: size * 0.48, weight: .semibold))
                    .foregroundStyle(first ? Theme.violet : Theme.info)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
