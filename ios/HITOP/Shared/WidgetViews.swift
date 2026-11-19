import AppIntents
import SwiftUI

// Compiled into the app and the widget extension (project.yml): the home-screen widgets' and the
// Live Activity's views. The extension puts them in its widgets; the app shows them in the widget
// gallery (`-UITestWidgetGallery`, screenshots: XCUITest can't see the extension). HITOP dark:
// the design canvas "Widgets" page. Backgrounds come from the container (the system's glass in a
// widget, `WidgetFrame` in the gallery).

enum WidgetStyle {
    static func color(_ hex: UInt32) -> Color {
        Color(.sRGB, red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255, opacity: 1)
    }

    static let text = color(0xEDEDF2)
    static let soft = color(0xC7C7D1)
    static let tab = color(0xB4B4C0)
    static let muted = color(0x9A9AA8)
    static let faint = color(0x8D8D9B)
    static let success = color(0x5EE6A0)
    static let danger = color(0xFF8A8A)
    static let dangerSoft = color(0xE8C2C2)
    static let info = color(0x7CC7FF)
    static let accentText = color(0xC9C0FF)
    static let ground = color(0x14141C)
    static var logo: LinearGradient { LinearGradient(colors: [color(0x667EEA), color(0x764BA2)], startPoint: .topLeading, endPoint: .bottomTrailing) }
    static var progress: LinearGradient { LinearGradient(colors: [success, info], startPoint: .leading, endPoint: .trailing) }

    static func method(_ m: String) -> Color {
        switch m.uppercased() {
        case "GET": color(0x5EC79B)
        case "POST": color(0xF2B45C)
        case "PUT": color(0x6CA8FF)
        case "PATCH": color(0xC69CFF)
        case "DELETE": color(0xFF8A8A)
        case "WS": color(0x7CC7FF)
        case "GQL": color(0xFF8AD8)
        default: color(0xA99CFF)
        }
    }

    static func status(_ code: Int?) -> Color {
        guard let code else { return muted }
        return code < 400 ? success : danger
    }

    static func geist(_ size: CGFloat, _ weight: String = "Regular", relativeTo style: Font.TextStyle = .body) -> Font {
        .custom("Geist-\(weight)", size: size, relativeTo: style)
    }

    static func mono(_ size: CGFloat, _ weight: String = "SemiBold", relativeTo style: Font.TextStyle = .body) -> Font {
        .custom("GeistMono-\(weight)", size: size, relativeTo: style)
    }

    static func ago(_ date: Date, now: Date) -> String {
        let s = max(0, now.timeIntervalSince(date))
        if s < 60 { return "just now" }
        if s < 3600 { return "\(Int(s / 60)) min ago" }
        if s < 86400 { return "\(Int(s / 3600)) h ago" }
        return "\(Int(s / 86400)) d ago"
    }
}

/// The HITOP mark: the logo gradient with an H.
struct WidgetLogo: View {
    var size: CGFloat = 30

    var body: some View {
        Text("H").font(WidgetStyle.geist(size * 0.42, "Bold")).foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(RoundedRectangle(cornerRadius: size * 0.3, style: .continuous).fill(WidgetStyle.logo))
            .accessibilityHidden(true)
    }
}

/// Small: the last run of any collection.
struct LastRunWidgetView: View {
    var snapshot: WidgetSnapshot
    var now: Date

