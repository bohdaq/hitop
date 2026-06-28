import SwiftUI

struct URLBarView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        HStack(spacing: 8) {
            Menu {
                ForEach(HTTPMethod.allCases, id: \.self) { method in
                    Button(method.rawValue) { vm.activeMethod = method.rawValue }
                }
            } label: {
                Text(vm.activeMethod)
                    .font(.subheadline.monospaced())
                    .fontWeight(.semibold)
                    .foregroundStyle(methodColor(vm.activeMethod))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
            }

            TextField("https://api.example.com/endpoint", text: Bindable(vm).activeURL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                .font(.subheadline.monospaced())
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))

            Button {
                Task { await vm.sendRequest() }
            } label: {
                if vm.isLoading {
                    ProgressView()
                        .frame(width: 44, height: 32)
                } else {
                    Image(systemName: "paperplane.fill")
                        .frame(width: 44, height: 32)
                }
            }
            .disabled(vm.isLoading || vm.activeURL.isEmpty)
            .buttonStyle(.borderedProminent)
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
