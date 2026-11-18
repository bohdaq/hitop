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
| I0 | Project: XcodeGen `ios/HITOP/project.yml` (bundle id of the live app, so it ships as an update), iOS 26 target, design tokens + glass components, Geist bundled (OFL), CI job on a macOS runner: build, unit tests, UI tests on the iOS 26 simulator, screenshots | done (branch `ios-i0-i1`) |
| I1 | Collections tab, request editor (params, headers, body, auth, scripts, tests), Send, response sheet (status, timing, body with folding, headers, tests, cookies), environments and variables (peek), history | done (branch `ios-i0-i1`) |
| I2 | HITOP Cloud sign-in and sync (library + shared collections via the sync API; key handling through hitop-sync-crypto rules, ported or via a Rust xcframework of hitop-wasm's core) | planned |
| I3 | Collection runner + run report, Live Activity, widgets, App Intents | planned |
| I4 | WebSocket, SSE, GraphQL schema, gRPC | planned |
| I5 | Apple Intelligence features, iPad three columns + keyboard, accessibility pass (VoiceOver, Dynamic Type, Reduce Transparency) | planned |
| I6 | App Store: screenshots, listing text, review notes, price → Free, submit (owner on a Mac) | planned |

Owner tasks: none until I6, apart from opening the CI-built simulator builds or TestFlight if wanted.
Agents can't run Xcode here (Linux bastion): every tranche is built and tested by the macOS CI job.

## I0 + I1: what landed (2026-11-18, branch `ios-i0-i1`)

CI: `.github/workflows/ios.yml`, green run https://github.com/bohdaq/hitop/actions/runs/44276302604 (commit 19d346d) (artifact `screenshots`: the five iPhone screens + iPad).

**Project (I0)**
- `ios/HITOP/project.yml` (XcodeGen 2.46.0, pinned by sha256 in CI): project `HITOPApp`, targets `HITOP` (app),
  `HITOPTests`, `HITOPUITests`; iOS/iPadOS 26.0, Swift 6 language mode, strict concurrency complete; HitopKit as a local
  package. Bundle id `hitop.rws8.tech.HiTop` (the live app's, from `ios/HiTop/HiTop.xcodeproj`), version 2.0.0 (1),
  display name HITOP, team XD7JHZ5DB3. Info.plist keeps the old app's ATS setting (arbitrary loads: an API client) and adds
  local networking + its usage text.
- Folders are `App/`, `UnitTests/`, `UITests/` and the project is `HITOPApp.xcodeproj` because macOS file systems are
  case-insensitive: `ios/HITOP` and `ios/HiTop` are one folder on a Mac, so nothing may be named `HiTop*` in it. The old
  app's files stay where they are (the old project still opens).
- Fonts: Geist and Geist Mono 1.9.2 (Regular, Medium, SemiBold, Bold / Regular, Medium, SemiBold) from the `geist` npm
  package, SIL OFL 1.1, `App/Resources/Fonts/OFL-Geist.txt`; listed in About → Acknowledgements (with the license text).
- Icon: the companion's 1024 AppIcon set (light, dark, tinted renders of `brand/png/logo-1024.png`).
- Design system (`App/UI/Theme.swift`, `Components.swift`): colour tokens of the drafts, Geist type styles that scale with
  Dynamic Type (`Font.custom(_:size:relativeTo:)`, nav bar titles through UIFontMetrics), real Liquid Glass (system glass
  tab bar with `Tab(role: .search)`, toolbars, `.buttonStyle(.glass)`, `.glassProminent` Send tinted with the accent,
  `GlassEffectContainer` for the Save + Send bar, the system's glass sheet for the response), method tag, variable chip,
  code view (JSON highlighting with `${var}` chips), status pill, timing bar, segment/capsule tabs. Every glass surface is
  a system one, so Reduce Transparency and Increase Contrast apply to it as the system does it; the app's own cards draw a
  stronger edge with Increase Contrast. No hand-made blur anywhere.
- `App/Core` is Foundation + HitopKit only (no SwiftUI), so it and its unit tests also build and run on Linux (a scratch
  SwiftPM package pointing at `App/Core` and `UnitTests`); `App/UI` is the SwiftUI layer.

**Core client (I1)**
- Tabs: Collections, History, Environments, Runs (placeholder until I3) + the search tab.
- Collections: collections with folders (stored `folders` + `file` paths like the web app); new request added at once as
  "Untitled" and named from its URL until saved under a name ("POST /v2/orders", web app FV1 rules, `AutoName`); rename,
  delete, move (sheet listing every collection/folder), duplicate, pin with swipe actions and context menus; folder rename,
  delete, new folder; filter chips All / Pinned / Recent.
- Request editor: method picker, URL with `${var}` chips (tap a chip to peek its value and scope; secrets masked with
  Show), Params (the URL's query + disabled rows), Headers, Body (JSON with validation, Format that keeps `${var}`, helper
  keys; Raw; Form/Multipart rows; GraphQL query + variables + operation name), Auth (from collection, none, bearer, basic,
  API key header/query), Tests and Pre-request scripts (JavaScriptCore + pm-shim.js via HitopKit). Unsaved state in the
  title area and the Save button; edits survive navigation (one editor per request while the app runs).
- Send: HitopKit `Runner` (variables, auth, GraphQL/form bodies, send rules, scripts, masking) on a URLSession client that
  also keeps headers in order, the raw body, cookies and URLSessionTaskMetrics. Script-set variables are kept per
  collection for the session (as the web app). WebSocket/gRPC requests show and edit but say they send in a later version.
- Response sheet (detents medium / 86 % / large): status, time, size, sent URL, timing bar (DNS, TLS + connect, waiting,
  download), Body (pretty JSON with folding: small objects on one line, long bodies fold top-level containers; Raw; image
  preview), Headers, Tests (pass/fail with details), Cookies. Errors are a headline + next step (`ExplainedError`, the web
  app's `explainError` rules + URLSession texts + missing variables) with the raw text under Details.
- Environments: list, pick the active one (also from the pill on Collections and the request card), add/rename/delete,
  variables with a Secret switch; secret values only in the Keychain (`ios/<env id>/<name>`, globals `ios/global/<name>`),
  shown masked; Globals.
- History: every send with its response (body ≤ 100 kB, tests), grouped by day; open as a new request in My requests.
- Storage: `Application Support/hitop-library/` = `collections/<id>.json` (`{formatVersion, updatedAt, collection}`, the
  web app's JSON model, unknown keys kept), `environments.json`, `globals.json`, `history.json` (100), `state.json`
  (active env, pins, legacy import flag). One file per collection so I2 sync can push/pull them one by one.
- Old app migration: on first launch `LegacyImport` reads the old app's `Application Support/hitop/collections.json`,
  `history.json` and `contexts.json` (script values become collection variables where unset) once; the old files are left
  untouched. The old app had no Keychain or UserDefaults data.
- iPad: NavigationSplitView with three columns (sidebar with collections + History/Environments/Runs, request, response);
  ⌘↩ send, ⌘S save, ⌘K search, ⌘N new request.
- No purchase UI or prices anywhere. VoiceOver labels and values on rows, chips, tabs and the timing bar; 44 pt targets;
  Dynamic Type through the type styles.

**Tests**
- Unit (`UnitTests/`): web-app JSON round trip with unknown keys, file store, history cap, legacy import (old shapes,
  once only, files kept), auto-name, `${var}` segments, explainError, JSON folding/format/validation, variable order,
  AppModel (new request + autoname + save, rename/move/pin/duplicate/delete, folders, collections, secrets only in the
  vault, environment switching, send + history + open as new, missing variable error, WebSocket refusal, params, format,
  search).
- UI (`UITests/`, launch argument `-UITestSeed` = shop-api sample + `StubURLProtocol` in the app process, no network):
  create a request, send, save, find it in History and open it as a new request; Create order → 201, JSON body, 4/4
  tests in the Tests tab (the sample's timing check is "Responds under 1 s", not the draft's 300 ms: cold CI runners
  are slower), cookies, switch to Production (other host), no environment → explained error; Environments tab
  switch + masked secrets; variable peek (tap the `${baseUrl}` chip → its value); screenshots.
- Keyboard shortcuts (⌘↩ send, ⌘S save, ⌘N new request, ⌘K search) are scene commands (a "Request" menu, so a focused
  sidebar list can't swallow ⌘↩; they also work on an iPhone with a keyboard) calling `Navigator.sendCurrent`,
  `saveCurrent`, `newRequest`, `showSearch`; `ShortcutTests` checks those actions on both layouts. Synthetic key presses
  (`typeKey`) on the CI iPad simulator dropped key commands at random over several runs, so there is no key-press UI
  test; check the keys on a device or a local simulator with a hardware keyboard.
- CI retries a failed UI test once (`-retry-tests-on-failure -test-iterations 2`): simulator app launches sometimes time out
  on the hosted runners. CI also runs HitopKit's `swift test` on macOS (JavaScriptCore). `ci.yml` (hitopkit on Linux, brand) runs only on main and
  pull requests, so it hasn't run on this branch; locally HitopKit's Linux tests pass with the CLI compatibility test
  skipped (no `hitop` binary built here).

**Screenshots against the drafts** (CI artifact `screenshots`, iPhone 17 Pro and iPad Air 11-inch (M3) simulators, Xcode 26.6)
- Collections: same header (identity pill, environment pill, + button), large Geist title, search field, filter chips,
  shop-api card with the gradient icon, folder labels and method-coloured rows, glass tab bar + separate search button.
- Request editor: same back/More glass buttons, title + breadcrumb, request card (method pill, environment chip, URL with
  `${baseUrl}` chip), five segment tabs with Body selected, JSON badge + Valid, highlighted body with the `${customerId}`
  chip, helper keys + Format, glass Save + accent Send at the bottom.
- Response sheet: the system glass sheet (the request shows through), status pill, time, size, timing bar with
  DNS/TLS/Waiting/Download, capsule tabs with Tests 4/4, folded JSON body. Differs: the sent URL line and Pretty/Raw + Copy are extra; small objects such as `total` fold instead of
  showing on one line when they don't fit the phone width; the tests list is the Tests tab, not under the body; Share
  instead of Explain (I5).
- History and Environments have no draft; they use the same cards, labels and type.
- iPad (11-inch, landscape 1180 × 820 like the draft): three tiled columns, sidebar with the HITOP header and ⌘K
  search, request column with the URL capsule and Send ⌘↩, response column with status, timing bar, 4/4 tests and the
  folded body. Differs: the sidebar lists every request flat per collection (folders only on iPhone) and keeps History,
  Environments and Runs below the collections; the response column has no Explain button (I5). CI captures the iPad
  with `XCUIScreen` (an app capture of a rotated iPad comes back clipped) and turns it upright with `sips`.

**Judgement calls**
- Method colours: the web app's (`--hitop-method-*`: GET #5EC79B, POST #F2B45C, PUT #6CA8FF, PATCH #C69CFF, DELETE #FF8A8A,
  other #A99CFF) as the plan says; the drafts' brighter shades only for WS #7CC7FF, SSE #F5D76E, GQL #FF8AD8 (no web token).
- The five iPhone screenshots are Collections, Request, Response, History and Environments: the drafts' Run and Apple
  Intelligence screens are I3/I5.
- Draft elements that need later tranches are left out rather than faked: organization switcher (shown as an About
  button), "synced 2 min ago" and the 13/13 run badge, Synced/Shared filter chips (All / Pinned / Recent instead),
  Explain with Apple Intelligence, the HTTP/2 label, "from OpenAPI createOrder".
- The response shows Body/Headers/Tests/Cookies as tabs; the draft shows the tests list under the body.
- `HitopKit`: public initialisers for `OutgoingRequest`, `HTTPResponse`, `HTTPError`, `Param`, and `StepResult` now
  carries the script-set variable/global updates (additive; the companion and the CLI compatibility test are unchanged).

**Deferred**
- Sync (I2), runs (I3), WebSocket/SSE/gRPC/GraphQL schema (I4), Apple Intelligence + full accessibility audit (I5).
- Import/export (Postman, OpenAPI, folders), code snippets, request examples, cookie jar across sends (each Send uses
  its own session, like a run), file fields in multipart bodies (text rows only), response search.
- Targets: rows, chips, tab buttons and toolbar buttons are 44 pt; the JSON fold rows are one line of code tall (the
  whole line toggles).
- Known nit: in the iPhone response sheet the size label can truncate ("285…") next to a wide status pill; give it
  priority over the spacer in I5.
- Not checked on a device: Keychain on hardware, the old app → new app upgrade with real data (unit-tested with fixture
  files of the old shapes), App Store submission (I6).
