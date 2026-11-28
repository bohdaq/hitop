# HITOP: the work after Android 1.0 (tranches C0–C9)

Owner's request (2026-11-24): what was done for Android (B0–B7), for every product, autonomously. All questions were asked in
advance; don't ask again. Work only in `~/git/hitop-android`. CLAUDE.md's rules hold. Read this after every context reset.
Status is kept in the table at the end; choices made on the way go under "Decisions".

## Owner's answers (2026-11-24)

| Question | Answer |
| --- | --- |
| Products | All: CLI and core, web and desktop, iOS, cloud and servers |
| Git | Branch + draft PR per tranche, signed off, fast-forward main when every check is green, delete the branch. No tags, no releases |
| Order | Safety first (data loss, security), then features, then polish |
| Compatibility | Bug fixes may change clearly wrong behaviour, listed in CHANGELOG; otherwise additive. Collection format v1 stays compatible (spec section 7) |
| Big features | All: OpenAPI lint and editor; plugins (with a sandbox design); Insomnia, Bruno and HAR imports; gRPC client and bidirectional streaming |
| TLS options | All, including skipping verification: an explicit per-command `--insecure` with a warning, never stored in a collection |
| Languages | No translation work |
| Cloud | Published docs pages (opt-in, library collections) and hosted runs (changes the security model for shared collections: THREAT_MODEL first) |
| Deploys | All allowed after tests pass: sync server (database dump first, additive migrations), site, web app and portal, hitop-server |
| Production read | Only `SELECT DISTINCT status FROM entitlements` |
| Versions | Bump and CHANGELOG prepared; the owner tags and submits |
| iOS | Green CI is enough to land; phone-only checks go on the owner's list |
| Old code | iOS 2.0 is live (owner): delete `ios/HiTop` and `ios/HitopCompanion` and their CI; HitopKit stays |
| Refactoring | A dedicated tranche splits `webapp/src/store.js` and `views/hitop-app.js`, no behaviour change |
| CI | Faster with the same coverage: long e2e jobs split; no test dropped or skipped |
| Crash reports | Web, desktop, iOS: local and redacted, shared by hand only |

## Plan

| # | What |
| --- | --- |
| C0 | Data safety: damaged or unreadable files never overwritten, emptied or deleted from the cloud (iOS, core/CLI/desktop, web); atomic writes; unsaved iOS edits survive |
| C1 | Server and state fixes (spec/STATE_MACHINES.md): billing allow-list, alert order, invites, Git sync unsuspend, SSO flags, proxy stream limits |
| C2 | CI split; delete `ios/HiTop` and `ios/HitopCompanion` |
| C3 | Refactor store.js and hitop-app.js; web and desktop fixes (cancel a run, dialogs, OAuth pop-up, AI abort, desktop OAuth and live streams) |
| C4 | Core and CLI: TLS options, OAuth refresh, gRPC client/bidi streaming, MCP gRPC, sync `--json`; the same in desktop |
| C5 | Imports (Insomnia, Bruno, HAR) in core and every app |
| C6 | iOS parity with Android: import/export, share extension, auth forms, uploads, settings, background sync, schedules, data files, compare/undo, roles, comments, team environments, SSO |
| C7 | OpenAPI lint and editor; plugins (threat model first) |
| C8 | Published docs; hosted runs (threat model first) |
| C9 | Local crash reports; docs; versions and CHANGELOG; deploys; owner checklist |

## Status

