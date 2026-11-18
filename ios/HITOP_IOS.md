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
| I0 | Project: XcodeGen `ios/HITOP/project.yml` (bundle id of the live app, so it ships as an update), iOS 26 target, design tokens + glass components, Geist bundled (OFL), CI job on a macOS runner: build, unit tests, UI tests on the iOS 26 simulator, screenshots | done (main 5d79794, 2026-11-18) |
| I1 | Collections tab, request editor (params, headers, body, auth, scripts, tests), Send, response sheet (status, timing, body with folding, headers, tests, cookies), environments and variables (peek), history | done (main 5d79794, 2026-11-18) |
| I2 | HITOP Cloud sign-in (Apple, Google, GitHub) and personal library sync (collections, environments, globals), Account screen (devices, organization, plan usage) | done; sync server deployed 2026-11-18 (branch `ios-i2`, 2026-11-18; needs the sync-server deploy below) |
| I2b | Shared end-to-end encrypted collections (workspaces) with hitop-sync-crypto's rules through a Rust C-ABI library of hitop-core's sync plan + hitop-sync-crypto (`ios/HITOP/SyncFFI`); team environments | done (branch `ios-i2b`, 2026-11-18; no server change) |
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

## I2: what landed (2026-11-18, branch `ios-i2`)

CI: `.github/workflows/ios.yml`, green run https://github.com/bohdaq/hitop/actions/runs/44300568654 (commit f6ede7b; jobs `app`, `core` with the real-server end-to-end test, `hitopkit`; artifact `screenshots`: 06–11 are I2's). UI test fixes on the way: List cells below the fold need a scroll before XCUITest sees them, the rename alert's field is found through the alert, and the sync status carries its id on the text only.

**Server change (needs a sync-server deploy)**
- sync-server `oidc.rs` `return_to` now also takes exactly `hitop://sign-in` (no port, path, query, user info or
  fragment), and `hand_off` puts the one-time code in its query (`hitop://sign-in?sso=<code>`), as for the CLI's loopback
  listener. Additive: loopback and web origins are unchanged; tests in `tests/oauth.rs` (Google, GitHub, refusals),
  `tests/apple.rs`, `tests/sso.rs`. The code is useless without the starting app's verifier, so another app claiming
  the scheme gets nothing. Until the server is deployed the app falls back by itself (below).
- Sign in with Apple stays the web flow: `HITOP_SYNC_APPLE_APP_IDS` is only an audience for Apple's server-to-server
  notifications; no endpoint takes a native identity token, and adding one (with its own nonce) wasn't worth it for I2.

