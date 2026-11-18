import SwiftUI
import UIKit

@main
struct HITOPApp: App {
    @State private var model: AppModel
    @State private var nav = Navigator()

    init() {
        let args = ProcessInfo.processInfo.arguments
        let uiTest = args.contains("-UITestSeed")
        let model: AppModel
        if uiTest {
            // UI tests and screenshots: a fresh library, the shop-api sample, secrets in memory and
            // every request answered in-process by StubURLProtocol (no network).
            StubURLProtocol.enabled = true
            let root = FileManager.default.temporaryDirectory.appendingPathComponent("hitop-uitest-\(UUID().uuidString)")
            model = AppModel(store: FileLibraryStore(root: root), vault: MemoryVault(), sender: Sender(configuration: StubURLProtocol.configuration))
            model.load()
            model.seed()
        } else {
            model = AppModel(store: FileLibraryStore(root: FileLibraryStore.defaultRoot()), vault: KeychainVault())
            model.load(legacyDirectory: LegacyImport.defaultDirectory())
        }
        _model = State(initialValue: model)
        Self.styleNavigationBars()
    }

    /// Geist in the system navigation bars (titles scale with Dynamic Type).
    private static func styleNavigationBars() {
        let metrics = UIFontMetrics(forTextStyle: .largeTitle)
        let large = UIFont(name: "Geist-Bold", size: 34).map { metrics.scaledFont(for: $0) }
        let small = UIFont(name: "Geist-SemiBold", size: 17).map { UIFontMetrics(forTextStyle: .headline).scaledFont(for: $0) }
        let appearance = UINavigationBar.appearance()
        var largeAttrs: [NSAttributedString.Key: Any] = [.foregroundColor: UIColor(Theme.text), .kern: -0.6]
        if let large { largeAttrs[.font] = large }
        var smallAttrs: [NSAttributedString.Key: Any] = [.foregroundColor: UIColor(Theme.text)]
        if let small { smallAttrs[.font] = small }
        appearance.largeTitleTextAttributes = largeAttrs
        appearance.titleTextAttributes = smallAttrs
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .environment(nav)
                .preferredColorScheme(.dark)
                .tint(Theme.accent)
                .font(Typo.body)
        }
        .commands {
            // Menu commands come before the focused view (a focused sidebar list would take ⌘↩ itself).
            CommandMenu("Request") {
                Button("Send") { nav.sendCurrent(model) }
                    .keyboardShortcut(.return, modifiers: .command)
                Button("Save") { nav.saveCurrent(model) }
                    .keyboardShortcut("s", modifiers: .command)
                Divider()
                Button("New Request") { nav.newRequest(model) }
                    .keyboardShortcut("n", modifiers: .command)
                Button("Search") { nav.showSearch() }
                    .keyboardShortcut("k", modifiers: .command)
            }
        }
    }
}