| # | Status |
| --- | --- |
| C0 | on main 63864f9, every check green (PR 20): core and CLI storage journal, damaged files kept (core, web, desktop, iOS) and never deleted from the cloud, iOS drafts |
| C1 | built (server side; the web and CLI items of STATE_MACHINES.md are in C3 and C4); on main, checks green after one rerun of a flaky Android SSE test (PR 21; see Open problems) |
| C2 | built: old iOS apps deleted (2.0.0 live since 2026-11-22, checked), the web e2e job split in five; on main 85975cc, every check green (PR 22) |
| C3 | on main 193350a (PRs 23-27, every check green): run cancel, OAuth windows, desktop streams and OAuth, dialogs, portal (deployed), store.js and hitop-app.js split |
| C4 | done, last part on main edb2044 (PR 41): CLI TLS options on main a9c1a18 (PR 28); gRPC client/bidi streaming on main 5801dac (PR 29); MCP gRPC tools and sync --json on main 2dc2dc8 (PR 30); OAuth refresh in core, Android and desktop (PR 31), desktop network settings and WebSocket/gRPC TLS tests built (CI pending); gRPC messages one at a time in desktop, web (needs the newer hitop-server), iOS and Android built (CI pending) |
| C5 | core, CLI, web, desktop and Android on main 17ab888 (PR 33); iOS in C6 |
| C6 | on main dd7ff78 (PR 39, every check green on that commit; the run on main then failed one flaky Android UI test, see Open problems): import and export (PR 34), OAuth/SigV4/Digest (PR 35) on main; uploads, data files, compare/undo (PR 36) on main; settings and TLS (PR 37), share extension, background sync and schedules (PR 38), roles, comments, team environments, SSO built |
| Security | Web scripts isolated from the page (PR 43), the guide's script names everywhere and the Chrome extension's scripts in the worker (PR 44): on main fbb2ffc, web app deployed 2026-11-26 00:51 UTC; the extensions need the owner's release |
| C7 | OpenAPI lint and editor on main ee2ec6d (PR 42, every check green). Plugins, part 1 (spec 5c, core grammar and refusals in core, web and HitopKit, CLI `hitop plugin` with the sandboxed child, `value` hook): main 9cfc71f (PR 45). Part 2a, web app and extensions (plugins worker, Settings → Plugins, IndexedDB, one answer per call per request in core and web): main 791f33c (PR 46). Part 2b, desktop (hitop-core `plugin_host` shared with the CLI, the app re-executes itself as the child; MAS/MSIX untested here, owner checklist): main 653006d (PR 47). Part 3a, safety of the `value` hook (T1 warning in the editor, answers shown with the response, 8 MB input limit, the desktop binary tested as its own child in CI on Linux and macOS): main a854f63 (PR 48). Part 3b, the `viewer`, `importer` and `snippet` hooks in hitop-core (one validator, also in hitop-wasm) and the CLI (`--view`, `import --plugin`, `snippet --plugin`): main 9afa610 (PR 49). Part 3c, their web app and desktop screens (View with plugin, the Import dialog's "Read it with", plugin languages in the Code dialog; threat model decision 12): main a9cd3a5 (PR 53). Done. Also from C7's testing: Android arm64 scripts fixed (boa `jsvalue-enum`, main 405895c, PR 51); iOS share order and audit-test wait (main 943ef98, PR 52); iOS CI can run on the owner's MacBook (main 1c47969, PR 50) |
| C8 | Started 2026-11-28. Hosted runs: threat model only (`docs/HOSTED_RUNS_THREAT_MODEL.md`), for the owner to read; nothing that gives the server a workspace key is built before the owner answers it. Published docs pages: next |
| C9 | not started |

## Decisions

| Date | Decision | Why |
| --- | --- | --- |
| 2026-11-25 | Core writes its JSON files in place with a journal next to them, not by rename. | The Microsoft Store build shares files with the CLI only while they are rewritten in place (CLAUDE.md MS1). |
| 2026-11-25 | A desktop keychain that fails marks its environments as damaged for the library. | Otherwise their empty secret values would be pushed. |
| 2026-11-25 | The Mac App Store build doesn't get `com.apple.security.network.server`; OAuth sign-in there fails saying it can't listen. | A new entitlement changes App Review; the owner may add it. |
| 2026-11-25 | A stopped desktop stream ends in Rust with its next piece of data. | hitop-core's blocking client can't be interrupted between reads. |
| 2026-11-25 | `--insecure` is refused for gRPC. | tonic has no switch for it; a custom connector would replace its TLS. |
| 2026-11-25 | A streaming call's messages are JSON values one after another in `grpc.message`; stdin message by message only with `-i`. | Additive to format v1 (a string stays a string); `-d -` still resolves secret references. |
| 2026-11-25 | The iOS accessibility audit is tried up to 3 times when it did not finish (code -56). | A tool timeout says nothing about the screen; real issues and an audit that never finishes still fail. |
| 2026-11-25 | The desktop has no switch that skips certificate verification. | The owner allowed it per command only; a desktop setting would last, and its Settings page promises there is none. |
| 2026-11-25 | Desktop network settings hold file paths; a proxy address with a password is refused; unusable settings block requests. | No secret in a file; a request never goes around a proxy silently. |
| 2026-11-25 | Insomnia, Bruno and HAR are turned into Postman's shape and go through its importer. | One importer and report for every app. Bruno's newer YAML layout (opencollection) is not read. |
| 2026-11-25 | Bruno scripts are kept as comments; simple assertions become tests. | Its script API is not HITOP's; running it would fail every request. |
| 2026-11-25 | Main b8a2300 (PR 34) was merged with a failed iOS check, against the rule: the merge command did not stop on it. Merges now go through a script that asks GitHub for every check run. | The failure was the new import UI test; fixed with PR 35. |
- iOS schedules: BGTaskScheduler, so a run is "not before", often much later; a run known to take under 8 s also goes in the short refresh.
  A run iOS stops is not recorded and counts as started (its requests aren't sent again at once). It never runs with another environment
  or without secrets the Keychain can't show.
- iOS share extension only puts the item in the app group (an extension can't open its app); the app shows it before anything is added.
- A damaged list of shared collections is set aside only after every collection the library doesn't hold is marked "this device only".
| 2026-11-28 | The owner's MacBook is a build machine: standard user `hitopci` over a reverse SSH tunnel the owner opens, and the self-hosted runner `macbook-hitopci` for manual `ios.yml` runs (`runner: macbook`), whose job-started hook on the Mac accepts only manual ios.yml runs of bohdaq/hitop by bohdaq. | GitHub's macos-26 queue held a run for 17 h; the Mac also has arm64 Android emulators, which CI lacks. |
| 2026-11-28 | Repository setting: workflow runs from fork pull requests need approval for every outside contributor (was first-time contributors). | A self-hosted runner on a public repository; defence in depth with the runner's hook. |
| 2026-11-28 | A plugin importer's collections get new ids and lose `sharedIn`, `org` and `workspace` in the web app; only the HITOP branch of the file reader is applied. | Where a collection is kept is the app's choice (plugins decision 10, 12). |

## Open problems

- Android `ProtocolFlowTest` on API 31: `eventStreamsLiveAndStop` failed on main dd7ff78 (2026-11-25) with "Live · 0 events" above the three events
  it had received. Cause found: the old Compose test rule (v1) collects flows on the thread that emits, so state was written in the middle of a
  composition and a label was never redrawn. Reproduced locally (6 stale labels of 299 with v1, 0 of 299 with v2, which collects on the main thread
  as the app does); the UI tests now use the v2 rule. The earlier failures (the SSE test on PR 21; `grpcFieldsAndAnUnreachableServer` on main 8da867a,
  "Calling…" for more than 30 s) fit the same cause but were not reproduced themselves: if either comes back with v2, it is something else.
- Leaked copies of shared collections (web app before PR 40): checked on production 2026-11-25 with the owner's OK: `request_history` and
  `library_items` are empty, so nothing was deleted. Dumps taken before that may hold copies; they expire after 35 days. No notice to users
  (owner, 2026-11-25).
- Portal e2e (CI job "Site and web app (checks and portal)"): axe once reported color contrast on the SCIM settings page's submit button
  (PR 53, 2026-11-28); passed on a re-run and on main before and after. Likely measured mid-transition. To fix by waiting for the button's
  final state before the audit, not by retrying.
- CI has no arm64 Android emulator (its emulators are x86_64), which is how the boa NaN-boxing panic (PR 51) got past it. Find a runner that
  can run one (GitHub's arm64 Linux runners and KVM; or the owner's Mac on demand).
- The API 37.0 aosp_atd arm64 emulator doesn't finish booting on the owner's Mac (its threads hang); API 31 works there.