**Sign-in (Core `Cloud.swift` `HitopCloud`, UI `AccountView.swift` `SignInSheet`)**
- Continue with Apple / Google / GitHub, only the ones `GET /v1/auth/methods` offers. `POST /v1/auth/<kind>/start
  {challenge, returnTo: "hitop://sign-in"}` → the provider page in ASWebAuthenticationSession (callback scheme `hitop`,
  shares Safari's cookies) → `hitop://sign-in?sso=<code>` → `POST .../finish {code, verifier}`. A server that refuses the
  return address (400 naming returnTo, any server not yet deployed) gets a start without it and the paste step (its page
  shows the code), as the web app does. Verifier = 64 hex chars, challenge = base64url(SHA-256) (`CloudCrypto`, plain Swift
  so Core stays Linux-buildable). No mailed-code UI (the web app has none either); Core keeps `startEmailSignIn` for the
  end-to-end test. Company SSO is not in the sheet (I2b or later; the sheet says to use the web or desktop app).
- Server setting: "Your own server…" in the sheet, default https://sync.hitop.rws8.tech, https only (http for localhost).
- After sign-in (`completeSignIn`, as `sync.js`): `GET /v1/me`; the device kept from before (same account and server,
  its key still listed) is reused, otherwise a new X25519 key pair (CryptoKit, raw 32 bytes base64url = hitop-sync-crypto's
  encoding, so I2b can use it) is registered with `POST /v1/me/keys {name: "HITOP on iPhone", publicKey}`.
- Kept: `hitop-library/cloud.json` (server, account {server, email, userId, keyId, deviceName}, the device after sign-out,
  current organization, organizations); Keychain `ios/cloud/<host>` = {session, secret}. An account whose session is gone
  from the Keychain is signed out at launch.
- `X-Hitop-Device: <keyId>` on every request of the account's session (a native app has no CORS, so unlike the web app
  it doesn't wait for `/v1/me` `devices: true`).
- Device limit (402 `limit: devices` on `POST /v1/me/keys`): "Remove a device to sign in here", the account's devices
  with Remove (`DELETE /v1/me/keys/:id` with the new session; 409 `key_in_use` = "holds keys to shared collections"),
  then the sign-in again. Store mode: "This account allows N devices", no plan name, no link.
- Sign out: `POST /v1/auth/logout`, the session leaves the Keychain, the device key stays (signing in again = same
  device). A 401 during sync does the same and says to sign in again.
- Store mode everywhere: error texts go through `StoreText.withoutPlanLink` (the web app's `withoutPlanLink`),
  `upgradeUrl` is never read; no price, plan offer, upgrade or billing link (UI tests assert it on the Account screen and
  the device-limit step).

**Personal library sync (Core `LibrarySync.swift`, the web app's `library.js` item for item)**
- Items: `collection/<id>` (the web app's JSON; this app keeps every key it doesn't model, now also unknown auth types
  such as oauth2/awsv4/digest, and fields of an unexpected type, so an edit here never strips what the web app wrote),
  `environments/all` `{environments, activeEnvironmentId}`, `globals/all` `{variables, secrets}`. Team environments
  (`team`) aren't the library's.
- Secrets (judgement call, the web app's rule): values travel in the item's `secrets` (`{envId: {name: value}}`, globals
  `{name: value}`), never in `data` (secret variables are "" there). The server seals them under its host key or, without
  one, doesn't keep them (`secretsStored: false`, then they stay in the Keychain and the Account screen says so). Not
  sending them would wipe other devices' values: `library.rs` replaces the sealed blob on every PUT. On the device they
  stay in the Keychain only; an item that comes back without a value keeps the Keychain's.
- Pull on launch, on coming to the foreground, pull to refresh on Collections, Sync now on the Account screen; push 1 s
  after a save (any collection, environment, globals or active-environment change), only items whose fingerprint differs
  from what the library last took (`known`), each with `baseVersion`.
- 409: the server's item is kept and this device's collection stays as "<name> (this iPhone)" ("(this iPad)" on iPad), a
  new collection that syncs like any other; a notice on Collections and the Account screen. A delete meeting an edit
  brings theirs back; environments/globals take theirs.
- Tombstones: a pull removes a collection only when this device had it from the library; a local delete sends
  `DELETE ...?baseVersion=`.
- Offline first: the queue is the difference between the files and `known` (`hitop-library/library-sync.json`: account,
  cursor, known versions + fingerprints, localOnly, refused), so it survives a relaunch. A transport error, 5xx or 429
  stops the round and retries after 15 s; a 402 (storage full), 413 (over 5 MB) or other refusal leaves that item on the
  device, marked, until it changes.
- First link: this device's collections go up except an untouched sample (`Sample: JSONPlaceholder` as the web app
  adds it) and an empty My requests / Default; an empty local My requests goes away when the library brings one. Linking a
  different account later uploads nothing from here (those stay "on this iPhone"). A kept-local collection has "Save to
  HITOP Cloud" in its menu.
- Cards: "13 requests · synced 2 min ago", "waiting to sync", "saving…", "not in HITOP Cloud" or "on this iPhone" (also
  the card's VoiceOver label).

**Account screen (`AccountView`)** opens from the organization pill on Collections (the drafts' pill; on iPad the
person icon in the sidebar header). Signed out: what HITOP Cloud does + Sign in. Signed in: email, server, sync status +
Sync now, notices, Devices (right under the account), Organization (personal + teams from `GET /v1/orgs`; picking one filters Collections the way the web
app's `visibleCollections` does: a collection's `org`, none = personal; new collections get `org`), Plan & usage
(`GET /v1/orgs/:org/usage`: plan name and meters with the web app's store wording), Devices (`GET /v1/me/devices`: this
device, last use; rename, remove with force for one holding shared keys), Sign out, About HITOP (moved here from the pill).

**Tests**
- Unit (`UnitTests/CloudTests.swift`, `LibrarySyncTests.swift`, also on Linux through `ios/HITOP/scripts/core-tests.sh`):
  SHA-256 vectors and the challenge, codes from the return address, server addresses, store-mode text, methods, sign-in +
  device registration + `X-Hitop-Device`, sign-out keeping the device, the paste fallback, device limit (remove, key in
  use, retry), devices rename/remove, organizations + neutral meters, ended session; library: first-link rules, pull and
  push with unknown keys kept, tombstones both ways, conflict copy, delete-vs-edit, offline across a relaunch, 503 stop +
  402 refusal, 401, environments/globals with secrets apart, no host key, numeric environment ids, organizations; a
  round trip of a web-shaped collection (awsv4/digest/oauth2 auth, GraphQL variables object, WebSocket, examples, openapi).
  `FakeCloud` (Core) answers with sync-server's shapes and rules (deny-unknown bodies, verifier check, versions, cursor,
  409 `current`, tombstones, 402 body).
- End-to-end (`UnitTests/CloudEndToEndTests.swift`, macOS/Linux only, CI job `core`): builds `hitop-sync-server`, starts
  it (SQLite, mail to the log, a host key), signs in by mailed code, pushes from one device's Core, reads it back through
  `GET /v1/library`, a second device pulls it (secret included), a conflict copy, a tombstone, devices (this device via
  `X-Hitop-Device`), sign-out/in keeping the device, organizations + usage. Required in CI (`HITOP_E2E_REQUIRED`).
- UI (`UITests/CloudFlowTests.swift`, StubURLProtocol → `FakeCloud.shared` at cloud.hitop.test, `StubWebSignIn` approving at
  once): sign in (only offered providers), Account screen, synced cards, sign out back to "on this iPhone"; a conflict copy
  (`-UITestCloudConflict`: the fake changes the collection on its first update); the device-limit step
  (`-UITestCloudDeviceLimit`); no purchase words. Screenshots 06-account-signed-out, 07-sign-in, 08-account,
  09-synced-collections, 10-conflict-copy, 11-device-limit.

**Deferred to I2b and later**
- Shared end-to-end encrypted collections (workspaces, wraps, pins, invites) and team environments: I2b (done, below).
- Company SSO and passwords in the sign-in sheet; Sign in with Apple through the native sheet (needs a server endpoint
  for the identity token); the Settings → Privacy switch to keep this device out of the library; per-request version
  history; Git sync status.
- Environments aren't filtered by organization (the web app's "Available in every organization" switch).
- Not checked on a device: ASWebAuthenticationSession against the hosted server (needs the deploy), Keychain on hardware.

## I2b: what landed (2026-11-18, branch `ios-i2b`)
CI: `.github/workflows/ios.yml`, green run https://github.com/bohdaq/hitop/actions/runs/44359787869 (commit 1c4b8f3; earlier runs had jobs cancelled when no macOS runner came within 15 minutes, re-run) (jobs `app`, `core` with the CLI end-to-end test, `hitopkit`;
artifact `screenshots`: 12–16 are I2b's).

**Crypto approach: (b), the real Rust behind a C ABI** (`ios/HITOP/SyncFFI`, crate `hitop-sync-ffi`, MIT, own workspace
like hitop-wasm, lock seeded from hitop-wasm's). One function, `hitop_sync_call(op, argsJSON) -> {"ok"|"error"}`
(module `CHitopSync`, `include/hitop_sync.h`), a thin copy of hitop-wasm's `sync.rs` glue over hitop-sync-crypto
(format 2 wraps, `accept_wraps`, blobs, key commitments, shared environment values) and hitop-core (`sync::plan`,
`pushed`, `resolve`, `is_synced_path`, `format::files` `read_files`/`to_files`/`plan_sync`). Panics are caught at the
boundary. Why not a Swift port: App/Core must build and test on Linux (CryptoKit's HPKE doesn't exist there), XChaCha20
would be hand-written, and the plan needs hitop-core's YAML key-by-key merge and the folder format writer, which
HitopKit doesn't have (it only reads); a port would be a second implementation to keep in step forever. With the FFI
the app runs the same bytes as the CLI and the web app, and nothing cryptographic is written in Swift.
- Build: `ios/HITOP/scripts/build-sync-ffi.sh xcframework` (macOS, rustup targets `aarch64-apple-ios` +
  `aarch64-apple-ios-sim`, `SIM_ARCHS="arm64 x86_64"` for an Intel simulator slice) → `ios/HITOP/Vendor/HitopSyncFFI.xcframework`
  (libraries only, git-ignored), linked by project.yml; the module map comes through `SWIFT_INCLUDE_PATHS`
  (`SyncFFI/include`), so the app and its unit tests see one `CHitopSync`. `build-sync-ffi.sh host` builds the Linux/macOS
  static library that `scripts/core-tests.sh` links (a SwiftPM system library target). CI time: about 2 minutes for the
  two iOS slices, 1 for the host library. **Owner's Mac build (I6) needs Rust (rustup) and this script before
  `xcodegen generate`.** A dependency change in hitop-core or hitop-sync-crypto needs `cargo update -p <crate>` in
  SyncFFI too (CI builds `--locked`).
- Verified: `UnitTests/SyncCoreTests.swift` runs the published `vectors/v1.json` (blobs both ways with the vector nonces,
  AAD, every blob and wrap failure, format 1 wraps and info) and `v2.json` (format 2 info, open with the pinned sender,
  sender/head, all 10 wrap failures, key commitments) through the same `SyncCore` ops the app calls, in the simulator
  (vectors bundled as a folder resource) and on Linux/macOS (`HITOP_SYNC_VECTORS`); `accept_wraps` cases ported from
  hitop-sync-crypto's tests_v2.rs (pinned sender accepted and remembered, forged sender, untrusted sender with and without
  a fingerprint, invite fingerprint pins only the inviter, swapped key for a known epoch, hidden rotation/replay/missing
  key, legacy only with a notice and never once strict, changed key announced); I2's CryptoKit device keys are
  hitop-sync-crypto keys (same public key, wraps open both ways; macOS and simulator); blobs bound to workspace, blob,
  version; the plan (pull, conflict never pushed, resolve, key-by-key hitop.yaml merge, paths outside a folder skipped);
  a collection round-trips through its files unchanged and `.yml` keeps its extension. SyncFFI's own `cargo test`.

**Shared collections (Core `SharedSync.swift`, sync.js `SyncClient` item for item)**
- A shared collection is an ordinary collection in the app (editing, sending, history as before) plus a link to its
  workspace. Its folder files are kept in `hitop-library/shared/<ws>.json`; `localFiles` = those files with only
  `hitop.yaml` and `requests/**` replaced by `to_files` of the collection as edited here (through `plan_sync`, so a CLI
  user's `.yml` stays), so `environments/*`, `globals.yaml` and `comments/**` (which the app doesn't edit) are never
  deleted for everyone. `hitop-library/shared-sync.json`: pins (key id → public key), the `WorkspaceMemory` per workspace,
  links (role, organization), the plan state, shared-value versions. Workspace keys are never stored.
- Keys only through `accept_wraps`: this device's own key is pinned from its local secret (never from `/v1/me`), the
  member list is fetched only when a sender isn't pinned, new pins from an invite fingerprint are kept, notices shown.
  A refused key is a `KeyRefusal` (forged/keyChanged/rollback are "Security warning: …"); an untrusted sender with a
  fingerprint becomes the Account screen's trust prompt (`trustSender` re-checks the server's key still has that
  fingerprint and refuses a changed one). Owners upgrade format 1 wraps of trusted devices as sync.js does.
- Sync: pull on launch, foreground, pull to refresh; push 1 s after a change to a shared collection; `stale_epoch`
  re-reads the keys; 409 retries (5 rounds). Conflicts: Keep mine / Take theirs (hitop-core `resolve`) and, for a request
  file both sides still have, Keep both = theirs plus mine as "<name> (mine)" in `<file>-mine` (as the server's Git sync
  answers "both"), with "View both" showing mine, theirs and the base, as the web app's conflict view.
- Share: a new workspace (random id, epoch 1 wrapped for every device of the account, each trusted here first; other own
  devices are listed with their fingerprints to trust) in the collection's organization; strict memory. Invite by email
  (or code only) → `<token>.<this device's fingerprint>` with Copy/Send. Members with roles and every device's
  fingerprint and trust state (Trust after comparing); an accepted invite's devices are trusted then added (wraps for
  every epoch this device holds, shared environment values resealed). Remove (owner): next epoch's key for the remaining
  trusted devices in the same call, the removed devices unpinned, every file sealed again under the new epoch, shared
  environment values re-keyed (sync.js `rekeyShares`; values this device can't open are reported, not skipped silently).
  New devices of members get the key on `syncAll` (owners/editors).
- Join: Account → "Join with invite code…" → the trust prompt shows the code's fingerprint ("They match, accept"; a
  bare token says the key's sender will be shown instead) → `POST /v1/invites/accept`, memory strict with the expected
  sender → "Tell the owner this device's fingerprint" while polling `/v1/workspaces`; once added, the collection opens by
  itself. Workspaces the account is in but hasn't opened here are listed with "Open here".
- Viewer: AppModel refuses every change to a viewer's collection (save, rename, move in or out, duplicate, delete
  request, folders; a new request asked for there goes to My requests; not a move target); the editor shows "View only",
  no Save, fields disabled, Send works; sync only pulls.
