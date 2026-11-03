# HITOP Cloud operations log

Every deploy, configuration change on the VM, access grant or removal, restore drill and incident,
newest last (change management and incident response policies, `policies/`). One entry each:
date (UTC), who, what, the commit, the checks run after it.

## 2026-11-02 C1 built, not deployed yet

- Who: agent. What: HITOP Cloud C1 on `main` (migration 4: organizations, plans, entitlements,
  usage, access tokens; `/v1/orgs`; plan limits behind `HITOP_SYNC_LIMITS=plans`; the portal in
  `cloud/portal/`; these policies). Not deployed: the deploy of the 1.2 sync server comes first
  (spec/CLOUD_SERVICE.md C1 "Needs").
- Deploy steps (for the entry below): spec/CLOUD_SERVICE.md section 9, C1 status.

## 2026-11-02 C1 deployed to the test server (14:42 UTC)

- Who: agent. Commit: 2049e95 (sync server and portal). Before it, the 1.2 sync server (c1d5e3c,
  no migration) and the 1.2.0 site and web app (b1233c3) were deployed the same day.
- Sync server: `install.sh` backed up `sync.db`, the binary and `/etc/hitop-sync/env` (`.bak-<ts>`
  next to each), added `HITOP_SYNC_LIMITS=plans` and `HITOP_SYNC_PORTAL_URL=https://hitop.rws8.tech/cloud/`
  (the `cloud.` host waits for DNS), restarted. Migration 4 applied: schema version 4.
- Checks: `over-limits` on a copy of the database: 0 over a limit. People (users with
  `runner_of = ''`) 1 = personal orgs 1. No error or panic in the journal. `/healthz` on
  sync.hitop.rws8.tech 200.
- Portal and site: `deploy/deploy.sh` from 2049e95. hitop.rws8.tech `/`, `/app/`, `/cloud/`,
  `/cloud/healthz` 200; rws8.tech and lit.rws8.tech 200.
- Not done: the `hitop-cloud` rws route and the `[acme]` name for cloud.hitop.rws8.tech wait for
  the owner's DNS record (C0).

## 2026-11-03 C2 built, not applied

- Who: agent. What: the C2 hardening on `main` (spec/CLOUD_SERVICE.md C2 status): `migrate-db`,
  `db-stats`, `dump`; Postgres setup, cutover and rollback scripts; sandboxed units and resource
  limits; disk/backup alerts; encrypted off-site backups and the restore drill; recovery phrases
  (migration 5); request rate limits; `cloud/RUNBOOK.md`; `cloud/status/`. Tested on CI (job
  `ops`, Postgres and LocalStack S3, ubuntu:24.04), nothing run on the VM.
- Apply: `cloud/RUNBOOK.md` "Apply C2", one entry here per step (with the cutover report).
- Waiting for the owner: bucket + write-only key + lifecycle rule, age key, uptime checker,
  `status.` DNS, write-freeze time, alert address.

## 2026-11-03 C2 applied to the test server, without backups and uptime (09:24–09:29 UTC)

- Who: agent, owner's go-ahead ("for now we won't do backups, also uptime, you can switch now").
  Server binary from c11a366 (`--features postgres`); alert script fix ALERT_SMTP_TLS_NAME on top.
- 1 Binary: `install.sh` backed up `sync.db.bak-20261103-092431`; `/healthz` schema 5; 0 over a limit.
- 2 Hardening: units sandboxed (exposure 1.4), MemoryMax 1 GiB, CPU 100 %, IO weight 50.
- 3 Alerts: `/etc/hitop-sync/alert.env` (0600) to bohdaq@gmail.com through the local submission
  port with the server's noreply account and `ALERT_SMTP_TLS_NAME=mail.rws8.tech` (the certificate
  name; the first test mail failed on it). One test mail sent; timer every 15 min.
- 4 Postgres 16, cluster `16/hitop`, socket only, port 5433, limits drop-in; the package's
  `16/main` stopped and disabled.
- 5 Cutover: report `/root/cutover-2026-11-03.txt`; write freeze 3 s; 10 rows in 20 tables,
  counts and content hashes equal; audit chain 1, 0 broken; backups
  `sync.db.pre-pg-20261103T092813Z`, `env.bak-20261103T092813Z`. `/healthz`
  `{"database":"postgres","ok":true,"schema":5}`.
