import SwiftUI

@main
struct HITOPApp: App {
    var body: some Scene {
        WindowGroup {
            TabView {
                Tab("Collections", systemImage: "folder") {
                    VStack(spacing: 20) {
                        Text("Collections").font(.custom("Geist-Bold", size: 34))
                        Text("GET /v2/orders").font(.custom("GeistMono-SemiBold", size: 15))
                        Button("Send") {}.buttonStyle(.glassProminent)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color(red: 0.043, green: 0.043, blue: 0.055))
                }
                Tab(role: .search) { Text("Search") }
            }
            .preferredColorScheme(.dark)
        }
    }
}
