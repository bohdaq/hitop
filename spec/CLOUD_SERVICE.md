# HITOP Cloud: the paid service

Plan for a hosted service on top of HITOP, for individual developers, small teams, mid-size
companies and enterprises. Written 2026-11-02 for the owner's approval; it becomes a work order
(tranches C0–C7, section 9) once approved. Every price and tier name in this file is a **proposal**:
the owner sets real prices (ROADMAP section 3: agents never set prices, change DNS, App Store or
money matters).

This file answers the owner's request of 2026-11-02: "create some nice web service to be used by
individual developers, small teams, medium and large corporations (paid features or subscription)".
The request is the owner's go-ahead for *planning* only. The ROADMAP section 2 line "No billing,
accounts for sale, or public launch: those wait for the owner" changes when the owner approves
this plan; building and launching then follow the tranches and handoffs below.

---

## 0. Owner decisions (2026-11-02)

These answers to the questions in section 10 replace the recommendations wherever they differ. Where a later section still says otherwise, this section wins until that section is rewritten.

| # | Decision | Differs from the recommendation? | What it changes |
| --- | --- | --- | --- |
| Q1 | Name: HITOP Cloud | no | |
| Q2 | Stay under `hitop.rws8.tech` | no | |
| Q3 | Billing: **Lemon Squeezy** (merchant of record) | yes (was Paddle) | C3 integrates Lemon Squeezy: checkout, subscriptions, webhooks into entitlements. The owner creates the store and API keys. |
| Q4 | Prices: Pro $6, Team $10 per seat, Business $20 per seat, Enterprise by quote (annual billing; monthly costs more) | no | Still a proposal until the owner sets them in Lemon Squeezy. |
| Q5 | Viewers count as seats; 5 free guest viewers per Team org | no | |
| Q6 | AGPL + paid support and indemnity, no dual licence | no | |
| Q7 | Portal, billing and later monitor code: this repo, `cloud/`, AGPL | no | |
| Q8 | **No cloud monitors until confidential computing** | yes (was runner-credential monitors on a separate host) | C4 drops hosted monitors. Paid tiers rely on hosting, storage, run history and alerts from the customer's own runners (CI, `hitop schedule`), SSO and audit. The "Cloud monitors" row in section 3 becomes "Runs from your runners: history + alerts", with the same limits on history days. |
| Q9 | Shared secret values: opt-in per environment, end-to-end to pinned devices | no | |
| Q10 | Run history: sealed under the workspace key; only status rows readable by the server | no | |
| Q11 | **Stay on the current VM** | yes (was two dedicated VMs) | C2 hardens the existing VM: Postgres on it, off-site encrypted backups and a restore drill, status page, resource limits. The service shares the host with other sites, so plan a move once revenue justifies it. No SLA above 99.5%. |
| Q12 | Transactional mail service before paid launch | no | The owner picks and creates it (Postmark or SES). |
| Q13 | **External crypto review later / not now** | yes (was before paid launch) | The paid launch doesn't wait for a review. The security page must say plainly that no external review has been done yet. ROADMAP section 6 keeps the review as an open owner item. |
| Q14 | Hosted Free tier: 3 members, 3 workspaces, 100 MB | no | |
| Q15 | No SSO on the shared test server until the Business tier | no | |
| Q16 | App Store builds: sign-in only, no purchase UI | no | The owner checks that Apple guideline 3.1.3 applies. |
| Q17 | **SOC 2 groundwork starts now** | yes (was when the first enterprise asks) | Policies, access reviews, change log and backups evidence start in C1/C2. The owner picks a compliance tool and auditor (owner handoff); Type 1 first. |
| Q18 | EU only at launch; US region later on demand | no | |

## 1. One page summary

**The offer.** HITOP stays a free, MIT, local-first API client with a git-native format, a CLI, a
GitHub Action, an MCP server and a self-hostable AGPL sync server with everything in it. HITOP
Cloud sells what people don't want to run themselves: a hosted, end-to-end encrypted team
workspace service, cloud monitors with run history and alerts, hosted SSO and audit export for
companies, and support, residency and an SLA for enterprises.

- **Name:** HITOP Cloud (the ROADMAP already calls it that). Portal at `cloud.hitop.rws8.tech`;
  the API host stays `sync.hitop.rws8.tech` (compiled into every client as the default).
- **Free forever:** every client, the format, CLI/CI/MCP/Action, the full AGPL sync server
  self-hosted (SSO, audit log, runners, nothing gated), and a limited hosted Free tier.
- **Paid (proposal):** Pro **$6**/month for one person; Team **$10** per seat/month; Business
  **$20** per seat/month (annual prices; monthly about 25% more); Enterprise by quote
  (hosted or self-hosted, with support and SLA). Between Hoppscotch/Bruno ($6) and Postman Team
  ($19), with E2E encryption, git-native files and CI/MCP that none of them combine.
- **Differentiation we can honestly claim:** the server can't read your collections (E2E); the
  files are YAML in your repo (review = pull request); the same runner in CLI, CI, Action, MCP and
  cloud monitors; AGPL self-host with no feature gates; secrets never in files.
- **The one hard design problem:** cloud monitors need a key. Solution: the existing *runner
  credential* (a viewer device with its own key), given to a separate "monitor" service that
  never shares a process or host with the sync server. Secret values are sealed by the owner's
  client to the runner's key (HPKE, the primitive we already use), so the sync server still
  stores only ciphertext. Copy: "HITOP's monitor can read this workspace. The sync server still
  can't."

**First three tranches.**

| ID | Tranche | Result |
| --- | --- | --- |
| C0 | Owner decisions and accounts (parallel to C1) | name/domain, legal entity, Paddle account, new VM + object storage + DNS, external crypto review commissioned, ToS/privacy/DPA drafted |
| C1 | Organizations, plans and entitlements in the sync server; portal MVP | org → plan → limits enforced; seats counted; portal: sign in, org, members, usage; Free tier limits live on the test server |
| C2 | Production hosting and recovery | dedicated EU host, Postgres, off-site encrypted backups with a restore drill, recovery key for lost devices, abuse limits, uptime check, status page |

C3 (billing + pricing page + in-client prompts) is the paid launch of Pro and Team. It needs C1,
C2 and the external review (C0) done.

---

## 2. Positioning and who pays

### What HITOP is today (site, 2026-11-02)

The site sells "a free API client with no account and no CORS errors": free web app, extensions and
CLI, $4.99 Mac app, $2.99 iOS app, paid once. The pricing FAQ says "Is there a subscription? No."
and the home page card says "Nothing to sign up for… no workspace sync, no analytics". The privacy
page promises no accounts except the optional sync test server, no body logging, no tracking.
`hitop sync` is live but labelled "in testing" with a hosted test server; the team guide tells
people not to put production credentials in a shared workspace before the external review.

