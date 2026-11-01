# hitop sync server

Keeps hitop collection folders in step across devices and team members, end-to-end encrypted:
clients encrypt every file before it leaves the device, and the server stores ciphertext,
version vectors and wrapped keys only. Design and limits: [THREAT_MODEL.md](THREAT_MODEL.md).

- `hitop-sync-server` (this crate): **AGPL-3.0-or-later** ([LICENSE](LICENSE)). If you run a
  modified version for others, offer them its source.
- `cli/crates/hitop-sync-crypto`: the client-side encryption the CLI, desktop app and web app use.
  **MIT**, like the clients. The server doesn't link it.

Status: for testing. No accounts for sale, no public service yet; a test instance runs at
https://sync.hitop.rws8.tech.

## Self-hosting

Requirements: a Linux host, a DNS name, a TLS-terminating reverse proxy (rws, Caddy, nginx), and
an SMTP account for sign-in mails.

### Binary and systemd

```bash
cd sync-server && cargo build --release       # → target/release/hitop-sync-server
# or with Postgres support: cargo build --release --features postgres
sudo install -m 0755 target/release/hitop-sync-server /usr/local/bin/
sudo install -m 0644 deploy/hitop-sync.service /etc/systemd/system/
sudo install -d -m 0700 /etc/hitop-sync
sudoedit /etc/hitop-sync/env                  # settings below, mode 0600
sudo systemctl enable --now hitop-sync
curl http://127.0.0.1:7881/healthz            # {"ok":true}
```

`deploy/install.sh` does the same and backs up the database and binary first. The unit runs as a
dynamic user with the database in `/var/lib/hitop-sync` (systemd keeps it under
`/var/lib/private/hitop-sync`). The server serves no files, so nothing on disk is reachable over
HTTP.

### Docker

```bash
docker build -t hitop-sync sync-server
docker run -d -p 127.0.0.1:7881:7881 -v hitop-sync:/data --env-file hitop-sync.env hitop-sync
```

### Settings (environment)

| Variable | Default | |
| --- | --- | --- |
| `HITOP_SYNC_PUBLIC_URL` | `http://localhost:7881` | the URL clients use; sign-in links point at `<url>/signin` |
| `HITOP_SYNC_DATABASE` | `sqlite:hitop-sync.db` | or `postgres://user:pass@host/db` (build with `--features postgres`) |
| `HITOP_SYNC_RP_ID` | host of the public URL | passkey relying party id; a parent domain lets a web app on a sibling host share passkeys |
| `HITOP_SYNC_RP_ORIGINS` | the public URL | comma separated origins allowed to use passkeys |
| `HITOP_SYNC_ALLOWED_ORIGINS` | none | comma separated web origins allowed by CORS (your web app) |
| `SMTP_URL` | none (email sign-in off) | `smtp://user:pass@host:587` (STARTTLS) or `smtps://...:465` |
| `SMTP_TLS_SERVER_NAME` | the SMTP host | name the mail server's certificate must match, when connecting by IP |
| `SMTP_FROM` | `hitop <user@domain>` | From address |
| `HITOP_SYNC_MAX_BLOB_BYTES` | 4194304 | per encrypted file |
| `HITOP_SYNC_MAX_WORKSPACE_BYTES` | 268435456 | per workspace |
| `HITOP_SYNC_MAX_WORKSPACES` | 50 | owned per account |
| `RWS_CONFIG_IP`, `RWS_CONFIG_PORT` | `127.0.0.1`, `7881` | listener (plain HTTP; TLS at the proxy) |
| `RWS_CONFIG_MAX_BODY_SIZE_IN_BYTES` | unlimited | set to ~6 MiB (the unit and image do) |
| `HITOP_SYNC_TRUSTED_PROXIES` | `127.0.0.1,::1` | peers whose `X-Forwarded-For` names the client (audit log IPs) |
| `HITOP_SYNC_AUDIT_IP` | `truncated` | client IP in the audit log: `truncated` (IPv4 /24, IPv6 /48) or `off` |
| `HITOP_SYNC_AUDIT_RETENTION_DAYS` | 365 | audit rows older than this are purged (hourly); 0 keeps them |
| `HITOP_SYNC_OIDC_ISSUER` | none (SSO off) | OpenID Connect issuer URL; see "Single sign-on" |
| `HITOP_SYNC_OIDC_CLIENT_ID` | | the client registered at the provider |
| `HITOP_SYNC_OIDC_CLIENT_SECRET` or `HITOP_SYNC_OIDC_CLIENT_SECRET_FILE` | | its secret, or a file holding it (never in the repository or a world-readable file) |
| `HITOP_SYNC_OIDC_ALLOWED_DOMAINS` | any | comma separated email domains that may sign in by SSO |
| `HITOP_SYNC_OIDC_REQUIRED_GROUPS`, `HITOP_SYNC_OIDC_GROUPS_CLAIM` | none, `groups` | the user must be in one of these groups (from that claim) |
| `HITOP_SYNC_SSO_REQUIRED` | off | `1`: addresses in the allowed domains (all, when none are set) sign in by SSO only: no sign-in mails, no passkey sign-in |
| `HITOP_SYNC_OIDC_ASSUME_VERIFIED` | off | `1` for providers that send no `email_verified` (Entra ID); needs allowed domains |
| `HITOP_SYNC_OIDC_SCOPES`, `HITOP_SYNC_OIDC_NAME` | `openid email profile`, `SSO` | scopes; the provider name clients show |

