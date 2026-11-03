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
| `HITOP_SYNC_PASSWORDS` | `on` | `off` turns password sign-in, sign-up and reset off |
| `HITOP_SYNC_PASSWORD_PEPPER` or `HITOP_SYNC_PASSWORD_PEPPER_FILE` | none | a secret (16+ characters) the server's password hashes are keyed with (Argon2 secret); keep it out of backups; hashes made before it was set keep working |
| `HITOP_SYNC_GOOGLE_CLIENT_ID`, `HITOP_SYNC_GOOGLE_CLIENT_SECRET` (or `_FILE`) | none (off) | "Continue with Google" (OpenID Connect); redirect URI `<public URL>/v1/auth/google/callback` |
| `HITOP_SYNC_GITHUB_CLIENT_ID`, `HITOP_SYNC_GITHUB_CLIENT_SECRET` (or `_FILE`) | none (off) | "Continue with GitHub" (OAuth app); callback URL `<public URL>/v1/auth/github/callback` |
| `HITOP_SYNC_LIMITS` | `off` | `plans`: enforce each organization's plan limits (HITOP Cloud). Off, a self-hosted server has no seats, plans or per-org limits |
| `HITOP_SYNC_PORTAL_URL` | `https://cloud.hitop.rws8.tech/` | the account portal limit errors link to |
| `HITOP_SYNC_RATE_LIMITS` | `on` | `off` turns the request rate limits off (src/ratelimit.rs) |
| `HITOP_SYNC_RATE_AUTH`, `HITOP_SYNC_RATE_WRITE`, `HITOP_SYNC_RATE_WRITE_IP`, `HITOP_SYNC_RATE_CREATE` | 60, 600, 1200 a minute; 200 an hour | `POST /v1/auth/*` per client IP; writes per session and per IP; creates (workspaces, orgs, tokens, keys, runners, invites, recovery) per session. Over: 429 `{error: "rate_limited", limit, retryAfter}` + `Retry-After`; 0 = no limit |
| `HITOP_SYNC_MAX_JSON_BYTES` | 1 MiB | the largest body of any request but a blob upload (413) |
| `HITOP_SYNC_PG_DUMP` | `pg_dump` | the pg_dump command `dump` runs (may have arguments) |

The mail server's certificate is always verified (Mozilla roots). Run one server process per
database: writes are serialized in the process, and passkey challenges live in its memory.

### Single sign-on (OpenID Connect)

Sign-in with your organization's identity provider, next to (or instead of) sign-in mails and
passkeys. The server is the OIDC client (authorization code + PKCE); hitop clients never see the
client secret. Register a **confidential web client** at the provider with:

- redirect URI `https://<your sync host>/v1/auth/sso/callback` (`HITOP_SYNC_PUBLIC_URL` +
  `/v1/auth/sso/callback`; the server prints it at start),
- scopes `openid email profile`, and a groups claim if you restrict by group.

Then set the issuer, client id and secret, usually `HITOP_SYNC_OIDC_ALLOWED_DOMAINS`, and restart.
With the systemd unit, put them in `/etc/hitop-sync/env` (root-only; systemd reads it before
dropping to the dynamic user, which couldn't read a root-only secret file). Elsewhere (Docker,
Kubernetes) `HITOP_SYNC_OIDC_CLIENT_SECRET_FILE` reads the secret from a mounted file. People sign in with `hitop sync login --sso` or **Sign in with SSO** on the Sync page. An
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

### Passwords, Google and GitHub

People can sign up and sign in with an email address and a password (confirmed by a mailed
code), and reset a forgotten password by mail; mail codes, passkeys and SSO stay. The password
never reaches the server: clients send a key derived from it (Argon2id, then HKDF) and the server
keeps its own Argon2id hash of that key. The same password unlocks the person's end-to-end keys
on a new device (a second key from it seals their key bundle); a reset by mail can't, and says
so. Design: [THREAT_MODEL.md](THREAT_MODEL.md) "Passwords". Sign-up and reset need mail
(`SMTP_URL`).