    var body: some View {
        if let run = snapshot.lastRun {
            VStack(alignment: .leading, spacing: 0) {
                Text(run.collectionName).font(WidgetStyle.geist(12, relativeTo: .caption)).foregroundStyle(WidgetStyle.tab).lineLimit(1)
                Spacer(minLength: 4)
                (Text("\(run.requestsPassed)").foregroundStyle(run.passed ? WidgetStyle.success : WidgetStyle.danger)
                    + Text("/\(run.requestsTotal)").font(WidgetStyle.geist(18, "Bold")).foregroundStyle(WidgetStyle.faint))
                    .font(WidgetStyle.geist(34, "Bold", relativeTo: .largeTitle))
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                Spacer(minLength: 4)
                Text([WidgetStyle.ago(run.finishedAt, now: now), run.environment].compactMap { $0 }.joined(separator: " · "))
                    .font(WidgetStyle.geist(11.5, relativeTo: .caption2)).foregroundStyle(WidgetStyle.muted).lineLimit(1)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("\(run.collectionName): \(run.summary), \(WidgetStyle.ago(run.finishedAt, now: now))")
        } else {
            VStack(alignment: .leading, spacing: 8) {
                WidgetLogo(size: 26)
                Spacer(minLength: 0)
                Text("No runs yet").font(WidgetStyle.geist(14, "SemiBold")).foregroundStyle(WidgetStyle.text)
                Text("Run a collection in HITOP to see it here.").font(WidgetStyle.geist(11.5)).foregroundStyle(WidgetStyle.muted)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        }
    }
}

/// Small: collections whose last run failed.
struct FailingWidgetView: View {
    var snapshot: WidgetSnapshot

    var body: some View {
        let failing = snapshot.failing
        VStack(alignment: .leading, spacing: 0) {
            if let first = failing.first, let run = first.lastRun {
                Text(failing.count == 1 ? first.name : "\(failing.count) collections").font(WidgetStyle.geist(12, relativeTo: .caption))
                    .foregroundStyle(WidgetStyle.tab).lineLimit(1)
                Spacer(minLength: 4)
                (Text("\(failing.count == 1 ? run.requestsFailed : failing.reduce(0) { $0 + ($1.lastRun?.requestsFailed ?? 0) })")
                    + Text(" failing").font(WidgetStyle.geist(15, "Bold")).foregroundStyle(WidgetStyle.dangerSoft))
                    .font(WidgetStyle.geist(34, "Bold", relativeTo: .largeTitle)).foregroundStyle(WidgetStyle.danger)
                    .minimumScaleFactor(0.6).lineLimit(1)
                Spacer(minLength: 4)
                Text(run.firstFailure ?? "Tap to see why").font(WidgetStyle.geist(11.5, relativeTo: .caption2)).foregroundStyle(WidgetStyle.muted).lineLimit(2)
            } else {
                Text("Failing").font(WidgetStyle.geist(12)).foregroundStyle(WidgetStyle.tab)
                Spacer(minLength: 4)
                Image(systemName: "checkmark.circle.fill").font(.system(size: 30)).foregroundStyle(WidgetStyle.success)
                Spacer(minLength: 4)
                Text(snapshot.collections.contains { $0.lastRun != nil } ? "Every last run passed" : "No runs yet")
                    .font(WidgetStyle.geist(11.5)).foregroundStyle(WidgetStyle.muted)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

/// Medium: pinned requests, each with a Send button that sends it in place (an App Intent run by
/// the app) and its last status.
struct PinnedRequestsWidgetView: View {
    var snapshot: WidgetSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Pinned requests")
                Spacer()
                Text(snapshot.activeEnvironment?.name ?? "No environment")
            }
            .font(WidgetStyle.geist(12, relativeTo: .caption)).foregroundStyle(WidgetStyle.tab)
            if snapshot.pinned.isEmpty {
                Spacer(minLength: 0)
                Text("Pin a request in HITOP (swipe right on it) to send it from here.")
                    .font(WidgetStyle.geist(13)).foregroundStyle(WidgetStyle.muted)
                Spacer(minLength: 0)
            }
            ForEach(snapshot.pinned.prefix(3)) { r in
                HStack(spacing: 10) {
                    Text(r.method).font(WidgetStyle.mono(11)).foregroundStyle(WidgetStyle.method(r.method)).frame(width: 44, alignment: .leading)
                    Text(r.name).font(WidgetStyle.geist(13.5)).foregroundStyle(WidgetStyle.text).lineLimit(1)
                    Spacer(minLength: 4)
                    if let s = r.lastStatus {
                        Text("\(s)").font(WidgetStyle.mono(11.5)).foregroundStyle(WidgetStyle.status(s))
                    } else if r.lastFailed {
                        Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 11)).foregroundStyle(WidgetStyle.danger)
                    }
                    Button(intent: SendRequestIntent(request: RequestEntity(r))) {
                        Image(systemName: "play.fill").font(.system(size: 12)).foregroundStyle(WidgetStyle.accentText)
                            .frame(width: 28, height: 28)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Send \(r.name)")
                }
                .padding(.horizontal, 10)
                .frame(height: 34)
                .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.06)))
            }
            Spacer(minLength: 0)
        }
    }
}

