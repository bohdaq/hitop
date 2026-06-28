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
        Button {
            vm.loadRequest(request, collectionId: collectionId)
        } label: {
            HStack(spacing: 8) {
                Text(request.method)
                    .font(.caption.monospaced())
                    .fontWeight(.semibold)
                    .foregroundStyle(methodColor(request.method))
                    .frame(width: 52, alignment: .leading)
                Text(request.name)
                    .lineLimit(1)
                    .foregroundStyle(isSelected ? .primary : .secondary)
            }
        }
        .listRowBackground(isSelected ? Color.accentColor.opacity(0.12) : Color.clear)
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

    private func methodColor(_ method: String) -> Color {
        switch method.uppercased() {
        case "GET":    return .green
        case "POST":   return .blue
        case "PUT":    return .orange
        case "PATCH":  return .yellow
        case "DELETE": return .red
        default:       return .secondary
        }
    }
}
