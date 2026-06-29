import SwiftUI

struct RequestDetailView: View {
    @Environment(AppViewModel.self) private var vm
    // 0=Headers, 1=Body, 2=Scripts, 3=Response
    @State private var selectedTab = 0

    var body: some View {
        VStack(spacing: 0) {
            URLBarView()
                .padding(.horizontal)
                .padding(.vertical, 10)

            Divider()

            Picker("", selection: $selectedTab) {
                Text("Headers").tag(0)
                Text("Body").tag(1)
                Text("Scripts").tag(2)
                Text("Response").tag(3)
            }
            .pickerStyle(.segmented)
            .padding(.horizontal)
            .padding(.vertical, 8)

            Divider()

            Group {
                switch selectedTab {
                case 0: HeadersEditorView()
                case 1: BodyEditorView()
                case 2: ScriptsEditorView()
                case 3: ResponseView()
                default: EmptyView()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .navigationTitle(requestTitle)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .navigationBarTrailing) {
                Button {
                    vm.saveActiveRequest()
                } label: {
                    Image(systemName: "square.and.arrow.down")
                }
                .disabled(vm.activeRequestId == nil)
            }
        }
        // Auto-switch to Response tab whenever a send completes
        .onChange(of: vm.responseReceived) { _, _ in
            selectedTab = 3
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
