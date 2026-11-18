import SwiftUI

/// What a rename/new-name prompt is about.
enum NameTarget: Identifiable, Hashable {
    case collection(UInt64)
    case request(UInt64)
    case folder(UInt64, String)
    case newCollection
    case newFolder(UInt64, String)

    var id: String { "\(self)" }

    var title: String {
        switch self {
        case .collection: "Rename collection"
        case .request: "Rename request"
        case .folder: "Rename folder"
        case .newCollection: "New collection"
        case .newFolder: "New folder"
        }
    }
}

/// Shared actions for requests and collections (lists on iPhone and the iPad sidebar).
struct LibraryActions: ViewModifier {
    @Environment(AppModel.self) private var model
    @Binding var naming: NameTarget?
    @Binding var moving: UInt64?
    @Binding var deleting: NameTarget?
    @State private var text = ""

    func body(content: Content) -> some View {
        content
            .alert(naming?.title ?? "", isPresented: Binding(get: { naming != nil }, set: { if !$0 { naming = nil } })) {
                TextField("Name", text: $text)
                    .accessibilityIdentifier("name-field")
                Button("Cancel", role: .cancel) { naming = nil }
                Button("Save") { commit() }
            }
            .onChange(of: naming) { _, target in text = initial(target) }
            .confirmationDialog(deleteTitle, isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }), titleVisibility: .visible) {
                Button("Delete", role: .destructive) { delete() }
                Button("Cancel", role: .cancel) { deleting = nil }
            }
            .sheet(item: Binding(get: { moving.map(MoveItem.init) }, set: { moving = $0?.id })) { item in
                MoveSheet(requestID: item.id)
            }
    }

    private struct MoveItem: Identifiable { var id: UInt64 }

    private var deleteTitle: String {
        switch deleting {
        case let .collection(id)?: model.isShared(id) ? "Remove \(model.collection(id)?.name ?? "the collection") from this device? It stays shared for the others." : "Delete \(model.collection(id)?.name ?? "the collection") and its requests?"
        case let .folder(_, path)?: "Delete the folder \(path) and its requests?"
        default: "Delete?"
        }
    }

    private func initial(_ t: NameTarget?) -> String {
        switch t {
        case let .collection(id)?: model.collection(id)?.name ?? ""
        case let .request(id)?: model.request(id)?.name ?? ""
        case let .folder(_, path)?: String(path.split(separator: "/").last ?? "")
        default: ""
        }
    }

    private func commit() {
        switch naming {
        case let .collection(id)?: model.renameCollection(id, to: text)
        case let .request(id)?: model.renameRequest(id, to: text)
        case let .folder(cid, path)?: model.renameFolder(path, to: text, in: cid)
        case .newCollection?: model.addCollection(name: text)
        case let .newFolder(cid, parent)?: model.addFolder(text, in: cid, parent: parent)
        case nil: break
        }
        naming = nil
    }

    private func delete() {
        switch deleting {
        case let .collection(id)?: model.deleteCollection(id)
        case let .folder(cid, path)?: model.deleteFolder(path, in: cid)
        default: break
        }
        deleting = nil
    }
}

struct MoveSheet: View {
    var requestID: UInt64
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                ForEach(model.collections) { c in
                    Section(c.name) {
                        target(c, "")
                        ForEach(c.allFolders, id: \.self) { f in target(c, f) }
                    }
                }
            }
            .navigationTitle("Move to")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
        .presentationDetents([.medium, .large])
    }

    private func target(_ c: SavedCollection, _ folder: String) -> some View {
        let here = model.collectionOf(request: requestID)?.id == c.id && model.request(requestID)?.folder == folder
        return Button {
            model.moveRequest(requestID, to: c.id, folder: folder)
            dismiss()
        } label: {
            HStack {
                Image(systemName: folder.isEmpty ? "tray" : "folder")
                    .foregroundStyle(Theme.textMuted)
                    .padding(.leading, CGFloat(folder.split(separator: "/").count) * 14)
                Text(folder.isEmpty ? "Top level" : String(folder.split(separator: "/").last ?? ""))
                    .foregroundStyle(Theme.text)
                Spacer()
                if here { Image(systemName: "checkmark").foregroundStyle(Theme.accent) }
            }
            .frame(minHeight: 44)
        }
        .disabled(here)
    }
}

