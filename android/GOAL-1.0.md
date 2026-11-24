# HITOP for Android: the work before 1.0.0 (tranches B0–B7)

Owner's request (2026-11-24): implement everything in the "what's left" review, autonomously. All questions
were asked in advance; don't ask again. `GOAL.md`'s rules still hold except where the table below differs.
Read this with `PROGRESS.md` after every context reset.

## Owner's answers (2026-11-24)

| Question | Answer |
| --- | --- |
| Git | As before: a branch and draft PR per tranche, signed off, fast-forward main when green, delete the branch. No tags, no releases |
| 1.0.0 | Waits for all of B0–B7 |
| hitop-core | Additive optional APIs only; behaviour and format unchanged; no crate publish |
| Deploys | **Both allowed**: the sync server and the site, to production, after their tests pass (B7; nothing else on the VM) |
| Certificates | Two opt-in settings, off by default: trust this phone's user CAs; use the system proxy. API requests only, never HITOP Cloud |
| OAuth 2 authorization code | Loopback `http://127.0.0.1:<port>/callback` with PKCE, short-lived listener |
| Background sync | On when signed in: about every 6 h with network and battery not low, plus a retry of unsent changes; Settings switch |
| Scheduled runs | Yes, approximate (WorkManager, at least 15 min), notification on failure; no exact-alarm permission |
| Languages | English only; UI text moves to resources |
| Crashes | Local redacted report the person may share by hand; nothing sent automatically |
| Sharing and account | All: role changes and deleting a shared collection, team environments (edit, use in sends), comments, company SSO |
| Physical phones in CI | Skipped |

## Plan

| # | What | Done when |
| --- | --- | --- |
| B0 | Reliability: drafts survive process death, cancel a send, time limit, `RunService.onTimeout`, storage never overwrites an unreadable file | Tests for each |
| B1 | Import (Postman, OpenAPI with sync, HITOP JSON, folder zip), export, share target, multipart files, OpenAPI from URL | The vendored fixtures import as in the CLI; a file upload arrives byte for byte |
| B2 | Auth forms: OAuth 2 (all three grants), SigV4, Digest | Sends checked against local servers |
| B3 | Settings (time limit, cookies, library sync, background sync), user CAs, system proxy | HITOP Cloud unaffected; a private-CA server works only with the switch on |
| B4 | Response tools: compare, saved examples; undo after move and delete | UI tests |
| B5 | Background sync, scheduled runs, data files | Worker tests |
| B6 | Sharing and account: roles, delete, team environments, comments, company SSO | Real sync server end to end |
| B7 | R8, test helpers out of production, baseline profile, strings to resources, crash report; docs, Data safety; deploy server and site; readiness to tag | Release check green, deploys verified |