- Never in the personal library: `LibrarySync` leaves linked collections out of every push, badge and pull (a collection
  shared from the library leaves it: its library item is deleted); a removed member's copy stays on the device marked
  "No longer shared" and still never goes to the library. Another account signing in marks the old account's links gone;
  a link syncs again once the account is a member again (re-added, or signed in again). As in the web app's
  library.js, sharing deletes the collection's library item and the library never changes a shared collection (a 409
  on that delete doesn't bring the library's copy back).
- Files other clients wrote keep their bytes: `localFiles` reads the stored files into the app's model and writes them
  back (`base`), and replaces a file only where the collection as it is now writes it differently, so opening a CLI
  user's id-less or `.yml` request rewrites nothing. A request's file now follows its name only when the name changes
  (AppModel `save`, also for unshared collections: before, saving renamed a hand-named file).
- Organizations: a shared collection shows in its workspace's team organization (the web app's `orgOf`), others with
  the personal ones; the card says "Shared with Acme" (or "Shared"), "View only", "1 conflict", and "N requests · synced
  …".
- Team environments: `GET /v1/orgs/:org/environments` of the current team organization on launch, foreground and org
  switch (`team: <org>`, secret names without values; each member's values stay in the Keychain; other organizations'
  are taken out of the pill); in the environment
  pill as "<name> (team)", read-only on the Environments screen except secret values, never in the personal library.
