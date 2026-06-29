import SwiftUI

struct URLBarView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        @Bindable var vm = vm
        HStack(spacing: 8) {
            Menu {
                ForEach(HTTPMethod.allCases, id: \.self) { method in
                    Button(method.rawValue) { vm.activeMethod = method.rawValue }
                }
            } label: {
                Text(vm.activeMethod)
                    .font(.subheadline.monospaced())
                    .fontWeight(.semibold)
                    .foregroundStyle(HTTPMethod.color(for: vm.activeMethod))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
            }
            .accessibilityIdentifier("methodMenu")

            TextField("https://api.example.com/endpoint", text: $vm.activeURL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                .font(.subheadline.monospaced())
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
                .submitLabel(.send)
                .onSubmit {
                    Task { await vm.sendRequest() }
                }
                .accessibilityIdentifier("urlTextField")
                .toolbar {
                    ToolbarItemGroup(placement: .keyboard) {
                        Spacer()
                        Button("Done") {
                            UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
                        }
                    }
                }

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
            .accessibilityIdentifier("sendButton")
            .accessibilityLabel("Send")
        }
    }
}

extension HTTPMethod {
    static func color(for method: String) -> Color {
        switch method.uppercased() {
        case "GET":    return .green
        case "POST":   return .blue
        case "PUT":    return .orange
        case "PATCH":  return Color(hue: 0.15, saturation: 0.9, brightness: 0.85)
        case "DELETE": return .red
        default:       return .secondary
        }
    }
}
