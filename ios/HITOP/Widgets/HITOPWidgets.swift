import ActivityKit
import AppIntents
import SwiftUI
import WidgetKit

// The HITOP widget extension (hitop.rws8.tech.HiTop.Widgets): home-screen widgets, the collection
// run Live Activity with its Dynamic Island, and a Control Center control. Everything it shows
// comes from the snapshot the app writes into the app group (`SharedContainer`); it sends and runs
// nothing itself (the intents its buttons start run in the app).

@main
struct HITOPWidgets: WidgetBundle {
    var body: some Widget {
        LastRunWidget()
        FailingWidget()
        PinnedRequestsWidget()
        RunLiveActivity()
        RunControl()
    }
}

struct SnapshotEntry: TimelineEntry {
    var date: Date
    var snapshot: WidgetSnapshot
}

struct SnapshotProvider: TimelineProvider {
    func placeholder(in context: Context) -> SnapshotEntry { SnapshotEntry(date: Date(), snapshot: .sample) }

    func getSnapshot(in context: Context, completion: @escaping (SnapshotEntry) -> Void) {
        let s = SharedContainer.current()
        completion(SnapshotEntry(date: Date(), snapshot: context.isPreview && s.collections.isEmpty ? .sample : s))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<SnapshotEntry>) -> Void) {
        // The app reloads the widgets after every run and send; the hourly entry keeps "min ago" fresh.
        let now = Date()
        let s = SharedContainer.current()
        let entries = (0..<4).map { SnapshotEntry(date: now.addingTimeInterval(Double($0) * 900), snapshot: s) }
        completion(Timeline(entries: entries, policy: .after(now.addingTimeInterval(3600))))
    }
}

/// The widgets' glass: the system's container with a graphite tint.
private struct WidgetGround: View {
    var body: some View {
        LinearGradient(colors: [WidgetStyle.color(0x1B1A28), WidgetStyle.color(0x0E0E14)], startPoint: .top, endPoint: .bottom)
    }
}

struct LastRunWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "hitop.last-run", provider: SnapshotProvider()) { entry in
            LastRunWidgetView(snapshot: entry.snapshot, now: entry.date)
                .containerBackground(for: .widget) { WidgetGround() }
                .widgetURL(entry.snapshot.lastRun.flatMap { URL(string: "hitop://run/\($0.collectionID)") })
        }
        .configurationDisplayName("Last run")
        .description("How your last collection run went.")
        .supportedFamilies([.systemSmall])
    }
}

struct FailingWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "hitop.failing", provider: SnapshotProvider()) { entry in
            FailingWidgetView(snapshot: entry.snapshot)
                .containerBackground(for: .widget) { WidgetGround() }
                .widgetURL(URL(string: "hitop://runs"))
        }
        .configurationDisplayName("Failing collections")
        .description("Collections whose last run failed.")
        .supportedFamilies([.systemSmall])
    }
}

struct PinnedRequestsWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "hitop.pinned", provider: SnapshotProvider()) { entry in
            PinnedRequestsWidgetView(snapshot: entry.snapshot)
                .containerBackground(for: .widget) { WidgetGround() }
        }
        .configurationDisplayName("Pinned requests")
        .description("Send your pinned requests from the Home Screen.")
        .supportedFamilies([.systemMedium])
    }
}

struct RunLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: RunActivityAttributes.self) { context in
            RunActivityView(attributes: context.attributes, state: context.state)
                .activityBackgroundTint(WidgetStyle.color(0x14141C).opacity(0.7))
                .activitySystemActionForegroundColor(WidgetStyle.text)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 8) {
                        WidgetLogo(size: 26)
                        VStack(alignment: .leading, spacing: 0) {
                            Text(context.attributes.collectionName).font(WidgetStyle.geist(14, "SemiBold")).lineLimit(1)
                            Text(context.attributes.environment ?? "No environment").font(WidgetStyle.geist(11.5)).foregroundStyle(WidgetStyle.tab)
                        }
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text("\(context.state.finished ? context.state.passed : context.state.done)/\(context.state.total)").font(WidgetStyle.mono(17))
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(spacing: 6) {
                        ProgressBar(fraction: context.state.fraction, failed: context.state.failed > 0 && context.state.finished)
                        HStack {
                            Text("\(context.state.passed) passed").foregroundStyle(WidgetStyle.success)
                            Spacer()
                            Text("\(context.state.failed) failed").foregroundStyle(context.state.failed > 0 ? WidgetStyle.danger : WidgetStyle.soft)
                            Spacer()
                            Text(context.state.elapsed).foregroundStyle(WidgetStyle.soft)
                        }
                        .font(WidgetStyle.geist(12))
                    }
                }
            } compactLeading: {
                RunIslandCompactLeading(state: context.state)
            } compactTrailing: {
                RunIslandCompactTrailing(state: context.state)
            } minimal: {
                Text("\(context.state.done)").font(WidgetStyle.mono(12))
                    .foregroundStyle(context.state.failed > 0 ? WidgetStyle.danger : WidgetStyle.success)
            }
            .widgetURL(URL(string: "hitop://run/\(context.attributes.collectionID)"))
        }
    }
}

/// Control Center (and the Action button): run the last collection again.
struct RunControl: ControlWidget {
    var body: some ControlWidgetConfiguration {
        StaticControlConfiguration(kind: "hitop.run-last") {
            ControlWidgetButton(action: RunLastCollectionIntent()) {
                Label("Run collection", systemImage: "play.circle")
            }
        }
        .displayName("Run last collection")
        .description("Runs the HITOP collection you ran last.")
    }
}
