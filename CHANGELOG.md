# Changelog

Releases before 1.2 are listed in ROADMAP.md (section 5) and on the GitHub Releases page.

## Unreleased

### HITOP Cloud: paid plans through Lemon Squeezy (C3, built, not live)

- **Buy Pro or Team in the portal** (Plan page): Pro for one person's account, Team per seat for
  an organization, yearly or monthly. The checkout and the receipts are Lemon Squeezy's (merchant
  of record); HITOP never sees the card. **Manage billing** opens Lemon Squeezy's customer portal
  (card, receipts, cancel). Team admins change seats on the Plan page: more seats are charged at once
  (prorated); fewer take effect at once with no credit, and the lower price starts at the next
  renewal.
- **14-day Team trial**, no card, once per organization, account and company email domain.
- **Nothing is ever deleted by a plan.** A failed payment shows a banner while Lemon Squeezy
  retries; when the plan ends (expired, unpaid, paused, refunded or a trial over) Free limits
  apply: new workspaces, people or uploads beyond them are refused, everything stored stays
  readable.
- A refused create in the portal links **See plans**; the web app's Sync page shows **Manage
  plan** (not in the desktop app: no purchase links there).
- Site: the pricing page lists HITOP Cloud (Free, Pro $6, Team $10 per seat, Business $20, prices
  until the store shows the final ones) next to the free apps; the home page and privacy policy
  no longer say there is no sync; the security page and the terms, Cloud privacy notice, DPA, SLA
  and sub-processor list are drafts awaiting review (not linked yet).
- Sync server: `src/billing.rs`, migration 7 (`billing_events`, `trials`, Lemon Squeezy ids on
  `entitlements`), `POST /v1/billing/lemonsqueezy` (signed webhooks), `/v1/orgs/:org/billing*`,
  commands `billing-events` and `billing-link`. Settings `HITOP_SYNC_LS_*` and
  `HITOP_SYNC_TRIAL_DAYS`; billing stays off on a self-hosted server.

### hitop sync: normal sign-in and sign-up (HITOP Cloud C1b)

- **Email and password.** `hitop sync signup --email you@example.com` (or **Create account** on
  the Sync page and in the portal) makes an account with a password, confirmed by a mailed code.
  `hitop sync login --email ...` now asks for the password (`--password-stdin` for scripts); the
  mailed code is `--code` (alone it mails one, as `login --email` did before). Forgot it:
  `hitop sync password forgot` / `reset`; change it: `hitop sync password set`.
- **The password unlocks your keys on a new computer.** It never leaves the device: hitop derives
  an auth key the server keeps only as a slow hash, and a second key that seals your device's
  keys (the same key bundle as the recovery phrase). Sign in with it on a new computer and it
  becomes your device again, so teammates trust nothing new. A reset by mail can't open the old
  bundle: hitop says so before and after, and the recovery phrase or another device gets the
  keys back (`hitop sync password unlock` once a device sealed them again).
- **Continue with Google / Continue with GitHub** (`hitop sync login --google|--github`, buttons on
  the Sync page and the portal) where the server offers them; they sign in but don't unlock keys.
- hitop-sync-crypto 0.4.0: `derive_password_keys`, `PasswordKdf` (Argon2id + HKDF), test vectors
  in `vectors/password.json`. Additive: released 1.1 and 1.2 clients work unchanged.
- Sync server: migration 6 (passwords), `/v1/auth/password/*`, `/v1/auth/{google,github}/*`,
  `GET /v1/auth/methods`, `POST /v1/me/password`. Settings `HITOP_SYNC_PASSWORDS`,
  `HITOP_SYNC_PASSWORD_PEPPER[_FILE]`, `HITOP_SYNC_GOOGLE_CLIENT_ID/SECRET`,
  `HITOP_SYNC_GITHUB_CLIENT_ID/SECRET`.

## 1.2.0 (2026-11-02)

### Same Content-Type on every surface

- `hitop run`, `hitop send`, `hitop requests send`, `hitop snippet`, `hitop mcp` and the iOS
  companion (HitopKit) now do what the web app and desktop have done since 1.1.0: a POST, PUT or
  PATCH with a raw body and no `Content-Type` header is sent with `application/json`,
  `application/xml`, `text/html` or `text/plain`, decided from the body as sent. A header of the
  request's own (or one its pre-request script sets) wins; nothing is written to the collection.
  Post-request scripts see the header in `pm.request`, and snippets show it.
- A URL without a scheme is sent with `https://` (`http://` for `localhost`, `127.x.x.x`,
  `0.0.0.0` and `[::1]`) in the CLI, MCP and HitopKit too, so `base: api.example.com` works in
  every app. Anything else that isn't a URL fails before sending with
  `"<url>" isn't a URL. Enter a full URL, such as https://api.example.com/users`.
- One implementation: hitop-core `sending` (the web app and desktop reach it through hitop-wasm;
  HitopKit follows it), specified in `spec/sending.md` with test vectors in
  `spec/vectors/send.json`. Collection format v1 is unchanged.
- Behaviour change for CLI users: a run that sent a raw body without `Content-Type` now sends one.
  Add your own `Content-Type` header to keep a server that rejects the detected one happy.

### hitop sync: authenticated workspace keys

- Workspace keys are now handed out with authenticated wraps (HPKE Auth mode, made with the
  sending device's key, the one whose fingerprint people compare). Every client (CLI, desktop, web
  app) uses a key only when a device it trusts made the wrap, never takes a different key for an
  epoch it already has, and notices when the server hides a key rotation. A malicious sync server
  can no longer give a new member a workspace key of its own choosing.
- Invite codes carry the inviter's key fingerprint (`<code>.<fingerprint>`), so the invitee's
  hitop checks who sent the key; `hitop sync invite` prints it (also for mailed invites, to send
  yourself). A key from a device you haven't trusted is refused with its fingerprint:
  `hitop sync trust '<fingerprint>'`, or "Trust this device…" on the Sync page.
- Workspaces shared with 1.1 keep working, with a warning; the owner's next sync upgrades their
  keys. hitop 1.1 can't read the new keys: everyone in a shared workspace updates to 1.2, and the
  sync server must be updated first. Runner credentials made with 1.2 need a 1.2 image; 1.1
  credentials keep working with a warning.
- `hitop-sync-crypto` 0.2.0 (new API: `seal_wrap`, `open_wrap`, `accept_wraps`), test vectors
  `vectors/v2.json`. Design: `sync-server/THREAT_MODEL.md`.
