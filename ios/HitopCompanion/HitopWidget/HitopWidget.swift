import HitopKit
import SwiftUI
import WidgetKit

/// The last run of each pinned collection (pin one with a swipe in the app),
/// read from the app group. The widget never runs anything itself.
struct StatusEntry: TimelineEntry {
    let date: Date
    let statuses: [(name: String, status: RunStatus?)]
}

struct StatusProvider: TimelineProvider {
    func placeholder(in context: Context) -> StatusEntry {
        StatusEntry(date: Date(), statuses: [("Shop API", nil)])
    }

    func getSnapshot(in context: Context, completion: @escaping (StatusEntry) -> Void) {
        completion(entry())
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<StatusEntry>) -> Void) {
        // The app and the Shortcuts intent reload the widget after each run.
        completion(Timeline(entries: [entry()], policy: .after(Date().addingTimeInterval(3600))))
    }

    func entry() -> StatusEntry {
        let store = AppGroup.statusStore
        let pinned = FolderLibrary.all().filter(\.pinned)
        return StatusEntry(date: Date(), statuses: pinned.map { ($0.name, store.status($0.id)) })
    }
}

struct StatusWidgetView: View {
    let entry: StatusEntry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        let rows = entry.statuses.prefix(family == .systemSmall ? 2 : 4)
        VStack(alignment: .leading, spacing: 6) {
            Text("hitop").font(.caption.bold()).foregroundStyle(.secondary)
            if rows.isEmpty {
                Text("Pin a collection in the app to see its last run here.").font(.caption)
            }
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                HStack {
                    Image(systemName: row.status.map { $0.passed ? "checkmark.circle.fill" : "xmark.circle.fill" } ?? "circle.dashed")
                        .foregroundStyle(row.status.map { $0.passed ? Color.green : Color.red } ?? .secondary)
                    VStack(alignment: .leading) {
                        Text(row.name).font(.caption.bold()).lineLimit(1)
                        Text(row.status.map { "\($0.summary) · \($0.finishedAt.formatted(.relative(presentation: .named)))" } ?? "not run yet")
                            .font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
            Spacer(minLength: 0)
        }
        .containerBackground(.fill.tertiary, for: .widget)
    }
}

@main
struct HitopStatusWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "HitopStatus", provider: StatusProvider()) { entry in
            StatusWidgetView(entry: entry)
        }
        .configurationDisplayName("hitop runs")
        .description("The last run of your pinned hitop collections.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}
