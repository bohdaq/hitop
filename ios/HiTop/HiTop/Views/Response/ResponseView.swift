import SwiftUI

struct ResponseView: View {
    @Environment(AppViewModel.self) private var vm
    @State private var selectedTab = 0

    var body: some View {
        VStack(spacing: 0) {
            if vm.isLoading {
                ProgressView("Sending…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = vm.responseError {
                VStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle")
                        .font(.title)
                        .foregroundStyle(.red)
                    Text(error)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding()
            } else if let response = vm.response {
                VStack(spacing: 0) {
                    // Status bar
                    HStack(spacing: 12) {
                        StatusBadge(statusCode: response.statusCode)
                        Text("\(response.durationMs) ms")
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                        Spacer()
                        Picker("", selection: $selectedTab) {
                            Text("Body").tag(0)
                            Text("Headers").tag(1)
                        }
                        .pickerStyle(.segmented)
                        .frame(maxWidth: 160)
                    }
                    .padding(.horizontal)
                    .padding(.vertical, 8)
                    .background(.bar)

                    Divider()

                    if selectedTab == 0 {
                        ResponseBodyView(responseBody: response.body, contentType: response.contentType)
                    } else {
                        ResponseHeadersView(headers: response.headers)
                    }
                }
            } else {
                Text("Send a request to see the response")
                    .foregroundStyle(.tertiary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .frame(maxHeight: .infinity)
    }
}

struct StatusBadge: View {
    let statusCode: Int

    var body: some View {
        Text(String(statusCode))
            .font(.subheadline.monospaced().bold())
            .foregroundStyle(.white)
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(badgeColor, in: RoundedRectangle(cornerRadius: 6))
    }

    private var badgeColor: Color {
        switch statusCode {
        case 200..<300: return .green
        case 300..<400: return .orange
        case 400..<500: return .red
        case 500...:    return .purple
        default:        return .gray
        }
    }
}

struct ResponseBodyView: View {
    let responseBody: String
    let contentType: ResponseContentType

    private var displayBody: String {
        if contentType == .json,
           let data = responseBody.data(using: .utf8),
           let obj = try? JSONSerialization.jsonObject(with: data),
           let pretty = try? JSONSerialization.data(withJSONObject: obj, options: .prettyPrinted),
           let str = String(data: pretty, encoding: .utf8) {
            return str
        }
        return responseBody
    }

    var body: some View {
        ScrollView([.horizontal, .vertical]) {
            Text(displayBody)
                .font(.system(.caption, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
        }
    }
}

struct ResponseHeadersView: View {
    let headers: [String: String]

    var sortedHeaders: [(String, String)] {
        headers.sorted { $0.key < $1.key }
    }

    var body: some View {
        List(sortedHeaders, id: \.0) { key, value in
            VStack(alignment: .leading, spacing: 2) {
                Text(key)
                    .font(.caption.bold().monospaced())
                    .foregroundStyle(.secondary)
                Text(value)
                    .font(.caption.monospaced())
                    .textSelection(.enabled)
            }
        }
        .listStyle(.plain)
    }
}
