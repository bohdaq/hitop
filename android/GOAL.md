# Building HITOP for Android: instructions for the build agent

You are building the HITOP Android app on your own, tranche after tranche, without stopping to ask.
Every decision the owner has made is below. When you meet a choice that is not covered, take the
safest reversible option, write it to `android/DECISIONS.md` with the reason, and keep going. When a
task needs something only the owner can provide (an account, a key, a click in a console, a device
check), build and test it with a stand-in, add it to `android/OWNER-NEEDED.md`, and keep going.

Read this file, `android/PROGRESS.md`, `android/DECISIONS.md`, `android/OWNER-NEEDED.md` and the
repo's `CLAUDE.md` at the start and again after every context compaction. `PROGRESS.md` says where
you are.

## Sources

| What | Where |
| --- | --- |
| Designs (owner approved 2026-11-22) | https://claude.ai/artifact/C5UoVaUEAMvDvNrF94jetb: Collections, Request editor, Response sheet, Collection run, Widgets + run notification, Tablet / foldable |
| Feature reference: what v1 must do | iOS 2.0: `ios/HITOP_IOS.md` (tranches I0–I6, every "what landed" section) and `ios/HITOP/App/Core` |
| Behaviour that must match byte for byte | `cli/crates/hitop-core`, `cli/crates/hitop-sync-crypto`, `spec/` (collection format, sending, run report, vectors) |
| The iOS Rust bridge to copy the shape of | `ios/HITOP/SyncFFI` (one C call `hitop_sync_call(op, argsJSON)`, sync plan, crypto, gRPC) |
| Sign-in and sync server | `sync-server/` (oidc.rs `returnTo`, github.rs, apple.rs, library.rs, account.rs) |
| Store rules | `launch/app-store-free.md` (store mode: nothing is sold), `ios/APP_STORE.md` (listing, privacy answers) |

## Decisions already made (owner, 2026-11-22)

- **Scope:** parity with iOS 2.0. Collections, request editor (params, headers, body incl. form and
  GraphQL, auth, scripts, tests), Send and the response sheet, environments and variables, history,
  collection runs and reports, HTTP, GraphQL (schema + completion), WebSocket, Server-Sent Events,
  gRPC (reflection, unary + server streaming), HITOP Cloud sign-in and personal library sync, shared
  end-to-end encrypted collections and team environments, Delete account, widgets, run notification,
  tablet / foldable layout, on-device AI.
- **Stack:** Kotlin, Jetpack Compose, Material 3. hitop-core, hitop-sync-crypto and the gRPC client
  come from Rust over JNI: a crate `android/hitop-ffi` (MIT, own workspace like SyncFFI, lock seeded
  from SyncFFI's) with the same op-over-JSON shape, built with cargo-ndk for arm64-v8a and x86_64 (the
  emulator). No cryptography and no sync plan written in Kotlin. Scripts and tests run with the shared
  `shared/pm-shim.js` in a JS engine (QuickJS via a maintained Android binding, or Rust boa through the
  FFI if that is simpler; decide, write it down).
- **Minimum Android:** 12 (API 31). Target the newest stable API.
- **Package id:** `tech.rws8.hitop` (permanent). App name HITOP.
- **Look:** HITOP dark as drafted, dark only, fixed brand colors (no dynamic wallpaper color),
  Geist / Geist Mono (OFL, bundled), method colors as in the drafts and the web app.
- **Sign-in:** Google with the native account picker (Credential Manager), GitHub and Apple in a
  Custom Tab returning to the app. The server already accepts `hitop://sign-in` as a `returnTo` for iOS;
  reuse it if it works with an Android intent filter, else add the narrowest new return address and
  test it in sync-server. Native Google sign-in needs the server to accept the Android client's ID
  token: add it to sync-server with tests if missing.
- **AI:** Gemini Nano through ML Kit GenAI where the phone supports it, hidden elsewhere; same
  features and the same `ModelInput` rules as iOS I5 (secrets masked, bodies cut). Nothing leaves the
  device.
- **Store mode:** no price, purchase, upgrade or plan-buying text anywhere (same rule as iOS and the
  Mac App Store); a UI test checks every screen for it.
- **Distribution:** Google Play (free) and a signed APK on GitHub Releases. CI builds a signed AAB and
  uploads it to the Play **internal** track from a manual workflow (like `ios-testflight.yml`); the
  owner promotes to production. Releases and tags are the owner's.
