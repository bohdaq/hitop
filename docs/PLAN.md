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
| C1 | built (server side; the web and CLI items of STATE_MACHINES.md are in C3 and C4); CI pending |
| C2 | built: old iOS apps deleted (2.0.0 live since 2026-11-22, checked), the web e2e job split in five; CI pending |
| C3–C9 | not started |

## Decisions

| Date | Decision | Why |
| --- | --- | --- |
| 2026-11-25 | Core writes its JSON files in place with a journal next to them, not by rename. | The Microsoft Store build shares files with the CLI only while they are rewritten in place (CLAUDE.md MS1). |
| 2026-11-25 | A desktop keychain that fails marks its environments as damaged for the library. | Otherwise their empty secret values would be pushed. |
