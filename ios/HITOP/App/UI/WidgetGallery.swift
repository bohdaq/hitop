import SwiftUI

/// The widgets, the Live Activity and the compact Dynamic Island drawn by the app with the same
/// views the extension uses (launch argument `-UITestWidgetGallery`, for the CI screenshots:
/// XCUITest can't see the extension). Its data is the real snapshot after a run of the seeded
/// shop-api; the Live Activity shows that run part way and at its end.
struct WidgetGallery: View {
    @Environment(AppModel.self) private var model
    @Environment(RunCenter.self) private var runs
    @State private var snapshot: WidgetSnapshot?

    var body: some View {
        ScrollView {
            if let snapshot {
                VStack(alignment: .leading, spacing: 14) {
                    label("HOME SCREEN WIDGETS")
                    HStack(spacing: 14) {
                        frame(LastRunWidgetView(snapshot: snapshot, now: Date()), width: 158, height: 158).accessibilityIdentifier("widget-last-run")
                        frame(FailingWidgetView(snapshot: snapshot), width: 158, height: 158, failing: !snapshot.failing.isEmpty).accessibilityIdentifier("widget-failing")
                    }
                    frame(PinnedRequestsWidgetView(snapshot: snapshot), width: 330, height: 158).accessibilityIdentifier("widget-pinned")
                    label("LOCK SCREEN · LIVE ACTIVITY")
                    if let run = snapshot.lastRun {
                        let attributes = RunActivityAttributes(collectionID: run.collectionID, collectionName: run.collectionName, environment: run.environment)
                        let part = RunActivityAttributes.ContentState(done: 8, total: run.requestsTotal, passed: 8, failed: 0, current: "Get order", elapsedMs: 1400, finished: false)
                        let end = RunActivityAttributes.ContentState(done: run.requestsTotal, total: run.requestsTotal, passed: run.requestsPassed,
                                                                     failed: run.requestsFailed, current: nil, elapsedMs: run.durationMs, finished: true)
                        frame(RunActivityView(attributes: attributes, state: part), width: 330, height: nil, padding: 0).accessibilityIdentifier("live-activity")
                        frame(RunActivityView(attributes: attributes, state: end), width: 330, height: nil, padding: 0)
                        HStack {
                            Spacer()
                            VStack(spacing: 6) {
                                RunIslandCompactPreview(state: part)
                                Text("Dynamic Island, compact").font(Typo.caption).foregroundStyle(Theme.textMuted)
                            }
                            Spacer()
                        }
                    }
                }
                .padding(20)
            } else {
                ProgressView().padding(60)
            }
        }
        .background(LinearGradient(colors: [Color(hex: 0x2A2350), Color(hex: 0x12101F), Color(hex: 0x07070A)], startPoint: .topLeading, endPoint: .bottom).ignoresSafeArea())
        .task {
            if runs.runs.isEmpty, let shop = model.collections.first(where: { $0.name == "shop-api" }) {
                await runs.start(collectionID: shop.id, settings: RunSettings(environmentID: "env-staging"))
            }
            snapshot = WidgetSnapshot.make(model: model, runs: runs.runs)
        }
    }

    private func label(_ text: String) -> some View {
        Text(text).font(Typo.geist(12, .semibold)).tracking(0.9).foregroundStyle(Color(hex: 0xB9B3E0))
    }

    private func frame(_ content: some View, width: CGFloat, height: CGFloat?, padding: CGFloat = 14, failing: Bool = false) -> some View {
        content
            .padding(padding)
            .frame(width: width, height: height)
            .background(RoundedRectangle(cornerRadius: 26, style: .continuous).fill(Color(hex: 0x14141C, opacity: 0.7)))
            .glassEffect(.regular.tint(Color(hex: 0x14141C, opacity: 0.4)), in: RoundedRectangle(cornerRadius: 26, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 26, style: .continuous).stroke(failing ? Theme.danger.opacity(0.35) : Color.white.opacity(0.16)))
    }
}