/// The environment picker pill (Collections header, request card).
struct EnvironmentMenu: View {
    @Environment(AppModel.self) private var model
    var compact = false

    var body: some View {
        Menu {
            Picker("Environment", selection: Binding(get: { model.state.activeEnvironmentID ?? "" },
                                                     set: { model.setActiveEnvironment($0.isEmpty ? nil : $0) })) {
                Text("No environment").tag("")
                ForEach(model.environments) { e in Text(e.extras["team"] == nil ? e.name : "\(e.name) (team)").tag(e.id) }
            }
        } label: {
            HStack(spacing: 6) {
                EnvironmentDot(name: model.activeEnvironment?.name, size: compact ? 7 : 8)
                Text(model.activeEnvironment?.name ?? "No environment")
                    .font(Typo.geist(compact ? 12 : 13, .medium, relativeTo: .subheadline))
                    .foregroundStyle(compact ? Theme.textTab : Theme.text)
                    .lineLimit(1)
            }
            .fixedSize()
            .padding(.horizontal, compact ? 9 : 12)
            .frame(minHeight: compact ? 24 : 36)
            .background {
                if compact { RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Theme.fill) }
            }
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .accessibilityLabel("Environment: \(model.activeEnvironment?.name ?? "none")")
        .accessibilityIdentifier("environment-menu")
    }
}

