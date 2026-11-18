import SwiftUI

struct RootView: View {
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(Navigator.self) private var nav

    var body: some View {
        @Bindable var nav = nav
        Group {
            if sizeClass == .regular {
                PadRoot()
            } else {
                PhoneRoot()
            }
        }
        .sheet(isPresented: $nav.showAbout) { AboutView() }
        .onAppear { nav.isPad = sizeClass == .regular }
        .onChange(of: sizeClass) { _, c in nav.isPad = c == .regular }
    }
}

/// The background of every screen: graphite with the accent glow of the drafts.
struct ScreenBackground: View {
    var glow = Theme.accent

    var body: some View {
        ZStack(alignment: .topLeading) {
            Theme.ground
            RadialGradient(colors: [Color(hex: 0x765DFF, opacity: 0.28), .clear], center: .center, startRadius: 0, endRadius: 210)
                .frame(width: 420, height: 320)
                .offset(x: -80, y: -120)
                .accessibilityHidden(true)
        }
        .ignoresSafeArea()
    }
}

struct PhoneRoot: View {
    @Environment(Navigator.self) private var nav

    var body: some View {
        @Bindable var nav = nav
        TabView(selection: $nav.tab) {
            Tab("Collections", systemImage: "folder", value: AppTab.collections) {
                NavigationStack(path: $nav.collectionsPath) {
                    CollectionsScreen()
                        .navigationDestination(for: Route.self) { RouteView(route: $0) }
                }
            }
            Tab("History", systemImage: "clock", value: AppTab.history) {
                NavigationStack(path: $nav.historyPath) {
                    HistoryScreen()
                        .navigationDestination(for: Route.self) { RouteView(route: $0) }
                }
            }
            Tab("Environments", systemImage: "globe", value: AppTab.environments) {
                NavigationStack(path: $nav.environmentsPath) {
                    EnvironmentsScreen()
                        .navigationDestination(for: Route.self) { RouteView(route: $0) }
                }
            }
            Tab("Runs", systemImage: "play", value: AppTab.runs) {
                NavigationStack { RunsScreen() }
            }
            Tab(value: AppTab.search, role: .search) {
                NavigationStack {
                    SearchScreen()
                        .navigationDestination(for: Route.self) { RouteView(route: $0) }
                }
                .searchable(text: $nav.searchText, prompt: "Requests, collections, history")
            }
        }
    }
}

struct RouteView: View {
    var route: Route

    var body: some View {
        switch route {
        case let .request(id): RequestScreen(id: id)
        case let .history(id): HistoryDetail(id: id)
        case let .environment(id): EnvironmentDetail(id: id)
        }
    }
}