- Checks: hitop-sync, hitop-server, postgresql@16-hitop, disk-alert timer active; rws8.tech,
  hitop.rws8.tech (`/`, `/cloud/`), lit.rws8.tech, mail.rws8.tech 200; `rws --check` OK (rws untouched).
- Not done (owner: later): 6 backups, 7 restore drill, 8 uptime checker and status page. Until
  backups run, the only copies are the SQLite snapshots above; rollback: `rollback-postgres.sh`.

## 2026-11-03 C1b built, not deployed (normal sign-in and sign-up)

- Who: agent. What: passwords (sign-up confirmed by a mailed code, sign-in, forgot/reset, change;
  the password unlocks the key bundle on a new device), "Continue with Google" and "Continue with
  GitHub", in the server, the CLI, the web app and desktop Sync page and the portal. Migration 6
  (additive: `password_credentials`, `password_tokens`, `server_secrets`; tested on SQLite and
  Postgres). Nothing applied on the VM.
- Deploy (when told): build `--release --locked --features postgres` (the test server runs on
  Postgres since this morning); a dump first (`hitop-sync-server dump /root/pre-c1b-<ts>`, the
  backup timer isn't on), then `install.sh` (RUNBOOK "Deploy a new sync server version");
  `/healthz` must say schema 6; then `deploy/deploy.sh` for the web app and the portal.
  Password sign-in works with no new setting (mail is already configured). Optional:
  `HITOP_SYNC_PASSWORD_PEPPER_FILE` (root-only file, 32 random bytes base64). Google/GitHub stay
  off until the owner registers the apps and their ids and secrets go into `/etc/hitop-sync/env`
  (`HITOP_SYNC_GOOGLE_CLIENT_ID/SECRET`, `HITOP_SYNC_GITHUB_CLIENT_ID/SECRET`; redirect URIs
  `https://sync.hitop.rws8.tech/v1/auth/google/callback` and `.../github/callback`).

## 2026-11-03 C1b deployed to the test server (10:55–11:05 UTC)

- Who: agent. What: 6ae47c9 (`--features postgres`). Dump first: `/root/pre-c1b-20261103T…Z`
  (`sync.pgdump`, 10 rows). A first try loaded `/etc/hitop-sync/env` with `. env`: the `&` in
  the Postgres URL cut it, so `dump` used the default `sqlite:hitop-sync.db` and wrote an empty
  dump; the stray files (`/home/ubuntu/hitop-sync.db*`, the empty dump) were removed. The
  service itself was never affected (systemd reads the file literally).
- `install.sh`: `/healthz` `{"database":"postgres","ok":true,"schema":6}`; 0 over a limit;
  `/v1/auth/methods` password on (signup, reset), Google and GitHub off (no client ids yet).
- `deploy/deploy.sh`: site, web app (Sync page "Forgot password"), portal; `/`, `/terms/`,
  `/cloud/`, `/cloud/healthz` 200; rws8.tech, lit.rws8.tech, mail.rws8.tech 200.

## 2026-11-03 C3 built, not deployed (Lemon Squeezy billing, pricing, legal drafts)

- Who: agent. What: billing in the sync server (`src/billing.rs`, migration 7: additive, SQLite
  and Postgres; `billing_events`, `trials`, three columns on `entitlements`), the portal's Plan
  page (buy, seats, trial, Manage billing, banner), the Sync page's Manage plan link, the pricing
  page, home and privacy copy, the security page and legal drafts (noindex, unlinked). Nothing
  applied on the VM; billing stays off there until the owner's store exists.
- Deploy (when told): a dump first, then the usual version deploy (`--features postgres`,
  RUNBOOK "Deploy a new sync server version"); `/healthz` must say schema 7 (8 if C4 ships in the
  same binary). With no `HITOP_SYNC_LS_*` set nothing changes for anyone (`/v1/orgs/<org>/billing`
  answers `enabled: false`, and the Team trial is off too: it opens with billing). Then `deploy/deploy.sh` for
  the site and the portal. Billing: RUNBOOK "Billing (Lemon Squeezy, C3)" once the owner's test
  store gives the store id, variant ids, API key and webhook secret.

## 2026-11-03 C3 and C4 deployed to the test server (13:08–13:15 UTC)

- Who: agent. What: acf2b84 (`--features postgres`; CI 41634478273 green). Dump first:
  `/root/pre-c3c4-20261103T130805Z` (postgres, 10 rows).
- `install.sh`: `/healthz` `{"database":"postgres","ok":true,"schema":8}` (migrations 7 and 8);
  0 over a limit. Billing off (no `HITOP_SYNC_LS_*`): `/v1/billing/lemonsqueezy` 404, no trial.
  install.sh's own health check ran before the restart finished and printed a connect error;
  the server was up a second later.
- `deploy/deploy.sh`: site (pricing with Free/$6/$10/$20, `/security.html` and `/legal/*` as
  noindex drafts), web app (run history, shared values), portal (Plan page, run history); `/`,
  `/pricing.html`, `/security.html`, `/cloud/` 200; rws8.tech, lit.rws8.tech, mail.rws8.tech 200.

## 2026-11-03 C5 deployed to the test server

- Who: agent. What: c7d43aa (`--features postgres`; CI 41657398691 green). Dump first:
  `/root/pre-c5-<ts>` (postgres, 10 rows).
- `install.sh`: `/healthz` schema 9; 0 over a limit. `HITOP_SYNC_HOST_KEY` and
  `HITOP_SYNC_DNS_URL` unset (Q15), so per-org SSO answers `org_sso_off`; `/v1/auth/methods`
  unchanged (`sso: null`). Business variants unset: not sold.
- `deploy/deploy.sh`: site (`/legal/support.html` draft), portal (Admin, Single sign-on, Audit
  pages); `/`, `/pricing.html`, `/cloud/` 200; rws8.tech, lit.rws8.tech, mail.rws8.tech 200;
  `rws --check` OK.

<!-- Next entry (template): record: commit, backup file names (sync.db.bak-<ts>), the row
counts before and after (users, workspaces, members; orgs with personal_of <> '' = users with
runner_of = ''), `hitop-sync-server over-limits` output (0 over a limit), curl of /healthz on sync
and on hitop.rws8.tech/cloud/healthz. -->

## 2026-11-03 C4 built, not deployed (run history, alerts, shared environment values)

- Who: agent. What: HITOP Cloud C4 on `main` (spec/CLOUD_SERVICE.md C4 status): run history from
  the customer's own runners (plaintext status rows and reports sealed with the workspace key),
  alerts (mail, webhook, Slack; missed runs), opt-in shared environment values sealed for the
  devices the sharer picks; hitop-sync-crypto 0.5.0; migration 8 (`runs`, `alert_settings`,
  `alert_state`, `alert_deliveries`, `env_shares`, `env_share_wraps`; tested on SQLite and
  Postgres). Nothing applied on the VM.
- Deploy (when told, with or after C3's migration 7): a dump (`hitop-sync-server dump
  /root/pre-c4-<ts>`), then `install.sh` with the `--features postgres` build (RUNBOOK "Deploy a
  new sync server version"); `/healthz` must say schema 8; then `deploy/deploy.sh` for the web app
  and the portal. No new setting needed; optional `HITOP_SYNC_MAX_RUN_BYTES`,
  `HITOP_SYNC_RUN_RETENTION_DAYS` (plans set retention on HITOP Cloud), `HITOP_SYNC_ALERTS_PER_HOUR`.
  After it: `hitop-sync-server db-stats` lists the new tables; `over-limits` still 0.

## 2026-11-03 C5 built, not deployed (SSO per org, audit export and anchors, admin console, Business)

- Who: agent. What: HITOP Cloud C5 on `main` (spec/CLOUD_SERVICE.md C5 status): 59dea46 (server,
  migration 9: `org_sso`, `org_domains`, `org_audit_settings`, `audit_anchors`,
  `workspaces.rotation_requested_at|_by`; tested on SQLite and Postgres), a87841e (CLI), 9cbed61
  (portal), then site drafts and docs. Nothing applied on the VM.
- Deploy (when told): a dump (`hitop-sync-server dump /root/pre-c5-<ts>`), then `install.sh` with
  the `--features postgres` build; `/healthz` must say schema 9; `deploy/deploy.sh` for the site
  (SLA and support drafts, noindex) and the portal. **Leave `HITOP_SYNC_HOST_KEY` and
  `HITOP_SYNC_DNS_URL` unset** (owner decision Q15: no SSO on the test server; every per-org SSO
  endpoint answers 404 `org_sso_off`). No other setting is required; Business stays unsold until
  `HITOP_SYNC_LS_VARIANT_BUSINESS_*` exist; daily anchors run only for Business orgs (none there).
  After it: `db-stats` lists the four new tables; `over-limits` still 0.