// MARK: - Live Activity

/// The Lock Screen / banner presentation: name, environment, n/total, passed/failed, progress.
struct RunActivityView: View {
    var attributes: RunActivityAttributes
    var state: RunActivityAttributes.ContentState

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                WidgetLogo(size: 30)
                VStack(alignment: .leading, spacing: 1) {
                    Text(state.finished ? attributes.collectionName : "Running \(attributes.collectionName)")
                        .font(WidgetStyle.geist(15, "SemiBold")).foregroundStyle(WidgetStyle.text).lineLimit(1).minimumScaleFactor(0.8)
                    // I5 audit: shrinks a little rather than clip ("Staging · 1 failed" next to the count).
                    Text(subtitle).font(WidgetStyle.geist(12)).foregroundStyle(WidgetStyle.tab).lineLimit(1).minimumScaleFactor(0.75)
                }
                Spacer(minLength: 6)
                (Text("\(state.finished ? state.passed : state.done)") + Text("/\(state.total)").foregroundStyle(WidgetStyle.faint))
                    .font(WidgetStyle.mono(18)).foregroundStyle(WidgetStyle.text).fixedSize()
            }
            ProgressBar(fraction: state.fraction, failed: state.failed > 0 && state.finished)
            HStack {
                Text("\(state.passed) passed").foregroundStyle(WidgetStyle.success)
                Spacer()
                Text("\(state.failed) failed").foregroundStyle(state.failed > 0 ? WidgetStyle.danger : WidgetStyle.soft)
                Spacer()
                Text(state.elapsed).foregroundStyle(WidgetStyle.soft)
            }
            .font(WidgetStyle.geist(12.5))
        }
        .padding(16)
        .accessibilityElement(children: .combine)
    }

    private var subtitle: String {
        let env = attributes.environment ?? "No environment"
        if state.finished { return state.failed == 0 ? "\(env) · all passed" : "\(env) · \(state.failed) failed" }
        return [env, state.current].compactMap { $0 }.joined(separator: " · ")
    }
}

struct ProgressBar: View {
    var fraction: Double
    var failed = false
    var height: CGFloat = 8

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.white.opacity(0.1))
                Capsule().fill(failed ? AnyShapeStyle(WidgetStyle.danger) : AnyShapeStyle(WidgetStyle.progress))
                    .frame(width: max(height, g.size.width * fraction))
            }
        }
        .frame(height: height)
        .accessibilityHidden(true)
    }
}

/// Dynamic Island, compact: the count on the leading side, a bar on the trailing one.
struct RunIslandCompactLeading: View {
    var state: RunActivityAttributes.ContentState

    var body: some View {
        HStack(spacing: 6) {
            WidgetLogo(size: 22)
            Text("\(state.finished ? state.passed : state.done)/\(state.total)").font(WidgetStyle.mono(13))
                .foregroundStyle(state.failed > 0 ? WidgetStyle.danger : WidgetStyle.success)
        }
    }
}

struct RunIslandCompactTrailing: View {
    var state: RunActivityAttributes.ContentState

    var body: some View {
        ProgressBar(fraction: state.fraction, failed: state.failed > 0 && state.finished, height: 4).frame(width: 44)
    }
}

/// The gallery's picture of the compact Dynamic Island (black capsule, as the drafts draw it).
struct RunIslandCompactPreview: View {
    var state: RunActivityAttributes.ContentState

    var body: some View {
        HStack(spacing: 10) {
            RunIslandCompactLeading(state: state)
            Spacer(minLength: 30)
            RunIslandCompactTrailing(state: state)
        }
        .padding(.horizontal, 10)
        .frame(width: 236, height: 38)
        .background(Capsule().fill(Color.black))
    }
}
