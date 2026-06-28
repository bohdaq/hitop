import SwiftUI
import UniformTypeIdentifiers

struct ImportView: View {
    @Environment(AppViewModel.self) private var vm
    @Environment(\.dismiss) private var dismiss
    @State private var json = ""
    @State private var errorMessage: String? = nil
    @State private var showFilePicker = false

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 12) {
                Text("Paste JSON or pick a file:")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal)

                TextEditor(text: $json)
                    .font(.system(.caption, design: .monospaced))
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .padding(.horizontal, 8)
                    .frame(maxHeight: .infinity)

                if let error = errorMessage {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .padding(.horizontal)
                }
            }
            .navigationTitle("Import Collections")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("File") { showFilePicker = true }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Import") { doImport() }
                        .disabled(json.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .fileImporter(isPresented: $showFilePicker, allowedContentTypes: [.json]) { result in
                if let url = try? result.get(),
                   let content = try? String(contentsOf: url) {
                    json = content
                }
            }
        }
    }

    private func doImport() {
        do {
            try vm.importJSON(json)
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
