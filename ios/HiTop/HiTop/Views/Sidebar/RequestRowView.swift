import SwiftUI

struct RequestRowView: View {
    @Environment(AppViewModel.self) private var vm
    let request: HTTPRequest
    let collectionId: UInt64
    @State private var showRename = false
    @State private var renameName = ""

    var isSelected: Bool {
        vm.selectedRequestId == request.id
    }

    var body: some View {
        NavigationLink(value: RequestNavTarget(requestId: request.id, collectionId: collectionId)) {
            HStack(spacing: 8) {
                Text(request.method)
                    .font(.caption.monospaced())
                    .fontWeight(.semibold)
                    .foregroundStyle(HTTPMethod.color(for: request.method))
                    .frame(width: 52, alignment: .leading)
                Text(request.name)
                    .lineLimit(1)
                    .foregroundStyle(isSelected ? .primary : .secondary)
            }
        }
        .listRowBackground(isSelected ? Color.accentColor.opacity(0.12) : Color.clear)
        // Also update ViewModel so the detail view knows what's active
        .simultaneousGesture(TapGesture().onEnded {
            if let col = vm.collections.first(where: { $0.id == collectionId }),
               let req = col.requests.first(where: { $0.id == request.id }) {
                vm.loadRequest(req, collectionId: collectionId)
            }
        })
        .contextMenu {
            Button { renameName = request.name; showRename = true } label: {
                Label("Rename", systemImage: "pencil")
            }
            Button(role: .destructive) {
                vm.deleteRequest(id: request.id, collectionId: collectionId)
            } label: {
                Label("Delete", systemImage: "trash")
            }
        }
        .alert("Rename Request", isPresented: $showRename) {
            TextField("Name", text: $renameName)
            Button("Rename") { vm.renameRequest(id: request.id, collectionId: collectionId, name: renameName) }
            Button("Cancel", role: .cancel) {}
        }
    }
}
