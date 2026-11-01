import SwiftUI

/// hitop companion: opens hitop collection folders from Files or iCloud
/// Drive and runs them. No editing; collections are edited with the hitop
/// CLI, desktop or web app and synced through the folder (git, iCloud Drive).
@main
struct HitopCompanionApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(model)
        }
    }
}
