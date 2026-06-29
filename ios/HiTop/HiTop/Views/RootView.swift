import SwiftUI

struct RootView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        NavigationSplitView {
            SidebarView()
        } detail: {
            // Shown in the detail column on iPad (sidebar always visible).
            // On iPhone the NavigationLink in RequestRowView pushes directly.
            if vm.selectedRequestId != nil {
                RequestDetailView()
            } else {
                ContentUnavailableView(
                    "Select a Request",
                    systemImage: "arrow.up.right.square",
                    description: Text("Pick a request from the sidebar or create a new one.")
                )
            }
        }
        .onAppear { vm.loadData() }
    }
}
