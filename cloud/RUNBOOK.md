# HITOP Cloud runbook

One operator, one shared VM (`bohdan-prod-donotdelete`, 168.119.17.57; mail, Gitea and other
sites run there too and are not ours). Every change here goes in `ops-log.md` (date UTC, who,
what, commit, checks). Policies: `policies/` (incident response, backup and recovery, change
management, access control). VM rules for agents: ROADMAP section 3.

Probes from the VM itself use `--resolve` (it can't reach its own public IP):
`curl -fsS --resolve sync.hitop.rws8.tech:443:127.0.0.1 https://sync.hitop.rws8.tech/healthz`.

| Thing | Where |
| --- | --- |
| Sync server | `hitop-sync.service`, 127.0.0.1:7881, binary `/usr/local/bin/hitop-sync-server`, env `/etc/hitop-sync/env` (root only), data `/var/lib/private/hitop-sync/` |
| Site, web app, proxy, portal | `hitop-server.service`, 127.0.0.1:7880, `/var/www/hitop` |
| Postgres (after C2) | cluster `<major>/hitop`, socket `/run/postgresql/.s.PGSQL.5433`, no TCP; URL in `/etc/hitop-sync/postgres.url` |
| Front proxy | rws, `/etc/rws/rws.config.toml` (`rws --check` needs `/etc/rws/admin.env`) |
| Backups (after C2) | `hitop-sync-backup.timer` hourly; settings `/etc/hitop-sync/backup.env`; last result `/var/lib/private/hitop-sync/backup-status.json` |
| Alerts (after C2) | `hitop-disk-alert.timer` every 15 min; settings `/etc/hitop-sync/alert.env`; `journalctl -t hitop-alert` |
| Status page | `cloud/status/` fed by `check.sh` off the VM; `status.hitop.rws8.tech` once the owner's DNS exists |

## Apply C2 (the hardening, in order; each step reversible)

Bundle on a workstation from `main`: `cargo build --release --locked --features postgres` in
`sync-server/` (Postgres support is needed from now on), then
`tar czf hitop-c2.tgz -C . cloud/ops sync-server/deploy deploy/hitop-server.service` plus the binary.
Copy both to the VM, unpack in a fresh directory, and as root:

1. **Binary** (still SQLite): `cp hitop-sync-server sync-server/deploy/ && sync-server/deploy/install.sh`
   (backs up sync.db, binary, env; migration 5 adds `recovery_blobs`). Check `/healthz` says
   `"schema":5`. Rollback: the `.bak-<ts>` binary back, `systemctl restart hitop-sync`
   (migration 5 is additive; an older binary ignores the table).
2. **Units and limits**: `cloud/ops/install.sh hardening`; it restarts each unit and puts it back
   by itself if it doesn't answer. Check `cloud/ops/install.sh status` (limits from
   `systemctl show`, exposure ≈ 1.4). Rollback: `cloud/ops/install.sh remove hardening`.
