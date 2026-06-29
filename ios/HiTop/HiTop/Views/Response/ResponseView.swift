import SwiftUI

struct ResponseView: View {
    @Environment(AppViewModel.self) private var vm
    @State private var selectedTab = 0

    var body: some View {
        if vm.isLoading {
            VStack(spacing: 12) {
                ProgressView()
                    .controlSize(.large)
                Text("Sending request…")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let error = vm.responseError {
            VStack(spacing: 12) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.largeTitle)
                    .foregroundStyle(.red)
                Text(error)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
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
                    ResponseBodyView(responseText: response.body, contentType: response.contentType)
                } else {
                    ResponseHeadersView(headers: response.headers)
                }
            }
        } else {
            VStack(spacing: 12) {
                Image(systemName: "arrow.up.right.square")
                    .font(.largeTitle)
                    .foregroundStyle(.tertiary)
                Text("Send a request to see the response")
                    .font(.subheadline)
                    .foregroundStyle(.tertiary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
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
    let responseText: String
    let contentType: ResponseContentType
    @State private var copied = false

    private var displayText: String {
        if contentType == .json,
           let data = responseText.data(using: .utf8),
           let obj = try? JSONSerialization.jsonObject(with: data),
           let pretty = try? JSONSerialization.data(withJSONObject: obj, options: .prettyPrinted),
           let str = String(data: pretty, encoding: .utf8) {
            return str
        }
        return responseText
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(contentType.label)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    UIPasteboard.general.string = displayText
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { copied = false }
                } label: {
                    Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                        .font(.caption)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(.bar)

            Divider()

            ScrollView([.horizontal, .vertical]) {
                Text(displayText)
                    .font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(12)
            }
        }
    }
}

struct ResponseHeadersView: View {
    let headers: [String: String]

    var sortedHeaders: [(String, String)] {
        headers.sorted { $0.key.lowercased() < $1.key.lowercased() }
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

private extension ResponseContentType {
    var label: String {
        switch self {
        case .json: return "JSON"
        case .html: return "HTML"
        case .xml:  return "XML"
        case .text: return "Text"
        }
    }
}
