# hitop sync server

Keeps hitop collection folders in step across devices and team members, end-to-end encrypted:
clients encrypt every file before it leaves the device, and the server stores ciphertext,
version vectors and wrapped keys only. Design and limits: [THREAT_MODEL.md](THREAT_MODEL.md).

- `hitop-sync-server` (this crate): **AGPL-3.0-or-later** ([LICENSE](LICENSE)). If you run a
  modified version for others, offer them its source.
- `crates/hitop-sync-crypto`: the client-side encryption the CLI, desktop app and web app use.
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

The mail server's certificate is always verified (Mozilla roots). Run one server process per
database: writes are serialized in the process, and passkey challenges live in its memory.

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
| `GET /v1/workspaces/:ws/changes?since=<seq>` | blobs changed after `seq` |
| `GET /v1/workspaces/:ws/blobs/:id` | envelope + version, vv, epoch |
| `PUT /v1/workspaces/:ws/blobs/:id {version, vv, epoch, content \| deleted}` | write version n+1 (owner, editor) |

A write must name the next version and a version vector that descends from the stored one, under
the current epoch; otherwise 409 with the current state (`conflict` or `stale_epoch`): fetch,
merge, re-encrypt and write again.

## Development

```bash
cargo test --workspace                        # SQLite, in-process
HITOP_SYNC_TEST_POSTGRES=postgres://postgres:pw@127.0.0.1:5432/postgres cargo test --features postgres
cargo clippy --workspace --all-targets -- -D warnings
HITOP_SYNC_MAIL=log cargo run                 # sign-in mails printed to stderr
```
