import SwiftUI

@main
struct HiTopApp: App {
    @State private var viewModel = AppViewModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(viewModel)
                .onAppear {
                    if CommandLine.arguments.contains("-uitesting") {
                        viewModel.resetForTesting()
                    }
                }
        }
    }
}
