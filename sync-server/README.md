# hitop sync server

Keeps hitop collection folders in step across devices and team members, end-to-end encrypted:
clients encrypt every file before it leaves the device, and the server stores ciphertext,
version vectors and wrapped keys only. Design and limits: [THREAT_MODEL.md](THREAT_MODEL.md).

- `hitop-sync-server` (this crate): **AGPL-3.0-or-later** ([LICENSE](LICENSE)). If you run a
  modified version for others, offer them its source.
- `cli/crates/hitop-sync-crypto`: the client-side encryption the CLI, desktop app and web app use.
  **MIT**, like the clients. The server doesn't link it.

Status: HITOP Cloud runs this server at https://sync.hitop.rws8.tech with the hosted plan limits
(Free today; Pro, Team and Business are sold through Stripe Managed Payments once the owner's
Stripe account is set up, spec `spec/CLOUD_SERVICE.md` C3). Self-hosting it is free, with every feature and no limits.

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

Signed images (SQLite and Postgres, linux/amd64) come with each release tag (`cli-v<version>`):
`ghcr.io/bohdaq/hitop-sync-server:<version>`. They are signed with cosign keyless (Sigstore) and
carry an SPDX SBOM attestation; check the signature before you run one:

```bash
cosign verify ghcr.io/bohdaq/hitop-sync-server:<version> \
  --certificate-identity "https://github.com/bohdaq/hitop/.github/workflows/release-sync-server.yml@refs/tags/cli-v<version>" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
docker run -d -p 127.0.0.1:7881:7881 -v hitop-sync:/data --env-file hitop-sync.env ghcr.io/bohdaq/hitop-sync-server:<version>
```

Or build it yourself from the repository root (the build needs the client crate next to the
server): `docker build -f sync-server/Dockerfile -t hitop-sync .`

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
| `HITOP_SYNC_APPLE_CLIENT_ID`, `_TEAM_ID`, `_KEY_ID`, `_PRIVATE_KEY_FILE` (or `_PRIVATE_KEY`); `HITOP_SYNC_APPLE_APP_IDS` | none (off) | "Continue with Apple": Services ID, team id, Sign in with Apple key id and `.p8` file; App IDs whose notifications are accepted. Return URL `<public URL>/v1/auth/apple/callback` |
| `HITOP_SYNC_LIMITS` | `off` | `plans`: enforce each organization's plan limits (HITOP Cloud). Off, a self-hosted server has no seats, plans or per-org limits |
| `HITOP_SYNC_PORTAL_URL` | `https://cloud.hitop.rws8.tech/` | the account portal limit errors link to |
| `HITOP_SYNC_RATE_LIMITS` | `on` | `off` turns the request rate limits off (src/ratelimit.rs) |
| `HITOP_SYNC_RATE_AUTH`, `HITOP_SYNC_RATE_WRITE`, `HITOP_SYNC_RATE_WRITE_IP`, `HITOP_SYNC_RATE_CREATE` | 60, 600, 1200 a minute; 200 an hour | `POST /v1/auth/*` per client IP; writes per session and per IP; creates (workspaces, orgs, tokens, keys, runners, invites, recovery) per session. Over: 429 `{error: "rate_limited", limit, retryAfter}` + `Retry-After`; 0 = no limit |
| `HITOP_SYNC_MAX_JSON_BYTES` | 1 MiB | the largest body of any request but a blob upload or a run upload (413) |
| `HITOP_SYNC_MAX_RUN_BYTES` | 1 MiB | the largest sealed run report (413); run uploads may be this big |
| `HITOP_SYNC_RUN_RETENTION_DAYS` | 365 | run history kept this long without plans (0 = forever); with `HITOP_SYNC_LIMITS=plans` the plan says (Free 7, Pro 30, Team 90, Business 365) |
| `HITOP_SYNC_ALERTS_PER_HOUR` | 30 | alerts a workspace may send an hour, on top of one per status change |
| `HITOP_SYNC_ALERT_WEBHOOKS` | `https` | `insecure-local` also allows `http://127.0.0.1` webhooks (tests only); otherwise webhooks are https to public addresses |
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

