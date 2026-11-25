# State machines in hitop

A lifecycle (a request, a connection, a sync link, a subscription) is a state machine whether or not the
code says so. This file lists them, the rule new code follows, and what is still implicit. Audit:
2026-11-09 (sync-server, CLI/core/server, web app/portal/desktop).

## The rule

1. **One status, one place.** A lifecycle's state is one value (a Rust enum, a JS string from a fixed set,
   a DB column), not several booleans, Options or timestamps whose combinations have to agree. Derive the
   rest from it.
2. **Transitions say where they come from.** A write that moves a status names the state it expects
   (`UPDATE ... WHERE outcome = 'queued'`, `if !current() return`), so a late, duplicate or cancelled
   actor can't overwrite a newer state. Async work that outlives a click carries an identity (a token
   per connection or call, the link's repo/branch/path) and checks it before every write.
3. **Wire strings don't change.** Status strings in report.json v1, API JSON, DB columns, MCP results and
   event streams are a contract: new states are added, existing ones keep their spelling.
4. **Machines are visible to automation.** UI state is exposed as `data-state` (plus `aria-busy` while
   busy), never only as display text. CLI/MCP states go out as JSON fields.
5. **Tests cover the edges**: cancel during connect, a late callback, a concurrent writer, the
   omitted field. A happy-path test doesn't test a state machine.

## Made explicit (2026-11-09)

| Machine | Was | Now |
|---|---|---|
| Web app secret-reference masking (`store.refValues`) | one map cleared by every send/run; a send in another tab removed values a request in flight needed for masking, so its History entry kept the secret in clear | `store.refSeen`: every resolved value since load (memory only) masks history; e2e `secret-refs.cjs` "two tabs in flight" |
| Web app HTTP send per tab | send start kept `responseType`/`truncated`; a prepare failure after a Cancel showed "Cancelled"; deleting a request left its send/socket/call running | send start resets every response field (`NO_RESPONSE`); `deleteTabRequest` cancels like `closeTab`; response panel `data-state` = idle, sending, streaming, done, error, cancelled |
| Web app WebSocket (`store.sockets`) | callbacks keyed by tab: a cancelled connection's late `onOpen` set "open", its `onClose` closed a newer one; "connecting" only after references resolved (double connect) | connection `token`, every callback checks it; "connecting" from the click; chip `data-state` = connecting, open, closed |
| Web app gRPC (`store.grpcTabs`) | no cancelled state: the chip showed the previous call's result; late events of a cancelled call were logged | call `token`; `Cancelled` log entry with `cancelled: true`; chip `data-state` = idle, running, ok, failed, cancelled; e2e `grpc.cjs` |
| Core WebSocket closing (`websocket.rs`) | `closing: bool` + a 5 s sleep re-armed on every frame: a server that kept talking and ignored Close hung `hitop ws`, the desktop and the bridge | `closing: Option<Instant>` deadline set once; test `close_wait_ends_while_the_server_keeps_talking` |
| Run report `file` (`runner.rs`, `run.rs`) | steps skip WebSocket/gRPC requests but files were mapped by position over all requests: every later step got the wrong file (report.json, JUnit) | `runner::is_run_step` decides both; test `report_files_skip_websocket_and_grpc_requests` |
| Mock servers and monitors plan gate (`mocks.rs`) | checked `entitlement.plan`, not the plan in force: an expired, refunded, paused, unpaid or incomplete org kept them | `plan_in_force()` like every other gate; test in `tests/mocks.rs` |
| Git sync link (`gitsync.rs`) | the sync's closing write (and `save_fail`) had no guard: a webhook `disconnected` or a relink during the GitHub calls was overwritten (old repo's head on the new link, a disconnected link revived) | `same_link(n)`: installation, repo, branch, path and `state <> 'disconnected'` in the WHERE |
| Alert deliveries (`alerts.rs`) | `UPDATE ... WHERE id` after the send | `AND outcome = 'queued'`: queued → sent, skipped, failed, or queued again |
| SCIM user `active` (`scim.rs`) | a PUT without `active` reactivated a deactivated user (and re-granted access) | only an explicit `active` moves it; test in `tests/scim.rs` |

## Still implicit (documented, not changed)

Ordered by risk. Each needs a decision or a wider change than a guard.

**Sync server**
- *Runs* (`runs.status`): `sealed` (no status) sits next to passed/failed.
- *Org invites* (portal + SCIM writers), *domain claims* (never re-verified).

**CLI and core**
- *Sync per path* (`hitop_core::sync::State`): `entries` + `conflicts` + `losers` maps and an empty-path
  sentinel for "delete our losing blob"; a loser-only sync prints nothing and counts loser deletes as local
  changes; a lost race clears an open conflict silently. No `--json` on `sync status|pull|push`.
- *Sync sign-in*: account.json + keychain + server session; the secret is written before the account;
  expiry is only a 401 hint; `--code` is a three-way Option with `""` as a value.
- *Run outcome*: `passed: bool` (report.json, MCP), passed/failed/sealed (sync rows), failed/recovered/missed
  (alerts), ''/passed/failed (monitors). Skipped requests aren't reported. MCP returns tool success for a
  failed run (agents read `passed`; changing it to `isError` is a contract change, not done).
- *Stream events*: CLI/MCP JSON key `event`, hitop-server bridges and desktop channels `type`; the gRPC
  bridge's time limit ends NDJSON without a `status` line; MCP `run_websocket` has no `open` event.
- *Telemetry*: `enabled: true, id: None` says "on" and never pings.

**Web app and portal**
- *Sync account page* (`views/sync-view.js`): its sign-in modes are gone (the Sign in dialog has them); a failed `loadTeam` offers Try again.

## Fixed since (C1, 2026-11-25)

- Billing: `plan_in_force` is an allow-list (`billing::LIVE`); the hosted database had only `active`.
- Alerts: a run that started before one already uploaded changes nothing; a start later than its upload doesn't count.
- Org SSO: "required" needs "enabled" (400), and turning it off clears "required".
- Workspace invites: accepting is guarded in the UPDATE. Git sync: `suspend` keeps the installation, `unsuspend` takes its links up again.
- hitop-server: a stalled proxy stream ends after `STREAM_FOR`; the WebSocket bridge's slot is given back on a panic; the gRPC
  bridge's time limit ends with a `status` line (4, DEADLINE_EXCEEDED).

## Fixed since (C3, 2026-11-25)

- Web app collection run: Cancel, one run at a time, `running` reset in a finally, an earlier run-all's summary cleared.
- OAuth 2 pop-up: Cancel closes it, each sign-in has its own window and ignores another's answer; the time limit doesn't run while a
  person signs in. The AI sheet already aborted on a new action, on close and on Escape (the note was out of date).
- App dialogs: an answer that comes after its dialog was closed (or another opened) doesn't write into it (`stillOpen`).
- Portal: buttons stay off on the way to a provider; `?server=` asks before signing a signed-in person out, and the sign-in page names
  a server that isn't HITOP Cloud. Cloud library: signing out during a push ends it instead of retrying.
