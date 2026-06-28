import SwiftUI

struct HeadersEditorView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        List {
            ForEach($vm.activeHeaders) { $header in
                HStack(spacing: 8) {
                    TextField("Name", text: $header.name)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .font(.subheadline.monospaced())
                    TextField("Value", text: $header.value)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .font(.subheadline.monospaced())
                        .foregroundStyle(.secondary)
                }
            }
            .onDelete { offsets in
                vm.activeHeaders.remove(atOffsets: offsets)
                if vm.activeHeaders.isEmpty { vm.activeHeaders = [Header()] }
            }

            Button {
                vm.activeHeaders.append(Header())
            } label: {
                Label("Add Header", systemImage: "plus")
                    .font(.subheadline)
            }
        }
        .listStyle(.plain)
    }
}
