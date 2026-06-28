import SwiftUI

struct SidebarView: View {
    @Environment(AppViewModel.self) private var vm
    @State private var showAddCollection = false
    @State private var newCollectionName = ""
    @State private var showHistory = false
    @State private var showImport = false
    @State private var importJSON = ""
    @State private var importError: String? = nil
    @State private var showExport = false

    var body: some View {
        List {
            ForEach($vm.collections) { $collection in
                CollectionSectionView(collection: $collection)
            }
        }
        .listStyle(.sidebar)
        .navigationTitle("HiTop")
        .toolbar {
            ToolbarItemGroup(placement: .navigationBarTrailing) {
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
