import SwiftUI

struct SidebarView: View {
    @Environment(AppViewModel.self) private var vm
    @State private var showAddCollection = false
    @State private var newCollectionName = ""
    @State private var showHistory = false
    @State private var showImport = false
    @State private var showExport = false
    @State private var editMode: EditMode = .inactive

    var body: some View {
        @Bindable var vm = vm
        List {
            ForEach($vm.collections) { $collection in
                CollectionSectionView(collection: $collection)
            }
        }
        .listStyle(.sidebar)
        .environment(\.editMode, $editMode)
        // Register the navigation destination for request rows
        .navigationDestination(for: RequestNavTarget.self) { target in
            RequestDetailView()
                .onAppear {
                    guard let col = vm.collections.first(where: { $0.id == target.collectionId }),
                          let req = col.requests.first(where: { $0.id == target.requestId }) else { return }
                    vm.loadRequest(req, collectionId: target.collectionId)
                }
        }
        .navigationTitle("HiTop")
        .toolbar {
            ToolbarItemGroup(placement: .navigationBarTrailing) {
                Button {
                    withAnimation {
                        editMode = editMode == .active ? .inactive : .active
                    }
                } label: {
                    Image(systemName: editMode == .active ? "checkmark.circle.fill" : "arrow.up.arrow.down")
                }

                Menu {
                    Button { showHistory = true } label: {
                        Label("History", systemImage: "clock")
                    }
                    Button { showExport = true } label: {
                        Label("Export", systemImage: "square.and.arrow.up")
                    }
                    Button { showImport = true } label: {
                        Label("Import", systemImage: "square.and.arrow.down")
                    }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }

                Button {
                    newCollectionName = ""
                    showAddCollection = true
                } label: {
                    Image(systemName: "folder.badge.plus")
                }
            }
        }
        .alert("New Collection", isPresented: $showAddCollection) {
            TextField("Collection name", text: $newCollectionName)
            Button("Add") { vm.addCollection(name: newCollectionName) }
            Button("Cancel", role: .cancel) {}
        }
        .sheet(isPresented: $showHistory) {
            HistoryView()
        }
        .sheet(isPresented: $showExport) {
            ExportView()
        }
        .sheet(isPresented: $showImport) {
            ImportView()
        }
    }
}
