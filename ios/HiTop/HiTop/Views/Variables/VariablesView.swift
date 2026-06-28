import SwiftUI

struct VariablesView: View {
    @Environment(AppViewModel.self) private var vm
    @Environment(\.dismiss) private var dismiss
    let collectionId: UInt64

    @State private var pairs: [(key: String, value: String, id: UUID)] = []

    var body: some View {
        NavigationStack {
            List {
                ForEach($pairs, id: \.id) { $pair in
                    HStack(spacing: 8) {
                        TextField("Variable name", text: $pair.key)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .font(.subheadline.monospaced())
                        TextField("Value", text: $pair.value)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .font(.subheadline.monospaced())
                            .foregroundStyle(.secondary)
                    }
                }
                .onDelete { offsets in pairs.remove(atOffsets: offsets) }

                Button {
                    pairs.append((key: "", value: "", id: UUID()))
                } label: {
                    Label("Add Variable", systemImage: "plus")
                }
            }
            .listStyle(.plain)
            .navigationTitle("Variables")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }
                }
            }
            .onAppear { loadPairs() }
        }
    }

    private func loadPairs() {
        let vars = vm.variables(for: collectionId)
        pairs = vars.map { (key: $0.key, value: $0.value, id: UUID()) }
            .sorted { $0.key < $1.key }
    }

    private func save() {
        var dict: [String: String] = [:]
        for pair in pairs where !pair.key.isEmpty {
            dict[pair.key] = pair.value
        }
        vm.updateVariables(dict, for: collectionId)
        dismiss()
    }
}