struct CollectionsScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(Navigator.self) private var nav
    @Environment(LibrarySync.self) private var sync
    @Environment(SharedSync.self) private var shared
    @Environment(HitopCloud.self) private var cloud
    @State private var sharing: ShareTarget?
    @State private var filter: Filter = .all
    @State private var expanded: Set<UInt64> = []
    @State private var openFolders: Set<String> = []
    @State private var naming: NameTarget?
    @State private var moving: UInt64?
    @State private var deleting: NameTarget?
    @State private var didSetUp = false

    enum Filter: String, CaseIterable { case all = "All", pinned = "Pinned", recent = "Recent" }

    var body: some View {
        List {
            Section {
                Button { nav.tab = .search } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "magnifyingglass").foregroundStyle(Theme.textMuted)
                        Text("Requests, collections, history").font(Typo.body).foregroundStyle(Theme.textMuted)
                        Spacer()
                    }
                    .padding(.horizontal, 12)
                    .frame(height: 38)
                    .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.white.opacity(0.07)))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Search")
                .listRowInsets(EdgeInsets(top: 0, leading: 20, bottom: 0, trailing: 20))
                HStack(spacing: 8) {
                    ForEach(Filter.allCases, id: \.self) { f in
                        FilterChip(title: f.rawValue, selected: filter == f) { filter = f }
                    }
                    Spacer()
                }
                .listRowInsets(EdgeInsets(top: 0, leading: 20, bottom: 0, trailing: 20))
            }
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)

            if let notice = sync.notices.first {
                Section {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: "arrow.triangle.2.circlepath.icloud").foregroundStyle(Theme.accent)
                        Text(notice).font(Typo.caption).foregroundStyle(Theme.textSoft).accessibilityIdentifier("sync-notice")
                        Spacer(minLength: 0)
                        Button { sync.dismissNotice(notice) } label: { Image(systemName: "xmark").font(.system(size: 12, weight: .bold)) }
                            .buttonStyle(.plain).foregroundStyle(Theme.textMuted).frame(width: 44, height: 44)
                            .accessibilityLabel("Dismiss")
                    }
                }
                .listRowBackground(Theme.surface)
            }

            if let notice = shared.notices.first {
                Section {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: "person.2").foregroundStyle(Theme.accent)
                        NoticeText(notice: notice).accessibilityIdentifier("shared-notice")
                        Spacer(minLength: 0)
                        Button { shared.dismissNotice(notice) } label: { Image(systemName: "xmark").font(.system(size: 12, weight: .bold)) }
                            .buttonStyle(.plain).foregroundStyle(Theme.textMuted).frame(width: 44, height: 44)
                            .accessibilityLabel("Dismiss")
                    }
                }
                .listRowBackground(Theme.surface)
            }

            switch filter {
            case .all:
                ForEach(model.visibleCollections) { c in collectionSection(c) }
            case .pinned:
                flatSection("Pinned", pinned, empty: "Pin a request or collection with a swipe or a long press.")
            case .recent:
                flatSection("Sent recently", recent, empty: "Requests you send show up here.")
            }
        }
        .listStyle(.insetGrouped)
        .listSectionSpacing(12)
        .environment(\.defaultMinListRowHeight, 26)
        .scrollContentBackground(.hidden)
        .background(ScreenBackground())
        .refreshable {
            await sync.sync()
            _ = try? await cloud.loadOrgs()
            await shared.syncAll()
        }
        .sheet(item: $sharing) { t in ShareSheet(collectionID: t.id) }
        .navigationTitle("Collections")
        .toolbar {
            ToolbarItem(placement: .topBarLeading) { AccountPill() }
            ToolbarItem(placement: .topBarTrailing) { EnvironmentMenu() }
            ToolbarSpacer(.fixed, placement: .topBarTrailing)
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button("New request", systemImage: "plus") { newRequest(nil) }
                    Button("New collection", systemImage: "folder.badge.plus") { naming = .newCollection }
                } label: {
                    Image(systemName: "plus").font(.system(size: 17, weight: .semibold))
                } primaryAction: {
                    newRequest(nil)
                }
                .accessibilityLabel("New request")
                .accessibilityIdentifier("new-request")
            }
        }
        .modifier(LibraryActions(naming: $naming, moving: $moving, deleting: $deleting))
        .onAppear {
            guard !didSetUp else { return }
            didSetUp = true
            // The first collection starts open, with its first folders (as the drafts show).
            if let first = model.visibleCollections.first {
                expanded.insert(first.id)
                if let folder = first.allFolders.first(where: { f in first.requests.filter { $0.folder == f }.count > 2 }) {
                    openFolders.insert("\(first.id)/\(folder)")
                }
            }
        }
    }

    private var pinned: [SavedRequest] { model.state.pinnedRequests.compactMap { model.request($0) } }

    private var recent: [SavedRequest] {
        var seen = Set<UInt64>()
        return model.history.compactMap { h in
            guard !seen.contains(h.request.id), let r = model.request(h.request.id) else { return nil }
            seen.insert(r.id)
            return r
        }.prefix(20).map { $0 }
    }

    private func newRequest(_ collectionID: UInt64?, folder: String = "") {
        let r = model.newRequest(in: collectionID, folder: folder)
        if let c = model.collectionOf(request: r.id) { expanded.insert(c.id) }
        nav.collectionsPath = [.request(r.id)]
    }

    private func flatSection(_ title: String, _ requests: [SavedRequest], empty: String) -> some View {
        Section {
            if requests.isEmpty {
                Text(empty).font(Typo.caption).foregroundStyle(Theme.textMuted).frame(minHeight: 44)
            }
            ForEach(requests) { r in requestRow(r, indent: 16) }
        } header: {
            Text(title.uppercased()).font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
        }
        .listRowBackground(Theme.surface)
    }

    @ViewBuilder
    private func collectionSection(_ c: SavedCollection) -> some View {
        Section {
            collectionHeader(c)
            if expanded.contains(c.id) {
                ForEach(c.requests.filter { $0.folder.isEmpty }) { r in requestRow(r) }
                ForEach(c.allFolders, id: \.self) { folder in
                    if isVisible(folder, in: c) {
                        folderRow(folder, in: c)
                        if openFolders.contains("\(c.id)/\(folder)") {
                            ForEach(c.requests.filter { $0.folder == folder }) { r in requestRow(r, depth: depth(folder)) }
                        }
                    }
                }
            }
        }
        .listRowBackground(Theme.surface)
        .listRowSeparatorTint(Theme.separator)
    }

    private func depth(_ folder: String) -> Int { folder.split(separator: "/").count - 1 }

    /// A folder shows when every parent is open.
    private func isVisible(_ folder: String, in c: SavedCollection) -> Bool {
        var parts = folder.split(separator: "/").map(String.init)
        parts.removeLast()
        var path = ""
        for p in parts {
            path = path.isEmpty ? p : "\(path)/\(p)"
            if !openFolders.contains("\(c.id)/\(path)") { return false }
        }
        return true
    }

    private func collectionHeader(_ c: SavedCollection) -> some View {
        let open = expanded.contains(c.id)
        return Button {
            withAnimation(.snappy) { if open { expanded.remove(c.id) } else { expanded.insert(c.id) } }
        } label: {
            HStack(spacing: 12) {
                CollectionIcon(name: c.name)
                VStack(alignment: .leading, spacing: 2) {
                    Text(c.name).font(Typo.title).foregroundStyle(Theme.text)
                    if model.isShared(c.id) {
                        SharedSyncLine(collection: c)
                        SharedBadge(collectionID: c.id, name: c.name)
                    } else {
                        SyncLine(collection: c)
                    }
                }
                Spacer()
                if model.isPinned(collection: c.id) {
                    Image(systemName: "pin.fill").font(.system(size: 11)).foregroundStyle(Theme.accentText).accessibilityLabel("Pinned")
                }
                Image(systemName: "chevron.right")
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(Theme.chevron)
                    .rotationEffect(.degrees(open ? 90 : 0))
            }
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .listRowInsets(EdgeInsets(top: 8, leading: 16, bottom: 8, trailing: 16))
        .accessibilityLabel(model.isShared(c.id) ? "\(c.name), \(SharedSyncLine.describe(c, shared)), \(SharedBadge.describe(c.id, shared, model))" : "\(c.name), \(SyncLine.describe(c, sync))")
        .accessibilityValue(open ? "expanded" : "collapsed")
        .accessibilityHint(open ? "Collapses the collection" : "Shows its requests")
        .accessibilityIdentifier("collection-\(c.name)")
        .contextMenu {
            if !model.isReadOnly(c.id) {
                Button("New request", systemImage: "plus") { newRequest(c.id) }
                Button("New folder", systemImage: "folder.badge.plus") { naming = .newFolder(c.id, "") }
                Button("Rename", systemImage: "pencil") { naming = .collection(c.id) }
            }
            Button("Run collection", systemImage: "play.circle") { nav.run(collection: c.id) }
                .disabled(Sender.runnable(c).isEmpty)
            Button(model.isPinned(collection: c.id) ? "Unpin" : "Pin", systemImage: "pin") { model.togglePin(collection: c.id) }
            if cloud.signedIn {
                Button(model.isShared(c.id) ? "People and sharing" : "Share…", systemImage: "person.2") { sharing = ShareTarget(id: c.id) }
            }
            if cloud.signedIn && sync.isLocalOnly(c.id) && !model.isShared(c.id) {
                Button("Save to HITOP Cloud", systemImage: "icloud.and.arrow.up") { Task { await sync.upload(c.id) } }
            }
            Button("Delete", systemImage: "trash", role: .destructive) { deleting = .collection(c.id) }
        }
        .swipeActions(edge: .trailing) {
            Button("Delete", systemImage: "trash", role: .destructive) { deleting = .collection(c.id) }
            if !model.isReadOnly(c.id) {
                Button("Rename", systemImage: "pencil") { naming = .collection(c.id) }.tint(Theme.accentDeep)
            }
            if cloud.signedIn {
                Button(model.isShared(c.id) ? "People" : "Share", systemImage: "person.2") { sharing = ShareTarget(id: c.id) }.tint(Theme.info)
            }
        }
        .swipeActions(edge: .leading) {
            Button(model.isPinned(collection: c.id) ? "Unpin" : "Pin", systemImage: "pin") { model.togglePin(collection: c.id) }.tint(Theme.warning)
            if !Sender.runnable(c).isEmpty {
                Button("Run", systemImage: "play.fill") { nav.run(collection: c.id) }.tint(Theme.success)
            }
        }
    }

    private func folderRow(_ folder: String, in c: SavedCollection) -> some View {
        let key = "\(c.id)/\(folder)"
        let open = openFolders.contains(key)
        let count = c.requests.filter { $0.folder == folder || $0.folder.hasPrefix(folder + "/") }.count
        let name = String(folder.split(separator: "/").last ?? "")
        return Button {
            withAnimation(.snappy) { if open { openFolders.remove(key) } else { openFolders.insert(key) } }
        } label: {
            HStack(spacing: 6) {
                Text(name.uppercased()).font(Typo.label).tracking(0.9).foregroundStyle(Theme.textFaint)
                if !open { Text("\(count)").font(Typo.geist(11, .medium)).foregroundStyle(Theme.chevron) }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(Theme.chevron)
                    .rotationEffect(.degrees(open ? 90 : 0))
            }
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .listRowInsets(EdgeInsets(top: 0, leading: 62 + CGFloat(depth(folder)) * 14, bottom: 0, trailing: 16))
        .listRowSeparator(.hidden, edges: .bottom)
        .accessibilityLabel("Folder \(name), \(count) requests")
        .accessibilityValue(open ? "expanded" : "collapsed")
        .accessibilityIdentifier("folder-\(folder)")
        .contextMenu {
            Button("New request here", systemImage: "plus") { openFolders.insert(key); newRequest(c.id, folder: folder) }
            Button("New folder inside", systemImage: "folder.badge.plus") { openFolders.insert(key); naming = .newFolder(c.id, folder) }
            Button("Rename", systemImage: "pencil") { naming = .folder(c.id, folder) }
            Button("Delete", systemImage: "trash", role: .destructive) { deleting = .folder(c.id, folder) }
        }
    }

    private func requestRow(_ r: SavedRequest, depth: Int = 0, indent: CGFloat = 62) -> some View {
        NavigationLink(value: Route.request(r.id)) {
            HStack(spacing: 12) {
                MethodTag(method: methodLabel(r))
                Text(r.name).font(Typo.row).foregroundStyle(Theme.textRow).lineLimit(1)
                Spacer(minLength: 4)
                if model.isPinned(request: r.id) {
                    Image(systemName: "pin.fill").font(.system(size: 10)).foregroundStyle(Theme.accentText).accessibilityLabel("Pinned")
                }
            }
            .frame(minHeight: 44)
        }
        .listRowInsets(EdgeInsets(top: 0, leading: indent + CGFloat(depth) * 14, bottom: 0, trailing: 16))
        .accessibilityIdentifier("request-\(r.name)")
        .swipeActions(edge: .trailing) {
            Button("Delete", systemImage: "trash", role: .destructive) { model.deleteRequest(r.id) }
            Button("Move", systemImage: "folder") { moving = r.id }.tint(Theme.info)
            Button("Rename", systemImage: "pencil") { naming = .request(r.id) }.tint(Theme.accentDeep)
        }
        .swipeActions(edge: .leading) {
            Button(model.isPinned(request: r.id) ? "Unpin" : "Pin", systemImage: "pin") { model.togglePin(request: r.id) }.tint(Theme.warning)
        }
        .contextMenu {
            Button("Rename", systemImage: "pencil") { naming = .request(r.id) }
            Button("Duplicate", systemImage: "plus.square.on.square") { model.duplicateRequest(r.id) }
            Button("Move to…", systemImage: "folder") { moving = r.id }
            Button(model.isPinned(request: r.id) ? "Unpin" : "Pin", systemImage: "pin") { model.togglePin(request: r.id) }
            Button("Delete", systemImage: "trash", role: .destructive) { model.deleteRequest(r.id) }
        }
    }
}
