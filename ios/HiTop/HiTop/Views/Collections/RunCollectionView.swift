import SwiftUI

struct RunCollectionView: View {
    @Environment(AppViewModel.self) private var vm
    @Environment(\.dismiss) private var dismiss
    let collection: Collection

    var body: some View {
        NavigationStack {
            List {
                if vm.runResults.isEmpty && !vm.isRunning {
                    ContentUnavailableView(
                        "Ready to Run",
                        systemImage: "play.circle",
                        description: Text("Tap Run to execute all \(collection.requests.count) requests sequentially.")
                    )
                } else {
                    ForEach(vm.runResults) { result in
                        HStack(spacing: 10) {
                            Image(systemName: result.success ? "checkmark.circle.fill" : "xmark.circle.fill")
                                .foregroundStyle(result.success ? .green : .red)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(result.requestName)
                                    .font(.subheadline)
                                Text(result.url)
                                    .font(.caption.monospaced())
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                            Spacer()
                            VStack(alignment: .trailing, spacing: 2) {
                                Text(result.statusCode)
                                    .font(.caption.monospaced().bold())
                                    .foregroundStyle(result.success ? .green : .red)
                                Text("\(result.durationMs) ms")
                                    .font(.caption2)
                                    .foregroundStyle(.tertiary)
                            }
                        }
                    }
                    if vm.isRunning {
                        HStack {
                            ProgressView()
                            Text("Running…")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            .listStyle(.plain)
            .navigationTitle("Run: \(collection.name)")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        Task { await vm.runCollection(collection) }
                    } label: {
                        Label("Run", systemImage: "play.fill")
                    }
                    .disabled(vm.isRunning)
                }
            }
        }
    }
}
