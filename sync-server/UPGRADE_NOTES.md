# hitop sync server: upgrade notes

One section per release (the `cli-v<version>` tags; the signed image
`ghcr.io/bohdaq/hitop-sync-server:<version>` has the same version). For each: the schema it
migrates to, what you must or may set, what older clients see, and how to go back.

How every upgrade works:

1. `hitop-sync-server dump /root/pre-<version>-<date>` (or your backup), and keep the old binary.
2. Install the new binary or image and start it: it migrates the database on start, each
   migration in one transaction (a failed one leaves the database as it was).
3. Check `GET /healthz` (`schema` below) and `hitop-sync-server support-bundle` (the schema in the
   database equals the binary's, no broken audit chain).
4. Going back: a binary never runs on a schema newer than its own. Stop the server, restore the
   dump, start the old binary. Changes made after the upgrade are lost, so decide within minutes.

hitop clients 1.1 and 1.2 keep working with every version below; nothing here needs a client
upgrade.

## Next release (on `main`, not tagged yet): schema 3 → 10

Migrations 4 to 10 run in order on the first start. All are additive (new tables, new columns
with defaults); no existing row changes meaning.

| Schema | Tranche | Adds | Settings (all optional on a self-hosted server) |
| --- | --- | --- | --- |
| 4 | C1 | organizations (every person gets a personal org that owns their workspaces), entitlements, usage counters, access tokens | `HITOP_SYNC_LIMITS` stays `off` (no plans); `hitop-sync-server over-limits` must print 0 |
| 5 | C2 | key bundles (recovery phrase) | none |
| 6 | C1b | passwords | `HITOP_SYNC_PASSWORDS=off` to keep them off; `HITOP_SYNC_PASSWORD_PEPPER[_FILE]` (keep it out of backups); Google/GitHub sign-in with `HITOP_SYNC_GOOGLE_*`, `HITOP_SYNC_GITHUB_*` |
| 7 | C3 | billing ids, webhook outcomes, trials | billing stays off without `HITOP_SYNC_STRIPE_*` (HITOP Cloud only) |
| 8 | C4 | run history, alerts, shared environment values | `HITOP_SYNC_MAX_RUN_BYTES`, `HITOP_SYNC_RUN_RETENTION_DAYS`, `HITOP_SYNC_ALERTS_PER_HOUR`, `HITOP_SYNC_ALERT_WEBHOOKS` |
| 9 | C5 | SSO per org, domain claims, audit anchors, rotation requests | per-org SSO needs `HITOP_SYNC_HOST_KEY[_FILE]` (keep a copy offline, never in backups) and `HITOP_SYNC_DNS_URL`; every team org gets daily audit anchors mailed to its owners on days its log changed (`HITOP_SYNC_AUDIT_ANCHORS=off` stops them) |
| 10 | C6 | SCIM provisioning (tokens, directory users and groups, group roles, grants) | none: SCIM stays off for an org until its owner creates a SCIM token |

Also in this release: `hitop-sync-server support-bundle`, `dump`, `db-stats`, `migrate-db`
(SQLite ⇄ Postgres), `audit-verify`; the image is built from the repository root and signed
(README "Docker"). A server that sends mail now sends daily anchor mails to team org owners (see
schema 9). Going back to 1.2's server from schema 10 means restoring the pre-upgrade dump.

## 1.2.0 (2026-11-02): schema 3

No migration from 1.1. Format 2 (authenticated) key wraps are accepted next to format 1; a
format 2 wrap replaces a format 1 one. Going back to 1.1's server: same database, no restore.

## 1.1.0 (2026-11-02): schema 3

Migrations 2 (teams: invites, runner accounts) and 3 (SSO identities, audit log). SSO with
`HITOP_SYNC_OIDC_*`; audit retention `HITOP_SYNC_AUDIT_RETENTION_DAYS` (365); IPs truncated
(`HITOP_SYNC_AUDIT_IP`). Going back: restore the dump.
