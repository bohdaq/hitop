# Changelog

Releases before 1.2 are listed in ROADMAP.md (section 5) and on the GitHub Releases page.

## Unreleased

### Lifecycle fixes (spec/STATE_MACHINES.md)

- **Security, web app:** a send or run in another tab no longer drops the secret-manager values
  a request in flight needs for masking, so its History entry can't keep a `${secret:...}` value in clear.
- Web app: a cancelled WebSocket connection or gRPC call can't change a newer one; gRPC shows
  Cancelled; deleting a request stops its send, socket and call; response panel and status chips
  carry `data-state` for tests and agents.
- `hitop ws`, desktop and the hitop-server bridge: closing ends within 5 s even when the server
  keeps sending and never answers Close.
- `hitop run`: report.json and JUnit `file` stay right when a folder has WebSocket or gRPC requests.
- Sync server: mock servers and monitors follow the plan in force (off after a lapse or refund);
  a Git sync no longer overwrites a disconnect or relink that happened during it; alert results
  are written only while queued; a SCIM PUT without `active` keeps the user's state.

### HITOP Cloud Enterprise: SCIM provisioning, self-host support package, SOC 2 readiness (C6, built, not deployed)

- **SCIM 2.0** (`/scim/v2`): an org's identity provider (Okta, Entra ID, Keycloak...) adds and
  removes members (userName = email address) and pushes groups; a group gives an org role and
  roles in the org's workspaces, set by an org admin on the portal's new SCIM page. A bearer token
  per org, created by an owner in the portal; off until one exists; Business and Enterprise on
  HITOP Cloud, every team org on a self-hosted server. SCIM gives access, never keys: someone not
  yet in a workspace shows to its owners as an accepted invite to add (`hitop sync members`, then
  `hitop sync add`, as in 1.1 and 1.2); removing someone takes out what SCIM gave and asks the
  owners to rotate. Owners are never changed by SCIM.
- **`hitop-sync-server support-bundle`**: versions, schema, redacted settings, row counts, health
  counts and scrubbed error lines in one file to send to support; never customer data or secrets.
- **Signed image** `ghcr.io/bohdaq/hitop-sync-server:<version>` (SQLite and Postgres) with each
  release, signed with cosign keyless and an SBOM attestation; the Dockerfile builds from the
  repository root. **Upgrade notes** per release: `sync-server/UPGRADE_NOTES.md`.
- Drafts (not in force): security questionnaire answers and the enterprise agreement template
  (support, upgrade windows, indemnity; AGPL stays the licence). SOC 2 Type 1 control → evidence
  map with the open owner decisions: `cloud/soc2-type1-readiness.md`.
- Sync server: migration 10 (`scim_tokens`, `scim_users`, `scim_groups`, `scim_group_members`,
  `scim_group_workspaces`, `scim_grants`). No new setting. Older clients keep working unchanged.

### HITOP Cloud Business: SSO per organization, signed audit export and daily anchors, admin console (C5, built, not deployed)

- **Single sign-on per organization**: an org claims its domains (a DNS TXT record
  `_hitop-verify.<domain>`, checked over DNS over HTTPS) and brings its own OpenID Connect
  provider (portal → Single sign-on). Its provider signs in only addresses of its verified
  domains; "SSO required" turns every other sign-in off for them. The client secret is sealed at
  rest under the server's host key. Sign in with `hitop sync login --sso --email <work address>`
  (1.1 and 1.2 clients too). Off unless the server has a host key (`HITOP_SYNC_HOST_KEY`); the
  hosted test server keeps it off.
- **Signed audit export and daily anchors**: `hitop sync audit --export --org <org>` downloads
  every chain of the org as signed JSON lines; once a day the server signs the org's chain heads
  and mails them to the owners (public anchors are opt-in). `hitop sync audit --verify --anchor
  <anchor.json>` (or `--file <export>` offline) recomputes the chains and catches a log rewritten
  after the anchor was sent. The server's audit key is pinned on first use.
- **Admin console** (portal → Admin): every workspace of the org with its people, every member's
  device; transfer a workspace's ownership, ask its owners to rotate the key, take a lost device out
  of the org's workspaces (its personal and other orgs' workspaces untouched). `hitop sync rotate` rotates a workspace's key (owners); status, pull and push say when
  an admin asked for it.
- **Business plan** ($20 per seat a month billed yearly, $240 a seat; $25 monthly; confirmed) sold through
  Stripe once its prices exist (lookup keys `business_yearly`/`business_monthly`); drafts of the SLA and a
  support page: one address for everyone, support@hitop.rws8.tech, best effort with Business
  answered first, no response-time promise (not in force yet). Migration 11
  (`workspace_excluded_keys`) keeps a removed device out of the org's workspaces.
