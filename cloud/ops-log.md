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

## 2026-11-03 Stripe billing, C6 and the owner's C5 decisions deployed to the test server (18:00 UTC)

- Who: agent. What: 7025f8d (`--features postgres`; CI 41680016900 green). Dump first:
  `/root/pre-c6-<ts>` (postgres, 12 rows).
- `install.sh`: `/healthz` schema 11 (migration 10 SCIM, 11 org-only device exclusion); the owner
  put the Stripe sandbox secret key and webhook signing secret in `/etc/hitop-sync/env`, so
  billing is on in test mode: unsigned `POST /v1/billing/stripe` 401. SCIM off until an org
  owner creates a token; per-org SSO still off (no host key).
- `deploy/deploy.sh`: site ("Sold through Link", "plus applicable tax"), portal; `/`,
  `/pricing.html`, `/cloud/`, `/cloud/healthz` 200; rws8.tech, lit.rws8.tech, mail.rws8.tech 200.
- Not done: the `hitop-cloud` rws route and the `cloud.hitop.rws8.tech` ACME name (DNS A
  records for `cloud.` and `status.` now point at the VM). The front rws config also serves mail
  and the other sites, so the owner applies it (deploy/README.md step 2–3, snippet in
  deploy/rws-routes.toml). `status.` waits for a decision: check.sh is meant to run off the VM.

## 2026-11-04 cloud.hitop.rws8.tech served (06:03 UTC)

- Who: agent, with the owner's permission for this one front rws change. What: the `hitop-cloud`
  route from deploy/rws-routes.toml (before the "Docs moved under /docs/" comment) and
  `"cloud.hitop.rws8.tech"` appended to `[acme] domains`. Backup
  `/etc/rws/rws.config.toml.bak-20261104T060321Z`; `rws --check` OK.
- Certificate: `cert.pem`/`key.pem` moved to `.bak-20261104T060321Z`, rws restarted; the new
  certificate (notBefore 2026-11-04 05:05 UTC) has 13 names including cloud.hitop.rws8.tech; mail
  TLS on :465 serves it.
- hitop-sync: `/etc/hitop-sync/env` backed up (`env.bak-20261104T060321Z`),
  `https://cloud.hitop.rws8.tech` appended to `HITOP_SYNC_ALLOWED_ORIGINS` and
  `HITOP_SYNC_RP_ORIGINS`, restarted: `/healthz` schema 11; CORS preflight from that origin 204.
- Checks: https://cloud.hitop.rws8.tech/ ("HITOP Cloud") and `/healthz` 200; rws8.tech,
  www.rws8.tech, hitop.rws8.tech, sync.hitop.rws8.tech/healthz, lit.rws8.tech, git.rws8.tech,
  mta-sts 200; php demo 302 to its login, then 200.
- Rollback: restore both `.bak-20261104T060321Z` files (config, cert/key) and restart rws.
- `HITOP_SYNC_PORTAL_URL` unchanged (the `/cloud/` path still answers).

## 2026-11-04 Portal sign-in redesign, Google and GitHub sign-in on

- Who: agent (portal), owner (deploy install step, OAuth apps, keys). What: 93fc033 (centered
  sign-in card, no sync server field; `?server=` for self-hosted), deployed with
  `deploy/deploy.sh`; cloud.hitop.rws8.tech, hitop.rws8.tech, `/app/`, rws8.tech, lit.rws8.tech 200.
- The owner made a GitHub OAuth app and a Google client (Auth Platform published, External) with
  callbacks `https://sync.hitop.rws8.tech/v1/auth/{github,google}/callback`, and put their ids and
  secrets in `/etc/hitop-sync/env`. `/v1/auth/methods`: github true, google true; both authorize
  URLs accepted by the providers (302 to their sign-in). The owner signed in with each.

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

## 2026-11-03 C3 billing moved to Stripe Managed Payments, not deployed

