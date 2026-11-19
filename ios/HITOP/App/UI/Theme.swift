import SwiftUI
import UIKit

/// HITOP dark (ios/HITOP_IOS.md "Look"): graphite ground, surfaces, accent, logo gradient,
/// Geist / Geist Mono, method colours. Dark only, like the web app.
enum Theme {
    static let ground = Color(hex: 0x0B0B0E)
    static let groundDeep = Color(hex: 0x08080A)
    static let surface = Color(hex: 0x15151B)
    static let surfaceRaised = Color(hex: 0x23232C)
    static let codeGround = Color(hex: 0x111116)
    static let sheetCode = Color(hex: 0x0F0F13)
    static let accent = Color(hex: 0xA99CFF)
    static let accentDeep = Color(hex: 0x8A79F5)
    static let accentText = Color(hex: 0xC9C0FF)
    static let onAccent = Color(hex: 0x120F24)
    static let text = Color(hex: 0xEDEDF2)
    static let textRow = Color(hex: 0xE4E4EA)
    static let textSoft = Color(hex: 0xC7C7D1)
    // Increase Contrast lifts the quieter text and the hairlines (I5).
    static let textTab = Color(hex: 0xB4B4C0, high: 0xD6D6DE)
    static let textMuted = Color(hex: 0x9A9AA8, high: 0xC7C7D1)
    static let textFaint = Color(hex: 0x8D8D9B, high: 0xB4B4C0)
    static let chevron = Color(hex: 0x5C5C6A, high: 0x9A9AA8)
    static let hairline = Color.white.opacity(0.06)
    static let separator = Color(hex: 0xFFFFFF, opacity: 0.05, highOpacity: 0.22)
    static let fill = Color.white.opacity(0.06)
    static let fillStrong = Color.white.opacity(0.14)
    static let success = Color(hex: 0x5EE6A0)
    static let warning = Color(hex: 0xFFB86B)
    static let danger = Color(hex: 0xFF8A8A)
    static let info = Color(hex: 0x7CC7FF)
    static let violet = Color(hex: 0xC4A7FF)
    static var logoGradient: LinearGradient { LinearGradient(colors: [Color(hex: 0x667EEA), Color(hex: 0x764BA2)], startPoint: .topLeading, endPoint: .bottomTrailing) }
    static var sendGradient: LinearGradient { LinearGradient(colors: [accent, accentDeep], startPoint: .top, endPoint: .bottom) }

    // Code colours (drafts).
    static let codeText = Color(hex: 0xD6D6DE)
    static let codeKey = Color(hex: 0x9CC4FF)
    static let codeString = Color(hex: 0xF2C98A)
    static let codeNumber = Color(hex: 0xFF9F9F)
    static let codeLiteral = Color(hex: 0xC4A7FF)
    static let codeMuted = Color(hex: 0x6E6E7C, high: 0x9A9AA8)
    static let chipFill = accent.opacity(0.18)

    /// Method colours: the web app's (`site/src/theme.css` `--hitop-method-*`) for HTTP methods,
    /// the drafts' for the kinds the web app has no token for (WS, SSE, GQL).
    static func method(_ m: String) -> Color {
        switch m.uppercased() {
        case "GET": Color(hex: 0x5EC79B)
        case "POST": Color(hex: 0xF2B45C)
        case "PUT": Color(hex: 0x6CA8FF)
        case "PATCH": Color(hex: 0xC69CFF)
        case "DELETE": Color(hex: 0xFF8A8A)
        case "WS": Color(hex: 0x7CC7FF)
        case "SSE": Color(hex: 0xF5D76E)
        case "GQL": Color(hex: 0xFF8AD8)
        default: Color(hex: 0xA99CFF)
        }
    }

    static func status(_ code: Int) -> Color {
        switch code {
        case 200..<300: success
        case 300..<400: info
        case 400..<500: warning
        default: danger
        }
    }

    /// An environment's dot: from its name, so the same environment keeps its colour.
    static func environment(_ name: String?) -> Color {
        guard let name else { return textMuted }
        switch name.lowercased() {
        case let n where n.contains("prod"): return danger
        case let n where n.contains("stag"): return warning
        case let n where n.contains("local") || n.contains("dev"): return success
        default:
            let palette = [info, violet, success, warning]
            return palette[Int(derivedID(name) % UInt64(palette.count))]
        }
    }
}

extension Color {
    init(hex: UInt32, opacity: Double = 1) {
        self.init(.sRGB, red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255, opacity: opacity)
    }

    /// A colour that becomes `high` with Increase Contrast.
    init(hex: UInt32, high: UInt32) {
        self.init(uiColor: Self.contrasted(hex, 1, high, 1))
    }

    /// A translucent colour that becomes more opaque with Increase Contrast.
    init(hex: UInt32, opacity: Double, highOpacity: Double) {
        self.init(uiColor: Self.contrasted(hex, opacity, hex, highOpacity))
    }

    private static func contrasted(_ hex: UInt32, _ opacity: Double, _ high: UInt32, _ highOpacity: Double) -> UIColor {
        func ui(_ h: UInt32, _ a: Double) -> UIColor {
            UIColor(red: CGFloat((h >> 16) & 0xFF) / 255, green: CGFloat((h >> 8) & 0xFF) / 255, blue: CGFloat(h & 0xFF) / 255, alpha: CGFloat(a))
        }
        return UIColor { traits in traits.accessibilityContrast == .high ? ui(high, highOpacity) : ui(hex, opacity) }
    }
}

/// Geist and Geist Mono (bundled, SIL OFL 1.1), scaled with Dynamic Type.
enum Typo {
    enum Weight { case regular, medium, semibold, bold }

    static func geist(_ size: CGFloat, _ weight: Weight = .regular, relativeTo style: Font.TextStyle = .body) -> Font {
        let name = switch weight {
        case .regular: "Geist-Regular"
        case .medium: "Geist-Medium"
        case .semibold: "Geist-SemiBold"
        case .bold: "Geist-Bold"
        }
        return .custom(name, size: size, relativeTo: style)
    }

    static func mono(_ size: CGFloat, _ weight: Weight = .regular, relativeTo style: Font.TextStyle = .body) -> Font {
        let name = switch weight {
        case .regular: "GeistMono-Regular"
        case .medium: "GeistMono-Medium"
        case .semibold, .bold: "GeistMono-SemiBold"
        }
        return .custom(name, size: size, relativeTo: style)
    }

    static let largeTitle = geist(34, .bold, relativeTo: .largeTitle)
    static let title = geist(16, .semibold, relativeTo: .headline)
    static let body = geist(15, relativeTo: .body)
    static let row = geist(15, relativeTo: .body)
    static let caption = geist(12.5, relativeTo: .caption)
    static let label = geist(11, .semibold, relativeTo: .caption2)
    static let code = mono(13.5, relativeTo: .body)
    static let tag = mono(11.5, .semibold, relativeTo: .caption)
}