- Sync server: migration 9 (`org_sso`, `org_domains`, `org_audit_settings`, `audit_anchors`,
  `workspaces.rotation_requested_at|_by`); new settings `HITOP_SYNC_HOST_KEY[_FILE]`,
  `HITOP_SYNC_DNS_URL`, `HITOP_SYNC_AUDIT_ANCHORS`. Older clients keep working unchanged.

### HITOP Cloud: run history and alerts from your own runners, shared environment values (C4, built, not deployed)

- **Run history**: `hitop run <folder> --sync-report` (as a member, or in CI with
  `HITOP_SYNC_RUNNER`) and `hitop sync report <report.json>` upload a run to the workspace: a
  status row (passed or failed, timing, counts, the collection's and environment's names) and the
  full report sealed end to end with the workspace key. `hitop sync runs [--json] [--id <run>]`
  lists the history and opens a report on your computer; the Sync page has **Run history**, the
  portal lists the status rows. `hitop schedule --sync` adds the upload (cron: `--sync-report
  --sync-secrets`; GitHub: a step after the Action, for hitop after 1.2). HITOP never runs your
  collections on its machines.
- **Alerts** when a collection and environment fail, pass again or miss a run: mail to members,
  a JSON webhook, Slack (`hitop sync alerts`, Run history → Alert settings, the portal; owners).
  Never a response, body or secret value; failed request names only if the owners allow them.
  Plans: Free keeps status rows 7 days with mail alerts; sealed reports and webhooks from Pro
  (30 days); Slack from Team (90 days); Business a year.
- **Shared environment values** (opt-in, end to end): `hitop sync secrets share --env prod`
  seals an environment's secret values for the trusted devices of the owners and editors
  (`--viewers`, `--runner` to add more); `hitop run --sync-secrets` and the app use them under your
  own values. New members and devices get them in the step that adds them; removing someone
  re-keys them (and says to rotate what that person saw). Secret values still never go into the
  files, and never sync unless you share them.
- hitop-sync-crypto 0.5.0: `seal_run`/`open_run`, `seal_share`/`open_share`,
  `seal_share_wrap`/`open_share_wrap` (`vectors/shared.json`), also in hitop-wasm.
- Sync server: migration 8 (`runs`, `alert_settings`, `alert_state`, `alert_deliveries`,
  `env_shares`, `env_share_wraps`), `/v1/workspaces/:ws/{run-policy,runs,alerts,alert-deliveries,
  env-shares}`, the hourly retention purge and missed-run check, an alert delivery worker.
  Settings `HITOP_SYNC_MAX_RUN_BYTES`, `HITOP_SYNC_RUN_RETENTION_DAYS`,
  `HITOP_SYNC_ALERTS_PER_HOUR`, `HITOP_SYNC_ALERT_WEBHOOKS`.

### HITOP Cloud: paid plans through Stripe Managed Payments (C3, built, not live)

- **Buy Pro, Team or Business in the portal** (Plan page): Pro for one person's account, Team
  and Business per seat for an organization, yearly or monthly, plus applicable tax. Stripe is the
  merchant of record (Stripe Managed Payments: the checkout says "Sold through Link"; Stripe
  handles VAT and sales tax and sends the receipts); HITOP never sees the card. **Manage billing**
  opens Stripe's customer portal (payment method, invoices, cancel). Admins change seats on the
  Plan page: more seats are invoiced at once (prorated); fewer take effect at once with no credit,
  and the lower price starts at the next renewal. (First built for Lemon Squeezy; moved to Stripe
  on the owner's decision of 2026-11-03 before anything was sold.)
- **14-day Team trial**, no card, once per organization, account and company email domain.
- **Nothing is ever deleted by a plan.** A failed payment shows a banner while Stripe
  retries; when the plan ends (expired, unpaid, paused, refunded or a trial over) Free limits
  apply: new workspaces, people or uploads beyond them are refused, everything stored stays
  readable.
- A refused create in the portal links **See plans**; the web app's Sync page shows **Manage
  plan** (not in the desktop app: no purchase links there).
- Site: the pricing page lists HITOP Cloud (Free, Pro $6, Team $10 per seat, Business $20, prices
  until the store shows the final ones) next to the free apps; the home page and privacy policy
  no longer say there is no sync; the security page and the terms, Cloud privacy notice, DPA, SLA
  and sub-processor list are drafts awaiting review (not linked yet).
- Sync server: `src/billing.rs`, migration 7 (`billing_events`, `trials`, the provider's ids on
  `entitlements`), `POST /v1/billing/stripe` (signed webhook events), `/v1/orgs/:org/billing*`,
  commands `billing-events` and `billing-link`. Settings `HITOP_SYNC_STRIPE_SECRET_KEY`,
  `HITOP_SYNC_STRIPE_WEBHOOK_SECRET` (prices by lookup key, `HITOP_SYNC_STRIPE_PRICE_*` overrides) and
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
