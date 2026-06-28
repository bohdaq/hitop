import SwiftUI

struct RequestDetailView: View {
    @Environment(AppViewModel.self) private var vm
    @State private var selectedTab = 0

    var body: some View {
        VStack(spacing: 0) {
            // URL Bar
            URLBarView()
                .padding(.horizontal)
                .padding(.vertical, 10)

            Divider()

            // Tab picker: Headers / Body / Scripts
            Picker("", selection: $selectedTab) {
                Text("Headers").tag(0)
                Text("Body").tag(1)
                Text("Scripts").tag(2)
            }
            .pickerStyle(.segmented)
            .padding(.horizontal)
            .padding(.vertical, 8)

            // Tab content
            Group {
                switch selectedTab {
                case 0: HeadersEditorView()
                case 1: BodyEditorView()
                case 2: ScriptsEditorView()
                default: EmptyView()
                }
            }

            Divider()

            // Response
            ResponseView()
        }
        .navigationTitle(requestTitle)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button("Save") { vm.saveActiveRequest() }
                    .disabled(vm.activeRequestId == nil)
            }
        }
    }

    private var requestTitle: String {
        guard let cId = vm.activeCollectionId,
              let rId = vm.activeRequestId,
              let col = vm.collections.first(where: { $0.id == cId }),
              let req = col.requests.first(where: { $0.id == rId }) else {
            return "Request"
        }
        return req.name
    }
}
