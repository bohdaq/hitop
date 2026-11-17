# HITOP for iOS 26: plan

Owner decisions (2026-11-17). Design drafts: https://claude.ai/artifact/F9vMS5fAJBE2u1A2U8Xruv
(Collections, Request editor, Response sheet, Collection run, Apple Intelligence, iPad, Widgets/Live
Activity/Siri).

| Topic | Decision |
| --- | --- |
| What | Full API client, shipped as an update of the existing App Store app id6785326628 (today "HiTop HTTP Client", $2.99). Replaces `ios/HiTop`; built on `ios/HitopKit`. |
| Name and price | HITOP, free. HITOP Cloud account optional and free to create; paid plans are never sold or mentioned in the app (same "store mode" rule as the Mac App Store build). |
| Platform | iOS/iPadOS 26 minimum, SwiftUI only, Swift 6 strict concurrency, native Liquid Glass (glass tab bar, toolbars, sheets). Users on iOS 17–25 keep the last old version. |
| Look | HITOP dark: graphite ground #0B0B0E, surfaces #15151B, accent #A99CFF, logo gradient #667eea → #764ba2, Geist / Geist Mono, method colours as in the web app; glass chrome on top. Dark only, like the web app. |
| Navigation | Tab bar: Collections, History, Environments, Runs + a separate Search button (iOS 26 search tab). Send at the bottom of the request screen; the response opens as a sheet with detents. iPad: three columns (sidebar, request, response), keyboard shortcuts (⌘↩ send, ⌘K search, ⌘S save), pointer. |
| Data | HITOP Cloud sync: sign in with Apple, Google or GitHub (paste-free: ASWebAuthenticationSession); personal library and shared (end-to-end encrypted) collections, environments and globals; offline first, conflicts kept as copies like the web app. Secrets in the Keychain. No iCloud Drive folders in v1. |
| Protocols | HTTP (all methods, forms, files), GraphQL (schema introspection + completion), WebSocket (URLSessionWebSocketTask, message log), Server-Sent Events (streamed events view), gRPC (reflection, unary + server streaming). Scripts and tests via JavaScriptCore with the shared `pm-shim.js`. |
| System | Widgets (last run, failing collections, pinned requests with an interactive Send), Live Activity + Dynamic Island for collection runs, App Intents (Siri, Shortcuts, Spotlight, Action button, Control Center), Apple Intelligence via the on-device Foundation Models framework (explain a response, suggest a test, request from text/curl). |

## Tranches

| # | Scope | Status |
| --- | --- | --- |
| I0 | Project: XcodeGen `ios/HITOP/project.yml` (bundle id of the live app, so it ships as an update), iOS 26 target, design tokens + glass components, Geist bundled (OFL), CI job on a macOS runner: build, unit tests, UI tests on the iOS 26 simulator, screenshots | planned |
| I1 | Collections tab, request editor (params, headers, body, auth, scripts, tests), Send, response sheet (status, timing, body with folding, headers, tests, cookies), environments and variables (peek), history | planned |
| I2 | HITOP Cloud sign-in and sync (library + shared collections via the sync API; key handling through hitop-sync-crypto rules, ported or via a Rust xcframework of hitop-wasm's core) | planned |
| I3 | Collection runner + run report, Live Activity, widgets, App Intents | planned |
| I4 | WebSocket, SSE, GraphQL schema, gRPC | planned |
| I5 | Apple Intelligence features, iPad three columns + keyboard, accessibility pass (VoiceOver, Dynamic Type, Reduce Transparency) | planned |
| I6 | App Store: screenshots, listing text, review notes, price → Free, submit (owner on a Mac) | planned |

Owner tasks: none until I6, apart from opening the CI-built simulator builds or TestFlight if wanted.
Agents can't run Xcode here (Linux bastion): every tranche is built and tested by the macOS CI job.