### Single sign-on per organization (HITOP Cloud C5)

On a server run for several organizations, each can bring its own provider for the domains it
proves it owns. Off unless the server has a **host key**: `HITOP_SYNC_HOST_KEY` = 64 hex characters
(`openssl rand -hex 32`) in the root-only env file, or `HITOP_SYNC_HOST_KEY_FILE` = a file holding
them (Docker, Kubernetes). Keep a copy offline and **out of database backups**: it opens the
organizations' client secrets, which the database keeps sealed under it.
Domain verification also needs `HITOP_SYNC_DNS_URL`, a DNS-over-HTTPS resolver's JSON API (e.g.
`https://cloudflare-dns.com/dns-query` or `https://dns.google/resolve`; the resolver learns which
domains are being verified). Then an org owner or admin (portal → Single sign-on, or the API):

1. claims a domain (`POST /v1/orgs/:org/domains {"domain"}`) and publishes the TXT record it
   names, `_hitop-verify.<domain>` = `hitop-verify=<token>`, then verifies it (`POST
   .../domains/<domain>/verify`). One org at a time per domain; free-mail domains can't be claimed.
2. registers a confidential web client at their provider with the server's one redirect URI
   (`<public URL>/v1/auth/sso/callback`) and saves issuer, client id and secret (`PUT
   /v1/orgs/:org/sso`, owners). `POST .../sso/test` checks discovery and keys.

People sign in with `hitop sync login --sso --email <you@corp.example>` (1.1 and 1.2 clients too)
or Sign in with SSO and their address. An org's provider signs in only addresses of the org's
verified domains; "SSO required" turns every other sign-in off for those domains, including the
server's own SSO. On HITOP Cloud (`HITOP_SYNC_LIMITS=plans`) configuring needs Business or
Enterprise; signing in never stops when a plan lapses. Without a host key nothing of this is
reachable (404 `org_sso_off`), and the server's own `HITOP_SYNC_OIDC_*` SSO works as before.

### SCIM provisioning (HITOP Cloud C6)

An organization's identity provider (Okta, Microsoft Entra ID, Keycloak, OneLogin...) can add and
remove its members over SCIM 2.0 (RFC 7643/7644). Off for every org until an org owner creates a
SCIM token (portal → SCIM, or `POST /v1/orgs/:org/scim/tokens {"name"}` with a signed-in session;
shown once, stored hashed; at most 5; `DELETE .../scim/tokens/:id` revokes). In the provider: SCIM
base URL `<public URL>/scim/v2`, authentication "bearer token" (`hitop_scim_...`). On HITOP Cloud
(`HITOP_SYNC_LIMITS=plans`) it needs Business or Enterprise; a self-hosted server allows every team
org.

- **Users** (`/scim/v2/Users`): `userName` must be the person's email address. An address with an
  account becomes an org member, one without gets an org invite (it joins on first sign-in); each
  takes a seat (402 when the plan has none left). `active: false` (also the string `"False"`
  Entra ID sends) or `DELETE` takes the address out of the org and out of the workspaces SCIM gave
  it; the account and its personal workspaces stay. Filters: `userName`, `externalId`, `id` with
  `eq`; paging with `startIndex`/`count`; PATCH `add`/`replace`/`remove` with or without a path.
  Other attributes (title, phone...) are accepted and ignored: the server keeps only userName,
  externalId, displayName, name parts and active.
- **Groups** (`/scim/v2/Groups`): members by user id, PATCH `members` add/remove/replace and
  `members[value eq "<id>"]`. What a group gives is set by an org admin (portal, or `PUT
  /v1/orgs/:org/scim/groups/:id {"orgRole": ""|"member"|"admin", "workspaces": [{"workspace",
  "role": "viewer"|"editor"}]}`): an org role (when any group gives one, members without such a
  group are `member`) and roles in the org's workspaces.