3. **Alerts**: write `/etc/hitop-sync/alert.env` (`ALERT_TO=<owner address>`; mail goes through
   127.0.0.1:587 with `ALERT_SMTP_USER`/`ALERT_SMTP_PASSWORD` of the `noreply` account, the same as
   `SMTP_URL`; `ALERT_SMTP_TLS_NAME=mail.rws8.tech`, the certificate's name), then `cloud/ops/install.sh disk-alert`. Rollback: `install.sh remove disk-alert`.
4. **Postgres**: `sync-server/deploy/postgres-setup.sh`, then `cloud/ops/install.sh pg-limits`.
   Nothing uses it yet. Rollback: `pg_dropcluster --stop <major> hitop` (and `apt purge postgresql`
   if the script installed it).
5. **Switch to Postgres** at the agreed write-freeze time (a minute or so):
   `REPORT=/root/cutover-<date>.txt sync-server/deploy/cutover-postgres.sh`. It stops by itself on
   any failed check and leaves the server on SQLite. Paste the report into `ops-log.md`. Rollback
   later: `sync-server/deploy/rollback-postgres.sh` (keeps writes made on Postgres) or
   `--snapshot /var/lib/private/hitop-sync/sync.db.pre-pg-<ts>` (the frozen file).
6. **Backups** (needs the owner's bucket and age key): `/etc/hitop-sync/backup.env` from
   `sync-server/deploy/backup.env.example` (0600), the owner's `age1...` public key in
   `/etc/hitop-sync/backup-recipients.txt`, `apt-get install -y age`, then
   `cloud/ops/install.sh backup` (runs one backup, enables the timer). Rollback:
   `install.sh remove backup`.
7. **First restore drill**, on the owner's computer (the private key never goes to the VM):
   `sync-server/deploy/restore-drill.sh --config backup.env --identity <key.txt>` with
   `BIN=<hitop-sync-server built with --features postgres>`; commit the evidence file it writes.
8. **Uptime and status page**: see below.

## Deploy a new sync server version

Build `cargo build --release --locked --features postgres` in `sync-server/` at the commit; copy
the binary and `sync-server/deploy/` to the VM; `sync-server/deploy/install.sh` (it backs up the
database, the binary and env, restarts, checks `/healthz` and `over-limits`). On Postgres, take a
backup first: `systemctl start hitop-sync-backup` (or `hitop-sync-server dump /root/pre-deploy-<ts>`). Give the commands the env file as systemd reads it, never with `. /etc/hitop-sync/env`: the `&` in the Postgres URL ends the line for the shell and the command falls back to an empty SQLite file. E.g. `sudo systemd-run --pipe --wait -p EnvironmentFile=/etc/hitop-sync/env -p WorkingDirectory=/root hitop-sync-server dump /root/pre-deploy-<ts>`.
Site, web app, portal: `deploy/deploy.sh` from the workstation.

## Roll back a deploy

`cp /usr/local/bin/hitop-sync-server.bak-<ts> /usr/local/bin/hitop-sync-server && systemctl restart
hitop-sync`. A migration is never undone by an old binary; migrations are additive, so the old
binary runs on the newer schema. If the new version wrote data the old one mishandles, restore
(next section). Env: `cp /etc/hitop-sync/env.bak-<ts> /etc/hitop-sync/env`.

## Restore from a backup

1. On the owner's computer: `restore-drill.sh --object <name> --identity <key>` proves the backup;
   then decrypt it: `age -d -i <key> <name> | tar -xf - -C restore/`.
2. Copy `restore/` to the VM (the decrypted data is customer ciphertext and account metadata:
   scp, then delete it from the workstation).
3. `systemctl stop hitop-sync`.
4. SQLite backup (`sync.db`): move the current `sync.db*` aside, put `sync.db` in
   `/var/lib/private/hitop-sync/`, `chown --reference=/var/lib/private/hitop-sync`.
   Postgres backup (`sync.pgdump`): `su postgres -c "dropdb -p 5433 hitop_sync && createdb -p
   5433 -O hitop_sync hitop_sync"`, then `su postgres -c "pg_restore -p 5433 --no-owner
   --role=hitop_sync -d hitop_sync restore/sync.pgdump"`.
5. `systemctl start hitop-sync`; `hitop-sync-server db-stats --json` (with `HITOP_SYNC_DATABASE`
   from the env file) must equal `restore/manifest.json`'s `stats.tables`; `audit-verify`.
6. Clients pull what they miss; writes made after the backup are on clients' devices and push
   again on their next sync (conflicts are resolved on the clients). Record it.

## Postgres down

`systemctl status postgresql@<major>-hitop`, `journalctl -u postgresql@<major>-hitop -n 100`.
Usual causes: disk full (below), out of memory (the drop-in's `MemoryMax`; raise it in
`/etc/systemd/system/postgresql@<major>-hitop.service.d/10-hitop-limits.conf`, `daemon-reload`,
restart), a crash (restart; Postgres recovers from its WAL). `hitop-sync` requires the cluster
(cutover drop-in), so restarting Postgres restarts it. If Postgres can't be brought back within the
hour: `rollback-postgres.sh --snapshot <the newest SQLite snapshot>` or restore the last backup
into SQLite (`migrate-db --from <restored Postgres> --to sqlite:...`), then say so on the status
page.

## Disk full

`df -h`; `du -xsh /var/* | sort -h | tail`. Ours: `/var/lib/private/hitop-sync` (old
`sync.db.bak-*`, `*.pre-pg-*`, `*.before-rollback-*`: keep the newest of each, delete older ones
after the next good backup), `/var/lib/postgresql/<major>/hitop` (`VACUUM` reclaims little; large
growth = a workspace near its storage limit), journal (`journalctl --vacuum-size=500M`). Not ours
(mail, Gitea, other sites): tell the owner, don't delete. Postgres refuses writes when its disk is
full and resumes when space returns; SQLite returns errors to writes. Clients retry.

## Neighbour site misbehaving

Symptoms: load, memory or disk pressure with our units within their limits (`systemctl status
hitop-sync`, `systemd-cgtop`). The limits (CPU/IO weight 50 vs the default 100) make our services
yield, and `MemoryMax` stops ours from taking the host down; nothing stops a neighbour taking ours
down. Tell the owner; post on the status page if users see errors. An outage over 4 hours caused by
a neighbour is a trigger to move hosts (spec section 5).

## Incident

1. Open an entry on the status page (`cloud/status/incidents.json`: `{title, started, text}`;
   resolve with `resolved`), short and factual.
2. Stabilise first (restart, roll back, restore), then find the cause.
3. A security incident (a leaked key or session, a compromised host): rotate (below), revoke
   sessions, keep logs; the incident response policy decides customer notice (72 h for personal
   data under GDPR).
4. Close: an `ops-log.md` entry with the incident template:

```
## <date> INCIDENT <one line>
- Detected: <when, how>. Impact: <who, what, how long>. Data loss: <none / since when>.
- Timeline (UTC): ...
- Cause: ...  Fix: ...  Follow-ups: ...
```

## Rotate secrets

- **Postgres password**: `postgres-setup.sh --rotate-password` (new URL in `postgres.url`), then
  `sed -i "s#^HITOP_SYNC_DATABASE=.*#HITOP_SYNC_DATABASE=$(cat /etc/hitop-sync/postgres.url)#"
  /etc/hitop-sync/env && systemctl restart hitop-sync` (a few seconds of errors).
- **Backup bucket key**: the owner makes a new key, then edit `backup.env`,
  `systemctl start hitop-sync-backup`, delete the old key at the provider.
- **Age key**: the owner makes a new key pair offline; replace `backup-recipients.txt` (keep the
  old public key in it until the 35 days of old backups have expired, then remove it; keep the old
  private key offline until then).
- **Host key** (`HITOP_SYNC_HOST_KEY`, C5): it seals the organizations' OIDC client
  secrets. There is no re-seal command yet: a new key means every org with SSO saves its client
  secret again (portal → Single sign-on). Keep the key out of database backups and a copy offline;
  losing it turns per-org SSO into config errors (503) until secrets are saved again, nothing else.
- **SMTP password** (`SMTP_URL`), **OIDC client secret**, Stripe keys (C3): edit
  `/etc/hitop-sync/env` (back it up first), `systemctl restart hitop-sync`. Stripe secret key:
  roll it in the Dashboard (Developers → API keys → Roll key, with an expiry of an hour for the
  old one), place the new one, restart. Webhook signing secret: Developers → Webhooks → the
  endpoint → Roll secret (keep the old one valid for a while: Stripe then signs with both and the
  server accepts either `v1`), place the new one, restart. Stripe retries failed deliveries for
  up to 3 days.
- **Revoke sessions**: one person: `DELETE FROM sessions WHERE user_id = '<id>'` (and
  `api_tokens`); everyone: `DELETE FROM sessions` (all clients sign in again; their device keys and
  workspaces stay). Run against the live database (`psql -p 5433 -U postgres hitop_sync` as the
  postgres user, or stop the service for SQLite and use `sqlite3`).

## Billing (Stripe Managed Payments, C3)

Stripe is the merchant of record (Managed Payments): it sells to the buyer, handles VAT and sales
tax, receipts, fraud, disputes and order support ("Sold through Link"; buyers can manage their
order at link.com too).

- **Settings** (root-only `/etc/hitop-sync/env`, or `_FILE` paths to root-only files):
  `HITOP_SYNC_STRIPE_SECRET_KEY` (a test key `sk_test_…`/`rk_test_…` first; a restricted key
  needs write on Checkout Sessions, Subscriptions and Customer portal, read on Prices) and
  `HITOP_SYNC_STRIPE_WEBHOOK_SECRET` (`whsec_…`): both turn billing on. Prices are found by their
  lookup keys (`pro_monthly`, `pro_yearly`, `team_monthly`, `team_yearly`, `business_monthly`,
  `business_yearly`; a plan isn't sold until its price exists); `HITOP_SYNC_STRIPE_PRICE_<PLAN>_<MONTHLY|ANNUAL>`
  pins one by id. Restart `hitop-sync`; `GET /v1/orgs/<org>/billing` shows `enabled`, `testMode`
  and the `offers` found.
- **Webhook endpoint**: `https://sync.hitop.rws8.tech/v1/billing/stripe`, API version
  `2026-10-28.endive` (the server pins it; bodies follow the endpoint's version), events
  `checkout.session.completed`, `customer.subscription.created`, `.updated`, `.deleted`,
  `.paused`, `.resumed`, `invoice.paid`, `invoice.payment_failed`, `charge.refunded`.
- **Check an event**: Dashboard → Developers → Webhooks → the endpoint → event deliveries (resend
  one there); on the VM `hitop-sync-server billing-events` (load the env file as systemd does, see
  "Deploy") lists the latest outcomes. `unmatched` = no org: a purchase made outside the portal's
  checkout (a Payment Link); find the org with the buyer, then `hitop-sync-server billing-link
  <org> <subscription id>` and resend the event. `unknown_price` = a price that is neither pinned
  nor carries one of the lookup keys; `livemode_mismatch` = a test event on a live key or the
  reverse; `superseded` on a `customer.subscription.created` = a second purchase for an org that
  already pays: refund it and cancel it in the Dashboard.
- **Sandbox → live**: in the live account create the same products, prices and lookup keys, the
  live webhook endpoint (its own signing secret) and the customer portal configuration; place the
  live secret key and webhook secret; restart. Test-mode subscriptions stay on their orgs until
  they end; end one at once with `set-plan <org> free`.
- **Refund**: refund in the Dashboard **and** cancel the subscription (a refund alone doesn't end
  it). A full refund (`charge.refunded`) sets the org to Free limits (`refunded`); a later paid
  invoice or a resumed subscription lifts it. Stripe/Link support may refund on its own (within 60
  days, or when you don't answer an escalation within 48 hours): the same event, the same effect.
- **A customer's plan by hand** (an outage of the webhook, a goodwill extension):
  `hitop-sync-server set-plan <org> <plan> [seats]`; the next webhook event of the subscription
  sets it again.

## Business features (C5): SSO per org, audit anchors, admin console

- **The test server keeps per-org SSO off** (owner decision Q15): no `HITOP_SYNC_HOST_KEY_FILE`,
  no `HITOP_SYNC_DNS_URL`. To turn it on later (owner's go): back up `/etc/hitop-sync/env`, then
  as root `echo "HITOP_SYNC_HOST_KEY=$(openssl rand -hex 32)" >> /etc/hitop-sync/env` (the env
  file is root-only and read by systemd before it drops to the dynamic user, which couldn't read a
  root-only key file; keep a copy of the line offline), add
  `HITOP_SYNC_DNS_URL=https://cloudflare-dns.com/dns-query` (a sub-processor: add it to the list),
  restart, check `GET /v1/orgs/<org>/sso` says `available: true`.
- **Daily anchors** go out from the hourly worker (one per Business org a day, mailed to owners
  when a head moved). Check: `SELECT org_id, day, mailed_at FROM audit_anchors ORDER BY day DESC
  LIMIT 20`. Stop them with `HITOP_SYNC_AUDIT_ANCHORS=off`. Public anchors are served by the sync
  server at `/v1/audit-anchors/<org>/<day>.json`; a `status.` route
  (`status.hitop.rws8.tech/audit-anchors/` → that path) needs the `status.` DNS and an rws route
  (owner's go).
- **A customer asks to unclaim a domain** verified by another org (a takeover dispute): check the
  TXT record yourself, then `DELETE FROM org_domains WHERE domain = '<d>' AND org_id = '<old org>'`
  and restart (the server reloads domains at start and after every change through the API).

## Uptime and status page

- **Checker** (owner picks; free tiers are enough): any hosted checker outside this provider
  (UptimeRobot, Better Stack, Hetrix, Checkly). Monitors, 1-5 min, alert to the owner's mail:
  `https://sync.hitop.rws8.tech/healthz` (expects 200 and `"ok":true`),
  `https://hitop.rws8.tech/`, `https://hitop.rws8.tech/cloud/healthz` (later
  `https://cloud.hitop.rws8.tech/healthz`).
- **Status page**, recommended: the checker's own hosted status page on `status.hitop.rws8.tech`
  (a CNAME the owner adds), so it stays up when the VM is down. Self-hosted alternative:
  `cloud/status/` (index.html + `check.sh`) published off the VM, e.g. a GitHub Actions schedule
  every 5 minutes running `check.sh` and pushing to GitHub Pages or a bucket website; never on the
  VM itself.
- C2's Done-when: 7 days green on the checker before the C3 launch.

## Move to another host (trigger in spec section 5)

1. New VM (same provider, EU); install the binary, units, Postgres (`postgres-setup.sh`), limits.
2. Restore the newest backup there (above) and test it at a temporary name.
3. Write freeze on the old VM (`systemctl stop hitop-sync`), a final `hitop-sync-server dump`,
   restore that on the new VM, compare `db-stats` with its manifest.
4. The owner points `sync.hitop.rws8.tech` (and `cloud.`) at the new VM; certificates via its rws
   ACME. Keep the old VM stopped, not deleted, for a week.