"Continue with Google" and "Continue with GitHub" are off until their client id and secret are
set. Register the apps with these redirect URIs (exactly, with your public URL):

| Provider | Where | Redirect / callback URI |
| --- | --- | --- |
| Google | Google Cloud console → APIs & Services → Credentials → OAuth client ID, Web application; consent screen with the `openid` and `email` scopes | `<HITOP_SYNC_PUBLIC_URL>/v1/auth/google/callback` |
| GitHub | Settings → Developer settings → OAuth Apps → New (not a GitHub App) | `<HITOP_SYNC_PUBLIC_URL>/v1/auth/github/callback` |

Any Google or GitHub account with a verified address may sign in; the first time, it is linked
to the account with that address (or one is made). GitHub must report the address as the
account's primary and verified one. Addresses that must use SSO (`HITOP_SYNC_SSO_REQUIRED`)
can't use either. Signing in with Google or GitHub doesn't unlock end-to-end keys on a new
device: the password or the recovery phrase does.

### Audit log

The server records security events (sign-ins and failures, SSO, keys, members, roles, invites,
rotations, runner credentials, workspace create and delete) with the account, the time, a
truncated client IP and the user agent; never contents or keys. Owners read their workspace's log
(`hitop sync audit`, or Members → Show audit log on the Sync page); everyone their own sign-ins
(`hitop sync audit --mine`). Rows form a hash chain per workspace: `hitop sync audit --verify`
checks it on the client, `hitop-sync-server audit-verify` checks every chain on the host (exit 1
when one is broken). On a copy, so the service's files keep their owner:
`sudo sqlite3 /var/lib/private/hitop-sync/sync.db ".backup /root/audit.db" && sudo env HITOP_SYNC_DATABASE=sqlite:/root/audit.db hitop-sync-server audit-verify; sudo rm -f /root/audit.db*`. Rows past
`HITOP_SYNC_AUDIT_RETENTION_DAYS` are purged hourly. If you run a service for others, mention the
log in your privacy notice.

### Reverse proxy

Forward everything for your host to `127.0.0.1:7881`. The server rate limits sign-in requests per
client IP and writes per session itself (it reads the client from `X-Forwarded-For` of a trusted
proxy, `HITOP_SYNC_TRUSTED_PROXIES`); proxy limits per IP on top are still a good idea. rws example:
[deploy/rws-routes.toml](deploy/rws-routes.toml).

### Backups and database moves

- `hitop-sync-server dump <empty dir>`: a backup (`sync.db`, a consistent SQLite snapshot, or
  `sync.pgdump` from `pg_dump` run in the same snapshot) plus `manifest.json` with the row count and
  content hash of every table in exactly that data. `snapshot <file>`: a SQLite copy alone.
- `db-stats [--json]`: those counts and hashes for the configured database (equal hashes = the same
  rows, across SQLite and Postgres).
- `migrate-db --from <url> --to <url>`: copies every table into an empty database (SQLite →
  Postgres, or back into a new SQLite file), then compares counts and hashes and runs
  `audit-verify` on the target; exit 0 only when all hold. Refuses a target with rows in it.
- HITOP Cloud runs them through `deploy/backup.sh` (age-encrypted, to an S3 bucket elsewhere),
  `deploy/restore-drill.sh`, `deploy/cutover-postgres.sh` / `rollback-postgres.sh`; see
  `../cloud/RUNBOOK.md`. Backups hold only ciphertext, but they do hold account emails and
  membership; protect them like any personal data.

## API (v1)