- **Owner's devices:** the owner has an Android phone for checks CI can't do (widgets, notification,
  Gemini Nano, real sign-in).

## The plan

Work in this order. A tranche is done when its CI workflow is green on main and its screens match the
drafts. Break each tranche into tasks of a day or less in `PROGRESS.md` before starting it.

| # | What | Done when |
| --- | --- | --- |
| A0 | Project: Gradle (version catalog, pinned plugins), Compose + Material 3 theme from the drafts (tokens, Geist, method colors, components), `android/hitop-ffi` JNI crate built by cargo-ndk, CI workflow `android.yml` (build, unit tests, Rust tests, instrumented UI tests on an API 31 and a newest-API emulator, screenshots artifact), licence check for Gradle dependencies | An empty app with the theme and one FFI call passes CI on both emulators |
| A1 | Collections, request editor, Send, response sheet, environments and variables (peek), history; local storage in the web app's JSON shape, unknown keys kept; secrets in Android Keystore-backed storage | The shop-api sample (seeded) sends and shows responses in UI tests with a stub transport |
| A2 | HITOP Cloud sign-in (Google native, GitHub, Apple), personal library sync, Account screen (devices, organization, plan usage, Delete account) | End-to-end test against a real sync server in CI, like the iOS `core` job |
| A3 | Shared end-to-end encrypted collections and team environments through hitop-ffi | Published test vectors pass in instrumented tests; a collection shared with the CLI round-trips |
| A4 | Collection runner and reports, run notification (foreground service with Stop), widgets (Jetpack Glance: last run, failing, pinned with Send), Quick Settings tile, App Actions shortcuts | Run report equals HitopKit's / hitop-core's on the sample; widgets render in a test harness |
| A5 | WebSocket, SSE, GraphQL schema + completion, gRPC through hitop-ffi | End-to-end tests against `cargo run -p hitop --example test_servers` |
| A6 | On-device AI (Gemini Nano), tablet / foldable three panes, keyboard shortcuts, accessibility pass (TalkBack, font scale, contrast) | Accessibility checks pass in UI tests; layouts at phone, foldable and tablet widths |
| A7 | Release: signing (upload key from secrets), `android-play.yml` internal-track upload, signed APK in GitHub Releases on an owner tag, store listing + screenshots at Play sizes, Data safety answers, `android/PLAY_STORE.md` owner guide | A signed AAB passes `bundletool validate`; the owner guide lists every console step |

## Rules for every task

- **Git:** work on a branch per tranche (`android-a0`, ...). Open a draft PR so `ci.yml` and the
  licence checks run (they run only on PRs and main). When every workflow is green, fast-forward
  main, push, delete the branch. Commits are signed off (`Signed-off-by: Bohdan Tsap
  <bohdaq@gmail.com>`, the DCO check needs it) and never mention Claude, Claude Code or Anthropic.
- **Never:** push a tag, make a GitHub Release, upload to Google Play outside the manual workflow,
  promote a Play track, deploy the sync server or the site, or touch other projects. Server changes are
  built and tested here; their deploy goes to `OWNER-NEEDED.md`.
- **Don't break anything:** CLI, web app, desktop, extensions, iOS and the sync server keep their CI
  green. A change in hitop-core or hitop-sync-crypto needs `cargo update -p` in SyncFFI and hitop-ffi.
- **Tests first-class:** every feature gets unit tests and an instrumented UI test; flaky tests are
  fixed by waiting for a condition, never by a longer sleep.
- **Secrets:** never in the repo or logs. Signing keys, the Play service account and OAuth client ids
  come from GitHub secrets the owner adds (`OWNER-NEEDED.md` says which).
- **Licences:** shipped dependencies are permissive (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0 file
  level is fine as elsewhere in the repo); no GPL or AGPL. Fonts OFL.
- **Docs:** keep `PROGRESS.md` current after every task (done, commit, CI run link). Add an "Android"
  section to `CLAUDE.md` as the structure settles.
- **Disk:** the build machine is often near full. Keep Gradle and Cargo caches in one place, delete
  your worktrees and build output when a tranche is merged.

## When it is done

All of A0 to A7 are on main and green, `OWNER-NEEDED.md` lists exactly what the owner must do to
publish, and `PROGRESS.md` ends with a short summary for the owner.
