#if canImport(UIKit) && canImport(SwiftUI)
import SwiftUI
import UIKit
import XCTest
@testable import HITOP

/// I5: every text colour token against every background it is drawn on, WCAG 2 AA (4.5:1 for
/// text), normal and with Increase Contrast. This is the contrast check: XCTest's audit samples
/// pixels under Liquid Glass and the accent glow, so it flags text that passes (AccessibilityTests).
@MainActor
final class ContrastTests: XCTestCase {
    static func luminance(_ c: UIColor) -> Double {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        c.getRed(&r, green: &g, blue: &b, alpha: &a)
        func lin(_ v: CGFloat) -> Double {
            let v = Double(v)
            return v <= 0.04045 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    }

    static func ratio(_ a: UIColor, _ b: UIColor) -> Double {
        let x = luminance(a), y = luminance(b)
        return (max(x, y) + 0.05) / (min(x, y) + 0.05)
    }

    func testTextTokensMeetWCAGAA() {
        let texts: [(String, Color)] = [
            ("text", Theme.text), ("textRow", Theme.textRow), ("textSoft", Theme.textSoft), ("textTab", Theme.textTab),
            ("textMuted", Theme.textMuted), ("textFaint", Theme.textFaint), ("accent", Theme.accent), ("accentText", Theme.accentText),
            ("success", Theme.success), ("warning", Theme.warning), ("danger", Theme.danger), ("info", Theme.info), ("violet", Theme.violet),
            ("codeText", Theme.codeText), ("codeKey", Theme.codeKey), ("codeString", Theme.codeString), ("codeNumber", Theme.codeNumber),
            ("codeLiteral", Theme.codeLiteral), ("codeMuted", Theme.codeMuted),
        ] + ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "WS", "SSE", "GQL", "GRPC"].map { ("method \($0)", Theme.method($0)) }
        let grounds: [(String, Color)] = [("ground", Theme.ground), ("surface", Theme.surface), ("surfaceRaised", Theme.surfaceRaised),
                                          ("codeGround", Theme.codeGround), ("sheetCode", Theme.sheetCode)]
        var failures: [String] = []
        for contrast in [UIAccessibilityContrast.normal, .high] {
            let traits = UITraitCollection(traitsFrom: [UITraitCollection(userInterfaceStyle: .dark), UITraitCollection(accessibilityContrast: contrast)])
            for (tn, t) in texts {
                for (gn, g) in grounds {
                    let r = Self.ratio(UIColor(t).resolvedColor(with: traits), UIColor(g).resolvedColor(with: traits))
                    if r < 4.5 { failures.append("\(tn) on \(gn) (\(contrast == .high ? "Increase Contrast" : "normal")): \(String(format: "%.2f", r)):1") }
                }
            }
        }
        // The Send label on the accent.
        XCTAssertGreaterThanOrEqual(Self.ratio(UIColor(Theme.onAccent), UIColor(Theme.accent)), 4.5)
        XCTAssertTrue(failures.isEmpty, failures.joined(separator: "\n"))
    }

    func testIncreaseContrastLiftsTheQuietTokens() {
        let normal = UITraitCollection(accessibilityContrast: .normal)
        let high = UITraitCollection(accessibilityContrast: .high)
        for c in [Theme.textMuted, Theme.textFaint, Theme.textTab, Theme.codeMuted] {
            XCTAssertGreaterThan(Self.luminance(UIColor(c).resolvedColor(with: high)), Self.luminance(UIColor(c).resolvedColor(with: normal)))
        }
    }
}
#endif