- **Workspace roles and keys.** The server can't add anyone to a workspace (that takes the workspace
  key wrapped for the person's devices). A member already in the workspace gets the role at once;
  anyone else gets an invite that shows as accepted (with their device keys) once they have
  signed in on a device, and a workspace owner adds them as for any accepted invite (`hitop sync
  members`, check the fingerprint, `hitop sync add <email>`; hitop 1.1 and 1.2 do this already).
  When a group stops giving a workspace, SCIM removes only what it gave: the open invite, or the
  membership it brought (with that person's wrapped keys) plus a key rotation request for the
  owners (`hitop sync rotate`). People who were in the workspace before stay.
- Owners are never created, demoted or removed by SCIM (deactivating an org owner answers 409).
- Discovery: `/scim/v2/ServiceProviderConfig`, `/Schemas`, `/ResourceTypes` (with the token). No
  bulk, sort, ETags or password changes. Every change is on the org's audit chain with
  `scim:<token name>` as the actor. `GET /v1/orgs/:org/scim` (admins) lists tokens (never their
  values), directory size and groups.

### Passwords, Google, GitHub and Apple

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
| Apple | developer.apple.com → Certificates, Identifiers & Profiles: Sign In with Apple on an App ID (primary); a Services ID with your public URL's host as domain; a key with Sign in with Apple (download the `.p8`) | `<HITOP_SYNC_PUBLIC_URL>/v1/auth/apple/callback` |

Apple has no client secret to copy: the server signs a short-lived one with the `.p8` key (keep
it readable only by the service, for example with systemd `LoadCredential=`). Apple posts the
callback as a form, so a proxy in front must pass `POST /v1/auth/apple/callback`. People may hide
their address behind `@privaterelay.appleid.com`; to mail them, register your sender address or
domain under Services → Sign in with Apple for Email Communication. Set the App ID's
server-to-server notification endpoint to `<HITOP_SYNC_PUBLIC_URL>/v1/auth/apple/notifications`
(and the App ID in `HITOP_SYNC_APPLE_APP_IDS`): when someone stops using Sign in with Apple with
your server, or deletes their Apple account, the Apple sign-in is unlinked from their account.

Any Google, GitHub or Apple account with a verified address may sign in; the first time, it is linked
to the account with that address (or one is made). GitHub must report the address as the
account's primary and verified one. Addresses that must use SSO (`HITOP_SYNC_SSO_REQUIRED`)
can't use them. Signing in with Google, GitHub or Apple doesn't unlock end-to-end keys on a new
device: the password or the recovery phrase does.

### Run history, alerts and shared environment values

Runs your own CI and cron upload (`hitop run --sync-report`, `hitop sync report`) are kept as a
status row (passed or failed, timing, counts, the collection's and environment's names) plus the
full report sealed with the workspace key, which this server can't read; alerts (mail to members,
JSON webhooks, Slack) go out when a series changes status or misses its run (a worker thread
delivers them every few seconds; the missed-run check is hourly). Shared environment values
(`hitop sync secrets share`) are sealed for the devices the sharer picks. Nothing here is gated on
a self-hosted server; set `SMTP_URL` for alert mail. See THREAT_MODEL.md "Run history and alerts"
and "Shared environment values".

### Monitors, mock servers, team environments and hosted MCP

On personal-library collections (which the server can read; shared workspaces stay end to end
encrypted and out of reach):

- **Monitors** run a collection on a schedule (`/v1/library/collection/:id/monitor`, every 5 minutes
  to every day) and mail when it starts failing and when it passes again. Each run is a child
  process (`hitop-sync-server monitor-run`, under `ulimit -v`, killed after 150 s) whose HTTP client
  refuses private and reserved addresses, also for IP literals and redirects; add the host's own
  public address with `HITOP_SYNC_DENY_IPS=<ip>[,<ip>]`. `HITOP_SYNC_MONITOR_BIN=off` turns monitors
  off (default: this executable).
