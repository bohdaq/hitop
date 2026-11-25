import SwiftUI

/// iPad: three columns (sidebar, request, response), keyboard shortcuts ⌘↩ send, ⌘S save,
/// ⌘K search, ⌘N new request.
struct PadRoot: View {
    @Environment(AppModel.self) private var model
    @Environment(Navigator.self) private var nav
    @State private var visibility: NavigationSplitViewVisibility = .all
    @State private var naming: NameTarget?
    @State private var moving: UInt64?
    @State private var deleting: NameTarget?
    @State private var sharing: ShareTarget?
    @Environment(SharedSync.self) private var shared
    @Environment(HitopCloud.self) private var cloud

    var body: some View {
        @Bindable var nav = nav
        NavigationSplitView(columnVisibility: $visibility) {
            sidebar
                .navigationSplitViewColumnWidth(min: 260, ideal: 290, max: 340)
        } content: {
            middle
                .navigationSplitViewColumnWidth(min: 380, ideal: 470)
        } detail: {
            detail
        }
        .navigationSplitViewStyle(.balanced)
        .sheet(item: $sharing) { t in ShareSheet(collectionID: t.id) }
        .sheet(isPresented: $nav.padSearch) {
            NavigationStack {
                SearchScreen()
                    .searchable(text: $nav.searchText, placement: .navigationBarDrawer(displayMode: .always), prompt: "Requests, collections, history")
                    .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Close") { nav.padSearch = false } } }
            }
            .onChange(of: nav.padRequest) { nav.padSearch = false }
        }
        .modifier(LibraryActions(naming: $naming, moving: $moving, deleting: $deleting))
        .modifier(ExplainHost())
        .onAppear {
            if nav.padRequest == nil, let first = model.collections.first?.requests.first { nav.padRequest = first.id }
        }
    }

    private func newRequest() {
        let r = model.newRequest()
        nav.open(request: r.id)
    }

    // MARK: Sidebar

