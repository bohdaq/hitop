# HITOP for Android: progress

Instructions: `android/GOAL.md`. Update this file after every task.

| Tranche | Status |
| --- | --- |
| A0 Project, theme, hitop-ffi, CI | done (2026-11-23, PR #9; green run https://github.com/bohdaq/hitop/actions/runs/45088798691) |
| A1 Core client | in progress (branch `android-a1`) |
| A2 Cloud sign-in + library sync | not started |
| A3 Shared encrypted collections | not started |
| A4 Runs, notification, widgets | not started |
| A5 WebSocket, SSE, GraphQL, gRPC | not started |
| A6 AI, tablet, accessibility | not started |
| A7 Release | not started |

## A1 tasks

| Task | Status |
| --- | --- |
| hitop-core `runner::RunState` + `run_step` (additive; `run_collection_jar` calls it) and hitop-ffi `send` (one saved request as a run sends it, state carried by the app) | done |
| core: web-app JSON models (unknown keys kept), file storage, Library (collections, requests, folders, pins, environments, globals, secrets, history), sender, seed, stub HTTP proxy; JVM tests through the real engine | done |
| Secrets in Android Keystore (AES-GCM key, encrypted file) | |
| App shell: ViewModel, bottom navigation, dark system bars | |
| Collections screen (drafts' Main) | |
| Request editor (drafts' Request): method, URL with variable chips + peek, Params, Headers, Body, Auth, Scripts, Tests | |
| Response sheet (drafts' Response): status, timing bar, Body (folding JSON), Headers, Tests, Cookies | |
| Environments, globals, History | |
| UI tests on the seeded shop-api through the stub + screenshots | |

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
