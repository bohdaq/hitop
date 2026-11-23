# HITOP for Android: decisions the agent made

Choices `android/GOAL.md` doesn't cover, each with its reason. The owner may overrule any of them.

| Date | Decision | Why |
| --- | --- | --- |
| 2026-11-23 | **The whole request pipeline runs in Rust** (hitop-core through hitop-ffi): prepare (variables, auth, GraphQL/form bodies, send rules), pre/post scripts, HTTP (reqwest + rustls: timings, cookies, redirects, digest, OAuth client credentials, SigV4), the runner and the reports. Kotlin keeps the UI, storage, the HITOP Cloud REST client and Android platform code. | There is no HitopKit for Kotlin; a port would be a second implementation of behaviour that must match hitop-core byte for byte. With the FFI the app sends exactly what the CLI sends. |
| 2026-11-23 | **Script engine: boa through hitop-ffi** (hitop-core `scripting`, the shared `pm-shim.js` it packages). | Follows from the row above (GOAL allows boa "if that is simpler"); no QuickJS binding to maintain, same engine as the CLI and the desktop. |
| 2026-11-23 | **hitop-ffi calls SyncFFI's operations** (`hitop-sync-ffi` as a path dependency, `call_json` for every op it doesn't own) instead of copying them. | One implementation of the sync plan, folder format, hitop-sync-crypto glue and gRPC ops for both apps. |
| 2026-11-23 | JNI: one `Native.call(byte[] op, byte[] args): byte[]`, UTF-8 bytes both ways; `jni` crate 0.21.1. | JNI strings are modified UTF-8 (NUL and emoji change); a unit test sends both. 0.21 is the widely used API (0.22 rewrote it) and is all this needs. |
| 2026-11-23 | compileSdk 37.2, targetSdk 37 (Android 17), minSdk 31. | GOAL: newest stable API; Compose 1.13 (BOM 2026.11.01) needs the 37.1+ platform. |
| 2026-11-23 | Toolchain: AGP 9.5.2 (built-in Kotlin), Kotlin 2.4.21, Gradle 9.9.2 (sha256-pinned wrapper), JDK 17, NDK r28c (28.2.13676358), cargo-ndk 4.1.2. | Newest stable releases on 2026-11-23; NDK r28 links 16 KB-aligned by default (CI checks it). |
| 2026-11-23 | Method colours are the Android drafts' (GET #4ADE80, POST #FBBF24, PUT #60A5FA, PATCH #C084FC, DELETE #F87171), not the web app's tokens iOS used. | The owner approved the Android drafts; GOAL says "method colors as in the drafts". |
| 2026-11-23 | Faint text is #8A869C instead of the drafts' #6F6B80. | #6F6B80 is about 3.6:1 on the code ground, under WCAG AA 4.5:1. |
| 2026-11-23 | Kotlin is split into `core` (plain JVM: models, storage, cloud, sync, runs) and `app` (Compose, Android). `core`'s tests run on the JVM against a host build of hitop-ffi. | Fast tests without an emulator, and the end-to-end tests against a real sync server run like iOS's `core` job. |
| 2026-11-23 | Instrumented UI tests run only in CI (`android.yml`, API 31 and 37 emulators). | The build machine has /dev/kvm but this user isn't in the kvm group, so no local emulator. |
| 2026-11-23 | TLS trusts the webpki roots only (as the CLI and the desktop): user-installed or corporate CAs and the system proxy aren't used. | hitop-core's client; same behaviour as every other HITOP client. Revisit if people ask. |
| 2026-11-23 | OAuth 2.0 authorization-code requests fail with hitop-core's message (it needs a browser on a loopback port); client credentials and password grants work. | hitop-core's `browser_sign_in` needs a terminal; iOS doesn't do it either. |
| 2026-11-23 | Gradle licence check: `app.cash.licensee` (Apache-2.0) on the app's runtime classpath, permissive SPDX ids only, exceptions named one by one with a reason. hitop-ffi joins `licenses.yml`'s cargo-deny matrix with the repo's deny.toml. | GOAL "Licences"; licensee fails the build on anything not allowed. |
| 2026-11-23 | `url` 2.5.8 → 2.5.9 in every Cargo.lock (RUSTSEC-2026-0592). | The advisory turned `licenses.yml` red on main the day A0 started; GOAL needs it green. |
| 2026-11-23 | `android.yml` runs on every push to main (no path filter, so a docs-only last commit runs it too), every push to `android-*` branches, and pull requests that touch android/, SyncFFI, hitop-core, hitop-sync-crypto, spec, brand. | GOAL: the workflow must be green on main. |