- This device's fingerprint is on the Account screen ("Others compare it before they share a collection's key").
- Store mode: no plan, price or upgrade link; server refusals (402 limits) go through `StoreText.withoutPlanLink`.

**No server change.** Every call is one the web app and the CLI already make (workspaces, wraps, members, invites,
blobs, env-shares, org environments).

**Tests**
- Unit (Linux and simulator): `SyncCoreTests` (above); `SharedSyncTests` with two phones on `FakeCloud`: share → invite
  code with the fingerprint → accept → an untrusted key refused, trusted, added → join (nothing pushed after joining, the
  server never sees the text) → edits both ways → conflict (keep both, keep mine) → remove → epoch 2, every envelope
  under it, the old key opens none, the removed phone gets 404 and keeps a "No longer shared" copy outside its library;
  never in the library (and leaves it); a hand-written `.yml` request keeps its bytes through a pull, a second sync and an
  edit; a 402 limit reads without the plan link; a re-added member syncs again; a library 409 never replaces a shared
  collection; only the current organization's team environments; viewer read-only; a bare invite → trust prompt → trustSender → join; damaged codes;
  a forged wrap refused as a security warning; team environments. `FakeCloud` now enforces the workspace rules: exact
  wraps for every member device, a format 2 wrap's sender = one of the caller's keys, roles (viewer 403, non-member 404),
  removal only with epoch + 1 and its wraps, `stale_epoch`, version + version-vector 409s, invites, deny-unknown bodies.
  `SharedScenarioTests` runs the UI tests' scenarios first.