- Who: agent, on the owner's decision of 2026-11-03 (spec/CLOUD_SERVICE.md "Later decisions"):
  Stripe Managed Payments (Stripe is the merchant of record) replaces Lemon Squeezy, which was
  deployed with billing off and never sold anything. What: `sync-server/src/billing.rs` on Stripe
  (Checkout Sessions with `managed_payments[enabled]=true`, `POST /v1/billing/stripe`, prices by
  lookup key, pinned `Stripe-Version: 2026-10-28.endive`), the Lemon Squeezy route and code
  removed; no migration (migration 7's `ls_*` columns hold the Stripe ids); portal, site and docs
  say Stripe. Nothing applied on the VM.
- Deploy (when told): the usual `install.sh` with the `--features postgres` build; `/healthz` still
  says the same schema. No `HITOP_SYNC_STRIPE_*` set = billing stays off exactly as today
  (`/v1/billing/stripe` 404, no trial); the old `HITOP_SYNC_LS_*` lines in `/etc/hitop-sync/env`
  (if any) are ignored and can go. Billing on: RUNBOOK "Billing (Stripe Managed Payments, C3)".

## 2026-11-03 C6 built, not deployed (SCIM, support bundle, signed image, SOC 2 readiness)

- Who: agent. What: HITOP Cloud C6's code parts on `main` (spec/CLOUD_SERVICE.md C6 status):
  SCIM 2.0 per org (`/scim/v2`, tokens and group roles in the portal's SCIM page), migration 10
  (`scim_tokens`, `scim_users`, `scim_groups`, `scim_group_members`, `scim_group_workspaces`,
  `scim_grants`; tested on SQLite and Postgres), `hitop-sync-server support-bundle`,
  `sync-server/UPGRADE_NOTES.md`, the signed image workflow (`release-sync-server.yml`, runs on
  the next `cli-v*` tag; no tag pushed), two legal drafts (security questionnaire, enterprise
  agreement template) behind DRAFT, `cloud/soc2-type1-readiness.md`. The US region and own host
  are not built (Q18). Nothing applied on the VM.
- Deploy (when told): a dump (`hitop-sync-server dump /root/pre-c6-<ts>`), then `install.sh`
  with the `--features postgres` build; `/healthz` must say schema 10; `deploy/deploy.sh` for
  the site (two more drafts, noindex) and the portal (SCIM page). No new setting. After it:
  `db-stats` lists the six new tables (0 rows), `over-limits` still 0, and `support-bundle
  --out /root/bundle-<ts>.json` runs (read it; delete it after).

## 2026-11-04 Sandbox refund check; `billing-events` fixed on Postgres, not deployed

- Who: agent, with the owner's sandbox purchases (Pro on the personal org, Team on testCo, 1 → 3
  seats). The owner fully refunded the Pro charge; the portal still says "pro".
- Stripe (sandbox, read only): Pro and Team are on different customers; the webhook endpoint
  (`2026-10-28.endive`) is subscribed to all nine events incl. `charge.refunded`; the refund event
  was delivered with a 2xx (`pending_webhooks: 0`) and nothing for the Pro subscription came after
  it. So the customer-collision theory doesn't hold. A refund keeps `plan = pro` and sets
  `status = refunded` (Free limits) by design, so "pro" in the plan heading and org list is
  expected; the server's outcome is not confirmed yet.
- Found: `hitop-sync-server billing-events` failed on Postgres ("error serializing parameter 0":
  `$1 = 0` makes Postgres type `$1` int4, we bind an i64). Fixed in `billing::list_events` (no
  parameter), `tests/postgres.rs` now runs both forms; the whole workspace and the Postgres suite
  pass. Deploy: the usual `install.sh` with the `--features postgres` build; no schema change.

## 2026-11-04 f3f4038 deployed; the sandbox refund was applied

- Who: owner ran the commands, agent prepared them. What: `f3f4038` (`--features postgres` release
  build, sha256 163c0408aa707e1b…) via `install.sh`, after `hitop-sync-server dump
  /root/pre-deploy-<ts>`. Checks: `/healthz` ok, Postgres, schema 11; `/sync.db` 404;
  `over-limits` on the live database 0.
- `billing-events` now works on Postgres: 9 deliveries. `charge.refunded` (the Pro refund) is
  `applied` to `personal-0f3afdf2-…` / `sub_1UY1aI…`. The server was right all along: the org is
  `status = refunded` (Free limits) and keeps `plan = pro` by design, which the portal shows as
  "pro" in the org list and the plan heading. The first-purchase `invoice.paid` events are
  `superseded` (they arrive before the subscription is tied to the org; they change nothing there).
