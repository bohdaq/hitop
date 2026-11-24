# HITOP for Android: progress

Instructions: `android/GOAL.md`. Update this file after every task.

| Tranche | Status |
| --- | --- |
| A0 Project, theme, hitop-ffi, CI | done (2026-11-23, PR #9; green run https://github.com/bohdaq/hitop/actions/runs/45088798691) |
| A1 Core client | done (2026-11-23, PR #10; green run https://github.com/bohdaq/hitop/actions/runs/45112172270) |
| A2 Cloud sign-in + library sync | done (2026-11-23, PR #11; green run https://github.com/bohdaq/hitop/actions/runs/45117339257) |
| A3 Shared encrypted collections | done (2026-11-23, PR #11; green run https://github.com/bohdaq/hitop/actions/runs/45117339257) |
| A4 Runs, notification, widgets | done (2026-11-23, PR #11; green run https://github.com/bohdaq/hitop/actions/runs/45117339257) |
| A5 WebSocket, SSE, GraphQL, gRPC | done (2026-11-23, PR #11; green run https://github.com/bohdaq/hitop/actions/runs/45117339257) |
| A6 AI, tablet, accessibility | done (2026-11-23, PR #11; green run https://github.com/bohdaq/hitop/actions/runs/45117339257) |
| A7 Release | done (2026-11-23, PR #11; green run https://github.com/bohdaq/hitop/actions/runs/45117339257: signed release AAB validated); publishing on Google Play = Owner-needed (OWNER-NEEDED.md) |

## A7 tasks

| Task | Status |
| --- | --- |
| Upload-key signing from the environment (`HITOP_UPLOAD_KEYSTORE`…), AAB split by ABI, version code from CI | done |
| `scripts/build-release.sh` + `check-release.sh`: `bundletool validate`, signature, both ABIs, 16 KB in the APK; CI job App runs it with a throwaway key | done (local: signed AAB validates) |
| `android-play.yml`: manual, internal track only | done; first run = Owner-needed (secrets, Play app) |
| `release-android.yml`: signed APK + AAB on an owner tag `android-v*`, never Latest | done; first tag = Owner-needed |
| Store pictures at Play sizes (`StoreScreenshots`: phone, 7-inch, 10-inch, feature graphic) | done (`android/store/screenshots/`) |
| Listing texts, Data safety answers, app content, every console step: `android/PLAY_STORE.md` | done |
| Privacy policy: Android sections (site/src/pages/privacy.js) | done; site deploy = Owner-needed |

## A6 tasks

| Task | Status |
| --- | --- |
| On-device AI: Gemini Nano (ML Kit GenAI Prompt), Explain, suggested tests that pass, request from text; curl import without a model; hidden without a model | done (`AiFlowTest`, `IntelligenceTest`) |
| Tablet / foldable: two panes ≥ 600 dp, three ≥ 1200 dp | done (`LayoutTest`) |
| Keyboard shortcuts Ctrl+Enter / S / K / N | done (`LayoutTest`: Ctrl+Enter sends) |
| Accessibility: ATF checks on the emulators (`AccessibilityTest`), contrast unit test (`ContrastTest`), 2× font scale (`LayoutTest`) | done (ATF passes on API 31 and 37) |

## A5 tasks

| Task | Status |
| --- | --- |
| WebSocket (hitop-core client, saved messages, log), SSE live stream, GraphQL schema + completion, gRPC (reflection) | done (`ProtocolFlowTest`, `ProtocolsTest`) |
| End to end against `test_servers` (ws echo, Greeter, SSE) | done (`ProtocolEndToEndTest`, required in CI) |

## A4 tasks

| Task | Status |
| --- | --- |
| Runner over hitop-core `run_step`; report.json, JUnit, HTML = hitop-core's; equals `hitop run` | done (`RunsTest` with `HITOP_BIN`) |
| Run screen, report, share; `dataSync` foreground service with Stop and Open | done (`RunFlowTest`) |
| Glance widgets (Last run, Failing, Pinned + Send), Quick Settings tile, App shortcuts / deep links | done (`WidgetRenderTest` in glance-appwidget-testing) |

## A3 tasks

| Task | Status |
| --- | --- |
| Shared collections through SyncFFI (`SharedSync.kt`): share, invite with fingerprint, accept, trust, members, roles, removal + rotation, conflicts, team environments | done (UI: `SharedFlowTest` share, invite, view only; real server + CLI: `CloudEndToEndTest` invite, trust, edits, conflicts both ways, removal with a new key) |
| Published vectors on the device and the JVM (v1, v2, accept_wraps policy) | done (androidTest `VectorTest`, core `VectorTest`) |
| CLI round trip: phone shares, `hitop sync accept/pull/push` on the CLI side, both edit | done (`CloudEndToEndTest.aSharedCollectionWithTheCLI`, required in CI) |

## A2 tasks

| Task | Status |
| --- | --- |
| HITOP Cloud client (`HitopCloud`): GitHub, Apple, Google in a Custom Tab → `hitop://sign-in`; Google native via Credential Manager | done |
| Sync server: `POST /v1/auth/google/native/{start,finish}`, `googleNative` method, `HITOP_SYNC_GOOGLE_ANDROID_CLIENT_IDS` (+ test) | done (deploy = owner) |
| Library sync (`LibrarySync`), Account screen (devices, organizations, usage, fingerprint), Delete account | done (`CloudFlowTest`) |
| End to end against a real sync server | done (`CloudEndToEndTest`, required in CI) |

## A1 tasks

| Task | Status |
| --- | --- |
| hitop-core `runner::RunState` + `run_step` (additive; `run_collection_jar` calls it) and hitop-ffi `send` (one saved request as a run sends it, state carried by the app) | done |
| core: web-app JSON models (unknown keys kept), file storage, Library (collections, requests, folders, pins, environments, globals, secrets, history), sender, seed, stub HTTP proxy; JVM tests through the real engine | done |
| Secrets in Android Keystore (AES-GCM key, encrypted file) | done |
| App shell: ViewModel, bottom navigation, dark system bars | done |
| Collections screen (drafts' Main) | done |
| Request editor (drafts' Request): method, URL with variable chips + peek, Params, Headers, Body, Auth, Scripts, Tests | done |
| Response sheet (drafts' Response): status, timing bar, Body (folding JSON), Headers, Tests, Cookies | done |
| Environments, globals, History | done |
| UI tests on the seeded shop-api through the stub + screenshots (src/sharedTest: emulators in CI, Robolectric locally) | done (API 31 + API 37 test-device image) |

## A0 tasks

| Task | Status |
| --- | --- |
| Gradle project: version catalog, pinned wrapper, `core` (JVM) + `app` (Compose) modules | done |
| Theme from the drafts: colour tokens, Geist / Geist Mono (OFL, bundled), method colours, chips, pills, logo mark | done |
| `android/hitop-ffi`: JNI crate over hitop-core + SyncFFI, cargo-ndk arm64-v8a + x86_64, host build for JVM tests | done |
| Launcher icon: adaptive layers rendered by `brand/export.mjs` (checked by `brand/check.sh`) | done |
| `android.yml`: Rust tests + clippy, app build, JVM tests, licensee, lint, 16 KB check, UI tests on API 31 + 37, screenshots | done (one retry of the emulator start, never of failed tests) |
| hitop-ffi in `licenses.yml`; `url` advisory fixed in every lock | done |

Screenshot check: theme, Geist, chips and method colours as drafted (API 31). Left for A1: the API 37 run uploads no screenshot; dark 3-button navigation bar.

The design drafts are copied into `android/design/` (from the owner's canvas) so they survive context resets.

## Summary for the owner (2026-11-23)

HITOP for Android v1 is built: every tranche A0–A7 is done in code and green in CI
(`android.yml`, `ci.yml`, `licenses.yml` on main). What it is:

- **The app** (Kotlin + Compose, dark, Geist, the drafts' colours): collections, request editor,
  response, environments with Keystore secrets, history; HITOP Cloud (Google / GitHub / Apple
  sign-in, library sync, Account, Delete account); shared end-to-end encrypted collections and
  team environments; collection runs with a Stop notification, reports equal to `hitop run`,
  widgets, Quick Settings tile, shortcuts; WebSocket, SSE, GraphQL completion, gRPC; Gemini Nano
  (explain, suggested tests, request from text) and curl import; phone, foldable and tablet
  layouts; keyboard shortcuts; accessibility checks. No price or purchase text anywhere (tested).
- **One engine:** requests, scripts, runs and reports go through hitop-core in Rust (hitop-ffi),
  so the phone sends what the CLI sends.
- **Tests:** JVM tests (incl. a real sync server, the CLI and the protocol test servers, all
  required in CI), the same UI tests on emulators (API 31 and 37) and under Robolectric, the
  published crypto vectors on the device, and a signed release AAB checked on every run.
- **Server change (not deployed):** sync-server routes for native Google sign-in.

What only you can do, in order (details and every console field: `android/PLAY_STORE.md`;
checklist: `android/OWNER-NEEDED.md`): Play developer account and the 12-tester / 14-day closed
test; upload key + six GitHub secrets; Play Console app, listing (texts and pictures are ready),
Data safety and app content answers; Google Cloud Android OAuth clients; deploy the sync server
(+ `HITOP_SYNC_GOOGLE_ANDROID_CLIENT_IDS`) and the site (privacy policy); push `android-v1.0.0`
for the first AAB, upload it to internal testing once by hand, then use Actions → Android Play;
check widgets, sign-in and Gemini Nano on a real phone; promote tracks when ready.

Choices made without you, each reversible: `android/DECISIONS.md`.


## Work before 1.0.0 (`GOAL-1.0.md`, started 2026-11-24)

| Tranche | Status |
| --- | --- |
| B0 | in progress |
| B1 | not started |
| B2 | not started |
| B3 | not started |
| B4 | not started |
| B5 | not started |
| B6 | not started |
| B7 | not started |