- End-to-end (`UnitTests/SharedEndToEndTests.swift`, CI job `core`, which now also builds the hitop CLI as `HITOP_BIN`):
  the app's Core (Ada) and the hitop CLI (Grace) against the real sync server: share, invite with the fingerprint, CLI
  accept, trust + add, CLI link (key accepted from the code's device), nothing to push on either side, edits both ways,
  a hand-written id-less request from the CLI taken by the phone with nothing sent back and its bytes kept,
  a conflict on the CLI (theirs) and on the phone (mine), Grace's epoch-1 key (from her CLI's key file and wraps) opens
  none of the files after the removal (all at epoch 2), her CLI pull fails.
- UI (`UITests/SharedFlowTests.swift`, `SharedScenario`: Grace's phone runs in the app process against `FakeCloud.shared`
  with her device key fixed, so the code is known): join by code with the trust prompt (fingerprint checked) → opens by
  itself → "Shared with Acme" badge; a viewer's read-only editor; a conflict (the fake holds Grace's write until Ada
  writes) shown and resolved by "Take theirs"; the Share sheet's members. Screenshots 12-shared-collections,
  13-join-trust, 14-conflict, 15-share-members, 16-viewer.

**Judgement calls**
- "Keep both" is offered for request files only (hitop.yaml, environments and globals merge key by key, and a copy of
  them makes no sense), as the server's Git sync does; the web app's shared conflicts have Keep mine / Take theirs and
  "View both" — the app has all three.
- Shared collections live in the app's collection list (so the editor, runner and history need nothing new) instead of a
  separate store; the workspace's files are the source of truth for what the app doesn't model.
- A joined collection takes hitop.yaml's id; if a local, unshared collection already has that id (the owner's other
  device that still has the library copy), the shared one replaces it.
- The Share sheet is the one place for people, invites, removal and conflicts (the collection's context menu or swipe
  "People"/"Share…"); the Account screen has the fingerprint, Join and "Open here".
- The join sheet polls `/v1/workspaces` every second for two minutes after accepting (the web app waits for a reload).

**Deferred**
- Shared environment values aren't used in sends here yet (opened, resealed for new devices and re-keyed on removal, but
  not merged into variables); creating env shares; folder environments (`environments/*` of a shared collection) aren't
  shown or edited (kept intact); comments aren't shown (kept intact); runner credentials, audit log, alerts, role change
  UI (the Core has `setRole`), deleting a workspace, editing team environments (read and pick only).
- Company SSO, passwords and recovery phrases (I2 deferred list).
- Not checked on a device: the hosted server with real invites, Keychain on hardware.
