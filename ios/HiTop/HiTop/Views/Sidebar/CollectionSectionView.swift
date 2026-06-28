import SwiftUI

struct CollectionSectionView: View {
    @Environment(AppViewModel.self) private var vm
    @Binding var collection: Collection
    @State private var isExpanded = true
    @State private var showRename = false
    @State private var renameName = ""
    @State private var showDeleteConfirm = false
    @State private var showVariables = false
    @State private var showRunResults = false

    var body: some View {
        Section(isExpanded: $isExpanded) {
            ForEach(collection.requests) { request in
                RequestRowView(request: request, collectionId: collection.id)
            }
            .onMove { from, to in vm.moveRequest(collectionId: collection.id, fromOffsets: from, toOffset: to) }
            .onDelete { offsets in
                for idx in offsets {
                    vm.deleteRequest(id: collection.requests[idx].id, collectionId: collection.id)
                }
            }

            Button {
                vm.addRequest(to: collection.id)
            } label: {
                Label("Add Request", systemImage: "plus")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
        } header: {
            CollectionHeaderView(
                collection: collection,
                onRename: { renameName = collection.name; showRename = true },
                onDelete: { showDeleteConfirm = true },
                onDuplicate: { vm.duplicateCollection(id: collection.id) },
                onVariables: { showVariables = true },
                onRun: { showRunResults = true }
            )
        }
        .alert("Rename Collection", isPresented: $showRename) {
            TextField("Name", text: $renameName)
            Button("Rename") { vm.renameCollection(id: collection.id, name: renameName) }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog("Delete \"\(collection.name)\"?", isPresented: $showDeleteConfirm, titleVisibility: .visible) {
            Button("Delete", role: .destructive) { vm.deleteCollection(id: collection.id) }
            Button("Cancel", role: .cancel) {}
        }
        .sheet(isPresented: $showVariables) {
            VariablesView(collectionId: collection.id)
        }
        .sheet(isPresented: $showRunResults) {
            RunCollectionView(collection: collection)
        }
    }
}

struct CollectionHeaderView: View {
    let collection: Collection
    let onRename: () -> Void
    let onDelete: () -> Void
    let onDuplicate: () -> Void
    let onVariables: () -> Void
    let onRun: () -> Void

    var body: some View {
        HStack {
            Text(collection.name)
                .font(.headline)
                .foregroundStyle(.primary)
            Spacer()
            Menu {
                Button { onRename() } label: { Label("Rename", systemImage: "pencil") }
                Button { onDuplicate() } label: { Label("Duplicate", systemImage: "doc.on.doc") }
                Button { onVariables() } label: { Label("Variables", systemImage: "curlybraces") }
                Button { onRun() } label: { Label("Run Collection", systemImage: "play.fill") }
                Divider()
                Button(role: .destructive) { onDelete() } label: { Label("Delete", systemImage: "trash") }
            } label: {
                Image(systemName: "ellipsis")
                    .foregroundStyle(.secondary)
            }
        }
    }
}