- **Mock servers** answer at `/mock/<id>/…` from the requests' saved examples (never HTML).
- **Team environments**: `/v1/orgs/:org/environments`, shared by an org's members without secret values.
- **Hosted MCP**: `POST /mcp` with a personal access token, for AI assistants.

Monitors and mock servers need a paid plan when `HITOP_SYNC_LIMITS=plans`.

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

Organizations (C5): owners and admins download a **signed export** of every chain of the org (the
org's own and each workspace's) as JSON lines (`GET /v1/orgs/:org/audit/export`, `hitop sync audit
--export --org <org>`), signed with the server's Ed25519 key (`GET /v1/audit/key`; its seed is a
row of `server_secrets`). Once a day the server signs an **anchor** of the org's chain heads and
mails it to the org's owners when a head moved (`HITOP_SYNC_AUDIT_ANCHORS=off` stops it; needs
`SMTP_URL`); owners may make the anchors public at `GET /v1/audit-anchors/:org/<day>.json` (it shows
how many workspaces the org has and their ids). `hitop sync audit --verify --anchor <file>` checks a
fresh export against a saved anchor: a log rewritten after the anchor left no longer matches. On
HITOP Cloud: Business and Enterprise.

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

### Support bundle (enterprise self-hosting)

`hitop-sync-server support-bundle [--out <file>] [--log <file> | --unit <unit> | --no-journal]
[--since-hours <n>]` writes one JSON file (mode 0600, never over an existing file) to send to
support: server version, schema in the database and of the binary, database kind and version,
OS; every `HITOP_SYNC_*`, `SMTP_*` and `RWS_CONFIG_*` setting by name with its value only when
it is on an allowlist of non-secret settings (everything else says `(set)`; database and SMTP
URLs are cut to scheme, host and port); what is in effect (mail, SSO, billing, host key...); row
counts per table (no content hashes); health counts (audit chains and broken ones, alert
deliveries, workspaces asking for a rotation); and the last 200 error lines of the server's log
(`--log`, or `journalctl -u hitop-sync` for the last 72 hours) with addresses, long ids, tokens,
IPs and URL queries replaced. It never holds customer data, ids, names or secret values. Read
it before you send it.

### Upgrades