HITOP Cloud must not break that story. The rule: **everything a person can do on their own device
or on their own server stays free and ungated.** Paid is only what runs on our machines, plus
contracts (support, SLA, residency, licenses).

### Segments

| Segment | Who | What they want | What they pay for | Tier |
| --- | --- | --- | --- | --- |
| Individual | one developer, side projects, consultants | sync between laptop, desktop and CI; a monitor on a hobby API | convenience, a few monitors, history | Free, Pro |
| Small team | 2–15 people, one or two products | shared collections without a Postman account, roles, invites, secrets not in Slack | hosting, seats, monitors, alerts | Team |
| Mid-size | 15–200 people, platform or QA teams | SSO with Okta/Entra/Google, audit, invoice billing, admin of many workspaces | SSO, audit export, admin, support | Business |
| Enterprise | >200 people, regulated | self-hosted or dedicated, DPA, SLA, security questionnaire, SOC 2 path, residency | contract, support, SLA, licence | Enterprise |

### Free forever (the open source story)

- All clients: CLI, desktop, web app, HitopKit, the Action, the Docker image, `hitop mcp`.
- The collection format (CC BY 4.0) and run report spec.
- The sync server (AGPL) with every feature it has: teams, roles, invites, runner credentials,
  OIDC SSO, audit log, Postgres. No licence keys, no feature flags, no seat counts in the
  self-hosted server. A company may run it for 5,000 people for free; AGPL obliges them to share
  changes they make if they offer it to others.
- Scheduled runs on your own runner (`hitop schedule --github|--cron`), forever.
- A hosted Free tier (section 3) that is enough for one person or a tiny team to try sync.

### What is paid

- Hosting at scale: more members, more workspaces, more storage, longer retention.
- Cloud monitors: HITOP runs your collections on a schedule, keeps history, sends alerts.
- Organization features on the hosted service: hosted SSO per org, SCIM, audit export, admin
  across workspaces, invoice billing, data residency.
- Contracts: support with response times, SLA, DPA, security questionnaires, an enterprise
  self-host offer (AGPL plus a support and indemnity contract; see section 7 on licensing).

### Honest differentiation

| Claim | True today | Competitor state (from `site/content/guides/hitop-vs-*.md`, checked 2026-11-01, and searches 2026-11-02) |
| --- | --- | --- |
| The server can't read your collections | yes: E2E (THREAT_MODEL), server links no crypto | Insomnia Cloud Sync "optionally end to end encrypted"; Postman, Apidog, Hoppscotch: server-readable |
| Collections are YAML files in your repo | yes, format v1 stable | Bruno (.bru files, Git features paid), Yaak (directory sync), Insomnia Git Sync (free ≤3) |
| One runner everywhere: CLI, CI, Action, MCP, monitors | yes but monitors | Postman: Newman/CLI + cloud monitors; Thunder Client CLI paid |
| Self-host the whole server, free | yes, AGPL | Hoppscotch self-host (Organization plan $6 for admin features); Postman/Insomnia: cloud only or enterprise on-prem |
| Secrets never in files, secret managers free | yes | Bruno and Thunder Client gate secret managers behind paid plans |
| No AI, no telemetry by default | yes | most add AI; irrelevant for pricing but part of the trust story |

What we don't have and shouldn't claim: a large ecosystem, mock servers, API docs publishing,
comments. Section 4 says what we build and what we leave out.

---

## 3. Tiers, limits and proposed prices

Prices are per month, billed annually; "monthly" is the month-to-month price. USD list; Paddle
shows local currency and tax. All numbers are **proposals for the owner**.

| | Free | Pro | Team | Business | Enterprise |
| --- | --- | --- | --- | --- | --- |
| **Price (proposal)** | $0 | $6 / month ($8 monthly), one person | $10 per seat ($12 monthly) | $20 per seat ($25 monthly) | quote; floor about $12k/year hosted or self-hosted |
| **Who** | try it, hobby | one developer | a team | a company | regulated, large |
| Members per org | 3 | 1 (plus guests, see below) | unlimited, paid per seat | unlimited, per seat | per contract |
| Shared workspaces | 3 | 25 | unlimited | unlimited | unlimited |
| Storage (ciphertext) | 100 MB | 2 GB | 10 GB per org + 1 GB per seat | 50 GB + 2 GB per seat | per contract |
| Devices per member | 3 | 10 | 10 | 10 | per contract |
| Runner credentials (own CI), per org | 5 | 10 | 10 per seat | 25 per seat | per contract |
| Cloud monitors | none | 5, 15-minute minimum interval | 25 pooled + 5 per seat, 5-minute minimum | 100 + 10 per seat, 1-minute minimum | per contract |
| Monitor run history | 7 days | 30 days | 90 days | 1 year | per contract |
| Alerts | email | email, webhook | email, webhook, Slack | plus PagerDuty-style webhook templates | plus custom |
| Shared environment secrets (E2E, opt-in) | no | yes | yes | yes | yes |
| Hosted SSO (OIDC per org) | no | no | no | yes | yes, plus SAML through an IdP bridge if asked |
| SCIM provisioning | no | no | no | add-on or Enterprise | yes |
| Audit log | own events | own events | workspace owners | org-wide, export + anchored chain | plus SIEM webhook |
| Admin console (all workspaces of the org) | no | no | basic | full | full |
| Data residency | EU (default host) | EU | EU | EU or US (second region, later) | choice, or dedicated |
| Billing | none | card | card | card or invoice (annual) | invoice, PO |
| Support | community (GitHub) | email, best effort | email, 2 business days | email, 1 business day | named contact, 4 business hours for Sev-1, shared channel |
| SLA | none | none | none | 99.5% (credits) | 99.9% once a second node exists (C6); dedicated or self-hosted |
| Self-hosted server | free, AGPL, no limits | same | same | same | AGPL + support/indemnity contract; optional dedicated hosted instance |

Notes:

- **Seat** = a distinct human account that is a member of at least one workspace owned by the org
  in the billing month. Runner credentials and monitor runners never count. Viewers count as
  seats (simple, like Postman); a free "guest viewer" quota per org (Team: 5) lets a team show a
  collection to a stakeholder without buying a seat. Owner's call: viewers free or paid.
- **Pro guests:** a Pro user may share a workspace with up to 2 other people (Free accounts) so an
  individual can work with one client or friend. Beyond that, Team.
- **Free** is deliberately useful for one person on three devices, not for a team: 3 members is
  enough to try sharing. It keeps the current test-server population.
- **Existing test-server accounts are grandfathered.** Today's server allows 50 workspaces per
  account, 256 MB per workspace and 10 runners per workspace, so some accounts will be over the
  Free limits on day one. Their personal org gets limits equal to max(Free, current usage);
  creates are refused only beyond that, nothing is deleted, and they get one notice (mail and Sync
  page) with the plan link.
- **One trial per account and per verified email domain**, otherwise free Team forever by making
  new orgs.