The mail server's certificate is always verified (Mozilla roots). Run one server process per
database: writes are serialized in the process, and passkey challenges live in its memory.

### Single sign-on (OpenID Connect)

Sign-in with your organization's identity provider, next to (or instead of) sign-in mails and
passkeys. The server is the OIDC client (authorization code + PKCE); hitop clients never see the
client secret. Register a **confidential web client** at the provider with:

- redirect URI `https://<your sync host>/v1/auth/sso/callback` (`HITOP_SYNC_PUBLIC_URL` +
  `/v1/auth/sso/callback`; the server prints it at start),
- scopes `openid email profile`, and a groups claim if you restrict by group.

Then set the issuer, client id and secret (prefer `HITOP_SYNC_OIDC_CLIENT_SECRET_FILE` pointing at
a root-only file, or the unit's `EnvironmentFile`), usually `HITOP_SYNC_OIDC_ALLOWED_DOMAINS`, and
restart. People sign in with `hitop sync login --sso` or **Sign in with SSO** on the Sync page. An
existing account (from a sign-in mail) is linked on the first SSO sign-in when the provider says
the address is verified; unverified addresses are refused. Details and limits: THREAT_MODEL.md,
"Single sign-on".

Provider recipes (configuration only):

| Provider | Register | Settings |
| --- | --- | --- |
| Keycloak | Clients → Create, OpenID Connect, Client authentication on, Standard flow; Valid redirect URI as above; for groups add a "Group Membership" mapper (claim `groups`, full path off) | `HITOP_SYNC_OIDC_ISSUER=https://<keycloak>/realms/<realm>` |
| Google Workspace | Google Cloud console → APIs & Services → Credentials → OAuth client ID, type Web application, the redirect URI; OAuth consent screen Internal | `HITOP_SYNC_OIDC_ISSUER=https://accounts.google.com`, `HITOP_SYNC_OIDC_ALLOWED_DOMAINS=<your domain>` (otherwise any Google account can sign in) |
| Entra ID | App registrations → New, single tenant, Web redirect URI as above; Certificates & secrets → New client secret; optional `groups` claim under Token configuration | `HITOP_SYNC_OIDC_ISSUER=https://login.microsoftonline.com/<tenant id>/v2.0`, `HITOP_SYNC_OIDC_ALLOWED_DOMAINS=<your verified domains>`, `HITOP_SYNC_OIDC_ASSUME_VERIFIED=1` (Entra sends no `email_verified`; only for domains verified in your tenant) |
| Okta | Applications → Create App Integration, OIDC, Web Application, the redirect URI; for groups add a `groups` claim (Sign On → OpenID Connect ID Token, with a filter) | `HITOP_SYNC_OIDC_ISSUER=https://<org>.okta.com` (or your custom authorization server's issuer) |

`HITOP_SYNC_SSO_REQUIRED=1` turns sign-in mails and passkey sign-in off for the allowed domains.
It doesn't end existing sessions; to lock someone out at once, delete them:
`sqlite3 <db> "DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE email = 'x@corp.example')"`.
A broken setting (wrong issuer, unreachable provider, refused client) answers 503 and is written to
the server log and the audit log (`sso_config_error`).

### Audit log

The server records security events (sign-ins and failures, SSO, keys, members, roles, invites,
rotations, runner credentials, workspace create and delete) with the account, the time, a
truncated client IP and the user agent; never contents or keys. Owners read their workspace's log
(`hitop sync audit`, or Members → Show audit log on the Sync page); everyone their own sign-ins
(`hitop sync audit --mine`). Rows form a hash chain per workspace: `hitop sync audit --verify`
checks it on the client, `hitop-sync-server audit-verify` checks every chain on the host (exit 1
when one is broken; run it with the service's environment). Rows past
`HITOP_SYNC_AUDIT_RETENTION_DAYS` are purged hourly. If you run a service for others, mention the
log in your privacy notice.

### Reverse proxy

Forward everything for your host to `127.0.0.1:7881`, and rate limit `/v1/auth/*` per client IP
(the server limits sign-in mails per address, not per IP). rws example:
[deploy/rws-routes.toml](deploy/rws-routes.toml).

### Backups

SQLite: `sqlite3 /var/lib/private/hitop-sync/sync.db ".backup /backup/sync-$(date +%F).db"`
(safe while running). Postgres: `pg_dump`. Backups hold only ciphertext, but they do hold
account emails and membership; protect them like any personal data.

## API (v1)

JSON over HTTPS; `Authorization: Bearer <session>`; binary fields are base64url without padding.

| Method and path | |
| --- | --- |
| `POST /v1/auth/email {email}` | mails a sign-in link (202) |
| `POST /v1/auth/email/verify {token}` | → `{session, expiresAt, user}` |
| `POST /v1/auth/passkey/register/start` / `finish {challengeId, name, credential}` | add a passkey (signed in) |
| `POST /v1/auth/passkey/login/start {email}` / `finish {challengeId, credential}` | → session |
| `POST /v1/auth/logout` | |
| `GET /v1/me`; `POST /v1/me/keys {name, publicKey}` | account; register a device's X25519 public key |
| `GET /v1/users/keys?email=` | another account's device keys (to add them) |
| `GET/POST /v1/workspaces` `{id, wraps}` | list; create (id = 32 hex digits, epoch 1 wrapped for each of your devices) |
| `GET/DELETE /v1/workspaces/:ws` | members, keys, epoch, seq; delete (owner) |
| `GET /v1/workspaces/:ws/wraps?keyId=` | your device's wrapped keys, every epoch |
| `POST /v1/workspaces/:ws/wraps {epoch, wraps}` | wrap an epoch for a member's new device |
| `POST /v1/workspaces/:ws/members {email, role, wraps}` | add a member (owner) |
| `DELETE /v1/workspaces/:ws/members/:user {epoch, wraps}` | remove a member **and** rotate the key (owner) |
| `POST /v1/workspaces/:ws/rotate {epoch, wraps}` | rotate the key (owner) |
| `PUT /v1/workspaces/:ws/members/:user {role}` | change a role (owner; a workspace keeps one owner) |
| `POST /v1/workspaces/:ws/invites {email?, role}` | invite (owner): mailed when `email` is set, else a link; 7 days → `{id, token, link, mailed}` |
| `GET /v1/workspaces/:ws/invites`; `DELETE .../invites/:id` | open invites, accepted ones with the account's device keys (owner) |
| `POST /v1/invites/accept {token}` | accept (signed in, with a device key); the owner then adds you with the wrapped key |
| `POST /v1/workspaces/:ws/runners {name, publicKey, wrapped}` | a runner credential for scheduled runs (owner): viewer of this workspace only, one-year token |
| `GET /v1/workspaces/:ws/changes?since=<seq>` | blobs changed after `seq` |
| `GET /v1/workspaces/:ws/blobs/:id` | envelope + version, vv, epoch |
| `PUT /v1/workspaces/:ws/blobs/:id {version, vv, epoch, content \| deleted}` | write version n+1 (owner, editor) |
| `GET /v1/auth/sso` | `{enabled, name, required, domains}` |
| `POST /v1/auth/sso/start {challenge, returnTo?, email?}` | `challenge` = base64url(SHA-256(verifier)) → `{url}` to open in a browser |
| `GET /v1/auth/sso/callback` | the provider's redirect (browser): sends a one-time code to `returnTo` (loopback or an allowed web origin) or shows it |
| `POST /v1/auth/sso/finish {code, verifier}` | → session |
| `GET /v1/workspaces/:ws/audit?limit&before&event&actor&since&until` | the audit log, newest first (owner) → `{events, next, head, anchor}` |
| `GET /v1/workspaces/:ws/audit/verify` | the server's check of the chain (owner) → `{ok, checked, head, problem}` |
| `GET /v1/me/audit` | your own account events |

Invites can't add anyone by themselves: only a member's client can wrap the workspace key for the
new member's devices, after checking their fingerprints. Runner accounts (`<id>@runner.invalid`)
can't sign in, register keys, accept invites, create workspaces or look up other accounts;
removing one (`DELETE .../members/:user` with the rotated key) deletes it and its session.

Upgrading from 0.1.0 (T23): the server migrates the database on start (migration 2 adds
`users.runner_of` and the `invites` table, SQLite and Postgres); back up first (`deploy/install.sh`
does).
Migration 3 (T25) adds `oidc_identities`, `audit_log` and `audit_chain`; nothing to configure
unless you turn SSO on. Events are logged from the upgrade on.

A write must name the next version and a version vector that descends from the stored one, under
the current epoch; otherwise 409 with the current state (`conflict` or `stale_epoch`): fetch,
merge, re-encrypt and write again.

## Development

```bash
cargo test --workspace                        # SQLite, in-process
HITOP_SYNC_TEST_POSTGRES=postgres://postgres:pw@127.0.0.1:5432/postgres cargo test --features postgres
cargo clippy --workspace --all-targets -- -D warnings
HITOP_SYNC_MAIL=log cargo run                 # sign-in mails printed to stderr
# SSO against Keycloak (tests/keycloak.rs; CI does the same):
docker run -d --name hitop-keycloak -p 127.0.0.1:18080:8080 \
  -v "$PWD/tests/fixtures/keycloak:/opt/keycloak/data/import:ro" quay.io/keycloak/keycloak:26.4.2 start-dev --import-realm
HITOP_SYNC_TEST_KEYCLOAK=http://127.0.0.1:18080 cargo test --test keycloak
```