Each release's database changes, settings and rollback notes are in
[UPGRADE_NOTES.md](UPGRADE_NOTES.md). A new version migrates the database on start, in one
transaction per migration; take a dump first (`hitop-sync-server dump <dir>`).

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
| `GET /v1/workspaces/:ws/run-policy` | what an upload carries (any member, runner credentials too): `{statusRows, failedNames, sealedHistory, historyDays, maxRunBytes}` |
| `POST /v1/workspaces/:ws/runs {id, startedAt, collection, environment, status, durationMs, requests, failed, failedNames, detail}` | a run's status row and its sealed report (runner credential, editor, owner; the one write a runner credential has) → 201 `{id, stored, note?}`; 402 `limit` over the plan's uploads a day |
| `GET /v1/workspaces/:ws/runs?collection=&environment=&before=&limit=`; `GET .../runs/:id` | the history (members): status rows newest first and each series' last status; one run with its sealed report (`detail`) |
| `GET`, `PUT /v1/workspaces/:ws/alerts {email, webhooks: [{kind: webhook\|slack, url}], failedNames, statusRows, missedAfterMinutes}`; `GET .../alert-deliveries` | alert settings and the alerts sent (owner); addresses must be members', webhooks https to public hosts; 402 `limit` for a channel the plan lacks |
| `GET /v1/workspaces/:ws/env-shares`; `PUT .../env-shares/:id {version, sealed, wraps}`; `POST .../env-shares/:id/wraps {version, wraps}`; `DELETE .../env-shares/:id` | shared environment values (sealed; one wrap of the share key per recipient device): list (members; runners see the ones wrapped for them), the next version (editor, owner; Pro and up with plans), wraps for new devices, stop sharing |
| `GET /v1/workspaces/:ws/changes?since=<seq>` | blobs changed after `seq` |
| `GET /v1/workspaces/:ws/blobs/:id` | envelope + version, vv, epoch |
| `PUT /v1/workspaces/:ws/blobs/:id {version, vv, epoch, content \| deleted}` | write version n+1 (owner, editor) |
| `GET /v1/auth/sso` | `{enabled, name, required, domains}` |
| `POST /v1/auth/sso/start {challenge, returnTo?, email?}` | `challenge` = base64url(SHA-256(verifier)) → `{url}` to open in a browser |
| `GET /v1/auth/sso/callback` | the provider's redirect (browser): sends a one-time code to `returnTo` (loopback or an allowed web origin) or shows it |
| `POST /v1/auth/sso/finish {code, verifier}` | → session |
| `GET /v1/auth/methods` | what a sign-in page can offer → `{password: {enabled, signup, reset}, emailCode, passkey, sso, google, github, apple}` |
| `POST /v1/auth/password/params {email}` | the account's password KDF `{kdf: {alg, m, t, p, salt}}` (made up, but stable, for an address without one) |
| `POST /v1/auth/password/signup {email, kdf, authKey}` | mails a confirmation code (202 for any address) |
| `POST /v1/auth/password/verify {token}` | the code → session (`created`); makes the account or adds the password |
| `POST /v1/auth/password/login {email, authKey}` | → session; 401 `invalid_credentials` for any wrong address or password; 429 after repeated failures |
| `POST /v1/auth/password/forgot {email}`; `POST /v1/auth/password/reset {token, kdf, authKey}` | mails a reset code (202); the code + new password → session and `bundleLost` (the password's key bundle is deleted, other sessions end) |
| `POST /v1/me/password {currentAuthKey?, kdf, authKey, bundle?}` | set or change (signed-in device; current required when there is one); `bundle` re-sealed under the new password replaces the old one, else it is deleted |
| `POST /v1/auth/google/start`, `POST /v1/auth/github/start` `{challenge, returnTo?}`; `GET .../callback`; `POST .../finish {code, verifier}` | as SSO, with Google / GitHub |
| `POST /v1/auth/apple/start`, `POST /v1/auth/apple/callback` (Apple's form), `POST /v1/auth/apple/finish` | as Google, with Apple |
| `GET /v1/me/devices` | your devices: `{devices: [{id, name, createdAt, lastUsedAt, workspaces, onlyHere, thisDevice}], limit}`; clients send `X-Hitop-Device: <key id>` (a session is tied to it, last use kept; `/v1/me` has `devices: true` on servers that read it) |
| `PUT /v1/me/keys/:id {name}`; `DELETE /v1/me/keys/:id[?force=true]` | rename a device; remove one (a device holding workspace keys needs `force`: its wraps go, those workspaces rotate, its sessions end; never the requesting device) |
| `POST /v1/auth/apple/notifications {payload}` | Apple's server-to-server notifications (a JWT signed by Apple): unlinks on consent revoked or account deleted |
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
an org's plan by hand (Enterprise contracts; billing sets Pro, Team and Business).

Billing (HITOP Cloud C3, `src/billing.rs`: Stripe Managed Payments, Stripe is the merchant of
record; off unless the Stripe secret key and webhook secret are set, so a self-hosted server sells
nothing):

| Endpoint | What |
| --- | --- |
| `POST /v1/billing/stripe` | Stripe webhook: `Stripe-Signature` `t=…,v1=…` = hex HMAC-SHA256 of `"<t>.<raw body>"`, `t` within 5 minutes (401 otherwise, before parsing); idempotent by event id; 200 `{outcome}` for every verified event (`applied`, `duplicate`, `stale`, `unmatched`, `ignored`, `livemode_mismatch`, `unknown_price`, `superseded`). Events: `checkout.session.completed`, `customer.subscription.created/updated/deleted/paused/resumed`, `invoice.paid`, `invoice.payment_failed`, `charge.refunded` |
| `GET /v1/orgs/:org/billing` | plan, status, seats bought and used, period end, `pastDue`, what can be bought (`offers`), the trial (member) |
| `POST /v1/orgs/:org/billing/checkout {plan, period, seats}` | a Stripe Checkout Session URL (`managed_payments[enabled]=true`, subscription mode, seats as quantity; Pro for a personal org, Team or Business per seat for a team org; admin, a session, not an access token) |
| `GET /v1/orgs/:org/billing/portal` | a fresh Stripe customer portal session URL (admin) |
| `POST /v1/orgs/:org/billing/seats {seats}` | Team and Business seats (the subscription item's quantity): more are invoiced at once, prorated; fewer take effect with no credit; never below the seats in use (admin) |
| `POST /v1/orgs/:org/billing/trial` | the 14-day Team trial, once per org, account and company email domain (owner) |

Settings: `HITOP_SYNC_STRIPE_SECRET_KEY[_FILE]` (`sk_…` or a restricted `rk_…`; a test key
counts only test-mode events, a live key only live ones), `HITOP_SYNC_STRIPE_WEBHOOK_SECRET[_FILE]`
(`whsec_…`); prices are found by lookup key (`pro_monthly`, `pro_yearly`, `team_monthly`,
`team_yearly`, `business_monthly`, `business_yearly`), or set one by id with
`HITOP_SYNC_STRIPE_PRICE_{PRO,TEAM,BUSINESS}_{MONTHLY,ANNUAL}`; `HITOP_SYNC_STRIPE_API_URL`
(tests), `HITOP_SYNC_TRIAL_DAYS` (14; 0 = none). Every call pins `Stripe-Version:
2026-10-28.endive`; create the webhook endpoint with the same version. Operator commands:
`hitop-sync-server billing-events [--unmatched] [--json]` (the latest events: ids and outcomes,
never bodies) and `billing-link <org> <subscription id>` (ties a subscription bought outside the
portal's checkout to its org).

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

Migration 9 (HITOP Cloud C5) adds `org_sso`, `org_domains`, `org_audit_settings`,
`audit_anchors` and `workspaces.rotation_requested_at|_by`; nothing changes for older clients
(`rotationRequested` appears in workspace answers only when an org admin asks for a rotation).
C5 endpoints: `GET/PUT/DELETE /v1/orgs/:org/sso`, `POST .../sso/test`, `GET/POST
/v1/orgs/:org/domains`, `POST .../domains/:domain/verify`, `DELETE .../domains/:domain`;
`GET /v1/orgs/:org/audit/export` (JSON lines), `GET/POST .../audit/anchors`, `PUT
.../audit/settings`, `GET /v1/audit/key`, `GET /v1/audit-anchors/:org/:day`; `GET
/v1/orgs/:org/admin`, `PUT .../admin/workspaces/:ws/owner {userId, keepOwners?}`, `POST
.../admin/workspaces/:ws/rotation`, `DELETE .../admin/devices/:key` (takes the device out of the
org's workspaces only; the key and the member's other workspaces stay). `GET /v1/auth/sso?email=`
names the org's provider; `POST /v1/auth/sso/start` takes `org` or finds it from `email`.

HITOP Cloud C6 (migration 10: `scim_tokens`, `scim_users`, `scim_groups`, `scim_group_members`,
`scim_group_workspaces`, `scim_grants`; nothing changes for older clients: SCIM's workspace
invites look like any accepted invite): `GET /v1/orgs/:org/scim`, `POST .../scim/tokens`,
`DELETE .../scim/tokens/:id`, `PUT .../scim/groups/:group`; the SCIM API under `/scim/v2`
(`ServiceProviderConfig`, `Schemas`, `ResourceTypes`, `Users`, `Groups`; GET, POST, PUT, PATCH,
DELETE; `application/scim+json`, SCIM error bodies).

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
