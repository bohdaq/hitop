import SwiftUI

struct HistoryView: View {
    @Environment(AppViewModel.self) private var vm
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if vm.history.isEmpty {
                    ContentUnavailableView("No History", systemImage: "clock", description: Text("Sent requests will appear here."))
                } else {
                    List {
                        ForEach(vm.history) { item in
                            Button {
                                vm.loadFromHistory(item)
                                dismiss()
                            } label: {
                                HistoryRowView(item: item)
                            }
                            .foregroundStyle(.primary)
                        }
                    }
                    .listStyle(.plain)
                }
            }
            .navigationTitle("History")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
                ToolbarItem(placement: .destructiveAction) {
                    Button("Clear", role: .destructive) { vm.clearHistory() }
                        .disabled(vm.history.isEmpty)
                }
            }
        }
    }
}

struct HistoryRowView: View {
    let item: HistoryItem

    var body: some View {
        HStack(spacing: 10) {
            Text(item.method)
                .font(.caption.monospaced().bold())
                .foregroundStyle(methodColor(item.method))
                .frame(width: 52, alignment: .leading)

            VStack(alignment: .leading, spacing: 2) {
                Text(item.url)
                    .font(.subheadline.monospaced())
                    .lineLimit(1)
                if let col = item.collectionName, let req = item.requestName {
                    Text("\(col) › \(req)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 2) {
                StatusBadge(statusCode: Int(item.statusCode))
                Text("\(item.durationMs) ms")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
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