JSON over HTTPS; `Authorization: Bearer <session>`; binary fields are base64url without padding.
A `wrapped` key is format 1 (hitop 1.1: 80 bytes, unauthenticated) or format 2 (hitop 1.2:
`0x02 || u32 head || u16 len || sender key id || 80 bytes`, HPKE Auth mode); the server accepts
both, checks that a format 2 wrap names one of the caller's own device keys as sender, and never
verifies a wrap (clients do, THREAT_MODEL.md "Authenticated wraps").

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
| `GET/DELETE /v1/workspaces/:ws` | members, keys (`hasKey`, `legacyEpochs`: epochs whose wrap for it is format 1), epoch, seq; delete (owner) |
| `GET /v1/workspaces/:ws/wraps?keyId=` | your device's wrapped keys, every epoch |
| `POST /v1/workspaces/:ws/wraps {epoch, wraps}` | wrap an epoch for a member's new device; a format 2 wrap also replaces a format 1 wrap (→ `{added, stored}`) |
| `POST /v1/workspaces/:ws/members {email, role, wraps}` | add a member (owner) |
| `DELETE /v1/workspaces/:ws/members/:user {epoch, wraps}` | remove a member **and** rotate the key (owner) |
| `POST /v1/workspaces/:ws/rotate {epoch, wraps}` | rotate the key (owner) |
| `PUT /v1/workspaces/:ws/members/:user {role}` | change a role (owner; a workspace keeps one owner) |
| `POST /v1/workspaces/:ws/invites {email?, role}` | invite (owner): mailed when `email` is set, else a link; 7 days → `{id, token, link, mailed}` |
| `GET /v1/workspaces/:ws/invites`; `DELETE .../invites/:id` | open invites, accepted ones with the account's device keys (owner) |
| `POST /v1/invites/accept {token}` | accept (signed in, with a device key); the owner then adds you with the wrapped key. A code `<token>.<fingerprint>` (hitop 1.2) is accepted too: the server ignores the part after the `.` |
| `POST /v1/workspaces/:ws/runners {name, publicKey, wrapped}` | a runner credential for scheduled runs (owner): viewer of this workspace only, one-year token |
| `GET /v1/workspaces/:ws/changes?since=<seq>` | blobs changed after `seq` |
| `GET /v1/workspaces/:ws/blobs/:id` | envelope + version, vv, epoch |
| `PUT /v1/workspaces/:ws/blobs/:id {version, vv, epoch, content \| deleted}` | write version n+1 (owner, editor) |
| `GET /v1/auth/sso` | `{enabled, name, required, domains}` |
| `POST /v1/auth/sso/start {challenge, returnTo?, email?}` | `challenge` = base64url(SHA-256(verifier)) → `{url}` to open in a browser |
| `GET /v1/auth/sso/callback` | the provider's redirect (browser): sends a one-time code to `returnTo` (loopback or an allowed web origin) or shows it |
| `POST /v1/auth/sso/finish {code, verifier}` | → session |
| `GET /v1/auth/methods` | what a sign-in page can offer → `{password: {enabled, signup, reset}, emailCode, passkey, sso, google, github}` |
| `POST /v1/auth/password/params {email}` | the account's password KDF `{kdf: {alg, m, t, p, salt}}` (made up, but stable, for an address without one) |
| `POST /v1/auth/password/signup {email, kdf, authKey}` | mails a confirmation code (202 for any address) |
| `POST /v1/auth/password/verify {token}` | the code → session (`created`); makes the account or adds the password |
| `POST /v1/auth/password/login {email, authKey}` | → session; 401 `invalid_credentials` for any wrong address or password; 429 after repeated failures |
| `POST /v1/auth/password/forgot {email}`; `POST /v1/auth/password/reset {token, kdf, authKey}` | mails a reset code (202); the code + new password → session and `bundleLost` (the password's key bundle is deleted, other sessions end) |
| `POST /v1/me/password {currentAuthKey?, kdf, authKey, bundle?}` | set or change (signed-in device; current required when there is one); `bundle` re-sealed under the new password replaces the old one, else it is deleted |
| `POST /v1/auth/google/start`, `POST /v1/auth/github/start` `{challenge, returnTo?}`; `GET .../callback`; `POST .../finish {code, verifier}` | as SSO, with Google / GitHub |
| `GET /v1/workspaces/:ws/audit?limit&before&event&actor&since&until` | the audit log, newest first (owner) → `{events, next, head, anchor}` |
| `GET /v1/workspaces/:ws/audit/verify` | the server's check of the chain (owner) → `{ok, checked, head, problem}` |
| `GET /v1/me/audit` | your own account events |
| `GET/POST /v1/orgs {name}` | your organizations (personal first) → `{orgs: [{id, name, personal, role, plan, status, seats, members, workspaces}]}`; create one (you own it) |
| `GET/PUT/DELETE /v1/orgs/:org` | read (member); rename (admin); delete an org without workspaces (owner) |
| `GET/POST /v1/orgs/:org/members {email, role}` | members and pending invites; add an account or invite an address (admin; owners by owners) → `{status: added\|invited}` |
| `PUT/DELETE /v1/orgs/:org/members/:user` `{role}` | change a role (admin; to or from owner by owners); remove (admin, or yourself); an org keeps one owner |
| `DELETE /v1/orgs/:org/invites/:email` | cancel an org invite (admin) |
| `POST /v1/orgs/:org/workspaces {id, wraps}` or `{workspace}` | create a workspace in the org; or move one you own into it (member). `POST /v1/workspaces` also takes `org` |
| `GET /v1/orgs/:org/usage`, `GET /v1/orgs/:org/limits` | usage next to the plan's limits (`null` = none; `enforced: false` with limits off) |
| `GET/POST /v1/me/tokens {name, scopes, expiresInDays}`; `DELETE /v1/me/tokens/:id` | personal access tokens (`hitop_pat_...`, scopes `read` or `read write`, at most 365 days); a session only |

With `HITOP_SYNC_LIMITS=plans` a create beyond the org's plan answers **402** `{error: "limit",
limit, plan, org, used, max, upgradeUrl, message}`: a new workspace, person (seat), device,
runner credential, invite, or a blob write that makes the org's storage grow. Reads, pulls,
deletes and writes that don't grow storage are never refused. `GET /v1/workspaces` gives each
workspace's `org` (`{id, name, personal}`).

Operator commands: `hitop-sync-server over-limits` lists every org over a limit of its plan (exit
0 when none); `hitop-sync-server set-plan <org> <free|pro|team|business|enterprise> [seats]` sets
an org's plan until billing does (C3).

Recovery (C2): `PUT/GET/DELETE /v1/me/recovery[?method=phrase]` keep one opaque key bundle per
account and unlock method (at most 256 KiB; `phrase`, and `password` for an account with one), sealed by the client
(`hitop_sync_crypto::seal_recovery`); only a signed-in device's session may use them (not access
tokens or runner credentials), each step is in the account's audit log, and `GET /v1/me` has
`recovery: {updatedAt} | null`. `DELETE /v1/me/keys/:id` removes one of your own device keys while
it holds no workspace key (409 `key_in_use` otherwise).

Invites can't add anyone by themselves: only a member's client can wrap the workspace key for the
new member's devices, after checking their fingerprints. Runner accounts (`<id>@runner.invalid`)
can't sign in, register keys, accept invites, create workspaces or look up other accounts;
removing one (`DELETE .../members/:user` with the rotated key) deletes it and its session.

Upgrading from 0.1.0 (T23): the server migrates the database on start (migration 2 adds
`users.runner_of` and the `invites` table, SQLite and Postgres); back up first (`deploy/install.sh`
does).
Migration 3 (T25) adds `oidc_identities`, `audit_log` and `audit_chain`; nothing to configure
unless you turn SSO on. Events are logged from the upgrade on.
Migration 4 (HITOP Cloud C1) adds `orgs`, `org_members`, `org_invites`, `workspaces.org_id`,
`entitlements`, `usage_counters` and `api_tokens`: every account gets a personal org on Free that
owns the workspaces it owns, and each personal org's limits are raised to what it already uses
(grandfathering), so `over-limits` lists nobody after the upgrade. Nothing changes for clients.

Migration 6 (HITOP Cloud C1b) adds `password_credentials`, `password_tokens` and
`server_secrets`; nothing changes for existing accounts or older clients. `GET /v1/me` adds
`password: {set, updatedAt}` and `bundles: {method: {updatedAt}}` (`recovery` still means the
phrase).

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
