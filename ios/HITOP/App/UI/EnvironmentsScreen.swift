import SwiftUI

struct EnvironmentsScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(Navigator.self) private var nav
    @State private var adding = false
    @State private var newName = ""

    var body: some View {
        List {
            Section {
                ForEach(model.environments) { e in row(e) }
                    .onDelete { idx in idx.map { model.environments[$0] }.filter { $0.extras["team"] == nil }.map(\.id).forEach(model.deleteEnvironment) }
                Button { model.setActiveEnvironment(nil) } label: {
                    HStack(spacing: 12) {
                        Image(systemName: model.state.activeEnvironmentID == nil ? "checkmark.circle.fill" : "circle")
                            .font(.system(size: 20)).foregroundStyle(model.state.activeEnvironmentID == nil ? Theme.accent : Theme.chevron)
                        Text("No environment").font(Typo.row).foregroundStyle(Theme.textSoft)
                        Spacer()
                    }
                    .frame(minHeight: 44)
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(model.state.activeEnvironmentID == nil ? .isSelected : [])
            } header: {
                Text("ENVIRONMENTS").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
            } footer: {
                Text("The active environment fills ${variables} in every request. Secret values stay in this device's Keychain.")
                    .font(Typo.caption).foregroundStyle(Theme.textMuted)
            }
            .listRowBackground(Theme.surface)
            .listRowSeparatorTint(Theme.separator)

            Section {
                NavigationLink(value: Route.environment(model.globals.id)) {
                    HStack(spacing: 12) {
                        Image(systemName: "globe").foregroundStyle(Theme.info).frame(width: 22)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Globals").font(Typo.row).foregroundStyle(Theme.textRow)
                            Text(count(model.globals)).font(Typo.caption).foregroundStyle(Theme.textMuted)
                        }
                    }
                    .frame(minHeight: 48)
                }
                .accessibilityIdentifier("environment-Globals")
            } header: {
                Text("GLOBALS").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
            } footer: {
                Text("Globals apply everywhere, below collection and environment values.").font(Typo.caption).foregroundStyle(Theme.textMuted)
            }
            .listRowBackground(Theme.surface)
        }
        .listStyle(.insetGrouped)
        .listSectionSpacing(12)
        .scrollContentBackground(.hidden)
        .background(ScreenBackground())
        .navigationTitle("Environments")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { newName = ""; adding = true } label: { Image(systemName: "plus") }
                    .accessibilityLabel("New environment")
            }
        }
        .alert("New environment", isPresented: $adding) {
            TextField("Name", text: $newName)
            Button("Cancel", role: .cancel) {}
            Button("Add") {
                let e = model.addEnvironment(name: newName)
                nav.environmentsPath = [.environment(e.id)]
            }
        }
    }

    private func count(_ e: SavedEnvironment) -> String {
        let n = Set(e.variables.keys).union(e.secrets).count
        let s = e.secrets.count
        return "\(n) \(n == 1 ? "variable" : "variables")" + (s > 0 ? " · \(s) secret" : "")
    }

    private func row(_ e: SavedEnvironment) -> some View {
        let active = model.state.activeEnvironmentID == e.id
        return HStack(spacing: 12) {
            Button { model.setActiveEnvironment(e.id) } label: {
                Image(systemName: active ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 20))
                    .foregroundStyle(active ? Theme.accent : Theme.chevron)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(active ? "\(e.name), active" : "Use \(e.name)")
            .accessibilityIdentifier("use-\(e.name)")
            NavigationLink(value: Route.environment(e.id)) {
                HStack(spacing: 10) {
                    EnvironmentDot(name: e.name)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(e.name).font(Typo.row).foregroundStyle(Theme.textRow)
                        Text(count(e) + (e.extras["team"] != nil ? " · team" : "")).font(Typo.caption).foregroundStyle(Theme.textMuted)
                    }
                    Spacer()
                    if active {
                        Text("Active").font(Typo.geist(12, .semibold)).foregroundStyle(Theme.accentText)
                            .padding(.horizontal, 8).padding(.vertical, 3)
                            .background(Capsule().fill(Theme.accent.opacity(0.16)))
                    }
                }
            }
            .accessibilityIdentifier("environment-\(e.name)")
        }
        .listRowInsets(EdgeInsets(top: 2, leading: 4, bottom: 2, trailing: 16))
        .swipeActions(edge: .leading) {
            Button("Use", systemImage: "checkmark") { model.setActiveEnvironment(e.id) }.tint(Theme.accentDeep)
        }
        .contextMenu {
            Button("Use this environment", systemImage: "checkmark") { model.setActiveEnvironment(e.id) }
            if e.extras["team"] == nil {
                Button("Delete", systemImage: "trash", role: .destructive) { model.deleteEnvironment(e.id) }
            }
        }
    }
}

/// One environment's (or the globals') variables. Secret values are shown masked and kept in the
/// Keychain.
struct EnvironmentDetail: View {
    var id: String
    @Environment(AppModel.self) private var model
    @State private var editing: VariableDraft?
    @State private var renaming = false
    @State private var newName = ""