- **Annual discount** about 20–25%, in line with Hoppscotch and Apidog.
- **Overages:** monitors beyond the pool are bought in packs (10 monitors for $10/month) rather
  than metered per run: predictable for the buyer, simpler to meter for us.

### Why these prices

| Competitor | Prices (per user per month, annual) | Source |
| --- | --- | --- |
| Postman | Free (1 user); Solo $9; Team $19; Enterprise by quote | `hitop-vs-postman.md`, checked 2026-11-01 |
| Bruno | OSS $0; Pro $6; Ultimate $11 | `hitop-vs-bruno.md` |
| Insomnia (Kong) | Essentials $0; Pro $12; Enterprise $45 | `hitop-vs-insomnia.md` |
| Yaak | personal free; Individual $79/year; Business $149/user/year (≈ $12.40) | `hitop-vs-yaak.md` |
| Hoppscotch | Free; Organization $6 ($8 monthly) | web search 2026-11-02 (toolradar, frontdeskreview) |
| Apidog | Free (≤4); Basic $9; Professional $18; Enterprise $27 | web search 2026-11-02 (apidog.com/pricing via aggregators) |
| Thunder Client | Free (limited); Starter $3; Business $7; Enterprise $16 | web search 2026-11-02 (thunderclient.com/pricing) |
| HTTPie | free; no published paid plans | web search 2026-11-02 |

Reasoning in one line each:

- **Pro $6** matches Bruno Pro and Hoppscotch and undercuts Postman Solo ($9); an individual gets
  sync plus 5 monitors, which Postman's $9 doesn't give.
- **Team $10** sits above Bruno/Hoppscotch ($6) because we host and run monitors, and well under
  Postman Team ($19) and Insomnia Pro ($12), where the buyer compares "Postman but encrypted and
  in git".
- **Business $20** is Postman Team price with SSO, audit export and invoice billing, which Postman
  and Insomnia only sell at $45–49 Enterprise.
- **Enterprise quote** with a floor so the sales effort pays; the self-host offer is a support
  contract, not a licence to use (AGPL already allows use).

The owner may prefer round EUR prices (€5/€9/€19) for an EU company; Paddle handles currency
display either way.

---

## 4. Features per tier: exists today vs must be built

Legend: **have** = in main today; **build** = new code; **owner** = owner handoff.

| Feature | Tier | State | Notes |
| --- | --- | --- | --- |
| Hosted sync, E2E workspaces, invites, roles, rotation | Free+ | have | `sync-server/`, clients in CLI, desktop, web; 1.2 signed wraps |
| Organizations, plans, seats, per-org limits | all | build (C1) | today: users and workspaces only; limits are server-global env (`HITOP_SYNC_MAX_*`, 50 workspaces, 10 runners, 50 invites) |
| Account portal (sign in, org, members, usage, plan, invoices, API tokens) | all | build (C1, C3) | separate small app; shares the sync-server session |
| Device recovery key | all | build (C2) | THREAT_MODEL open item 4; a paying customer who loses a laptop must not lose the workspace |
| Web app device key not in localStorage | all | build (C2, partial) | THREAT_MODEL item 5: at least an explicit warning + recovery key; a WebCrypto non-extractable key can't drive the Rust HPKE code |
| Billing (checkout, seats, trials, dunning, invoices, tax) | Pro+ | build (C3) + owner | Paddle as merchant of record; webhooks → entitlements |
| Pricing page, in-client upgrade prompts | all | build (C3) | never blocking local use |
| Runner credentials for your own CI/cron | Free+ | have | `hitop sync runner create`, `hitop schedule --sync` |
| Cloud monitors (HITOP runs the collection) | Pro+ | build (C4) | separate "monitor" service with sealed runner credentials; see design below |
| Monitor run history and dashboards | Pro+ | build (C4) | report.json v1 (secrets masked) sealed under the workspace key by the runner; portal decrypts client-side |
| Alerts: email, webhook, Slack | Pro+/Team+ | build (C4) | status + link only, never bodies |
| Shared environment secret values (E2E) | Pro+ | build (C4) | opt-in per environment; sync-only blob, never in `environments/*.yaml`; changes a published promise ("secret values never sync"): privacy page and team guide updated with it |
| Hosted SSO per organization (OIDC) | Business | build (C5) | today: one issuer per server from env (`HITOP_SYNC_OIDC_*`); needs per-org client config in the DB, domain claims, `SSO required` per org |
| SCIM 2.0 provisioning | Business add-on / Enterprise | build (C6) | users + groups → org members and workspace roles |
| Audit log export, chain anchored outside the server | Business | build (C5) | signed JSONL export + daily anchor of chain heads (public, or mailed); closes THREAT_MODEL item 7 |
| Org-wide admin console | Team (basic) / Business | build (C5) | all workspaces of the org, members, transfer ownership, force rotation |
| Data residency (second region) | Business/Enterprise | build (C6) + owner | one more host in US; org pinned to a region at creation |
| SLA, status page, uptime history | Business+ | build (C2 status page) + owner | credits in the contract |
| Enterprise self-host offer | Enterprise | owner + build (C6) | AGPL + support/indemnity contract; `hitop-sync-server` already self-hostable; add a support bundle command and upgrade notes |
| API docs publishing | – | **not planned** | OpenAPI import exists; publishing docs is a different product; point to Redoc/Scalar from a repo |
| Mock servers | – | **not planned (revisit after C6)** | a mock server contradicts "the server can't read your collections" unless the user opts in; not our differentiation |
| Comments and review | – | **not planned** | the format is files in git: review happens in pull requests; the desktop Git panel exists |

### Cloud monitors with end-to-end encryption

The problem: a monitor that runs on our machines must decrypt the workspace and must have the
secret values, both of which the sync server is designed never to see
(`sync-server/tests/no_crypto_in_server.rs`; THREAT_MODEL "Server-side runs are not offered").

The design, built on what exists:

1. **A monitor is a runner credential.** The owner's client creates a runner (`POST
   .../runners`: viewer, own X25519 key, one-year session) exactly as for CI, and hands the
   credential to the **monitor service**, a separate binary (`hitop-monitor`, AGPL, in
   `cloud/monitor/`) on a separate process and, in production, a separate host from the sync
   server. The sync server keeps its invariant: no crypto, ciphertext only. The monitor runner
   appears in the member list as `monitor@hitop.cloud`-style account, with its fingerprint.
2. **Secrets are sealed to the runner.** The owner's client takes the environment's secret values
   from the keychain and seals them to the runner's public key with HPKE (Auth mode, sender = the
   owner's pinned device key; `hitop-sync-crypto` has it). The sealed envelope is stored as a
   sync-only blob (`monitor-secrets/<env>`), never in `environments/*.yaml`, so the format stays
   v1 and the files in git never carry a value. Only the monitor's device secret opens it.
3. **Runs produce report.json v1** with secrets masked (existing `runner::run_collection_with`
   masking). The monitor seals the report under the workspace key and writes it as a sync-only
   blob (`runs/<ts>`); the portal and clients decrypt and render history. The monitor keeps only
   plaintext *status* rows (pass/fail, duration, request count, time) for alerting and the
   dashboard summary; the owner can turn even those off ("alerts carry the status only").
4. **Revocation** = remove the monitor runner (existing removal with key rotation); monitors
   stop at once, and nothing written after the rotation is readable with the monitor's key.
   **The monitor host is a trusted component**: it holds runner credentials and opens shared
   secrets, so its compromise exposes every monitored workspace and the secret values shared with
   it. Sealing credentials at rest (age key in a root-only env file) protects against disk theft
   only. This is stated in THREAT_MODEL, the portal and the Sync page; the sync server's own
   guarantee is unchanged. `${secret:vault|aws|azure|op:...}` references can't resolve on a
   monitor (no customer credentials there): such requests fail with a note pointing to shared
   secret values or your own runner.
5. **Egress safety.** The monitor executes customer-written requests from our IPs. It reuses the
   proxy's address policy (`server/src/guard.rs`: resolved-address checks on every hop, bad ports,
   own IP denied), adds per-org rate limits and a 60 s run limit, and we publish the monitor
   egress IPs so customers can allowlist them. Private targets are not reachable from monitors by
   design: "use your own runner for internal APIs".
6. **How it's explained** (Sync page, portal, THREAT_MODEL "Teams"): "HITOP's monitor can read
   this workspace and the secret values you share with it. The sync server still can't. Monitors
   run on separate machines with their own key; remove the monitor to revoke it." Confidential
   computing (a TEE-attested runner) is a possible later step, not promised.

### Metering and quotas

Counted per org, enforced in the sync server (limits) and the monitor (schedules):
members (seats), guest viewers, workspaces, ciphertext bytes, devices per member, runner
credentials, monitors and their minimum interval, monitor runs per day (a ceiling against abuse,
not a price), alert channels, history retention days, invites per day, sign-in mails per hour,
API requests per minute per session. A usage table in the portal shows each against its limit.
Over the limit: creates are refused with a 402-style error that names the limit and the plan;
nothing already stored is deleted or locked (reads and pulls always work, so a lapsed plan never
takes data hostage).

---

## 5. The web service

### Shape

```
cloud.hitop.rws8.tech      portal (Lit, prerendered like the site): account, org, members, usage,
                           plan, invoices, API tokens, monitors, history dashboards, status
sync.hitop.rws8.tech       hitop-sync-server (AGPL): the API every client already uses, plus
                           /v1/orgs/*, entitlements, billing webhooks (C1, C3)
monitor (internal)         hitop-monitor (AGPL): schedules, runs, sealed reports, alerts;
                           separate process and host; talks to sync.* like any client
hitop.rws8.tech            site + /app/ web app (unchanged); pricing page; upgrade prompts
```

- **One app or two?** Two. The API client (web app and desktop) keeps its Sync page for daily
  work and gets a "Manage plan" link and tasteful limit notes; billing, invoices, org admin and
  dashboards live in the portal. Reason: the client is MIT and ships in the App Store; billing UI
  there raises IAP questions and bloats an offline tool.
- **Sessions:** the portal signs in with the sync server's existing sign-in (mail code, passkey,
  SSO) and uses its session; no second identity system.
- **Multi-tenancy:** one Postgres, tenant = org id on every org-scoped row; workspaces get an
  `org_id` (null = personal, counted under the owner's personal org). Row filters in every query
  are covered by the existing `tests/roles.rs` matrix extended with "other org".
- **Backups:** nightly `pg_dump` plus WAL archiving to an S3-compatible bucket in another
  provider or region, encrypted with an age key held offline by the owner; a monthly restore drill
  into a scratch database with a checksum of row counts, recorded in the ops log. Backups hold
  ciphertext, emails and membership: personal data under GDPR, retention 35 days.
- **Observability:** JSON logs without bodies or tokens (privacy page promise), metrics for
  requests, errors, queue depth, monitor lag, mail failures; an external uptime check from
  outside the provider; a public status page (static, updated by the uptime checker and by hand).
- **Abuse:** sign-in mail limits per address and IP (have), invite limits (have), per-org
  creates, per-session request rate, monitor egress policy, disposable-email block on Free,
  manual review queue for Free orgs that hit limits fast. Report-abuse address on the site.
- **Availability targets:** 99.5% on a single host with tested restores (what we can honestly
  promise after C2); 99.9% only after a second node and managed or replicated Postgres (C6,
  Enterprise). No target for the Free tier.

### Hosting: what changes for paid customers

Today everything runs on `bohdan-prod-donotdelete` (168.119.17.57): the front rws, rws8.tech, a
live mail server, Gitea, a PHP demo, hitop-server and the sync server with SQLite backed up by
hand. That is fine for a test server and wrong for paying customers: shared blast radius, no
off-host backups, one person's SSH as the only access path.

| Change | Why | Who |
| --- | --- | --- |
| A dedicated VM for HITOP Cloud, EU, same provider (Hetzner, where the VM is), 4 vCPU / 8 GB to start, with a second small VM for the monitor service | isolate from mail/Gitea; the monitor runs customer requests and must not share a host with the sync server | **owner** provisions; agent installs |
| Postgres on the sync host (the server already supports it), later managed or replicated | SQLite is single-writer and backup-by-copy | agent (C2) |
| S3-compatible bucket in another location for backups (object storage) | off-host, tested restores | **owner** creates; agent wires |
| DNS: move `sync.hitop.rws8.tech` A record to the new host; add `cloud.` and `status.` | the API name is compiled into clients; it must not change | **owner** |
| Front rws on the new host with ACME, rate limits, `X-Forwarded-For` as today | TLS, per-IP limits on `/v1/auth/*` | agent |
| Uptime checker + status page | SLA evidence, incident comms | agent; **owner** picks the checker account |
| Secrets on the host: Paddle keys, SMTP, OIDC, backup key, in root-only env files | as today | **owner** places keys |
| Access: a second admin path (owner's key on the new host; the agent through the tunnel pod as today), an ops runbook | one person is a single point of failure | both |
| Mail: keep sending through the existing `noreply@rws8.tech` relay or a transactional mail service | sign-in codes and alerts must be deliverable at volume; the current mail host is on another VM | **owner decides**; recommend a transactional service (Postmark/SES-class) once alerts exist |

Costs to expect (owner's numbers, rough): two VMs ~€40–60/month, object storage ~€5, uptime
checker free–€10, transactional mail ~€15 at low volume. The external review is the large one-off
(section 10).

---

## 6. Billing

### Provider

| Option | Fee (checked 2026-11-02, third-party) | Tax | Fit |
| --- | --- | --- | --- |
| **Paddle** (merchant of record) | 5% + $0.50 per transaction, plus currency conversion | Paddle is the seller: collects and remits EU VAT, UK VAT, US sales tax, GST; invoices in its name | **recommended**: one solo operator, EU-based, selling worldwide; B2B invoices and reverse charge handled; seat-based subscriptions and proration supported |
| Lemon Squeezy (MoR, Stripe-owned) | 5% + $0.50 plus surcharges (international, PayPal, subscriptions) | same model | fine for digital products; weaker B2B invoicing and dunning than Paddle |
| Stripe Billing + Stripe Tax | 2.9% + $0.30 + 0.7% Billing + 0.5% Tax (+ international 1.5%) | **we** are the seller: VAT OSS registration and filings in the EU, nexus tracking in the US | cheapest at scale and most flexible; right only if the owner's company already files VAT and wants to own invoicing. Stripe Managed Payments (MoR, preview) is a possible later switch |

Recommendation: **Paddle**. The owner creates the account (Paddle vets SaaS sellers, a few days),
sets the tax profile and payout bank, and creates products and prices matching section 3 in the
Paddle dashboard (sandbox first). The agent writes the webhook integration.

### What the code does (C3)

- Checkout: the portal opens Paddle's overlay checkout for a plan and seat count; Paddle
  collects card and tax details; we never see card data.
- Webhooks (`POST /v1/billing/paddle`, signature verified, idempotent by event id): subscription
  created, updated, past due, paused, cancelled; transaction completed. They update one
  `entitlements` row per org (plan, seats, monitor pool, status, period end). Entitlements are the
  only thing the rest of the server reads.
- Seats: the portal shows seats used vs bought; adding a member beyond the bought seats asks the
  owner to add seats (Paddle updates the subscription, prorated). Removing members reduces seats
  at the next renewal, never mid-term (simple, predictable).
- Trials: 14 days of Team for a new org, no card; after the trial the org drops to Free limits
  with nothing deleted (reads always work; creates beyond Free are refused with the plan note).
- Dunning: Paddle retries and mails; we show a "payment failed" banner for 14 days of grace, then
  Free limits.
- Invoices and receipts: Paddle's; linked from the portal. Business annual plans can pay by
  invoice through Paddle's invoicing.
- Tests: Paddle sandbox webhook replays in `sync-server/tests/billing.rs` (every event type,
  signature failure, replay, out-of-order); entitlements enforcement in `tests/limits.rs`.
- App Store: subscriptions are sold on the web only; the Mac and iOS apps sign in to an account
  and never show a buy button. The owner verifies whether guideline 3.1.3(b) (multiplatform
  services) applies to HITOP Cloud in their jurisdiction; if it does, the MAS/iOS builds either
  add IAP for the same plans or hide the Cloud features (compile-time flag, like telemetry).

---

## 7. Legal and compliance (owner)

Everything here is the owner's: agents draft text, the owner signs.

- **Legal entity and bank**: a company (or the owner's existing one) as the Paddle seller.
- **Terms of Service**: none exist today (`/terms/*.html` is the guides path and must keep
  working). New path `/legal/terms.html`, `/legal/privacy.html` (the current `/privacy.html` stays
  and links there), `/legal/dpa.html`, `/legal/sla.html`, `/legal/subprocessors.html`. Drafts by an
  agent, reviewed by a lawyer before the paid launch.
- **Privacy**: rewrite the privacy page for accounts, billing (Paddle), monitors (what the monitor
  sees), alerts (Slack/webhook recipients), logs, backups, retention, data subject rights, and the
  home/pricing copy ("Nothing to sign up for", "Is there a subscription? No") must change.
- **GDPR**: the owner is EU-based, so a DPA for Business/Enterprise with the standard clauses, a
  sub-processor list (hosting provider, Paddle, mail service, uptime checker, object storage), a
  record of processing, deletion on request (org delete → everything, within 30 days including
  backups' expiry), data export (the client already has it: `hitop sync pull`).
- **AGPL and the hosted service**: AGPL §13 requires offering the source of the running version to
  users of the network service. The code is public, so the portal footer shows the commit and a
  link; the monitor service is AGPL too for the same reason.
- **Enterprise self-host license vs "no CLA"**: ROADMAP section 2 (2026-11-01) chose MIT + AGPL +
  DCO and no CLA. A dual-licensed (commercial) copy of `sync-server/` needs the owner to hold the
  copyright of all of it; the first outside AGPL contribution makes that impossible. Options:
  (a) a license grant/CLA scoped to `sync-server/` and `cloud/` only; (b) no outside contributions
  to those directories; (c) **no commercial license at all**: enterprise self-host = AGPL (free)
  plus a paid support, upgrade and indemnity contract. Recommendation: **(c)** now; it sells what
  enterprises actually buy (someone to call) and keeps the licensing story simple. Revisit (a) only
  if a buyer insists on a non-AGPL copy.
- **SOC 2 path**: not before revenue. Order: written policies (access, change, incident, backup,
  vendor), the status page and incident log, the restore drill record, the external crypto review
  report; then a Type 1 when an enterprise deal asks for it (budget €15–30k with a platform like
  Vanta/Drata, the owner's choice). Until then: a security page with the threat model, the review
  summary and the sub-processor list answers most questionnaires.
- **Trademark**: `TRADEMARK.md` exists; "HITOP Cloud" falls under it. Optional EU trademark
  filing (owner).
- **Export/sanctions**: Paddle screens buyers as MoR. Nothing more needed at this scale.

---

## 8. Go-to-market basics

- **Pricing page** (`/pricing.html`, same URL): the tier table of section 3, "free forever"
  box first, the native apps keep their one-time prices as a separate row, FAQ rewritten
  ("Is there a subscription?" → "Only for HITOP Cloud; the apps and CLI stay free").
- **Home page**: the "Nothing to sign up for" card becomes "No account needed; HITOP Cloud is
  optional and end-to-end encrypted". One new card: "Monitors and team sync, encrypted".
- **Upgrade prompts in clients** (rules): never modal, never block a local action, never in the
  CLI except one line when a hosted limit is hit (`hitop sync` prints the limit and the plan
  URL, exit 2 as today). In the web app/desktop: a one-line note on the Sync page when a limit is
  reached, and a "Manage plan" link. No prompts in the MAS/iOS builds beyond the same Sync-page
  note (no purchase links there; owner checks the App Store rule).
- **Docs**: a "HITOP Cloud" guide (sign up, org, seats, monitors, alerts, SSO), the THREAT_MODEL
  "Teams" and "Monitors" sections, a security page, the self-host guide gets an "Enterprise
  support" paragraph. Comparison guides get a Cloud row.
- **Launch**: Cloud early access to the people on the test server first (mail), then the usual
  channels in `launch/` (owner posts; agents never post).
- **Sales for Business/Enterprise**: a contact form to the owner's mail, a one-page security
  overview PDF, the DPA. No sales tooling until the first deal.

---

## 9. Execution plan: tranches

Rules as in ROADMAP sections 1 and 3: agents commit to `main`, run every **Done when** check,
update this file's status table, record owner items in section 11. Never: set prices in code
without the owner's figures, change DNS/App Store/money, post publicly, touch the VM beyond what
ROADMAP allows, store secrets anywhere but root-only env files. Code for the service goes in
`cloud/` (portal, monitor) and `sync-server/` (orgs, entitlements, billing webhooks), all AGPL;
client changes stay MIT. Format v1 stays additive-only: every new thing is a sync-only blob or
server-side row, never a new file in `environments/`.

| ID | Tranche | Status | Needs |
| --- | --- | --- | --- |
| C0 | Owner decisions and accounts | open | owner |
| C1 | Organizations, plans, entitlements; portal MVP | open | approval of this plan; the 1.2 sync server deployed to the test server first (ROADMAP section 6, "1.2 signed key wraps") so migration 4 and the 1.2 deploy don't collide |
| C2 | Production hosting, backups, recovery key, abuse limits, status page | open | C0 (VM, bucket, DNS), C1 |
| C3 | Billing, pricing page, upgrade prompts: **paid launch of Pro and Team** | open | C1, C2, C0 (Paddle, legal, external review done) |
| C4 | Cloud monitors, run history, alerts, shared secrets | open | C2 (monitor host), C3 |
| C5 | Business tier: hosted SSO per org, audit export + anchoring, admin console, invoice billing | open | C3 |
| C6 | Enterprise: SCIM, second region, SLA plumbing, self-host support offer, SOC 2 groundwork | open | C5, owner (contracts) |
| C7 | Deferred: mock servers, docs publishing, comments | not planned | revisit after C6 with demand evidence |

### C0 Owner decisions and accounts (parallel with C1)

- Goal: everything only the owner can do, started now so C3 isn't blocked.
- Scope (owner): answer the open questions in section 10; legal entity for Paddle; Paddle
  account (sandbox + live), tax profile, payout; provision the two VMs, object storage and the
  DNS names; choose the uptime checker and transactional mail; commission the external crypto
  review (ROADMAP section 6 rows of T23/T24/T25/1.2 give the scope; budget ~€10–25k for a
  two-week review by a firm that does Rust and applied crypto); lawyer review of the legal drafts
  the agent writes in C3.
- Scope (agent): draft ToS, privacy update, DPA, sub-processor list, SLA text under
  `site/src/pages/legal/` behind a `draft` flag (not linked until the owner approves).
- **Done when:** the owner has ticked each row in section 11 for C0; `curl -sI https://cloud.hitop.rws8.tech/healthz` answers from the new host; the review firm's start date is recorded here.

### C1 Organizations, plans, entitlements; portal MVP

- Goal: the data model and enforcement that every paid feature hangs on, running on the test
  server with Free limits, so nothing is sold before it's enforced.
- Scope:
  - sync-server migration 4: `orgs` (id, name, plan, region, created, billing ref),
    `org_members` (role owner/admin/member), `workspaces.org_id`, `entitlements` (plan, seats,
    limits JSON, status, period end), `usage` counters; every account gets a personal org.
  - `GET/POST /v1/orgs`, members CRUD, `POST /v1/orgs/:org/workspaces` (or move a workspace into
    an org, owner only), `GET /v1/orgs/:org/usage`, `GET /v1/orgs/:org/limits`.
  - Enforcement of section 3 Free/Pro/Team/Business limits in one `limits.rs` module
    (seats, workspaces, storage, devices, runners, invites), with a 402-style JSON error
    `{error: "limit", limit, plan, upgradeUrl}`; reads and pulls never refused.
  - Clients: CLI and Sync page show the limit message as given; `hitop sync ls` shows the org;
    `hitop sync init --org <name>`.
  - Portal MVP (`cloud/portal/`, Lit + lit-material like the site, prerendered, served by a small
    rws app or by hitop-server at `cloud.*`): sign in (mail code / passkey / SSO via the sync
    server), org list, create org, members and roles, usage vs limits, API tokens (personal access
    tokens for the sync API with scopes and expiry, listed and revocable; audit event
    `token_created/revoked`).
  - Audit events for org changes; `tests/roles.rs` extended with org roles and "other org".
- **Done when:** `cd sync-server && cargo test --workspace` (new: `tests/orgs.rs` every org role ×
  operation, `tests/limits.rs` every limit at boundary for each plan, migration 3 → 4 upgrade of a
  T25-shaped database on SQLite and Postgres); `cd cli && cargo test --locked` with `tests/sync.rs`
  cases for a refused create and `--org`; `cd cloud/portal && npm test` (e2e against the real server
  binary: sign in, org, invite, usage, token); **client compatibility**: the released `hitop`
  1.2 binary (from GitHub Releases, not the tree) and the live web app run the full sync flow
  (login, init, push, pull, invite, runner) unchanged against a migration-4 server, with the
  personal org implied (a CI job downloads the release binary); test server deployed with Free
  limits, existing accounts migrated into personal orgs (count before == count after) and
  grandfathered limits (no existing account is over its limit after the migration: a query
  returns zero rows).

### C2 Production hosting, backups, recovery key, abuse limits, status page

- Goal: a host we can take money on.
- Scope: install on the new VM (front rws, hitop-sync-server with Postgres, hitop-server for the
  portal), move `sync.hitop.rws8.tech` by DNS (owner) with a dual-run window (SQLite → Postgres
  export/import tool `hitop-sync-server migrate-db`, tested on a copy); nightly `pg_dump` + WAL
  to the bucket, age-encrypted, 35-day retention, `deploy/restore-drill.sh` run and recorded;
  JSON logs without bodies; metrics endpoint; external uptime check; `status.hitop.rws8.tech`
  static page; ops runbook `cloud/RUNBOOK.md` (restore, rotate keys, revoke sessions, incident
  template).
  - **Recovery key**: `hitop sync recovery create` derives a second device key from a printed
    24-word phrase (BIP-39 wordlist or 32 bytes base32), registers it as a device named
    "recovery", and the owner's client wraps workspace keys for it like any device; `hitop sync
    recovery use` restores on a new device. Web app: same flow on the Sync page; the page warns
    that its device key lives in browser storage and offers the recovery key at first share.
  - Abuse: per-IP and per-org creates, disposable-mail list for Free, request rate per session
    (server-side, not only at rws), report-abuse page.
- **Done when:** `restore-drill.sh` restores last night's backup into a scratch database and
  `audit-verify` passes on it; `cargo test --features postgres --test postgres` on CI; recovery
  tests in `cli/tests/sync.rs` and `webapp/tests/sync.cjs` (lose the device, recover, read the
  workspace; a wrong phrase fails); uptime checker shows 7 days green before C3 launch; the old VM
  only proxies `sync.*` to the new host during the window, then the route is removed.

### C3 Billing, pricing page, upgrade prompts: paid launch of Pro and Team

- Goal: a person can buy Pro or a team can buy Team, and the limits lift at once.
- Needs: C1, C2, owner's Paddle live account and prices, legal pages approved, external review
  report received (findings fixed or accepted in THREAT_MODEL).
- Scope: Paddle checkout in the portal, `POST /v1/billing/paddle` webhooks → entitlements,
  seat management, 14-day Team trial, grace and downgrade to Free limits, invoices link; pricing
  page and home copy (section 8); Sync page limit notes and "Manage plan"; CLI limit line;
  `/legal/*` pages linked; privacy page updated; THREAT_MODEL "Hosted service" section; the Cloud
  guide; `sync-server/README.md` status line changes from "for testing" to the real status.
- **Done when:** `sync-server/tests/billing.rs` replays every Paddle sandbox event (signature,
  replay, order) and the entitlements match; an e2e buys Team in the Paddle sandbox and adds a
  4th member; dunning: a `past_due` replay shows the banner and `canceled` drops to Free with no
  data loss (pull still works); `site` build has no "no subscription" sentence left
  (`grep -r "Is there a subscription" site/src` empty); owner's green light recorded in section 11.

### C4 Cloud monitors, run history, alerts, shared environment secrets

- Goal: the feature people pay Postman for, without giving up E2E.
- Scope: `cloud/monitor/` (`hitop-monitor`, AGPL, links hitop-core's runner and
  hitop-sync-crypto; its own host): schedules per workspace (`monitors/<id>` sync-only blob with
  cron, env, bail, timeout, alert channels), the runner credential sealed at rest on the monitor
  host (age key in root-only env), pull → run → sealed `runs/<ts>` report blob + plaintext status
  row; egress guard from `server/src/guard.rs` (move it to a shared crate), per-org run ceilings,
  published egress IPs; alerts: email (SMTP path), webhook (JSON: status, collection, env, link,
  failed request names only), Slack incoming webhook; portal dashboards (decrypt client-side,
  pass/fail timeline, durations, last failures) and a CLI `hitop sync runs [--json]`; shared
  environment secrets: `hitop sync secrets share --env prod` seals the values to every pinned
  member device and to monitor runners (`monitor-secrets/<env>` blob), opt-in with a confirmation
  that names the recipients; clients merge shared values under their own keychain values.
  **Re-sealing:** a shared value is sealed per recipient device, so devices added later would
  miss it. Rule: whoever adds a member or trusts a new device (the adder's client) re-seals every
  shared environment it can open for the new device in the same operation (it already wraps the
  workspace key then); a sharer's client also re-seals for any pinned device without a copy on
  its next sync. Removal rotates the workspace key but can't take back values already seen:
  the Sync page says "rotate the shared secrets this person could see" (same as today for files).
  Sync page: "Monitors" section with the exact copy from section 4.
- **Done when:** `cloud/monitor` tests: a scheduled run against httpmock through a real sync
  server (sealed report readable by an owner client, not by the server; status row only
  plaintext; a removed monitor stops and can't read post-rotation blobs); egress guard tests
  (private IP, bad port, redirect to private refused); alert tests (webhook body has no response
  bodies or secret values, email sent once per state change); CLI `tests/sync.rs` shared-secret
  cases (a viewer without the opt-in sees names only; the server's files hold no value; a member
  and a device added after the share receive the values; a removed member can't open values
  re-sealed after the rotation); web `tests/sync.cjs` monitor flow; privacy page and team guide updated ("secret values never sync
  unless you share them").

### C5 Business: hosted SSO per org, audit export and anchoring, admin console, invoice billing

- Goal: what a company's IT and security teams require before they let a team buy.
- Scope: per-org OIDC configuration in the DB (issuer, client id, secret encrypted at rest with
  the host key, domains, groups claim, `required`), domain verification by DNS TXT so an org can
  claim `@corp.example`, the existing `oidc.rs` flow keyed by org; audit export (`GET
  .../audit/export` signed JSONL with the chain head) and a daily anchor of the org's chain
  heads mailed to the org owners (default) or, opt-in per org, published at
  `status.*/audit-anchors/<org>/<date>.json` (public anchors leak workspace count and ids, so
  not the default), closing THREAT_MODEL item 7; admin console in the portal (all org workspaces,
  members, transfer owner, force rotation, remove device org-wide); Paddle invoice billing for
  annual Business; priority support address and the SLA page text.
- **Done when:** `tests/sso.rs` and `tests/keycloak.rs` run per org (two orgs, two issuers, a
  user of org A can't sign in through org B's IdP); domain claim tests; export verify with
  `hitop sync audit --verify --anchor <file>`; admin console e2e; the T25 handoff ("hosted SSO:
  leave it off") is superseded here and noted in ROADMAP section 6.

### C6 Enterprise: SCIM, second region, SLA plumbing, self-host support offer, SOC 2 groundwork

- Goal: close enterprise deals without a bespoke project per customer.
- Scope: SCIM 2.0 server (`/scim/v2/Users|Groups`, bearer per org) mapping to org members and
  workspace roles; US region: second host pair, org pinned at creation, portal picks the API host
  per org (clients get the host from the invite/link, `HITOP_SYNC_SERVER` stays the default);
  replicated Postgres or a managed one for 99.9%; SLA measurement from the uptime checker into the
  status page; enterprise self-host package (`hitop-sync-server support-bundle`, upgrade notes per
  release, a signed Docker image, a contract template: support, upgrade windows, indemnity); SOC
  2 groundwork: policies in `cloud/policies/`, evidence folder structure, vendor list; security
  questionnaire answers page.
- **Done when:** SCIM conformance tests (create, update, deactivate, group → role); a two-region
  e2e (org in US, member signs in, data never in the EU database: a row-count check per region);
  restore drill and failover drill recorded; the owner has the contract template and the first
  questionnaire filled.

### C7 Deferred, not planned

Mock servers, API docs publishing, comments/review. Reason: none is our differentiation, two of
them need the server to read collections, and review already happens in pull requests. Revisit
after C6 when there is demand evidence (support mail, lost deals).

---

## 10. Risks and open questions (with recommendations)

| # | Question | Options | Recommendation |
| --- | --- | --- | --- |
| Q1 | Service name | A: HITOP Cloud; B: HITOP Teams; C: HITOP Sync | **A**. ROADMAP already uses it; covers monitors and enterprise, not only teams |
| Q2 | Domain | A: stay under `hitop.rws8.tech` (`cloud.`, `sync.`, `status.`); B: buy a product domain (for example hitop.dev) now and move everything | **A** now. The API host is compiled into clients; a domain move is a separate project with redirects; buy the domain anyway if free, use it later |
| Q3 | Billing provider | A: Paddle (MoR); B: Lemon Squeezy; C: Stripe Billing + Stripe Tax | **A**. Removes EU VAT OSS and non-EU tax filings for a solo operator; B2B invoices included. C only if the owner's company already files VAT and wants to own invoicing |
| Q4 | Prices | A: $6 / $10 / $20 as proposed; B: lower ($5/$8/$16) to lead on price; C: higher ($9/$15/$29) nearer Postman | **A**. Above Bruno/Hoppscotch because we host and run monitors; below Postman Team where the comparison will be made. Owner may set EUR equivalents |
| Q5 | Viewers | A: viewers count as seats, with 5 free guest viewers per Team org; B: viewers free, unlimited; C: all members count | **A**. B invites seat gaming (everyone a viewer, one editor); C makes a demo to a stakeholder cost money |
| Q6 | Enterprise license model | A: AGPL + paid support/indemnity contract (no dual license); B: dual license with a CLA scoped to `sync-server/` and `cloud/`; C: no outside contributions to those directories | **A**. Sells what enterprises buy, keeps "no CLA" (ROADMAP 2026-11-01). Revisit B if a buyer insists |
| Q7 | Code visibility of portal, billing and monitor code | A: in this repo under `cloud/`, AGPL; B: a private repo | **A**. AGPL §13 wants the hosted source offered anyway; a billing portal is no secret; one CI, one CLAUDE.md |
| Q8 | Monitor model | A: runner-credential monitors on a separate host (section 4); B: no monitors, only "bring your own runner"; C: wait for a confidential-computing runner | **A**. B leaves the main paid feature out; C is years and unproven. A is honest and revocable |
| Q9 | Shared environment secret values | A: opt-in per environment, E2E to members' pinned devices; B: never (status quo) | **A**. Teams need it; the promise changes from "never" to "only if you share them, encrypted" with the pages updated |
| Q10 | Run history storage | A: sealed under the workspace key, status rows plaintext; B: plaintext reports server-side for easy dashboards | **A**. The differentiation is the server can't read; dashboards decrypt in the portal |
| Q11 | Hosting | A: two dedicated Hetzner VMs (EU) + object storage elsewhere; B: stay on the current VM; C: a Kubernetes platform | **A**. B shares a host with the mail server and Gitea; C is more than a solo team needs now |
| Q12 | Mail for codes and alerts | A: a transactional mail service; B: keep `noreply@rws8.tech` on the current mail host | **A** before C4 (alert volume, deliverability, bounce handling); B is fine through C3 |
| Q13 | External crypto review timing | A: before C3 (paid launch), as ROADMAP requires; B: launch paid "beta" first, review in parallel | **A**. Taking money for E2E before the review undermines the one claim we make. Budget it in C0 |
| Q14 | Free hosted tier | A: 3 members / 3 workspaces / 100 MB; B: no hosted free tier (self-host is the free path); C: generous free (unlimited like Hoppscotch) | **A**. B kills the top of the funnel; C makes Team hard to sell and costs hosting |
| Q15 | Hosted SSO on the shared test server now | A: no (T25 recommendation stands until C5) ; B: yes with Google and the owner's domain | **A** |
| Q16 | App Store apps and subscriptions | A: sign in only, no purchase UI in MAS/iOS builds; B: add IAP for the same plans; C: hide Cloud features in store builds | **A** if guideline 3.1.3(b) doesn't apply (owner verifies); else B or C |
| Q17 | SOC 2 | A: after the first enterprise asks, Type 1 first; B: start now | **A** |
| Q18 | Second region | A: C6, only with a paying Business/Enterprise customer asking; B: launch with EU + US | **A** |

### Risks

- **Single operator.** One person runs the service and the company. Mitigation: runbook, tested
  restores, the agent's tunnel access, status page, and honest 99.5% until there's a second node.
- **Crypto review finds a design flaw.** Mitigation: review before money (Q13); the threat model
  already lists the residual risks, and the clients refuse bad keys, so a finding is more likely a
  fix than a redesign.
- **Monitors as an abuse vector** (our IPs hitting third parties). Mitigation: egress guard,
  ceilings, published IPs, Pro+ only (a card on file), manual review of fast growers.
- **Promise drift.** The site today says "no accounts, no sync, no subscription". Mitigation:
  C3 rewrites every such sentence in one commit with a grep check.
- **Paddle approval or terms.** Paddle may refuse or delay. Mitigation: apply in C0; Stripe +
  Stripe Tax as the fallback with the owner's company doing VAT OSS.
- **Format v1 pressure.** Monitor config and shared secrets must not become files in
  `environments/`. Mitigation: sync-only blobs by rule (section 9 header).
- **Seat gaming and plan downgrades.** Mitigation: viewers count (Q5), downgrades never delete,
  creates refused with a clear note.

---

## 11. Owner handoffs for this plan

| Date | Tranche | What the owner needs to do | Done |
| --- | --- | --- | --- |
| 2026-11-02 | plan | Approve or amend this plan: answer Q1–Q18 (section 10). Prices in section 3 are proposals; set the real ones. | |
| 2026-11-02 | C0 | Legal entity for the Paddle account; Paddle sandbox + live account, tax profile, payout bank; create products/prices in Paddle matching section 3; give the agent the sandbox API key and webhook secret through a root-only env file on the new host. | |
| 2026-11-02 | C0 | Provision two VMs (EU, same provider): `hitop-cloud` (4 vCPU / 8 GB, Postgres + sync server + portal) and `hitop-monitor` (2 vCPU / 4 GB); an S3-compatible bucket in another location for backups; DNS `cloud.hitop.rws8.tech`, `status.hitop.rws8.tech` → the cloud VM; plan the `sync.hitop.rws8.tech` move for C2. Agent access as today (tunnel pod) or an SSH key you place. | |
| 2026-11-02 | C0 | Commission the external review (scope: ROADMAP section 6 rows T23/T24/T25/1.2; `sync-server/THREAT_MODEL.md`, `cli/crates/hitop-sync-crypto`, `cli/crates/hitop-core/src/sync.rs`, `cli/crates/hitop/src/commands/sync/`, `webapp/src/sync.js`, `sync-server/src/{oidc,audit,workspaces,blobs}.rs`). Rough budget €10–25k. Tell the agent the start date and give it the report when done. | |
| 2026-11-02 | C0 | Pick an uptime checker and (by C4) a transactional mail service; create the accounts and place the keys. | |
| 2026-11-02 | C3 | Lawyer review of the drafts the agent writes: ToS, privacy update, DPA, sub-processor list, SLA. Confirm the App Store position (sign-in only, no purchase UI in the apps). | |
| 2026-11-02 | C3 | Green light for the paid launch after C1–C2 are done and the review report is in. Post the launch yourself (drafts in `launch/`). | |
