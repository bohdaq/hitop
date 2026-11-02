# Information security policy

Status: draft for the owner's approval (C1, 2026-11-02). Review yearly.

## Scope

HITOP Cloud: the hosted sync server (`sync.hitop.rws8.tech`, `sync-server/`), the account portal
(`cloud.hitop.rws8.tech`, `cloud/portal/`), the site and web app that link to them, the VM they run
on (`bohdan-prod-donotdelete`), its backups, and the accounts used to run them (section "Access
control"). The clients (CLI, desktop, web app, iOS) are open source and run on customers' devices;
they are in scope as far as they decide what reaches the server.

## Roles

- **Owner** (Bohdan Tsap): accountable for security, the only person with production access
  rights of his own, approves policies, vendors and every access grant.
- **Agents** (AI coding agents working for the owner): write code, tests and documents in the
  repository and deploy under the rules in `ROADMAP.md` section 3. Their access to the VM goes
  through the owner's tunnel and is listed in the access review. They never set prices, change DNS,
  App Store or payment settings, or post publicly.
- **Customers**: own their data and their device keys. HITOP can't read their collections.

## Principles

1. **The server can't read customer collections.** Workspaces are encrypted on the client
   (XChaCha20-Poly1305 blobs, HPKE-wrapped workspace keys, `sync-server/THREAT_MODEL.md`). The
   server holds ciphertext, public keys, wrapped keys and the metadata the threat model lists
   (account addresses, workspace ids, sizes, times, org names and memberships, plan and usage
   counters). `tests/no_crypto_in_server.rs` keeps the server from linking client crypto;
   `tests/plaintext.rs` and the client tests check the database holds no plaintext.
2. **Secrets stay out of the repository.** Keys and passwords live in root-only env files on the
   VM (`/etc/hitop-sync/env`, mode 0600) or the owner's password manager; never in git, logs,
   fixtures or reports.
3. **Least privilege.** The sync server runs as a systemd dynamic user with `ProtectSystem=strict`
   and no home; it listens on loopback only, behind the front rws (TLS, rate limits).
4. **Tokens are stored hashed.** Sessions, sign-in links, invites and access tokens: only their
   SHA-256 is in the database.
5. **Everything is logged that matters for security**, without bodies or secrets: the audit log
   (hash chained per workspace, per org and per account, `sync-server/src/audit.rs`).
6. **Honesty.** The security page says what isn't done: no external crypto review yet, one shared
   server. Those sentences stay until they are untrue.

## Known gaps (tracked in spec/CLOUD_SERVICE.md)

- One shared VM; no SLA above 99.5% (C2 hardens it).
- Off-site encrypted backups and restore drills start in C2.
- No external cryptography review yet (owner item, Q13).
- One operator: a second admin path is planned in C2.