    struct VariableDraft: Identifiable {
        var id = UUID()
        var original: String?
        var name: String
        var value: String
        var secret: Bool
    }

    private var env: SavedEnvironment? { id == model.globals.id ? model.globals : model.environments.first { $0.id == id } }

    /// A team environment (the organization's, I2b): read here, edited in the web app; only secret
    /// values can be entered (they stay in this device's Keychain).
    private var team: Bool { env?.extras["team"] != nil }

    var body: some View {
        if let env {
            let names = Array(Set(env.variables.keys).union(env.secrets)).sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
            List {
                Section {
                    if names.isEmpty {
                        Text("No variables yet.").font(Typo.caption).foregroundStyle(Theme.textMuted).frame(minHeight: 44)
                    }
                    ForEach(names, id: \.self) { name in
                        let secret = env.secrets.contains(name)
                        let value = secret ? model.secretValue(env, name) : env.variables[name] ?? ""
                        Button {
                            editing = VariableDraft(original: name, name: name, value: value, secret: secret)
                        } label: {
                            HStack(spacing: 10) {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(name).font(Typo.mono(14, .semibold)).foregroundStyle(Theme.codeKey)
                                    Text(secret ? (value.isEmpty ? "no value on this device" : "••••••") : (value.isEmpty ? "(empty)" : value))
                                        .font(Typo.mono(13.5))
                                        .foregroundStyle(secret && value.isEmpty ? Theme.warning : Theme.codeText)
                                        .lineLimit(2)
                                }
                                Spacer()
                                if secret { Image(systemName: "lock.fill").font(.system(size: 12)).foregroundStyle(Theme.textMuted).accessibilityLabel("Secret") }
                            }
                            .frame(minHeight: 48)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(team && !secret)
                        .accessibilityIdentifier("variable-\(name)")
                        .swipeActions {
                            if !team {
                                Button("Delete", systemImage: "trash", role: .destructive) { model.removeVariable(env, name: name) }
                            }
                        }
                    }
                } header: {
                    Text("VARIABLES").font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
                } footer: {
                    if team {
                        Text("A team environment of your organization: change it in the web app. Secret values you enter stay in this device's Keychain.")
                            .font(Typo.caption).foregroundStyle(Theme.textMuted)
                    }
                }
                .listRowBackground(Theme.surface)
                .listRowSeparatorTint(Theme.separator)
                Section {
                    if !team {
                        Button {
                            editing = VariableDraft(original: nil, name: "", value: "", secret: false)
                        } label: {
                            Label("Add variable", systemImage: "plus").font(Typo.row).frame(minHeight: 44)
                        }
                        .accessibilityIdentifier("add-variable")
                    }
                    if id != model.globals.id, model.state.activeEnvironmentID != id {
                        Button { model.setActiveEnvironment(id) } label: {
                            Label("Use this environment", systemImage: "checkmark.circle").font(Typo.row).frame(minHeight: 44)
                        }
                    }
                }
                .listRowBackground(Theme.surface)
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Theme.ground.ignoresSafeArea())
            .navigationTitle(env.name)
            .toolbar {
                if id != model.globals.id && !team {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("Rename") { newName = env.name; renaming = true }
                    }
                }
            }
            .alert("Rename environment", isPresented: $renaming) {
                TextField("Name", text: $newName)
                Button("Cancel", role: .cancel) {}
                Button("Save") {
                    var e = env
                    e.name = newName.isEmpty ? e.name : newName
                    model.updateEnvironment(e)
                }
            }
            .sheet(item: $editing) { draft in
                VariableEditor(draft: draft) { d in
                    model.setVariable(env, name: d.name, value: d.value, secret: d.secret, renamedFrom: d.original)
                }
            }
        } else {
            ContentUnavailableView("Environment deleted", systemImage: "globe")
        }
    }
}

struct VariableEditor: View {
    @State var draft: EnvironmentDetail.VariableDraft
    var save: (EnvironmentDetail.VariableDraft) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var reveal = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Name") {
                    TextField("baseUrl", text: $draft.name)
                        .font(Typo.mono(15))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("variable-name")
                }
                Section {
                    HStack {
                        Group {
                            if draft.secret && !reveal {
                                SecureField("value", text: $draft.value)
                            } else {
                                TextField("value", text: $draft.value, axis: .vertical)
                            }
                        }
                        .font(Typo.mono(15))
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityIdentifier("variable-value")
                        if draft.secret {
                            Button { reveal.toggle() } label: { Image(systemName: reveal ? "eye.slash" : "eye") }
                                .accessibilityLabel(reveal ? "Hide value" : "Show value")
                        }
                    }
                    Toggle("Secret", isOn: $draft.secret).tint(Theme.accent)
                } header: {
                    Text("Value")
                } footer: {
                    Text(draft.secret ? "Kept in this device's Keychain, never in the collection files or exports; shown as •••• in responses and history."
                                      : "Saved with the environment.")
                }
            }
            .navigationTitle(draft.original == nil ? "New variable" : "Edit variable")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save(draft); dismiss() }.disabled(draft.name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
