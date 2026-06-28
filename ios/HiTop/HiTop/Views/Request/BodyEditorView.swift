import SwiftUI

struct BodyEditorView: View {
    @Environment(AppViewModel.self) private var vm

    var body: some View {
        TextEditor(text: Bindable(vm).activeBody)
            .font(.system(.body, design: .monospaced))
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
            .padding(.horizontal, 12)
    }
}
