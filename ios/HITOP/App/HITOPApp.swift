import HitopKit
import SwiftUI
import UIKit

@main
struct HITOPApp: App {
    @State private var model: AppModel
    @State private var nav: Navigator
    @State private var cloud: HitopCloud
    @State private var sync: LibrarySync
    @State private var cloudUI: CloudUI
    @State private var shared: SharedSync
    @State private var runs: RunCenter
    @State private var ai: Intelligence
    private let system: SystemBridge
    /// UI tests of shared collections: the scenario prepared before the first sync.
    private let scenario: String?

    init() {
        let args = ProcessInfo.processInfo.arguments
        let nav = Navigator()
        _nav = State(initialValue: nav)
        let uiTest = args.contains("-UITestSeed")
        let model: AppModel
        let cloud: HitopCloud
        let web: WebSignIn
        let syncState: LibrarySyncStateStore
        let sharedStore: SharedSyncStore
        let runStore: RunStore
        if uiTest {
            // UI tests and screenshots: a fresh library, the shop-api sample, secrets in memory and
            // every request answered in-process by StubURLProtocol (no network), HITOP Cloud
            // included (FakeCloud at cloud.hitop.test, signed out, the provider page approving at once).
            StubURLProtocol.enabled = true
            let root = FileManager.default.temporaryDirectory.appendingPathComponent("hitop-uitest-\(UUID().uuidString)")
            model = AppModel(store: FileLibraryStore(root: root), vault: MemoryVault(), sender: Sender(configuration: StubURLProtocol.configuration),
                             webSockets: EchoWebSocketTransport(), grpcClient: FakeGrpcClient())
            model.load()
            model.seed()
            let fake = FakeCloud.shared
            if args.contains("-UITestCloudDeviceLimit") {
                fake.deviceLimit = 2
                fake.addDevice(fake.signInEmail, name: "Old iPad")
                fake.addDevice(fake.signInEmail, name: "Work laptop")
            }
            fake.conflictOnFirstUpdate = args.contains("-UITestCloudConflict")
            cloud = HitopCloud(transport: URLSessionCloudTransport(configuration: StubURLProtocol.configuration()),
                               credentials: CloudCredentials(store: MemorySecretStore()),
                               settingsStore: MemoryCloudSettingsStore(CloudSettings(server: FakeCloud.server)),
                               keyMaker: X25519KeyMaker(), deviceName: DeviceInfo.name)
            web = StubWebSignIn()
            syncState = MemoryLibrarySyncStateStore()
            sharedStore = MemorySharedSyncStore()
            runStore = FileRunStore(root: root)
        } else {
            let root = FileLibraryStore.defaultRoot()
            model = AppModel(store: FileLibraryStore(root: root), vault: KeychainVault())
            model.load(legacyDirectory: LegacyImport.defaultDirectory())
            cloud = HitopCloud(transport: URLSessionCloudTransport(), credentials: CloudCredentials(store: platformSecretStore()),
                               settingsStore: FileCloudSettingsStore(root: root), keyMaker: X25519KeyMaker(), deviceName: DeviceInfo.name)
            web = SystemWebSignIn()
            syncState = FileLibrarySyncStateStore(root: root)
            sharedStore = FileSharedSyncStore(root: root)
            runStore = FileRunStore(root: root)
        }
        // I5: Apple Intelligence on the on-device model; UI tests use the deterministic fake (CI
        // simulators have no model), `-UITestAIUnavailable` the path of a device without it.
        let assistant: LanguageAssistant = uiTest
            ? FakeAssistant(availability: args.contains("-UITestAIUnavailable") ? .notEligible : .available)
            : AppleIntelligenceAssistant()
        _ai = State(initialValue: Intelligence(assistant: assistant))
        scenario = uiTest ? ["-UITestSharedInvite", "-UITestSharedJoined"].first { args.contains($0) } : nil
        _model = State(initialValue: model)
        _cloud = State(initialValue: cloud)
        _sync = State(initialValue: LibrarySync(cloud: cloud, model: model, stateStore: syncState, deviceLabel: DeviceInfo.label))
        _cloudUI = State(initialValue: CloudUI(web: web))
        _shared = State(initialValue: SharedSync(cloud: cloud, model: model, store: sharedStore))
        let runs = RunCenter(model: model, store: runStore)
        _runs = State(initialValue: runs)
        system = SystemBridge(model: model, runs: runs)
        // UI tests: no Live Activity over the screenshots, no snapshot for the real widgets.
        system.liveActivities = !uiTest
        IntentBridge.services = IntentBridge.Services(model: model, runs: runs, system: system, nav: nav)
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
            Group {
                if let scenario {
                    ScenarioGate(scenario: scenario)
                } else if ProcessInfo.processInfo.arguments.contains("-UITestWidgetGallery") {
                    WidgetGallery()
                } else {
                    RootView()
                }
            }
                .environment(model)
                .environment(shared)
                .environment(nav)
                .environment(cloud)
                .environment(sync)
                .environment(cloudUI)
                .environment(runs)
                .environment(ai)
                .onOpenURL { nav.open(url: $0) }
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
                Button("New Request from Text or curl…") { nav.newRequestFromText() }
                    .keyboardShortcut("n", modifiers: [.command, .shift])
                Button("Search") { nav.showSearch() }
                    .keyboardShortcut("k", modifiers: .command)
                Divider()
                Button("Explain Response") { nav.explainCurrent(model) }
                    .keyboardShortcut("e", modifiers: [.command, .shift])
            }
        }
    }
}

/// UI tests of shared collections: prepares `SharedScenario` (Ada signed in here, Grace's phone in
/// this process) before the app's first sync, then shows the app.
struct ScenarioGate: View {
    var scenario: String
    @SwiftUI.Environment(HitopCloud.self) private var cloud
    @SwiftUI.Environment(SharedSync.self) private var shared
    @State private var ready = false
    @State private var failure: String?

    var body: some View {
        Group {
            if ready {
                RootView()
            } else {
                VStack(spacing: 12) {
                    ProgressView()
                    Text(failure ?? "Preparing…").font(Typo.caption).foregroundStyle(Theme.textMuted).accessibilityIdentifier("scenario-status")
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Theme.ground.ignoresSafeArea())
            }
        }
        .task {
            do {
                if scenario == "-UITestSharedInvite" {
                    try await SharedScenario.invite(app: cloud, fake: FakeCloud.shared)
                } else {
                    try await SharedScenario.joined(app: cloud, shared: shared, fake: FakeCloud.shared)
                }
                ready = true
            } catch {
                failure = "Scenario failed: \(error.localizedDescription)"
            }
        }
    }
}
