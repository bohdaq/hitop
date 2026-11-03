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

<!-- Next entry (template): record: commit, backup file names (sync.db.bak-<ts>), the row
counts before and after (users, workspaces, members; orgs with personal_of <> '' = users with
runner_of = ''), `hitop-sync-server over-limits` output (0 over a limit), curl of /healthz on sync
and on hitop.rws8.tech/cloud/healthz. -->
