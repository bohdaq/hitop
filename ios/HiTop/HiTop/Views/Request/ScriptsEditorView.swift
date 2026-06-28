import SwiftUI

struct ScriptsEditorView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Label("Pre-Request Script", systemImage: "arrow.up.circle")
                    .font(.caption.bold())
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 12)
                    .padding(.top, 12)
                    .padding(.bottom, 4)

                TextEditor(text: Bindable(vm).activePreScript)
                    .font(.system(.body, design: .monospaced))
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .frame(minHeight: 120)
                    .padding(.horizontal, 8)

                Divider().padding(.vertical, 8)

                Label("Post-Request Script", systemImage: "arrow.down.circle")
                    .font(.caption.bold())
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 4)

                TextEditor(text: Bindable(vm).activePostScript)
                    .font(.system(.body, design: .monospaced))
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .frame(minHeight: 120)
                    .padding(.horizontal, 8)

                Text("Available: setUrl(url), setBody(body), setHeader(name, value), setContext(key, value), getContext(key), getVariable(key), response, statusCode, responseText, getResponseHeader(name)")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .padding(12)
            }
        }
    }
}