    private var sidebar: some View {
        List(selection: Binding(get: { nav.padPage == .collections ? nav.padRequest : nil }, set: { id in
            if let id { nav.padPage = .collections; nav.padRequest = id }
        })) {
            Section {
                HStack(spacing: 8) {
                    Text("H").font(Typo.geist(12, .bold)).foregroundStyle(.white)
                        .frame(width: 28, height: 28).background(Circle().fill(Theme.logoGradient))
                    Text("HITOP").font(Typo.geist(18, .bold))
                    Spacer()
                    Button { nav.showAccount = true } label: { Image(systemName: "person.crop.circle") }
                        .buttonStyle(.plain).foregroundStyle(Theme.textMuted).accessibilityLabel("Account").accessibilityIdentifier("account-pill")
                }
                .listRowBackground(Color.clear)
                Button { nav.padSearch = true } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "magnifyingglass")
                        Text("Search")
                        Spacer()
                        Text("⌘K").font(Typo.mono(12, .medium))
                    }
                    .font(Typo.geist(14))
                    .foregroundStyle(Theme.textMuted)
                    .padding(.horizontal, 10)
                    .frame(height: 36)
                    .background(RoundedRectangle(cornerRadius: 11, style: .continuous).fill(Color.white.opacity(0.07)))
                }
                .buttonStyle(.plain)
                .hoverEffect(.highlight)
                .listRowBackground(Color.clear)
                .accessibilityLabel("Search")
                .accessibilityHint("Command K")
                .accessibilityIdentifier("pad-search")
            }
            ForEach(model.visibleCollections) { c in
                Section {
                    ForEach(c.requests) { r in
                        HStack(spacing: 10) {
                            MethodTag(method: methodLabel(r), width: 46, size: 11)
                            Text(r.name).font(Typo.geist(14)).lineLimit(1)
                        }
                        .tag(r.id)
                        .accessibilityIdentifier("request-\(r.name)")
                        .contextMenu {
                            Button("Rename", systemImage: "pencil") { naming = .request(r.id) }
                            Button("Duplicate", systemImage: "plus.square.on.square") { model.duplicateRequest(r.id) }
                            Button("Move to…", systemImage: "folder") { moving = r.id }
                            Button(model.isPinned(request: r.id) ? "Unpin" : "Pin", systemImage: "pin") { model.togglePin(request: r.id) }
                            Button("Delete", systemImage: "trash", role: .destructive) { model.deleteRequest(r.id) }
                        }
                        .swipeActions {
                            Button("Delete", systemImage: "trash", role: .destructive) { model.deleteRequest(r.id) }
                            Button("Move", systemImage: "folder") { moving = r.id }.tint(Theme.info)
                        }
                    }
                } header: {
                    HStack(spacing: 6) {
                        Text(c.name.uppercased()).font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint).accessibilityAddTraits(.isHeader)
                        if let b = shared.badge(c.id) {
                            Image(systemName: "person.2.fill").font(.system(size: 9)).foregroundStyle(Theme.accentText).accessibilityLabel(b)
                        }
                    }
                    .contextMenu {
                        if !model.isReadOnly(c.id) {
                            Button("New request", systemImage: "plus") { nav.open(request: model.newRequest(in: c.id).id) }
                            Button("Rename", systemImage: "pencil") { naming = .collection(c.id) }
                        }
                        Button("Run collection", systemImage: "play.circle") { nav.run(collection: c.id) }
                            .disabled(Sender.runnable(c).isEmpty)
                        ExportMenu(collection: c)
                        if cloud.signedIn {
                            Button(model.isShared(c.id) ? "People and sharing" : "Share…", systemImage: "person.2") { sharing = ShareTarget(id: c.id) }
                        }
                        Button("Delete", systemImage: "trash", role: .destructive) { deleting = .collection(c.id) }
                    }
                }
            }
            Section {
                pageRow(.history, "History", "clock")
                pageRow(.environments, "Environments", "globe")
                pageRow(.runs, "Runs", "play")
            }
        }
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .background(Theme.ground)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { nav.padSearch = true } label: { Image(systemName: "magnifyingglass") }
                    .accessibilityLabel("Search")
            }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Button("New request", systemImage: "plus") { newRequest() }
                    Button("New request from text or curl…", systemImage: "text.badge.plus") { nav.newRequestFromText() }
                    Button("Import…", systemImage: "square.and.arrow.down") { nav.importing = ImportTarget() }
                        .accessibilityIdentifier("import")
                        .accessibilityIdentifier("new-from-text")
                } label: {
                    Image(systemName: "plus")
                } primaryAction: {
                    newRequest()
                }
                .accessibilityLabel("New request")
                .accessibilityHint("Hold for a request from text or curl")
                .accessibilityIdentifier("pad-new-request")
            }
        }
    }

    private func pageRow(_ page: AppTab, _ title: String, _ icon: String) -> some View {
        Button { nav.padPage = page } label: {
            Label(title, systemImage: icon)
                .font(Typo.geist(14))
                .foregroundStyle(nav.padPage == page ? Color.white : Theme.textSoft)
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .hoverEffect(.highlight)
        .accessibilityIdentifier("pad-page-\(title)")
        .listRowBackground(nav.padPage == page ? RoundedRectangle(cornerRadius: 11).fill(Theme.accent.opacity(0.16)) : nil)
        .accessibilityAddTraits(nav.padPage == page ? .isSelected : [])
    }

    // MARK: Middle and detail

    @ViewBuilder
    private var middle: some View {
        switch nav.padPage {
        case .history: NavigationStack { HistoryScreen().navigationDestination(for: Route.self) { RouteView(route: $0) } }
        case .environments: NavigationStack { EnvironmentsScreen().navigationDestination(for: Route.self) { RouteView(route: $0) } }
        case .runs: NavigationStack { RunsScreen() }
        default:
            if let id = nav.padRequest, let editor = model.editor(for: id) {
                PadRequestColumn(editor: editor).id(id)
            } else {
                ContentUnavailableView("No request open", systemImage: "doc", description: Text("Pick a request in the sidebar, or press ⌘N."))
            }
        }
    }

    @ViewBuilder
    private var detail: some View {
        if nav.padPage == .collections, let id = nav.padRequest, let editor = model.editor(for: id) {
            ScrollView {
                ResponsePanel(editor: editor, style: .column)
                    .padding(18)
            }
            .background(Color(hex: 0x121217).ignoresSafeArea())
            .navigationTitle("Response")
            .navigationBarTitleDisplayMode(.inline)
        } else {
            ContentUnavailableView("Response", systemImage: "arrow.down.doc", description: Text("The response of the open request shows here."))
                .background(Color(hex: 0x121217).ignoresSafeArea())
        }
    }
}

struct PadRequestColumn: View {
    @Bindable var editor: RequestEditor
    @Environment(AppModel.self) private var model

    private var readOnly: Bool { model.isReadOnly(editor.collectionID) }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 10) {
                    HStack(spacing: 10) {
                        MethodPicker(editor: editor, height: 30).disabled(readOnly)
                        URLField(url: $editor.draft.url, context: model.context(for: editor.collectionID), size: 14.5, singleLine: true).disabled(readOnly)
                        EnvironmentMenu(compact: true)
                    }
                    .padding(.leading, 8)
                    .padding(.trailing, 8)
                    .frame(minHeight: 52)
                    .background(Capsule().fill(Theme.surface))
                    .overlay(Capsule().stroke(Color.white.opacity(0.12)))
                    SendButton(title: editor.sendTitle, icon: editor.sendIcon, shortcut: "⌘↩", sending: editor.sendBusy, height: 52) { editor.primaryAction() }
                        .fixedSize()
                        .disabled(!editor.sendEnabled)
                }
                RequestEditorView(editor: editor, wide: true)
            }
            .padding(.horizontal, 12)
            .padding(.top, 6)
            .padding(.bottom, 24)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(Theme.ground.ignoresSafeArea())
        .navigationTitle(editor.draft.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                if readOnly {
                    Label("View only", systemImage: "eye").labelStyle(.titleAndIcon).accessibilityIdentifier("view-only")
                } else {
                    Button { editor.save() } label: {
                        Label(editor.isDirty ? "Save" : "Saved", systemImage: editor.isDirty ? "square.and.arrow.down.fill" : "checkmark")
                    }
                    .disabled(!editor.isDirty)
                    .accessibilityIdentifier("save")
                }
            }
        }
    }
}
