# HITOP Cloud: the paid service

Plan for a hosted service on top of HITOP, for individual developers, small teams, mid-size
companies and enterprises. Written 2026-11-02 and aligned the same day with the owner's
decisions (section 0). It is the work order for tranches C0–C7 (section 9). Every price and tier
name in this file is a **proposal** until the owner sets it in Stripe (ROADMAP section 3:
agents never set prices, change DNS, App Store or money matters).

This file answers the owner's request of 2026-11-02: "create some nice web service to be used by
individual developers, small teams, medium and large corporations (paid features or subscription)".
The ROADMAP section 2 line "No billing, accounts for sale, or public launch: those wait for the
owner" is replaced by this plan: building and launching follow the tranches and handoffs below.

---

## 0. Owner decisions (2026-11-02)

The owner's answers to Q1–Q18 (section 10). The rest of this file follows them.

| # | Decision | Changed from the first draft? |
| --- | --- | --- |
| Q1 | Name: HITOP Cloud | no |
| Q2 | Stay under `hitop.rws8.tech` | no |
| Q3 | Billing: **Stripe Managed Payments** (Stripe is the merchant of record); since 2026-11-03, see Later decisions | yes (was Paddle, then Lemon Squeezy) |
| Q4 | Prices: Pro $6, Team $10 per seat, Business $20 per seat, Enterprise by quote (annual; monthly costs more); a proposal until set in Stripe | no |
| Q5 | Viewers count as seats; 5 free guest viewers per Team org | no |
| Q6 | AGPL + paid support and indemnity, no dual licence | no |
| Q7 | Portal and billing code: this repo, `cloud/`, AGPL | no |
| Q8 | **No hosted monitors until confidential computing**; paid tiers get run history and alerts from the customer's own runners | yes (was runner-credential monitors on a separate host) |
| Q9 | Shared secret values: opt-in per environment, end-to-end to pinned devices | no |
| Q10 | Run history: sealed under the workspace key; only status rows readable by the server | no |
| Q11 | **Stay on the current VM** (`bohdan-prod-donotdelete`), hardened; no SLA above 99.5% | yes (was two dedicated VMs) |
| Q12 | Transactional mail service before paid launch (owner picks: Postmark or SES) | no |
| Q13 | **External crypto review later**; the paid launch doesn't wait; the security page says no review has been done yet | yes (was before paid launch) |
| Q14 | Hosted Free tier: 3 members, 3 workspaces, 100 MB | no |
| Q15 | No SSO on the shared test server until the Business tier | no (C5 built per-org SSO; it stays off there: no host key) |
| Q16 | App Store builds: sign-in only, no purchase UI (owner checks guideline 3.1.3) | no |
| Q17 | **SOC 2 groundwork starts now** (C1/C2); owner picks a compliance tool and auditor; Type 1 first | yes (was when the first enterprise asks) |
| Q18 | EU only at launch; US region later on demand | no |

### Later decisions

| Date | Decision | What it changes |
| --- | --- | --- |
| 2026-11-03 | Normal sign-in and sign-up before C2 is applied: email + password (sign-up with email verification, password reset) **and** "Continue with Google" / "Continue with GitHub". Mail codes, passkeys and SSO stay. | New tranche C1b (after C2's code, before C2 is applied on the VM). Owner registers the Google and GitHub OAuth apps. |
| 2026-11-03 | **Billing through Stripe Managed Payments**, replacing Lemon Squeezy (Q3): Stripe is the seller of record and handles VAT and sales tax, receipts, fraud, disputes and order support ("Sold through Link"). The owner set up the Stripe sandbox: products Pro, Team, Business, USD prices, tax-exclusive, tax category "SaaS – business use", flat rate with quantity = seats, lookup keys `pro_monthly`, `pro_yearly`, `team_monthly`, `team_yearly`, `business_monthly`, `business_yearly`. | C3's provider code is replaced (built for Lemon Squeezy, deployed with billing off, nothing sold): Checkout Sessions with `managed_payments[enabled]=true`, webhook `/v1/billing/stripe`, prices by lookup key, the customer portal for changes. Every "Lemon Squeezy" in this file reads Stripe now; the provider-independent parts (entitlements, seats, trial, grace) stay. Status in C3 below. |
| 2026-11-03 | The password also unlocks the end-to-end keys on a new device (a password-derived key wraps the account's key bundle, like the recovery phrase). | The server stores one more wrapped bundle per account (never the password or keys). A forgotten password loses the keys unless the person has a recovery phrase or another device; reset says so. Google/GitHub sign-in alone doesn't unlock keys. |

## 1. One page summary

**The offer.** HITOP stays a free, MIT, local-first API client with a git-native format, a CLI, a
GitHub Action, an MCP server and a self-hostable AGPL sync server with everything in it. HITOP
Cloud sells what people don't want to run themselves: a hosted, end-to-end encrypted team
workspace service, run history and alerts for the runs their own CI and schedules already do,
hosted SSO and audit export for companies, and support and contracts for enterprises.

- **Name:** HITOP Cloud. Portal at `cloud.hitop.rws8.tech`; the API host stays
  `sync.hitop.rws8.tech` (compiled into every client as the default).
- **Free forever:** every client, the format, CLI/CI/MCP/Action, the full AGPL sync server
  self-hosted (SSO, audit log, runners, nothing gated), and a limited hosted Free tier.
- **Paid (proposal):** Pro **$6**/month for one person; Team **$10** per seat/month; Business
  **$20** per seat/month (annual prices; monthly about 25% more); Enterprise by quote
  (hosted or self-hosted, with support). Sold through Stripe Managed Payments (Stripe is the merchant of record).
- **Differentiation we can honestly claim:** the server can't read your collections (E2E); the
  files are YAML in your repo (review = pull request); the same runner in CLI, CI, Action and MCP;
  AGPL self-host with no feature gates; secrets never in files.
- **No hosted monitors.** HITOP never runs a customer's collection on its machines: that would need
  a key the server is designed never to hold. Customers run on their own runners (CI,
  `hitop schedule`); HITOP Cloud stores the sealed run reports and sends alerts from the plaintext
  status rows. Hosted monitors wait for confidential computing (section 4).
- **Honest about what isn't done:** HITOP Cloud runs on one shared VM (99.5% at most) and no
  external crypto review has been done yet. The security page says both plainly.

**Tiers at a glance (proposal).**

| | Free | Pro | Team | Business | Enterprise |
| --- | --- | --- | --- | --- | --- |
| Price (annual) | $0 | $6 | $10 per seat | $20 per seat | quote |
| Members / workspaces | 3 / 3 | 1 (+2 guests) / 25 | per seat / unlimited | per seat / unlimited | contract |
| Run history from your runners | 7 days | 30 days | 90 days | 1 year | contract |
| Alerts | email | email, webhook | + Slack | + webhook templates | + custom |
| SSO, audit export, admin | no | no | basic admin | yes | yes + SCIM |

**First tranches.**

| ID | Tranche | Result |
| --- | --- | --- |
| C0 | Owner decisions and accounts (parallel to C1) | Stripe account (Managed Payments), mail service, backup bucket, DNS names, compliance tool picked, legal drafts |
| C1 | Organizations, plans, entitlements; portal MVP; SOC 2 policies | org → plan → limits enforced; portal: sign in, org, members, usage; written policies and access review |
| C2 | Harden the current VM | Postgres on the VM, systemd resource limits, off-site encrypted backups + restore drill, recovery key, status page, off-host uptime check, runbook |
| C3 | Stripe Managed Payments billing, pricing page, prompts | **paid launch of Pro and Team**; no review gate |
| C4 | Run history and alerts from your own runners; shared environment secrets | sealed history, status rows, email/webhook/Slack alerts |

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
contracts (support, SLA, licenses).

### Segments

| Segment | Who | What they want | What they pay for | Tier |
| --- | --- | --- | --- | --- |
| Individual | one developer, side projects, consultants | sync between laptop, desktop and CI; know when a scheduled run fails | convenience, history, alerts | Free, Pro |
| Small team | 2–15 people, one or two products | shared collections without a Postman account, roles, invites, secrets not in Slack | hosting, seats, history, alerts | Team |
| Mid-size | 15–200 people, platform or QA teams | SSO with Okta/Entra/Google, audit, admin of many workspaces | SSO, audit export, admin, support | Business |
| Enterprise | >200 people, regulated | self-hosted or dedicated, DPA, security questionnaire, SOC 2 report | contract, support, licence terms | Enterprise |

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
- Run history and alerts: your CI and `hitop schedule` runs upload a sealed report; HITOP Cloud
  keeps the history and sends alerts when the status changes.
- Organization features on the hosted service: hosted SSO per org, SCIM, audit export, admin
  across workspaces.
- Contracts: support with response times, DPA, security questionnaires, an enterprise
  self-host offer (AGPL plus a support and indemnity contract; see section 7 on licensing).

### Honest differentiation

| Claim | True today | Competitor state (from `site/content/guides/hitop-vs-*.md`, checked 2026-11-01, and searches 2026-11-02) |
| --- | --- | --- |
| The server can't read your collections | yes: E2E (THREAT_MODEL), server links no crypto | Insomnia Cloud Sync "optionally end to end encrypted"; Postman, Apidog, Hoppscotch: server-readable |
| Collections are YAML files in your repo | yes, format v1 stable | Bruno (.bru files, Git features paid), Yaak (directory sync), Insomnia Git Sync (free ≤3) |
| One runner everywhere: CLI, CI, Action, MCP | yes | Postman: Newman/CLI + cloud monitors; Thunder Client CLI paid |
| Run history the server can't read | after C4 | Postman monitors and run history are server-readable |
| Self-host the whole server, free | yes, AGPL | Hoppscotch self-host (Organization plan $6 for admin features); Postman/Insomnia: cloud only or enterprise on-prem |
| Secrets never in files, secret managers free | yes | Bruno and Thunder Client gate secret managers behind paid plans |
| No AI, no telemetry by default | yes | most add AI; irrelevant for pricing but part of the trust story |

What we don't have and shouldn't claim: hosted monitors, a large ecosystem, mock servers, API docs
publishing, comments, an external security review. Section 4 says what we build and what we leave
out.

---

## 3. Tiers, limits and proposed prices

Prices are per month, billed annually; "monthly" is the month-to-month price. USD list; Stripe
shows local currency and adds tax at checkout. All numbers are **proposals for the owner**.

| | Free | Pro | Team | Business | Enterprise |
| --- | --- | --- | --- | --- | --- |
| **Price (proposal)** | $0 | $6 / month ($8 monthly), one person | $10 per seat ($12 monthly) | $20 per seat ($25 monthly) | quote; floor about $12k/year hosted or self-hosted |
| **Who** | try it, hobby | one developer | a team | a company | regulated, large |
| Members per org | 3 | 1 (plus guests, see below) | unlimited, paid per seat | unlimited, per seat | per contract |
| Shared workspaces | 3 | 25 | unlimited | unlimited | unlimited |
| Storage (ciphertext) | 100 MB | 2 GB | 10 GB per org + 1 GB per seat | 50 GB + 2 GB per seat | per contract |
| Devices per member | 3 | 10 | 10 | 10 | per contract |
| Runner credentials (own CI), per org | 5 | 10 | 10 per seat | 25 per seat | per contract |
| Runs from your runners (CI, `hitop schedule`): sealed history + plaintext status rows | status rows only, 100 runs/day | yes, 1,000 runs/day | yes, 5,000 runs/day per org | yes, 20,000 runs/day per org | per contract |
| Run history | 7 days | 30 days | 90 days | 1 year | per contract |
| Alerts on status change | email | email, webhook | email, webhook, Slack | plus PagerDuty-style webhook templates | plus custom |
| Shared environment secrets (E2E, opt-in) | no | yes | yes | yes | yes |
| Hosted SSO (OIDC per org) | no | no | no | yes | yes, plus SAML through an IdP bridge if asked |
| SCIM provisioning | no | no | no | add-on or Enterprise | yes |
| Audit log | own events | own events | workspace owners | org-wide, export + anchored chain | plus SIEM webhook |
| Admin console (all workspaces of the org) | no | no | basic | full | full |
| Data residency | EU | EU | EU | EU (US region later, on demand) | EU, US on demand, or self-hosted |
| Billing | none | card (Stripe) | card (Stripe) | card, annual (invoice payment: see section 6) | contract, invoice, PO |
| Support | community (GitHub) | email, best effort | email, 2 business days | email, 1 business day | named contact, 4 business hours for Sev-1, shared channel |
| Availability target | none | none | none | 99.5% (credits) | 99.5% hosted; higher only on a dedicated host or self-hosted |
| Self-hosted server | free, AGPL, no limits | same | same | same | AGPL + support/indemnity contract |

Notes:

- **Seat** = a distinct human account that is a member of at least one workspace owned by the org
  in the billing month. Runner credentials never count. Viewers count as seats (simple, like
  Postman); a free "guest viewer" quota per org (Team: 5) lets a team show a collection to a
  stakeholder without buying a seat.
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
- **Run ceilings** per day are against abuse, not a price: the runs happen on the customer's
  machines; we only store a report and maybe send an alert. No overage packs.

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

Without hosted monitors the prices still hold, one line each:

- **Pro $6** matches Bruno Pro and Hoppscotch. Bruno Pro sells local features; Hoppscotch's server
  can read your data. Pro gives encrypted hosted sync on 10 devices, a CI runner credential, 30
  days of run history and webhook alerts. It undercuts Postman Solo ($9).
- **Team $10** sits above Bruno/Hoppscotch ($6) because we host the team's data end to end
  encrypted, keep 90 days of run history from their CI, share secret values encrypted and send
  Slack alerts. It is well under Postman Team ($19) and Insomnia Pro ($12), where the buyer
  compares "Postman but encrypted and in git". A team that needs runs on our machines is told to
  use its CI; most teams already run API tests there.
- **Business $20** is Postman Team price with SSO, audit export with an anchored chain, an admin
  console and a year of history, which Postman and Insomnia only sell at $45–49 Enterprise.
- **Enterprise quote** with a floor so the sales effort pays; the self-host offer is a support
  contract, not a licence to use (AGPL already allows use).
- **Risk to the price:** Postman's monitors are a visible feature we don't have. If trials show
  teams leaving over it, the owner can lower Team to $8 before touching the plan's no-monitor
  rule.

The owner may prefer round EUR prices (€5/€9/€19) for an EU company; Stripe shows the
buyer's currency either way.

---

## 4. Features per tier: exists today vs must be built

Legend: **have** = in main today; **build** = new code; **owner** = owner handoff.

| Feature | Tier | State | Notes |
| --- | --- | --- | --- |
| Hosted sync, E2E workspaces, invites, roles, rotation | Free+ | have | `sync-server/`, clients in CLI, desktop, web; 1.2 signed wraps |
| Organizations, plans, seats, per-org limits | all | build (C1) | today: users and workspaces only; limits are server-global env (`HITOP_SYNC_MAX_*`, 50 workspaces, 10 runners, 50 invites) |
| Account portal (sign in, org, members, usage, plan, API tokens) | all | build (C1, C3) | separate small app; shares the sync-server session; billing details live in the Stripe customer portal |
| Device recovery key | all | build (C2) | THREAT_MODEL open item 4; a paying customer who loses a laptop must not lose the workspace |
| Web app device key not in localStorage | all | build (C2, partial) | THREAT_MODEL item 5: at least an explicit warning + recovery key; a WebCrypto non-extractable key can't drive the Rust HPKE code |
| Billing (checkout, seats, trials, dunning, receipts, tax) | Pro+ | build (C3) + owner | Stripe Managed Payments as merchant of record; signed webhooks → entitlements |
| Pricing page, in-client upgrade prompts | all | build (C3) | never blocking local use |
| Runner credentials for your own CI/cron | Free+ | have | `hitop sync runner create`, `hitop schedule --sync` |
| Run upload from your runners | Free+ (status) / Pro+ (history) | build (C4) | `hitop run --sync-report` (and the Action input `sync-report`): the runner seals report.json v1 (secrets masked) under the workspace key and uploads it with a plaintext status row |
| Run history and dashboards | Pro+ | build (C4) | portal and clients decrypt history client-side; the server sees status rows only |
| Alerts: email, webhook, Slack | Free (email) / Pro+ / Team+ | build (C4) | from status rows on a status change; status + link only, never bodies or values |
| Shared environment secret values (E2E) | Pro+ | build (C4) | opt-in per environment; sync-only blob, never in `environments/*.yaml`; sealed to members' pinned devices and the runner credentials the sharer picks; changes a published promise ("secret values never sync"): privacy page and team guide updated with it |
| Hosted SSO per organization (OIDC) | Business | build (C5) | today: one issuer per server from env (`HITOP_SYNC_OIDC_*`); needs per-org client config in the DB, domain claims, `SSO required` per org |
| SCIM 2.0 provisioning | Business add-on / Enterprise | build (C6) | users + groups → org members and workspace roles |
| Audit log export, chain anchored outside the server | Business | build (C5) | signed JSONL export + daily anchor of chain heads (mailed, or public opt-in); closes THREAT_MODEL item 7 |
| Org-wide admin console | Team (basic) / Business | build (C5) | all workspaces of the org, members, transfer ownership, force rotation |
| Status page, uptime history | all | build (C2) | off-host uptime check feeds it |
| SOC 2 groundwork (policies, access reviews, evidence) | – | build (C1, C2, C6) + owner | starts now (Q17); Type 1 audit when the owner books it |
| Data residency (second region) | Business/Enterprise | on demand (C6) + owner | only when a paying customer asks |
| Enterprise self-host offer | Enterprise | owner + build (C6) | AGPL + support/indemnity contract; `hitop-sync-server` already self-hostable; add a support bundle command and upgrade notes |
| Hosted monitors (HITOP runs your collection) | – | **deferred** | see below |
| API docs publishing | – | **not planned** | OpenAPI import exists; publishing docs is a different product; point to Redoc/Scalar from a repo |
| Mock servers | – | **not planned (revisit after C6)** | a mock server contradicts "the server can't read your collections" unless the user opts in |
| Comments and review | – | **not planned** | the format is files in git: review happens in pull requests; the desktop Git panel exists |

### Runs from your own runners

1. **The runner is yours.** A CI job (`bohdaq/hitop-action@v1`, GitLab, any CI) or a cron line from
   `hitop schedule` pulls the workspace with its runner credential (have) and runs it.
2. **Upload.** With `--sync-report`, the runner seals report.json v1 (secrets already masked by
   `runner::run_collection_with`) under the workspace key and writes it as a sync-only blob
   (`runs/<ts>`). Runner credentials are viewers today; the server lets a runner write only under
   `runs/` (a new narrow permission, tested in `tests/roles.rs`).
3. **Status row.** Next to the blob the runner posts a plaintext row: pass/fail, duration, request
   count, failed request count, time, collection id. The server keeps only that for alerts and
   the dashboard summary. A workspace may turn status rows off ("history only, no alerts").
4. **Alerts.** The server compares a status row with the last one for the same collection and
   environment and alerts on a change (pass → fail, fail → pass). Email, webhook (JSON: status,
   collection name, environment, link, names of failed requests only if the owner allows), Slack
   incoming webhook. A missing run (no row within the expected interval, set per schedule) is a
   "missed run" alert.
5. **The sync server stays crypto-free.** It stores ciphertext and status rows; it can't open a
   report. `tests/no_crypto_in_server.rs` stays green.

### Hosted monitors (deferred until confidential computing)

A monitor that runs on our machines must decrypt the workspace and hold secret values, which the
service is built never to do. The first draft solved it with a trusted monitor host; the owner
chose not to (Q8). Revisit when **both** hold: a confidential-computing runner (a TEE such as AMD
SEV-SNP or Intel TDX) is available at our host or a provider we'd use, with remote attestation a
client can check before sealing keys to it; **and** customers ask for it (support mail, lost
trials). Until then the answer is "run it in your CI; HITOP Cloud keeps the history and alerts".

### Metering and quotas

Counted per org, enforced in the sync server: members (seats), guest viewers, workspaces,
ciphertext bytes, devices per member, runner credentials, run uploads per day (a ceiling against
abuse, not a price), alert channels, history retention days, invites per day, sign-in mails per
hour, API requests per minute per session. A usage table in the portal shows each against its
limit. Over the limit: creates are refused with a 402-style error that names the limit and the
plan; nothing already stored is deleted or locked (reads and pulls always work, so a lapsed plan
never takes data hostage). History older than the plan's retention is purged daily.

---

## 5. The web service

### Shape

```
cloud.hitop.rws8.tech      portal (Lit, prerendered like the site): account, org, members, usage,
                           plan (links to the Stripe checkout and customer portal),
                           API tokens, run history dashboards
sync.hitop.rws8.tech       hitop-sync-server (AGPL): the API every client already uses, plus
                           /v1/orgs/*, entitlements, run status rows, alerts, billing webhooks
status.hitop.rws8.tech     static status page, fed by the off-host uptime check
hitop.rws8.tech            site + /app/ web app (unchanged); pricing page; upgrade prompts
```

All of it runs on the current VM, `bohdan-prod-donotdelete` (Q11).

- **One app or two?** Two. The API client (web app and desktop) keeps its Sync page for daily
  work and gets a "Manage plan" link and tasteful limit notes; plans, org admin and dashboards
  live in the portal. Reason: the client is MIT and ships in the App Store; billing UI there
  raises IAP questions and bloats an offline tool.
- **Sessions:** the portal signs in with the sync server's existing sign-in (mail code, passkey,
  SSO) and uses its session; no second identity system.
- **Multi-tenancy:** one Postgres, tenant = org id on every org-scoped row; workspaces get an
  `org_id` (null = personal, counted under the owner's personal org). Row filters in every query
  are covered by the existing `tests/roles.rs` matrix extended with "other org".
- **Observability:** JSON logs without bodies or tokens (privacy page promise), metrics for
  requests, errors, alert delivery, mail failures; the uptime check runs off-host.
- **Abuse:** sign-in mail limits per address and IP (have), invite limits (have), per-org
  creates, per-session request rate, run upload ceilings, disposable-email block on Free, manual
  review queue for Free orgs that hit limits fast. Report-abuse address on the site.

### Hosting: hardening the current VM (C2)

Today everything runs on `bohdan-prod-donotdelete` (168.119.17.57): the front rws, rws8.tech, a
live mail server, Gitea, a PHP demo, other sites, hitop-server and the sync server with SQLite
backed up by hand. The owner chose to stay on it (Q11). **HITOP Cloud shares this host with other
sites**: a fault, overload or compromise of any of them can take HITOP Cloud down. The security
page, the SLA page and the DPA say so. The plan makes the shared host as safe as it can be:

| Change | Why | Who |
| --- | --- | --- |
| Postgres on the VM (the sync server already supports it), local socket only, its own system user and data directory | SQLite is single-writer and backup-by-copy; Postgres gives WAL archiving | agent |
| SQLite → Postgres move with `hitop-sync-server migrate-db`, tested on a copy, a short write freeze, row counts compared | no data loss on the switch | agent |
| systemd resource limits for `hitop-sync`, `postgresql` and `hitop-server`: `MemoryMax`, `CPUWeight`, `IOWeight`, `TasksMax`, plus `ProtectSystem=strict`, `PrivateTmp`, `NoNewPrivileges` | a runaway neighbour (or HITOP) can't starve the rest; smaller blast radius | agent |
| Disk: alerts at 80% full; Postgres on its own directory with a size check | a full disk is the most likely outage on a shared host | agent |
| Off-site encrypted backups: nightly `pg_dump` plus WAL archiving to an S3-compatible bucket at **another provider or region**, encrypted with an age key whose private half the owner keeps offline; 35-day retention | the VM itself can be lost | **owner** creates the bucket and key; agent wires |
| Restore drill: `deploy/restore-drill.sh` restores last night's backup into a scratch database, checks row counts and `audit-verify`; monthly, recorded in `cloud/ops-log.md` | a backup is only real once restored; SOC 2 evidence | agent |
| Status page `status.hitop.rws8.tech` (static, updated by the uptime check and by hand) | incident comms | agent; **owner** adds DNS |
| Uptime check from **outside** the VM and its provider (a hosted checker) on `/healthz` of sync, portal and site | the VM can't report its own outage | **owner** picks the account; agent configures |
| Ops runbook `cloud/RUNBOOK.md`: restore, rotate keys, revoke sessions, disk full, neighbour site misbehaving, incident template | one operator; repeatable steps | agent |
| Secrets in root-only env files: Stripe secret key and webhook secret, mail service key, OIDC, backup public key | as today | **owner** places keys |
| A second admin path: the owner's SSH key plus the agent's access as today; both listed in the access review | one access path is a single point of failure | both |
| Mail through the transactional service the owner picks (Postmark or SES), before the paid launch | deliverability of codes and alerts | **owner** |

**Availability target: 99.5% at most** (about 3.6 hours a month), for Business only, with
credits. No higher figure is offered while the service shares one VM.

**Trigger to move hosts.** Move HITOP Cloud to its own VM (same provider, EU) when any of these
happens: monthly revenue covers the new host about ten times over (about €500/month); a Business
or Enterprise customer asks for an isolated host or a higher SLA in a deal; an outage of more than
4 hours caused by a neighbour; the VM's CPU, memory or disk stays above 70% for a week. The move
keeps `sync.hitop.rws8.tech` (a DNS change by the owner) and reuses the C2 backups for the cutover.

Costs to expect (rough): object storage ~€5/month, uptime checker free–€10, transactional mail
~€15 at low volume. No new VM costs until the trigger.

---

## 6. Billing: Stripe Managed Payments

**Stripe Managed Payments** is the merchant of record (Q3, owner decision 2026-11-03; it replaced
Lemon Squeezy before anything was sold): Stripe sells to the buyer, calculates, collects and
remits VAT, sales tax and GST in 80+ countries, handles fraud, disputes and transaction support,
and sends receipts and invoices; buyers see "Sold through Link" and can manage their order at
link.com. Fees: Stripe's processing fee plus the Managed Payments fee (the owner confirms them in
the Dashboard). We never see card data. Docs (checked 2026-11-03):
<https://docs.stripe.com/payments/managed-payments>.

### Account setup (owner)

- The owner's Stripe account (business in Poland: a supported location), Managed Payments enabled
  in Dashboard → Settings → Managed Payments, sandbox first.
- **Products and prices:** one product per tier (Pro, Team, Business), tax code **SaaS – business
  use** (`txcd_10103001`, eligible for Managed Payments), each with a monthly and a yearly USD
  price, **tax-exclusive**, flat rate with quantity = seats (Team and Business; Pro quantity 1),
  with the lookup keys `pro_monthly`, `pro_yearly`, `team_monthly`, `team_yearly`,
  `business_monthly`, `business_yearly` (the server finds prices by them). Enterprise is not sold
  through checkout.
- A secret (or restricted) key and a webhook endpoint (`https://sync.hitop.rws8.tech/v1/billing/stripe`,
  API version `2026-10-28.endive`, the events below, its signing secret), placed in a root-only
  env file on the VM; the customer portal configured (section 11).

### What the code does (C3)

- **Checkout:** the portal's Buy button asks the server for a Stripe Checkout Session
  (`mode=subscription`, `managed_payments[enabled]=true`, the price, quantity = seats, the buyer's
  email, `client_reference_id` = the org id and signed metadata on the session and the
  subscription) and redirects to its URL. Parameters Managed Payments refuses (tax, payment method,
  shipping, customer update, invoice settings, Connect) are never sent.
- **Customer portal:** the portal's "Manage billing" button opens a Stripe customer portal session
  for the org's customer (payment method, invoices, cancel, switch monthly/yearly).
- **Webhooks** (`POST /v1/billing/stripe`): `Stripe-Signature` is checked (`v1` = HMAC-SHA256 of
  `"<t>.<raw body>"` with the signing secret, constant-time compare, `t` within 5 minutes) before
  anything is parsed; events are idempotent by event id and ordered by event `created` per
  subscription; an event whose `livemode` differs from the key's mode is ignored. Events used:
  `checkout.session.completed`, `customer.subscription.created`, `.updated`, `.deleted`,
  `.paused`, `.resumed`, `invoice.paid`, `invoice.payment_failed`, `charge.refunded`. They update
  one `entitlements` row per org (plan, seats, status, period end, Stripe ids). Entitlements are
  the only thing the rest of the server reads.
- **Trials:** 14 days of Team for a new org, no card, run by our server (not Stripe). After the
  trial the org drops to Free limits with nothing deleted (reads always work; creates beyond Free
  are refused with the plan note).
- **Seat changes:** the portal shows seats used vs bought. Adding a member beyond the bought seats
  asks the owner to add seats; the server updates the subscription item's quantity through the
  API (prorated, invoiced at once; a declined card changes nothing). Fewer seats take effect at
  once with no credit; the lower price starts at the next renewal.
- **Dunning:** Stripe retries failed payments (Smart Retries, the owner's settings) and mails the
  buyer. We show a "payment failed" banner while the subscription is `past_due`; when it becomes
  `unpaid`, `paused` or is cancelled the org drops to Free limits, nothing deleted.
- **Refunds:** issued by the owner in the Dashboard (with the subscription cancelled), or by
  Stripe/Link support within 60 days; a full refund (`charge.refunded`) ends the entitlement. Policy
  text on the pricing page (owner's call; proposal: full refund within 14 days of the first
  payment).
- **Invoices for Business:** Stripe issues receipts and invoices for card payments. Invoice or PO
  payment isn't offered under Managed Payments; until a deal needs it, Business pays by card and
  Enterprise is invoiced by the owner's company outside Stripe.
- **Tests:** `sync-server/tests/billing.rs` replays event bodies shaped like Stripe's documented
  ones (every event above, a bad or old signature, a replay, out-of-order events, a test event on a
  live key) against a fake Stripe API; entitlements enforcement in `tests/limits.rs`. The whole
  flow runs in the sandbox before the owner switches to live.
- **App Store:** subscriptions are sold on the web only; the Mac and iOS apps sign in to an account
  and never show a buy button. The owner checks whether guideline 3.1.3(b) (multiplatform
  services) applies; if it does, the MAS/iOS builds either add IAP for the same plans or hide the
  Cloud features (compile-time flag, like telemetry).

---

## 7. Legal and compliance (owner)

Everything here is the owner's: agents draft text, the owner signs.

- **Legal entity and bank**: the owner's company (or a new one) as the Stripe account owner
  and payout account.
- **Terms of Service**: none exist today (`/terms/*.html` is the guides path and must keep
  working). New path `/legal/terms.html`, `/legal/privacy.html` (the current `/privacy.html` stays
  and links there), `/legal/dpa.html`, `/legal/sla.html`, `/legal/subprocessors.html`. Drafts by an
  agent, reviewed by a lawyer before the paid launch.
- **Privacy**: rewrite the privacy page for accounts, billing (Stripe), run uploads (what the
  status row holds), alerts (Slack/webhook recipients), logs, backups, retention, data subject
  rights; the home/pricing copy ("Nothing to sign up for", "Is there a subscription? No") must
  change.
- **Security page** (`/security.html`, C3): the threat model in plain words, and two sentences that
  stay until they are untrue: "No external security review of HITOP's encryption has been done
  yet. HITOP Cloud runs on a single server shared with other sites." It links the threat model,
  the sub-processor list and the SOC 2 status. When the review happens, the page links its summary.
- **External crypto review (Q13):** not now and not a launch gate. It stays an open owner item in
  ROADMAP section 6 and in section 11, with the scope already written there.
- **GDPR**: the owner is EU-based, so a DPA for Business/Enterprise with the standard clauses, a
  sub-processor list, a record of processing, deletion on request (org delete → everything, within
  30 days including backups' expiry), data export (the client already has it: `hitop sync pull`).
- **AGPL and the hosted service**: AGPL §13 requires offering the source of the running version to
  users of the network service. The code is public, so the portal footer shows the commit and a
  link.
- **Enterprise self-host license vs "no CLA"** (Q6 decided: option c): enterprise self-host = AGPL
  (free) plus a paid support, upgrade and indemnity contract. No commercial licence, so no CLA is
  needed. Revisit only if a buyer insists on a non-AGPL copy (that would need a CLA scoped to
  `sync-server/` and `cloud/`).
- **SOC 2 groundwork, starting now (Q17).** Type 1 first, when the owner books the auditor. What
  the agent builds, and where:
  - **Policies** in `cloud/policies/` (C1): information security, access control, change
    management, incident response, backup and recovery, vendor management, data retention and
    deletion, acceptable use. Short, true to what one operator actually does.
  - **Access reviews** (C1, then quarterly): a list of every account with access to the VM, the
    database, the backup bucket, Stripe, the mail service, DNS, GitHub and the App Store,
    reviewed and recorded in `cloud/evidence/access-reviews/`.
  - **Change management = git history + CI**: every change lands as a signed-off commit on `main`
    with CI green; deploys are recorded with the commit they ship (`cloud/ops-log.md`). The policy
    says exactly that; no extra tooling.
  - **Backup evidence** (C2): each restore drill's output and date in `cloud/evidence/restore-drills/`.
  - **Vendor list** (`cloud/policies/vendors.md`, C1): hosting provider of the VM, backup
    object-storage provider, Stripe, the transactional mail service, the uptime checker,
    GitHub, DNS provider; what data each sees and its own compliance report.
  - **Incident log** and status page history (C2).
  - The owner picks the **compliance tool** (Vanta, Drata, Secureframe or a spreadsheet) and the
    **auditor**; rough budget €15–30k for the first Type 1 with a tool. Until the report exists the
    security page says "SOC 2: groundwork in progress, no report yet".
- **Trademark**: `TRADEMARK.md` exists; "HITOP Cloud" falls under it. Optional EU trademark
  filing (owner).
- **Export/sanctions**: Stripe screens buyers as merchant of record. Nothing more needed at
  this scale.

---

## 8. Go-to-market basics

- **Pricing page** (`/pricing.html`, same URL): the tier table of section 3, "free forever"
  box first, the native apps keep their one-time prices as a separate row, FAQ rewritten
  ("Is there a subscription?" → "Only for HITOP Cloud; the apps and CLI stay free"; "Do you run
  monitors?" → "No: run in your CI, HITOP Cloud keeps the history and alerts").
- **Home page**: the "Nothing to sign up for" card becomes "No account needed; HITOP Cloud is
  optional and end-to-end encrypted". One new card: "Team sync and run history, encrypted".
- **Upgrade prompts in clients** (rules): never modal, never block a local action, never in the
  CLI except one line when a hosted limit is hit (`hitop sync` prints the limit and the plan
  URL, exit 2 as today). In the web app/desktop: a one-line note on the Sync page when a limit is
  reached, and a "Manage plan" link. No prompts in the MAS/iOS builds beyond the same Sync-page
  note (no purchase links there; owner checks the App Store rule).
- **Docs**: a "HITOP Cloud" guide (sign up, org, seats, run history, alerts, SSO), the THREAT_MODEL
  "Teams" and "Hosted service" sections, the security page, the self-host guide gets an
  "Enterprise support" paragraph. Comparison guides get a Cloud row.
- **Launch**: Cloud early access to the people on the test server first (mail), then the usual
  channels in `launch/` (owner posts; agents never post).
- **Sales for Business/Enterprise**: a contact form to the owner's mail, a one-page security
  overview PDF, the DPA. No sales tooling until the first deal.

---

## 9. Execution plan: tranches

Rules as in ROADMAP sections 1 and 3: agents commit to `main`, run every **Done when** check,
update this file's status table, record owner items in section 11. Never: set prices in code
without the owner's figures (prices live in Stripe; code finds them by lookup key), change
DNS/App Store/money, post publicly, touch the VM beyond what ROADMAP allows and C2 lists, store
secrets anywhere but root-only env files. Code for the service goes in `cloud/` (portal, policies,
runbook) and `sync-server/` (orgs, entitlements, billing webhooks, run status, alerts), all AGPL;
client changes stay MIT. Format v1 stays additive-only: every new thing is a sync-only blob or
server-side row, never a new file in `environments/`.

| ID | Tranche | Status | Needs |
| --- | --- | --- | --- |
| C0 | Owner decisions and accounts | decisions done; accounts open | owner |
| C1 | Organizations, plans, entitlements; portal MVP; SOC 2 policies and first access review | **deployed** to the test server 2026-11-02 (2049e95, CI green; `cloud/ops-log.md`); left: the `cloud.` route after the owner's DNS, and the owner's approvals | the 1.2 sync server deployed to the test server first (ROADMAP section 6, "1.2 signed key wraps") so migration 4 and the 1.2 deploy don't collide |
| C2 | Harden the current VM: Postgres, limits, backups + drill, recovery key, status page, runbook | **applied** 2026-11-03 except backups, restore drill and uptime (owner: later); the sync server runs on Postgres (`cloud/ops-log.md`) | C0 (bucket, backup key, uptime checker, `status.` DNS), C1 |
| C1b | Normal sign-in and sign-up: email + password (mailed confirmation, reset, change), Continue with Google / GitHub, the password unlocks the key bundle on a new device | **deployed** to the test server 2026-11-03 (schema 6; Google/GitHub off until the OAuth apps exist) | C1, C2's code; the owner registers the Google and GitHub OAuth apps |
| C3 | Stripe Managed Payments billing (first built on Lemon Squeezy), pricing page, prompts, legal and security pages: **paid launch of Pro and Team** | **deployed** to the test server 2026-11-03 on Lemon Squeezy with billing off; **moved to Stripe** 2026-11-03, built, not deployed (billing off until `HITOP_SYNC_STRIPE_*` are set; legal pages `DRAFT`; status below) | C1, C2, C0 (Stripe account with Managed Payments, mail service, legal drafts reviewed) |
| C4 | Run history and alerts from your own runners; shared environment secrets | **deployed** to the test server 2026-11-03 (schema 8; `cloud/ops-log.md`) | C3 (sells it); deploy with or after C3's migration 7 |
| C5 | Business tier: hosted SSO per org, audit export + anchoring, admin console | **deployed** to the test server 2026-11-03 (schema 9; per-org SSO off: no host key or DoH resolver, Q15; Business not sold until its prices exist; drafts behind DRAFT; status below) | C3 |
| C6 | Enterprise: SCIM, self-host support offer, SOC 2 Type 1 readiness; second region and own host on demand | code parts **built, not deployed** 2026-11-03 (SCIM, support bundle, upgrade notes, signed image workflow, questionnaire and contract drafts behind DRAFT, SOC 2 map; US region and own host not built, on demand; status below) | C5, owner (contracts, auditor) |
| C7 | Deferred: hosted monitors (until confidential computing), mock servers, docs publishing, comments | not planned | revisit with the triggers in section 4 and demand evidence |

### C0 Owner decisions and accounts (parallel with C1)

- Goal: everything only the owner can do, started now so C2 and C3 aren't blocked.
- Scope (owner): Q1–Q18 answered (done 2026-11-02); legal entity and payout for Stripe;
  the Stripe account with Managed Payments enabled, products and prices, secret key and webhook
  secret (sandbox first), Stripe's eligibility review; backup bucket at another provider and the age key; DNS `cloud.` and `status.`; uptime
  checker account; transactional mail service (Postmark or SES); compliance tool and auditor;
  lawyer review of the legal drafts.
- Scope (agent): draft ToS, privacy update, DPA, sub-processor list, SLA (99.5%, shared host
  stated) and the security page under `site/src/pages/legal/` behind a `draft` flag (not linked
  until the owner approves).
- **Done when:** the owner has ticked each C0 row in section 11; `curl -sI
  https://cloud.hitop.rws8.tech/healthz` answers from the VM; a sandbox webhook event from
  Stripe reaches the server and passes the signature check.
- Owner handoffs: section 11, rows C0.

### C1 Organizations, plans, entitlements; portal MVP; SOC 2 policies

- Goal: the data model and enforcement that every paid feature hangs on, running on the test
  server with Free limits, so nothing is sold before it's enforced; and the first SOC 2 documents.
- Scope:
  - sync-server migration 4: `orgs` (id, name, plan, region, created, billing ref),
    `org_members` (role owner/admin/member), `workspaces.org_id`, `entitlements` (plan, seats,
    limits JSON, status, period end, provider customer/subscription/price ids), `usage`
    counters; every account gets a personal org.
  - `GET/POST /v1/orgs`, members CRUD, `POST /v1/orgs/:org/workspaces` (or move a workspace into
    an org, owner only), `GET /v1/orgs/:org/usage`, `GET /v1/orgs/:org/limits`.
  - Enforcement of section 3 Free/Pro/Team/Business limits in one `limits.rs` module
    (seats, workspaces, storage, devices, runners, invites), with a 402-style JSON error
    `{error: "limit", limit, plan, upgradeUrl}`; reads and pulls never refused.
  - Clients: CLI and Sync page show the limit message as given; `hitop sync ls` shows the org;
    `hitop sync init --org <name>`.
  - Portal MVP (`cloud/portal/`, Lit + lit-material like the site, prerendered, served by
    hitop-server at `cloud.*`): sign in (mail code / passkey / SSO via the sync server), org list,
    create org, members and roles, usage vs limits, API tokens (personal access tokens for the
    sync API with scopes and expiry, listed and revocable; audit event `token_created/revoked`).
  - Audit events for org changes; `tests/roles.rs` extended with org roles and "other org".
  - SOC 2: the policies, the vendor list (VM host, backup storage, Stripe, mail service,
    uptime checker, GitHub, DNS) and the first access review (section 7) in `cloud/policies/` and
    `cloud/evidence/`; `cloud/ops-log.md` started with the C1 deploy.
- **Done when:** `cd sync-server && cargo test --workspace` (new: `tests/orgs.rs` every org role ×
  operation, `tests/limits.rs` every limit at boundary for each plan, migration 3 → 4 upgrade of a
  T25-shaped database on SQLite and Postgres); `cd cli && cargo test --locked` with `tests/sync.rs`
  cases for a refused create and `--org`; `cd cloud/portal && npm test` (e2e against the real server
  binary: sign in, org, invite, usage, token); **client compatibility**: the released `hitop`
  1.2 binary (from GitHub Releases, not the tree) and the live web app run the full sync flow
  (login, init, push, pull, invite, runner) unchanged against a migration-4 server, with the
  personal org implied (a CI job downloads the release binary); test server deployed with Free
  limits, existing accounts migrated into personal orgs (count before == count after) and
  grandfathered limits (a query for accounts over their limit returns zero rows); the policies,
  vendor list and first access review are committed and the deploy is in the ops log.
- Needs: the 1.2 sync server deployed to the test server first (met 2026-11-02, b5389cf).
- Owner handoffs: read and approve the policies (they describe the owner's own practice); confirm
  the access review list.

**Status (2026-11-02): built, CI green, not deployed.**

- Commits on `main`: cd2d0c6 (migration 4, `orgs.rs`, `limits.rs`, `tokens.rs`), ded3115
  (`tests/orgs.rs`, `tests/limits.rs`), 70e3b9a (CLI `sync init --org`, `sync ls` org), b9be01c
  (portal, `cloud/compat/released-cli.sh`), 6f6755c (policies, vendors, access review, ops log),
  62394b7 (deploy: `www/cloud/`, route, install), 213d028 (CI), 788ef5c (Sync page link),
  690d3ec (docs). e55b514 (this status). CI run 41459531300 on e55b514: green (every job, compat with hitop 1.1.0 and 1.2.0 included; the macOS job re-run after a GitHub runner capacity cancel).
- **Migration 4** (additive; SQLite and Postgres): tables `orgs`, `org_members`, `org_invites`,
  `entitlements`, `usage_counters`, `api_tokens`, column `workspaces.org_id`. Every person
  (`users.runner_of = ''`) gets a personal org `personal-<user id>` on Free, owner, with an
  entitlements row; each workspace goes to its first owner's personal org; then, in the same
  transaction, `limits::grandfather` writes the org's current usage as floors wherever it is
  above Free (seats, workspaces, storage, devices, runners; marked `grandfathered`). No existing
  row changes or goes; runner accounts get no org. `tests/migration.rs` and
  `tests/postgres.rs` upgrade a T25-shaped database (`tests/common/t25.rs`: an account with 7
  workspaces, 150 MB, 6 people, 6 devices, 6 runners) and check: row counts of every old table
  equal, personal orgs = people, `over_limits` empty, every read answered, creates refused only
  beyond the floor, a delete frees room.
- **Enforcement is opt-in**: `HITOP_SYNC_LIMITS=plans` (HITOP Cloud). The default `off` keeps the
  self-hosted promise of section 2 (no plans, seats or per-org limits; the server-wide
  `HITOP_SYNC_MAX_*` limits stay). The plan numbers of section 3 are in `limits::plan_limits`;
  invites a day (Free 20, Pro 50, Team 200, Business 1000) are an abuse ceiling not in section 3.
  Seats: org members, pending org invites and people in the org's workspaces; Team's 5 guest
  viewers are free; runners never count. Devices: the best limit of the person's orgs. Lapsed
  (`expired`/`unpaid`) = Free, nothing deleted. Operator commands: `hitop-sync-server
  over-limits`, `set-plan <org> <plan> [seats]` (until C3).
- Deviations: the portal is a static shell plus a Lit app (hash routes), not prerendered pages
  (nothing in it is public content); usage counters are daily rows (`usage_counters`) rather than
  one `usage` row, and seats/workspaces/storage are counted live from the tables; the Sync page
  limit note is the server's message as clients already show it (no new client UI); CLI limit
  hint is the server message (it names plan, limit and the portal URL). Run uploads per day and
  history retention wait for C4 (no runs yet).
- **Done when**, as run: `cd sync-server && cargo test --workspace` (orgs, limits, migration; all
  green), `cargo test --features postgres --test postgres` (5 tests incl. the 3 → 4 upgrade);
  `cd cli && cargo test --locked` with `tests/sync.rs` `organizations_and_a_refused_create`, and
  the whole `tests/sync.rs` again with `HITOP_SYNC_LIMITS=plans`; `cd cloud/portal && npm test`
  (22 checks: sign in, org, invite, usage, 402 on seats, role, plan page, token, passkey);
  **client compatibility**: CI job `compat` downloads every released `cli-v*` from 1.1.0 on (the
  GitHub Release binary, sha256 checked) and runs `cloud/compat/released-cli.sh` (login, init,
  push, pull, conflict, invite, accept, add, viewer, runner, remove + rotation, audit verify,
  a refused 4th workspace, `over-limits`, no plaintext) against this tree's server with limits on:
  hitop 1.1.0 and 1.2.0 (both release binaries also passed locally). The live web app's flow is `webapp/tests/sync.cjs`,
  also run with `HITOP_SYNC_LIMITS=plans` in the `web` job. Not yet (deploy): the test server
  migrated, "count before == count after" and `over-limits` = 0 on the VM, the ops-log entry.
- **Deploy steps** (after the 1.2 sync server is live): build `cargo build --release --locked`
  in `sync-server/`; copy the binary and `sync-server/deploy/` to the VM and run `install.sh`
  (backs up `sync.db`, binary, env; adds `HITOP_SYNC_LIMITS=plans` and
  `HITOP_SYNC_PORTAL_URL=https://cloud.hitop.rws8.tech/`; restarts; runs `over-limits` on a copy:
  must print `0 over a limit`). Before/after counts: `SELECT COUNT(*) FROM users WHERE runner_of =
  ''` = `SELECT COUNT(*) FROM orgs WHERE personal_of <> ''`. Then `deploy/deploy.sh` (site, web
  app and the portal in `www/cloud/`; checks `/cloud/` and `/cloud/healthz`). Add the
  `hitop-cloud` route from `deploy/rws-routes.toml` (back up, `rws --check`, restart, probe the
  other sites). Once the owner's DNS for `cloud.hitop.rws8.tech` resolves: append it to `[acme]`
  domains, and add `https://cloud.hitop.rws8.tech` to `HITOP_SYNC_ALLOWED_ORIGINS` and
  `HITOP_SYNC_RP_ORIGINS` in `/etc/hitop-sync/env`; until then the portal works at
  `https://hitop.rws8.tech/cloud/` (that origin is already in both settings). Record it in
  `cloud/ops-log.md`.
- Left for the owner: DNS `cloud.hitop.rws8.tech` (C0 row); approve the policies; confirm the
  access review (`cloud/evidence/access-reviews/2026-Q4.md`, "owner lists" rows) and the vendor
  list; set real prices later (the limits table is the proposal).

### C1b Normal sign-in and sign-up (owner decision 2026-11-03)

- Goal: HITOP Cloud signs in like any service: email + password with a confirmed address,
  "Forgot password?", and "Continue with Google" / "Continue with GitHub", in every client (portal,
  web app Sync page and the desktop build of it, CLI). Mail codes, passkeys and SSO stay. The
  password also unlocks the person's end-to-end keys on a new device, so no trust step.
- **Done when**: server unit + integration tests (hash, enumeration, backoff, reset, linking,
  OAuth mocks, rate limits); crypto vectors for the derivation; `cli/tests/sync.rs` and
  `webapp/tests/sync.cjs` sign up → password unlock on a second device with no trust step, a reset
  loses the bundle with a warning; the compat job (released 1.1/1.2 CLIs) stays green.

**Status (2026-11-03): built, not deployed.** (C2 itself was applied the same morning, at the
owner's go-ahead, before this tranche: its deploy is a normal version deploy on Postgres.)

- Commits on `main`: 3c7ad34 (hitop-sync-crypto 0.4.0), bd13fd9 (server: passwords, migration 6, Google, GitHub), 3553626 (hitop-wasm), b3fe3d0 (Sync page), 6c66a8d (portal), 6875b15 (CLI), c25548e (docs), and the web reset fix after it.
- **Key derivation** (THREAT_MODEL "Passwords"; hitop-sync-crypto 0.4.0 `derive_password_keys`,
  `vectors/password.json`): `master = Argon2id(NFC(password), salt, m=64 MiB, t=3, p=1, ad
  "hitop-sync/v1/password")`; `authKey = HKDF(master, "…/auth")` goes to the server, which keeps
  only its own Argon2id hash of it (19 MiB, t=2, optional pepper as the Argon2 secret);
  `bundleKey = HKDF(master, "…/bundle" || canonical params)` seals the same key bundle as the
  recovery phrase (`seal_key_bundle`, method `password`) and never leaves the client. Clients
  refuse parameters below the floor (a server can't weaken the KDF to make `authKey` an oracle).
- **Identity on unlock**: a password sign-in on a new device opens the bundle and becomes the
  device in it (same key id and public key, checked against the server's listing; pins and key
  memory carried over), registering no key: teammates' pins and owners' wraps already cover it.
  Trade-off (documented): every device unlocked so is the same key, revocable only together.
  Only the bundle's own device keeps `bundleKey` and re-seals; it forgets the key when the
  server's bundle no longer opens with it or holds another device (no stale overwrite).
- **Server** (migration 6: `password_credentials`, `password_tokens`, `server_secrets`):
  `/v1/auth/password/{params,signup,verify,login,forgot,reset}`, `POST /v1/me/password`,
  `GET /v1/auth/methods`, `/v1/auth/{google,github}/{start,callback,finish}`; `GET /v1/me` adds
  `password` and `bundles` (`recovery` keeps meaning the phrase, for 1.2 clients). No account
  enumeration (made-up stable params, one 401 with a dummy hash on misses, 202 for sign-up and
  forgot); a per-address backoff after 5 wrong passwords (30 s doubling to 15 min) on top of the C2
  IP limit; mails share the per-address hourly cap. A reset deletes the password's bundle, ends
  every session and answers `bundleLost`; a change re-seals it in the same request.
- **Google** reuses oidc.rs (code + PKCE, JWKS, nonce; `iss` with or without `https://`); **GitHub**
  is OAuth 2 (code + PKCE + secret, `/user`, `/user/emails`, primary *verified* address only).
  Linking by verified address only; SSO-only domains refused; off unless configured.
- **Clients**: CLI `hitop sync login` asks for the password (`--password-stdin`), `--code [CODE]`,
  `--sso`, `--google`, `--github` (loopback like SSO); `hitop sync signup`; `hitop sync password
  set|forgot|reset|unlock`. Sync page (web and desktop): password form, Create account, Forgot
  password? (warning before and after), Continue with Google/GitHub when offered, Use a code
  instead, passkey, SSO; Password section (set/change, Unlock with password). Portal: the same
  sign-in page and a Password section (re-seals the bundle on change; it never makes one).
- **TOTP**: not built (documented follow-up in THREAT_MODEL "Passwords"); passkeys stay the
  phishing-resistant option.
- **Deviations**: the wrong-password backoff is its own per-address table (password.rs `Backoff`,
  off with `HITOP_SYNC_RATE_LIMITS=off`) beside the C2 IP limiter, which still covers every
  `/v1/auth/*` request; its 429 carries `retryAfter` in the body, not a `Retry-After` header. A
  reset on a computer that wasn't the account's device signs in with a new key but doesn't claim
  the password's bundle (it holds no workspace key); the next password sign-in of a device that
  has the keys seals them again, and `password unlock` then works anywhere.
- **Tests, as run**: `sync-server` `cargo test --workspace` (new `tests/password.rs`,
  `tests/oauth.rs` with mock Google (common/idp.rs) and mock GitHub (common/github.rs)),
  `--features postgres` (new `postgres_passwords`; `migrate_db` fill has a password account);
  `hitop-sync-crypto` vectors; hitop-wasm and `webapp/tests/unit.mjs` run them;
  `cli/tests/sync.rs` `a_password_signs_in_and_unlocks_the_keys_on_a_new_computer`,
  `google_and_github_need_the_server_to_offer_them`; `webapp/tests/sync.cjs` (web and Tauri) the
  Carol password flow; `cloud/portal` e2e (32 checks). CI: see `cloud/ops-log.md`.
- **Deploy** (not done; `cloud/ops-log.md` 2026-11-03 C1b): a dump, then the usual version deploy
  (`--features postgres`), `/healthz` schema 6, `deploy/deploy.sh` for web app and portal.
  Optional `HITOP_SYNC_PASSWORD_PEPPER_FILE`. Google/GitHub: env ids and secrets after the owner's
  registrations (section 11).

### C2 Harden the current VM

- Goal: a host we can take money on, on the VM we already have, with its limits stated honestly.
- Scope:
  - Postgres on the VM (local socket, own user); `hitop-sync-server migrate-db` SQLite → Postgres,
    tested on a copy, then a short write freeze for the switch.
  - systemd limits and sandboxing for `hitop-sync`, `postgresql`, `hitop-server` (section 5);
    disk alert at 80%.
  - Nightly `pg_dump` + WAL archiving to the off-site bucket, age-encrypted, 35-day retention;
    `deploy/restore-drill.sh` run and its output kept in `cloud/evidence/restore-drills/`.
  - JSON logs without bodies; a metrics endpoint; the off-host uptime check;
    `status.hitop.rws8.tech` static page; the incident log.
  - `cloud/RUNBOOK.md` (restore, rotate keys, revoke sessions, disk full, neighbour site
    misbehaving, move-host procedure for the trigger in section 5, incident template).
  - **Recovery key**: `hitop sync recovery create` derives a second device key from a printed
    24-word phrase (BIP-39 wordlist or 32 bytes base32), registers it as a device named
    "recovery", and the owner's client wraps workspace keys for it like any device; `hitop sync
    recovery use` restores on a new device. Web app: same flow on the Sync page; the page warns
    that its device key lives in browser storage and offers the recovery key at first share.
  - Abuse: per-IP and per-org creates, disposable-mail list for Free, request rate per session
    (server-side, not only at rws), report-abuse page.
- **Done when:** `restore-drill.sh` restores last night's backup into a scratch database and
  `audit-verify` passes on it, and the output is committed as evidence; `systemctl show` lists
  the limits for the three units; `cargo test --features postgres --test postgres` on CI;
  recovery tests in `cli/tests/sync.rs` and `webapp/tests/sync.cjs` (lose the device, recover,
  read the workspace; a wrong phrase fails); after the switch the Postgres row counts equal the
  SQLite ones; the uptime check shows 7 days green before C3 launch; the runbook covers each case
  above.
- Needs: C0 (bucket, age key, uptime checker, `status.` DNS), C1.
- Owner handoffs: create the bucket and keep the age private key offline; place the bucket
  credentials; add the `status.` DNS record; agree the write-freeze time for the Postgres switch.

**Status (2026-11-03): built and tested off the VM; nothing applied.**

- Commits on `main`: 7183eba (`migrate-db`, `db-stats`, `snapshot`), 160154f (rate limits, body
  caps, `/healthz` detail), 9bd3542 (hitop-sync-crypto 0.3.0 recovery phrases), bb59aa2 (server
  recovery blobs, migration 5), 1dc61fa (CLI `hitop sync recovery`), 59fa124 (`dump`, snapshot
  reads), 7fb2597 (Sync page recovery), 83906f3 (deploy and ops scripts, units), f65d6de (key
  bundle per unlock method), 14b71e8 (CI job `ops`), 1002717 (status page), 196811c (docs).
- **Database**: `hitop-sync-server migrate-db --from --to` copies every table (the list comes from
  the migrations, columns per backend) into an empty database, reading one snapshot of the source
  and writing one transaction on the target, then compares row counts and per-table content hashes
  (rows hashed and sorted in Rust, so SQLite and Postgres agree) and runs `audit-verify` on the
  target; refuses a non-empty target; works both ways (Postgres → a new SQLite file = a rollback
  that keeps later writes). Also `db-stats`, `dump` (SQLite `VACUUM INTO`, or `pg_dump --snapshot`
  of the transaction the manifest's stats come from) and `snapshot`. Scripts:
  `sync-server/deploy/postgres-setup.sh` (own cluster `hitop`, `listen_addresses = ''`, socket port
  5433, scram role with a generated password in root-only `/etc/hitop-sync/postgres.url`; a `main`
  cluster the package install made is disabled; an existing Postgres is left alone),
  `cutover-postgres.sh` (preflight, write freeze, backups, migrate, switch, verify, rolls itself
  back), `rollback-postgres.sh` (`--snapshot` for the frozen file).
- **systemd**: the hitop-sync and hitop-server units are sandboxed (`systemd-analyze security`
  exposure 1.4 each); resource drop-ins for both and for `postgresql@<major>-hitop`
  (`cloud/ops/systemd/`: CPUQuota, CPU/IO weight 50, MemoryHigh/Max, TasksMax).
  `cloud/ops/install.sh <part>` applies each part with backups and puts a unit back when it stops
  answering; `remove <part>` undoes it.
- **Alerts**: `cloud/ops/disk-alert.sh` + timer (15 min): a disk over 80%, the Postgres data over
  10 GiB, the last good backup older than 3 h; journal, and mail through 127.0.0.1:587 once per 6 h
  per problem.
- **Backups**: `backup.sh` (hourly timer, the service's dynamic user, recipients as a systemd
  credential): `dump`, `tar | age -R`, upload with `s3.sh` (curl `--aws-sigv4`, so the only new
  tool on the VM is `age`; S3 or a directory). Pruning is optional; recommended: a write-only key and
  a 35-day lifecycle rule on the bucket. `restore-drill.sh` (runs where the private key is): newest
  backup → scratch database (a SQLite file, `--scratch-url`, or a throwaway postgres:17 container),
  counts and hashes against the manifest, `audit-verify`, an evidence file for
  `cloud/evidence/restore-drills/`.
- **Recovery key**: `hitop sync recovery create|use|status|remove` and the Sync page (both builds;
  it warns that the web app's device key lives in browser storage). A 24-word BIP-39 phrase seals
  the device's key bundle (key id, secret, pins, key memory); the server keeps it per account and
  unlock method (`?method=phrase`). A password unlock (next tranche) is one more row: the client
  seals the same bundle under a password-derived key (`seal_key_bundle` takes any 32-byte unlock
  key). `use` checks the listed public key and deletes the key the new login registered.
  THREAT_MODEL "Recovery" (open item 4 done). hitop-sync-crypto 0.3.0 (additive; the compat job
  runs the released 1.1/1.2 clients unchanged).
- **Abuse**: fixed windows in the server (`ratelimit.rs`): sign-in 60/min per client IP (the full
  IP from the trusted proxy, not the /24 the log keeps), writes 600/min per session and 1200/min per
  IP, creates 200/h per session; 429 + `Retry-After`; JSON bodies capped at 1 MiB except blob
  uploads.
- **Status**: `/healthz` adds the version, database kind and schema. `cloud/status/` is a static
  page + `check.sh` feeder meant to run **off** the VM; recommended instead: the checker's hosted
  page on `status.` (RUNBOOK "Uptime and status page").
- **Runbook**: `cloud/RUNBOOK.md` (apply C2, deploy, rollback, restore, Postgres down, disk full,
  neighbour, incident + template, rotate secrets, revoke sessions, uptime and status, move host).
- **Deviations**: hourly encrypted dumps instead of a nightly dump + WAL archiving (RPO 1 h, no
  archive plumbing on the shared host); JSON logs and a metrics endpoint not done (the unit keeps
  `RWS_CONFIG_LOG_FORMAT=off` for the privacy promise; `/healthz` detail only); the disposable-mail
  list for Free and a report-abuse page left for C3 with the legal pages; per-org creates are the
  existing plan limits plus the per-session create rate.
- **Done when, as run**: `cargo test --workspace` (new `migrate_db`, `abuse`, `recovery`) and
  `--features postgres --test postgres` (8, incl. SQLite → Postgres → SQLite and dump + pg_restore)
  on CI; `cli/tests/sync.rs` `a_lost_device_comes_back_with_its_recovery_phrase`;
  `webapp/tests/sync.cjs` recovery checks in web and Tauri modes; CI job `ops`
  (`cloud/ops/test/ops-e2e.sh`: backup → drill on SQLite, LocalStack S3 and Postgres, wrong key,
  tampered backup, no recipients, alerts, status feeder, `systemd-analyze verify/security`;
  `cutover-sim.sh`: cutover, refused second run, rollback keeps writes, non-empty target;
  `postgres-setup-docker.sh` in ubuntu:24.04). **Not yet** (VM, owner): a drill of a real backup
  committed as evidence; `systemctl show` on the VM; row counts after the real switch; 7 days green
  on the checker.
- **Apply** (agent, when told): RUNBOOK "Apply C2" steps 1–5 need nothing from the owner but the
  write-freeze time for step 5; steps 6–8 need the owner items.
- **Owner items** (section 11): a bucket at another provider or region with a write-only key and a
  35-day lifecycle rule; an age key pair (public key to the VM, private key offline); the uptime
  checker; DNS `status.`; the write-freeze time; an alert address (`ALERT_TO`).

### C3 Stripe Managed Payments billing, pricing page, prompts: paid launch of Pro and Team

- Goal: a person can buy Pro or a team can buy Team, and the limits lift at once. This is the paid
  launch; it does **not** wait for an external review (Q13).
- Scope: Stripe Checkout Sessions with Managed Payments from the portal (price + seat quantity +
  signed org metadata); `POST /v1/billing/stripe` with signature verification →
  entitlements; customer portal link; seat changes through the API; the 14-day Team trial; dunning
  banner and downgrade to Free limits; refunds; pricing page and home copy (section 8); Sync page
  limit notes and "Manage plan"; CLI limit line; `/legal/*` pages linked; privacy page updated;
  the security page with the "no external review yet" and "shared server" sentences;
  THREAT_MODEL "Hosted service" section; the Cloud guide; `sync-server/README.md` status line
  changes from "for testing" to the real status; mail through the transactional service.
- **Done when:** `sync-server/tests/billing.rs` replays every recorded test-mode event (bad
  signature refused before parsing, replay ignored, out-of-order handled) and the entitlements
  match; an e2e in test mode buys Team with 3 seats, adds a 4th member (quantity 4 in Stripe),
  and an `invoice.payment_failed` then `customer.subscription.deleted` replay shows the banner
  and drops to Free with no data loss (pull still works); a refund replay ends the entitlement;
  `site` build has no "no subscription" sentence left (`grep -r "Is there a subscription? No"
  site/src` empty); the security page contains the no-review sentence (a site test checks it);
  the owner's switch to live mode and green light are recorded in section 11.
- Needs: C1, C2, C0 (Stripe account with Managed Payments, live keys, mail service, legal drafts
  reviewed).
- Owner handoffs: set the prices in Stripe; approve the legal pages and the security page
  wording; switch to live and place the live keys; give the launch green light; post the launch.

**Status of the first build (2026-11-03, on Lemon Squeezy; deployed with billing off, never
sold; superseded by "Moved to Stripe" below, which keeps everything here that isn't about the
provider: entitlements, statuses, seats, trial, ordering, org matching, clients):**

- Commits on `main`: 14509b4 (server: `billing.rs`, migration 7, tests), 0c0f2a9 (portal, web app
  Sync page), e30a23a (site: pricing, home, privacy, security and legal drafts, check script), and
  the docs commit after them.
- **Webhook** `POST /v1/billing/lemonsqueezy`, checked against Lemon Squeezy's documentation
  (2026-11-03: signing, request format, event types, subscription, invoice and order objects):
  `X-Signature` = hex HMAC-SHA256 of the raw body, constant-time compare, **before** parsing
  (401). Lemon Squeezy sends no event id and retries with the same bytes, so a delivery's id is
  the SHA-256 of its body (`billing_events`; replay = `duplicate`, nothing changes). Ordering:
  subscription events apply when the subscription's `updated_at` (microseconds) is not older than
  the last applied (`entitlements.ls_updated_at`), else `stale`; invoice events never move that
  mark, except a refund. The org comes from the subscription id already on an `entitlements` row,
  else from `custom_data` the server signed at checkout (org, buyer, HMAC under a server secret):
  a share link or a forged org is `unmatched` (recorded, nothing changes;
  `hitop-sync-server billing-link` fixes it). Wrong store and test-mode events (unless
  `HITOP_SYNC_LS_TEST_MODE=on`) are ignored. Every verified delivery answers 200 with its outcome;
  only a database error answers 500 (Lemon Squeezy retries). Bodies are never stored. A second
  subscription for an org takes over only when the org's own isn't running (a purchase after a
  refund or expiry); otherwise it is `superseded` (a double purchase: the operator refunds it).
- **Events → entitlements** (all 13 of section 6): `subscription_*` set plan (from the variant id),
  seats (Team: the item quantity), status, period end and the ids; `subscription_payment_failed`
  → `past_due` (banner, plan kept), `_success`/`_recovered` → `active`; a full
  `subscription_payment_refunded` or `order_refunded` → `refunded`; `order_created` and partial
  refunds are recorded only. Plan in force: `active`, `on_trial`, `past_due`, `cancelled` (until
  the period ends), `trialing` keep the plan; `expired`, `unpaid`, `paused`, `refunded` are Free
  limits. Nothing is deleted on any status (tests read every blob after expiry).
- **Seats (Q5)**: as C1 counts them (members, pending invites, workspace people; viewers are seats;
  Team's 5 guest viewers free). `POST .../billing/seats` changes the Team subscription item's
  quantity through the API: more = `invoice_immediately` (prorated, charged now), fewer =
  `disable_prorations`, never below the seats in use.
- **Checkout**: `POST .../billing/checkout` (an org admin with a session; access tokens can't buy)
  creates the checkout through the API with the buyer's email, the signed custom data, the variant
  and quantity, and the portal's plan page as the redirect. **Customer portal**: a fresh signed URL
  from `GET /v1/subscriptions/:id` each time (they expire in 24 h, so none is stored).
- **Trial**: 14 days of Team (5 seats, a proposal), no card, once per org, per account and per
  company email domain (free-mail domains excepted); ends by `billing::expire` (at every limit
  check, every read of the plan, hourly). Only where billing is configured.
- **Prices (Q4)**: never in the server (the variant decides the plan); the portal and pricing page
  show $6 / $10 / $20 (monthly $8 / $12 / $25) as the owner's figures until Lemon Squeezy shows the
  final ones.
- **Clients**: portal Plan page (buy yearly/monthly with seats, trial, Manage billing, seats,
  status line, payment-failed banner), "See plans" link on a 402 anywhere in the portal; web app
  Sync page "Manage plan" link on a 402 (https only, never in the desktop build) and the HITOP Cloud
  link renamed "Manage plan on HITOP Cloud"; CLI unchanged (the server's 402 message names the plan
  and the URL).
- **Site**: pricing page (free forever first, the Cloud tiers, the apps, rewritten FAQ with
  "Is there a subscription?" → only for HITOP Cloud, "Do you run monitors?" → no, refunds `#refunds`),
  home copy ("No account needed", "Team sync, encrypted"), a billing section in `/privacy.html`;
  `/security.html` (with both sentences) and `/legal/{terms,privacy,dpa,sla,subprocessors}.html`
  from `site/content/legal/*.md`, drafts behind `site/src/legal-status.js` `DRAFT` (banner,
  noindex, out of the sitemap, linked from nothing). `site/scripts/check-pages.mjs` (CI job `web`).
- **Docs**: THREAT_MODEL "Hosted service (HITOP Cloud C3)", sync-server README (status line,
  endpoints, settings), RUNBOOK "Billing (Lemon Squeezy, C3)" and secret rotation, CHANGELOG.
- **Deviations**: checkout is a plain redirect to Lemon Squeezy's page, not the Lemon.js overlay
  (no third-party script in the portal); fewer seats take effect at once with no credit (Lemon
  Squeezy's `disable_prorations`) instead of waiting for the renewal; `paused` counts as Free; a
  refund stays until a newer paid invoice or `subscription_resumed`; a trial needs billing on;
  Business isn't sold yet (C5); the e2e "in test mode" is the portal e2e against a fake API and
  recorded-shape deliveries, not the owner's real test store (that check is a handoff); mail
  through the transactional service is configuration (`SMTP_URL`), waiting for the owner's pick;
  the `cloud.` portal links on the pricing page need the owner's DNS before the deploy.
- **Done when, as run**: `cargo test --workspace` (new `tests/billing.rs`: 8 tests, every event,
  bad signatures refused before parsing, replay, out of order, Team 3 seats → 4th member after a 4th
  seat, payment failed → expired with every byte readable, refunds, trial, roles, unmatched and
  forged orgs) and `--features postgres --test postgres` (new `postgres_billing`; `migrate_db`
  copies `billing_events` and `trials`); clippy; portal e2e (44 checks, 13 new for billing);
  `webapp` unit (plan link); site build + `check-pages.mjs`. **Not yet** (owner): the same flow
  against the real test store, the switch to live, the green light.

**Moved to Stripe Managed Payments (2026-11-03, owner decision; built and tested against a fake
Stripe API and event bodies shaped like Stripe's documented ones; not deployed).**

- Checked against Stripe's docs on 2026-11-03: Managed Payments (overview, how it works,
  eligibility, set up, update a Checkout integration), the Checkout Session create API, Billing
  subscriptions and items, the customer portal, webhook signatures, the API changelog to
  `2026-10-28.endive`. Every call sends `Stripe-Version: 2026-10-28.endive` (Managed Payments needs
  `2025-03-31.basil` or later; since basil the period end is on the subscription item and an
  invoice names its subscription under `parent.subscription_details`).
- **Checkout** (`POST .../billing/checkout`): `POST /v1/checkout/sessions` form-encoded with
  `mode=subscription`, `managed_payments[enabled]=true`, `line_items[0][price]`,
  `line_items[0][quantity]` (seats; 1 for Pro), `customer_email`, `client_reference_id` = org,
  `metadata[org_id|user_id|sig]` and the same under `subscription_data[metadata]` (the signed
  custom data of C3, now on the session and the subscription), `success_url`/`cancel_url` = the
  portal's plan page. Never sent (Managed Payments refuses them; a test checks every request):
  `automatic_tax`, `tax_id_collection`, `subscription_data[default_tax_rates]`,
  `payment_method_collection`, `payment_method_configuration`, `payment_method_options`,
  `payment_method_types`, `saved_payment_method_options`, `customer_update[name|address]`,
  `shipping_address_collection`, `shipping_options`, `subscription_data[application_fee_percent|
  on_behalf_of|transfer_data|invoice_settings]`.
- **Prices**: found by lookup key (`GET /v1/prices?lookup_keys[]=…&active=true`, cached 10
  minutes; `pro_yearly` = Pro annual and so on), so the env needs only the two secrets;
  `HITOP_SYNC_STRIPE_PRICE_<PLAN>_<MONTHLY|ANNUAL>` pin one by id. The webhook knows the plan by
  the price id or the `lookup_key` on the event's price.
- **Webhook** `POST /v1/billing/stripe` (route `/v1/billing/lemonsqueezy` removed):
  `Stripe-Signature` `t=…,v1=…` (any `v1` may match: two while a secret rolls), HMAC-SHA256 of
  `"<t>.<raw body>"`, `t` within 300 s, before parsing (401). Idempotent by event id
  (`billing_events.id` = `evt_…`); ordering by event `created` per subscription (ties: the later
  delivery wins; subscription events often arrive before `checkout.session.completed`, so the
  subscription's own metadata names the org). `livemode` ≠ the key's mode (`sk_live_`/`rk_live_`
  vs test) = `livemode_mismatch` (replaces `HITOP_SYNC_LS_TEST_MODE` and the store id check).
  Outcomes `applied`, `duplicate`, `stale`, `unmatched`, `ignored`, `livemode_mismatch`,
  `unknown_price`, `superseded`.
- **Events → entitlements**: `customer.subscription.*` set plan, seats (item quantity), status and
  period end (item `current_period_end`, `cancel_at`, `ended_at`); Stripe statuses map to ours:
  `active`; `trialing` → `on_trial` (never `trialing`, which is our own trial); `active` with
  `cancel_at_period_end` or `cancel_at` → `cancelled` (plan until the period ends, then `expire`);
  `past_due`; `unpaid`; `paused`; `canceled` and `incomplete_expired` → `expired`; `incomplete` →
  `incomplete` (new: Free limits until the first payment, and no block on a second checkout).
  `checkout.session.completed` ties the subscription and customer to the org;
  `invoice.payment_failed` → `past_due`, `invoice.paid` → `active` again; `charge.refunded` in full
  (matched by customer) → `refunded`, partial → ignored.
- **Seats**: `POST /v1/subscriptions/:id` with `items[0][id]`, `items[0][quantity]`; more:
  `proration_behavior=always_invoice` + `payment_behavior=error_if_incomplete` (a declined card is
  Stripe's 402 → 502 `billing_refused`, nothing changes); fewer: `proration_behavior=none`.
- **Customer portal**: `POST /v1/billing_portal/sessions` (`customer`, `return_url`), a fresh
  session each time; 409 `no_subscription` until the org has a Stripe customer.
- **Settings**: `HITOP_SYNC_STRIPE_SECRET_KEY[_FILE]`, `HITOP_SYNC_STRIPE_WEBHOOK_SECRET[_FILE]`
  (both or neither; billing off when unset), optional `HITOP_SYNC_STRIPE_PRICE_*`,
  `HITOP_SYNC_STRIPE_API_URL` (tests). Every `HITOP_SYNC_LS_*` setting is gone (ignored if left in
  the env file).
- **Schema**: no migration. Migration 7's columns keep their names and hold Stripe ids
  (`ls_customer_id` = `cus_…`, `ls_subscription_id` = `sub_…`, `ls_variant_id` = `price_…`,
  `ls_item_id` = `si_…`, `ls_order_id` = the Checkout Session `cs_…`, `ls_updated_at` = event
  `created` in microseconds); the deployed server holds no billing data.
- **Clients**: portal copy says Stripe ("Sold through Link", "plus applicable tax", test mode =
  Stripe test purchases); the pricing page says "plus applicable tax"; privacy page, legal drafts
  (privacy, terms, security, sub-processors) and SOC 2 vendor list name Stripe.
- **Tests**: `tests/billing.rs` (10 tests: every event above, bad/old/rolled signatures, replay,
  stale and same-second events, forged and other-org metadata, live/test mismatch, Team 3 → 4
  seats, a declined seat charge, payment failed → deleted with every byte readable, refunds, the
  trial, roles, Business, prices by lookup key with the Managed Payments refused-parameter check),
  `postgres_billing`, unit tests (signature construction, statuses, settings); portal e2e against
  a fake Stripe API (lookup keys, Checkout Session form, seats, portal, refund, deleted).
- **Deviations**: refunds come from `charge.refunded` (Lemon Squeezy had refund events on the
  subscription); a refund doesn't cancel the subscription in Stripe (the RUNBOOK says to cancel
  it too). More seats use `always_invoice`: Managed Payments forbids one-off invoices, and a
  subscription update's proration invoice should not be one, but the owner's sandbox run must
  confirm it (fallback: `create_prorations`, charged at the next renewal). `incomplete` is a new
  Free status. Stripe's own trials aren't used (our server runs the Team trial).
- **Not yet** (owner): the sandbox run (section 11), live mode, the green light; deploy when told.

### C4 Run history and alerts from your own runners; shared environment secrets

- Goal: the history and alerts people pay Postman for, without HITOP running anything on its
  machines and without the server reading a report.
- Scope:
  - Server: a `runs/` write permission for runner credentials (only that path); `POST
    .../runs` status rows (pass/fail, duration, request count, failed count, time, collection,
    environment); retention purge per plan; alert rules per workspace (channels, "missed run"
    interval); alert delivery: email (mail service), webhook (JSON: status, collection, env, link,
    failed request names only when allowed), Slack incoming webhook; one alert per status change.
  - CLI: `hitop run --sync-report` (seal report.json v1 under the workspace key, upload blob +
    status row; needs a runner credential or a member session); `hitop sync runs [--json]` lists
    and decrypts history; `hitop schedule --sync` adds `--sync-report`; the Action gets a
    `sync-report` input (separate repo, after the CLI release).
  - Portal and Sync page: history timeline, durations, last failures (decrypted client-side);
    alert settings.
  - Shared environment secrets: `hitop sync secrets share --env prod` seals the values to every
    pinned member device and to runner credentials the sharer picks (`env-secrets/<env>` blob),
    opt-in with a confirmation that names the recipients; clients and runners merge shared values
    under their own keychain or `HITOP_SECRET_*` values. **Re-sealing:** a shared value is sealed
    per recipient device, so devices added later would miss it. Rule: whoever adds a member or
    trusts a new device re-seals every shared environment it can open for the new device in the
    same operation; a sharer's client also re-seals for any pinned device without a copy on its
    next sync. Removal rotates the workspace key but can't take back values already seen: the Sync
    page says "rotate the shared secrets this person could see".
  - No hosted monitors: the docs and pricing FAQ say "run it in your CI".
- **Done when:** sync-server tests: a runner uploads a sealed report and a status row (the server
  stores ciphertext only, `no_crypto_in_server` green; a runner can't write outside `runs/`);
  alert tests (webhook body has no response bodies or secret values; one alert per change; a
  missed run alerts once); retention purge per plan; CLI `tests/sync.rs`: `run --sync-report`
  then `sync runs` shows the decrypted report on another member's client, and shared-secret cases
  (a viewer without the opt-in sees names only; the server's files hold no value; a member and a
  device added after the share receive the values; a runner given the share runs with them; a
  removed member can't open values re-sealed after the rotation); web `tests/sync.cjs` history
  view; privacy page and team guide updated ("secret values never sync unless you share them").
- Needs: C3 to sell it; the server and CLI work can start after C1.
- Owner handoffs: none new (mail service from C0); release the CLI version that carries
  `--sync-report` and move the Action's `v1` tag as usual.

**Status (2026-11-03): built, not deployed.**

- Commits on `main`: 5f22d1a (hitop-sync-crypto 0.5.0: `seal_run`, `seal_share`,
  `seal_share_wrap`, `vectors/shared.json`), 3421b7f (server: migration 8, `runs.rs`,
  `alerts.rs`, `shares.rs`), 0f8afdf (CLI), 15edaee (hitop-wasm), then the web app and portal and
  the docs (see `git log -- webapp/src/sync.js cloud/portal spec/CLOUD_SERVICE.md`).
- **Server** (migration 8, SQLite and Postgres, after C3's 7): `runs` (status row + sealed
  `detail`, its bytes counted in `workspaces.bytes`), `alert_settings`, `alert_state` (each
  series' last status), `alert_deliveries` (queue and 30-day log), `env_shares`,
  `env_share_wraps`. Endpoints: `GET .../run-policy` (any member, runners too), `POST .../runs`
  (runner credentials, editors, owners: the one write a runner has; runners can't read history),
  `GET .../runs[/:id]`, `GET/PUT .../alerts` and `GET .../alert-deliveries` (owners),
  `GET .../env-shares`, `PUT .../env-shares/:id` (version = current + 1, editors and owners),
  `POST .../env-shares/:id/wraps`, `DELETE .../env-shares/:id`. Member removal deletes the
  removed member's share wraps; workspace delete removes all of it. Background: alert delivery
  worker every 5 s, retention purge and missed-run check hourly. `migrate-db` and `db-stats`
  cover the new tables (`Db::tables()`; `tests/common/fill.rs` fills them).
- **Plans** (`HITOP_SYNC_LIMITS=plans`, `runs::plan_of`; proposal numbers of section 3): run
  uploads a day per org Free 100 / Pro 1,000 / Team 5,000 / Business 20,000 (402 `runsPerDay`);
  history Free 7 days (status rows only: the sealed report is dropped with a note) / Pro 30 /
  Team 90 / Business 365; channels Free mail, Pro + webhook (402 `alertWebhooks`), Team + Slack
  (402 `alertSlack`); shared values Pro and up (402 `sharedSecrets`). Enterprise may override
  `runsPerDay` and `historyDays`. Without plans: everything, `HITOP_SYNC_RUN_RETENTION_DAYS`
  (365). `GET /v1/orgs/:org/limits` has `runs`, `usage` has `runsToday`.
- **Alerts**: on a series' (collection + environment) status change (passed → failed, failed →
  passed, a first run that fails) and once per missed interval; mail only to current members'
  addresses (checked on save and on send), webhooks https to public addresses (no redirects,
  10 s; `HITOP_SYNC_ALERT_WEBHOOKS=insecure-local` for tests), `HITOP_SYNC_ALERTS_PER_HOUR` (30)
  a workspace; webhook URLs only for owners, never logged. Webhook JSON: `event, workspace,
  collection, environment, status, run, startedAt, durationMs, requests, failed, link` and
  `failedRequests` only when allowed; Slack `{text}`.
- **Clients**: CLI `hitop run --sync-report` (never changes the exit code; a failed upload is a
  warning) and `--sync-secrets`; `hitop sync report <report.json>`, `sync runs [--json] [--id]
  [--limit]` (compares the sealed report with the status row), `sync alerts [--email]
  [--webhook] [--slack] [--failed-names on|off] [--status-rows on|off] [--missed-after] [--log]`,
  `sync secrets share --env E [--viewers] [--runner ID] [--runners] [--yes]`, `secrets ls`,
  `secrets unshare`; shares handed to new devices in `members`, `add`, `trust` and after each
  push; re-keyed in `remove`; `shares.json` remembers versions (rollback refused). `hitop
  schedule --sync`: cron line `--sync-report --sync-secrets`; GitHub workflow an upload step after
  the Action (`hitop sync report` in the image) for `--hitop-version` after 1.2. Web app and
  desktop Sync page: **Run history** (status rows, open a report on this device, the shared
  environments' names, owners' **Alert settings**), shared values in memory under the
  environment's own (`store.setSharedValues`, masked), re-seal on add and on giving keys to new
  devices, re-key on remove with the "rotate the shared secrets this person could see" note.
  Portal: **Run history** (`#/runs`, `#/workspaces/<ws>/runs[/<run>]`, the alert links), alert
  settings and the alerts sent, run uploads today on the usage table.
- **Deviations**: runs are rows of their own (`runs` table), not `runs/<ts>` blobs: they stay out
  of the change feed (1.1 and 1.2 clients unaffected), and purging them doesn't touch version
  vectors; the "runs/ write permission" is the upload endpoint. Shares are rows too (not
  `env-secrets/<env>` blobs), with their own key wrapped per device rather than per-device
  sealing of the values (one sealed copy, re-keyed on removal); the environment name is inside
  the sealed content. The portal shows status rows only (it holds no keys); reports open in the
  CLI and the Sync page. The web app reads shared values but doesn't share (sharing is
  `hitop sync secrets share`). The GitHub Action gets no `sync-report` input yet (separate repo,
  after the CLI release): the generated workflow uploads with `hitop sync report` after it, and
  CI runs through the Action don't use shared values until the Action passes `--sync-secrets`.
  Run upload status rows carry the collection name as the runner sends it (plaintext, as section
  4 says); the pricing FAQ's "run it in your CI" line is C3's page (handoff), the team guide says
  it.
- **Tests, as run**: `sync-server` `cargo test --workspace` (new `tests/runs.rs`: a runner's
  sealed report opened by another member, ciphertext only on disk, runner writes runs and nothing
  else, upload checks, one alert per change with no bodies or values and names only when allowed,
  a missed run once, status rows off, the hourly ceiling, a removed member's address skipped,
  every plan's limits and retention, self-hosted retention; `tests/shares.rs`: versions, wraps per
  device, viewers, removal, runner visibility, plan gate), `--features postgres` (new
  `postgres_runs_alerts_and_shares`; `migrate_db` fill covers the new tables);
  hitop-sync-crypto `vectors/shared.json`; hitop-wasm and `webapp/tests/unit.mjs` run them;
  `cli/tests/sync.rs` `run_history_from_a_runner_opens_on_another_members_computer` and
  `shared_secret_values_reach_only_the_devices_they_are_for` (also with
  `HITOP_SYNC_LIMITS=plans`), `tests/schedule.rs` (upload step, actionlint); `webapp/tests/sync.cjs`
  (history view, the report opened on the other member's device, alert settings);
  `cloud/portal` e2e (run history, the alert link, alert settings, run uploads today). CI: see
  `cloud/ops-log.md`.
- **Deploy** (not done): a dump, then the usual version deploy (`--features postgres`) after or
  with C3's; `/healthz` schema 8; `deploy/deploy.sh` for the web app and the portal. No new
  setting is required (alert mail uses the configured `SMTP_URL`; defaults: 1 MiB reports, 30
  alerts an hour, https webhooks). The next CLI release carries `--sync-report` and publishes
  hitop-sync-crypto 0.5.0 first (release-cli.yml); then move the Action's `v1` and add its
  `sync-report` input.

### C5 Business: hosted SSO per org, audit export and anchoring, admin console

- Goal: what a company's IT and security teams require before they let a team buy.
- Scope: per-org OIDC configuration in the DB (issuer, client id, secret encrypted at rest with
  the host key, domains, groups claim, `required`), domain verification by DNS TXT so an org can
  claim `@corp.example`, the existing `oidc.rs` flow keyed by org; audit export (`GET
  .../audit/export` signed JSONL with the chain head) and a daily anchor of the org's chain
  heads mailed to the org owners (default) or, opt-in per org, published at
  `status.*/audit-anchors/<org>/<date>.json` (public anchors leak workspace count and ids, so
  not the default), closing THREAT_MODEL item 7; admin console in the portal (all org workspaces,
  members, transfer owner, force rotation, remove device org-wide); priority support address and
  the SLA page (99.5%, shared host).
- **Done when:** `tests/sso.rs` and `tests/keycloak.rs` run per org (two orgs, two issuers, a
  user of org A can't sign in through org B's IdP); domain claim tests; export verify with
  `hitop sync audit --verify --anchor <file>`; admin console e2e; the T25 handoff ("hosted SSO:
  leave it off") is superseded here and noted in ROADMAP section 6.
- Needs: C3.
- Owner handoffs: approve the SLA text and the support address.

**Status (2026-11-03): built, not deployed. Per-org SSO stays off on the hosted test server
(Q15: no host key there).**

- Commits on `main`: 59dea46 (server: migration 9, `orgsso.rs`, `hostkey.rs`, `anchors.rs`,
  `admin.rs`, Business in `billing.rs`, tests), a87841e (CLI: `hitop sync audit --export`,
  `--verify --file/--anchor`, `hitop sync rotate`, rotation notes), 9cbed61 (portal: Admin, Single
  sign-on, Audit pages, Business on the Plan page), then the site drafts and docs commit.
- **SSO per org** (`orgsso.rs`): `GET/PUT/DELETE /v1/orgs/:org/sso` (owners change it with a
  session; admins read), `POST .../sso/test`; issuer, client id, secret, scopes, name, groups
  claim, required groups, `required`, `assumeVerified`, `enabled`. The client secret is sealed with
  AES-256-GCM under the **host key** (`HITOP_SYNC_HOST_KEY` or `_FILE`) and never answered.
  oidc.rs's flow keyed by org: one callback URL; `POST /v1/auth/sso/start` takes `org` or finds it
  from the `email` hint's verified domain; `GET /v1/auth/sso?email=` names the org's provider.
  An org's provider signs in **only addresses of its verified domains**; "SSO required" closes
  every other sign-in (mail, password, passkey, Google, GitHub, the server's own SSO) for those
  domains. 1.1/1.2 clients: `hitop sync login --sso --email <work address>`; without the address
  the server answers 400 naming `--email`. Without a host key every endpoint answers 404
  `org_sso_off` and `/v1/auth/sso` is unchanged.
- **Domains**: `GET/POST /v1/orgs/:org/domains`, `POST .../domains/:domain/verify`, `DELETE`;
  TXT `_hitop-verify.<domain>` = `hitop-verify=<token>` looked up over DNS over HTTPS
  (`HITOP_SYNC_DNS_URL`, a resolver's JSON API; unset = 503 `dns_off`); one org at a time (the first
  to verify keeps it), no free-mail domains, at most 20 per org.
- **Audit export and anchors** (`anchors.rs`): `GET /v1/orgs/:org/audit/export` (owners, admins):
  JSON lines (header, chains with head and `purgedThrough`, every event with its hash inputs, an
  Ed25519 signature line over the SHA-256 of the lines before); daily anchors of the org's chain
  heads (canonical text signed), mailed to the owners when a head moved; public at
  `GET /v1/audit-anchors/:org/:day` only when the owners opt in (`PUT .../audit/settings`);
  `GET/POST .../audit/anchors`; `GET /v1/audit/key`; `HITOP_SYNC_AUDIT_ANCHORS=off`. The signing
  key's seed is a `server_secrets` row. CLI: `hitop sync audit --export --org <org> [--out]`,
  `--verify --file <export> [--anchor <file>]...`, `--verify --anchor <file>...` (online); the
  anchor file may be the saved mail; the server's key is pinned on first use
  (`audit-keys.json`). Closes THREAT_MODEL item 7.
- **Admin console** (`admin.rs`, portal `#/orgs/<org>/admin`): every org workspace with members
  and roles, every member device; transfer owner (to a current member), ask for a key rotation
  (`rotationRequested` on the workspace; `hitop sync rotate` does it and clears it; status, pull and
  push tell owners), remove a device from the member's account everywhere (its wraps go, its
  workspaces ask for a rotation). Team has the basic console (list, transfer owner); Business adds
  the rotation request and device removal.
- **Business plan**: (since the Stripe move) the prices with lookup keys `business_yearly` and
  `business_monthly`, or `HITOP_SYNC_STRIPE_PRICE_BUSINESS_*` (off while none exists);
  per seat like Team (checkout, seats, webhook quantity). Plans with `HITOP_SYNC_LIMITS=plans`:
  SSO and domains (402 `sso`), export and anchors (402 `auditExport`), the admin actions (402
  `adminActions`) need Business or Enterprise; the console list Team and up (402
  `adminConsole`); sign-in never stops when a plan lapses. Self-hosted: everything, no gate.
- **Site drafts** (behind `DRAFT`): `/legal/sla.html` with the priority support line,
  `/legal/support.html` (proposed address `business-support@hitop.rws8.tech`, response times),
  the DoH resolver in the sub-processor list, Business points on the pricing page.
- **Migration 9** (SQLite and Postgres): `org_sso`, `org_domains`, `org_audit_settings`,
  `audit_anchors`, `workspaces.rotation_requested_at|_by`; `migrate-db`/`db-stats` copy them
  (`tests/common/fill.rs` fills every one).
- **Deviations**: forced rotation is a request the owners' clients carry out (`hitop sync rotate`,
  manual), since the server holds no key; "remove device org-wide" deletes the device key from the
  member's account (it can't be in some workspaces and not others), so it also drops that device's
  keys of the member's personal and other-org workspaces and asks those to rotate, with the org
  admin named as the requester (owner question: keep, or limit to org workspaces only); public anchors are served by
  the sync server at `/v1/audit-anchors/...`, the `status.*/audit-anchors/` route waits for the
  `status.` DNS (owner); domain checks need a DoH resolver (a sub-processor, unset on the test
  server); no re-seal command for a new host key (orgs save their secret again); the web app's
  Sync page doesn't offer `rotate` or the export (CLI and portal do).
- **Done when, as run**: `sync-server` `cargo test --workspace --features postgres` (Postgres 17)
  green with Keycloak 26.4.2: new `tests/sso.rs` per-org cases (two orgs, two issuers; B's provider
  can't sign in A's user; tokens from the other provider refused; SSO required beats the server's
  own SSO; off without a host key; plan gate), `tests/keycloak.rs` `per_org::*` (realms `hitop`
  and `hitop-b`: alice through A, erin through B, B's "alice@corp.test" refused), `tests/domains.rs`
  (TXT verify, one org at a time, free mail, admins only, no resolver), `tests/audit_export.rs`
  (export and anchors verify, a rewrite after the mailed anchor caught, public opt-in, plans),
  `tests/admin.rs`, `tests/billing.rs` Business, `postgres_org_sso_audit_export_and_admin`,
  `migrate_db` with the new tables; clippy. CLI `tests/sync.rs`
  `an_org_audit_export_checks_against_its_anchor_and_a_rewrite_shows` (export, anchor JSON and
  saved mail, a byte changed, a rewritten chain, a changed server key, the rotation note, `rotate`)
  and the audit unit tests; portal e2e 54 → 78 checks (admin console, SSO and domains with a fake
  resolver, audit export and anchors, Business purchase, no host key); site build +
  `check-pages.mjs`; compat job (released 1.1/1.2 against this server) on CI. ROADMAP section 6
  notes the T25 handoff as superseded.
- **Deploy** (not done): a dump, the usual version deploy (`--features postgres`), `/healthz`
  schema 9, `deploy/deploy.sh` for site and portal. Nothing new is required: leave
  `HITOP_SYNC_HOST_KEY` and `HITOP_SYNC_DNS_URL` unset on the test server (per-org SSO off, Q15).
  Daily anchors start for Business orgs (none on the test server). Business is sold once the owner
  adds its variants. On a self-hosted server (no plans) every team org gets daily anchors and the
  owners a mail on days the log changed, from the upgrade on (`HITOP_SYNC_AUDIT_ANCHORS=off` stops
  it); stored anchors aren't purged yet (one small row per org a day).

### C6 Enterprise: SCIM, self-host support offer, SOC 2 Type 1; region and host on demand

- Goal: close enterprise deals without a bespoke project per customer.
- Scope: SCIM 2.0 server (`/scim/v2/Users|Groups`, bearer per org) mapping to org members and
  workspace roles; enterprise self-host package (`hitop-sync-server support-bundle`, upgrade notes
  per release, a signed Docker image, a contract template: support, upgrade windows, indemnity);
  SOC 2 Type 1 readiness: evidence gathered since C1 loaded into the owner's compliance tool, gaps
  closed, security questionnaire answers page. **On demand only:** the US region (second host, org
  pinned at creation, portal picks the API host per org) when a paying customer asks (Q18), and the
  move to an own host when a trigger in section 5 fires.
- **Done when:** SCIM conformance tests (create, update, deactivate, group → role); the support
  bundle tested on a self-hosted instance; the compliance tool shows no open control without an
  owner decision; the owner has the contract template and the first questionnaire filled. For the
  on-demand items, when built: a two-region e2e (org in US, data never in the EU database: a
  row-count check per region) and a failover drill recorded.
- Needs: C5, owner (contracts, auditor booked).
- Owner handoffs: book the Type 1 audit; sign the contract template; decide each on-demand item.

**Status (2026-11-03): code parts built, not deployed. The on-demand items (US region, own
host) are not built (Q18: EU only; they wait for a paying customer or a section 5 trigger).**

- **SCIM 2.0** (`sync-server/src/scim.rs`, `/scim/v2`, RFC 7643/7644): ServiceProviderConfig,
  Schemas, ResourceTypes, Users and Groups (GET with `eq` filters on userName/externalId/id and
  displayName, paging; POST; PUT; PATCH add/replace/remove with or without path, capitalized ops
  and `"False"` as Entra ID sends them, `members[value eq "<id>"]`; DELETE), SCIM error bodies,
  `application/scim+json`. The router has no PATCH, so api.rs sends `/scim/v2/*` to scim.rs after
  the body cap and rate limits. **Token per org** (`hitop_scim_` + 32 bytes, SHA-256 stored, at
  most 5): `POST/DELETE /v1/orgs/:org/scim/tokens[/:id]` (owners, signed-in session), `GET
  /v1/orgs/:org/scim` (admins: tokens without values, directory size, groups);
  off until a token exists. Plans: Business and Enterprise (402 `scim`, on every SCIM call and on
  token creation; revoking always works); self-hosted: every team org.
- **Mapping**: Users = org members by address (`userName` must be the email address; an account
  joins, an address without one gets an org invite; seats checked, 402 without half-applying);
  `active: false`/DELETE takes the address out of the org and out of what SCIM gave in its
  workspaces (account and sessions stay). Groups carry roles set by org admins (`PUT
  /v1/orgs/:org/scim/groups/:id {orgRole, workspaces: [{workspace, role}]}`): org role `admin` or
  `member` (when any group gives one, members without one are `member`), workspace roles `viewer`
  or `editor`. A member already in the workspace gets the role; anyone else an invite marked
  accepted with their device keys (filled in when the owner lists invites, once they have a
  device), which the owner's client adds as any accepted invite (old clients included). Taking a
  group's workspace away removes only what SCIM gave (`scim_grants`: the invite, or the membership
  it brought with its wraps, plus a rotation request); members from before SCIM stay. Owners are
  never created, demoted or removed (409). SCIM invites don't count against the 50 open invites.
- **Portal**: SCIM page (`#/orgs/<org>/scim`): base URL, create/revoke tokens (shown once), each
  group's org role and workspace roles.
- **Support bundle**: `hitop-sync-server support-bundle [--out] [--log | --unit | --no-journal]
  [--since-hours]` (`support.rs`): versions, schema (database and binary), settings by name with
  values only on an allowlist (database/SMTP URLs cut to host), what is in effect, row counts (no
  hashes), health counts, last 200 error lines of `--log` or the journal with addresses, ids,
  tokens, IPs and URL queries scrubbed; 0600, never overwrites. `tests/support_bundle.rs` fills a
  self-hosted SQLite instance through the API (`fill.rs`, SCIM included), sets secrets in the
  environment and a log with addresses and tokens, and checks no address, id, name, token hash or
  secret of the database or environment is in the file; the image smoke test runs it inside the
  container.
- **Upgrade notes** per release: `sync-server/UPGRADE_NOTES.md` (schema 3 → 10 for the next
  release, 1.2.0, 1.1.0), linked from the README.
- **Signed image** (none existed): `.github/workflows/release-sync-server.yml` on the existing
  `cli-v*` tags: `ghcr.io/bohdaq/hitop-sync-server:<version>` (+ `latest` without a suffix),
  linux/amd64, SQLite and Postgres, provenance, cosign keyless signature by digest, SPDX SBOM
  attestation, both verified with the workflow identity, `deploy/image-smoke.sh` (/healthz on a
  fresh volume, support-bundle in the container). The Dockerfile now builds from the repository
  root (the old sync-server-only context lacked the client crate the Cargo.toml names);
  `ci.yml` job `sync-server-image` builds and smoke-tests it on every push without pushing.
- **Drafts behind `DRAFT`**: `/legal/security-questionnaire.html` (answers, with what isn't done:
  no external review, no certification, backups and uptime waiting on C0/C2 items) and
  `/legal/enterprise-agreement.html` (template: support with response times, supported versions
  and upgrade windows, security and DPA, IP indemnity, liability; AGPL stays the licence, Q6).
- **SOC 2 Type 1 readiness**: `cloud/soc2-type1-readiness.md`, every control → the evidence since
  C1 (path, in place / built / gap) and twelve owner decisions (S1–S12: scope and date, tool and
  auditor, approvals, backups and a restore drill, uptime, 2FA, single-operator oversight, records,
  vendor reports, review timing, pinning Actions, legal drafts).
- **Migration 10** (SQLite and Postgres): `scim_tokens`, `scim_users`, `scim_groups`,
  `scim_group_members`, `scim_group_workspaces`, `scim_grants`; `migrate-db`/`db-stats` copy them
  (`fill.rs` fills each, `tests/migrate_db.rs` lists them).
- **Done when, as run**: `tests/scim.rs` (discovery and SCIM errors; tokens: owners only, session
  only, revoke, personal org, other org 404; create, filter `userName eq` with and without a match,
  uniqueness, PATCH and PUT, deactivate (`"False"`), reactivate, new userName, delete, owner never
  removed; group → org admin and workspace editor, the accepted invite with device keys, the
  owner's add, role set for an existing member, removal with wraps gone and rotation requested,
  pre-SCIM member kept, invite removed with the group, account-less address accepted later,
  deactivation clearing invites; another org's token sees nothing; plans Team/Pro/Free 402,
  Enterprise allowed, revoke on a lapsed plan, seats 402), `tests/support_bundle.rs`, scim/support
  unit tests, `postgres_scim` (CI Postgres 17), `cargo test --workspace` and clippy with all
  features green; portal e2e + SCIM checks; the image job on CI.
- **Deviations**: the "compliance tool shows no open control without an owner decision" and "the
  first questionnaire filled" parts are owner items (the tool isn't picked; the answers page is the
  questionnaire basis); groups' roles are set in the portal, not taken from IdP attributes;
  `userName` must be an email address (Okta default; Entra ID users map `userPrincipalName` or
  `mail` to it); no Bulk, sort, ETags; the image is amd64 only (arm64 under QEMU would take hours
  for the Rust build); the image is tagged with the release version while the crate stays 0.1.0.
- **Deploy** (not done): a dump, the usual version deploy (`--features postgres`), `/healthz`
  schema 10, `deploy/deploy.sh` for site (two new drafts, noindex) and portal (SCIM page). No new
  setting; SCIM stays off for every org until an owner makes a token (Business only on the hosted
  server, none there).

### C7 Deferred, not planned

Hosted monitors (until confidential computing, section 4), mock servers, API docs publishing,
comments/review. Reason: monitors need a key the server must not hold; none of the others is our
differentiation, two of them need the server to read collections, and review already happens in
pull requests. Revisit after C6 when there is demand evidence (support mail, lost trials or deals).

---

## 10. Questions (answered) and risks

All questions were answered by the owner on 2026-11-02 (section 0).

| # | Question | Options | Decision |
| --- | --- | --- | --- |
| Q1 | Service name | A: HITOP Cloud; B: HITOP Teams; C: HITOP Sync | **A** HITOP Cloud |
| Q2 | Domain | A: stay under `hitop.rws8.tech`; B: buy a product domain now | **A** |
| Q3 | Billing provider | A: Paddle; B: Lemon Squeezy; C: Stripe Billing + Stripe Tax; D (2026-11-03): Stripe Managed Payments | **B** Lemon Squeezy (merchant of record), changed to **D** Stripe Managed Payments (merchant of record) on 2026-11-03 |
| Q4 | Prices | A: $6 / $10 / $20; B: lower; C: higher | **A**, a proposal until set in Stripe |
| Q5 | Viewers | A: seats, 5 free guest viewers per Team org; B: free; C: all count | **A** |
| Q6 | Enterprise license model | A: AGPL + support/indemnity; B: dual license + scoped CLA; C: no outside contributions | **A** |
| Q7 | Code visibility of portal and billing code | A: this repo, `cloud/`, AGPL; B: private repo | **A** |
| Q8 | Monitor model | A: runner-credential monitors on a separate host; B: no monitors, bring your own runner; C: wait for confidential computing | **B now, C later**: no hosted monitors until confidential computing; history and alerts from the customer's runners |
| Q9 | Shared environment secret values | A: opt-in per environment, E2E; B: never | **A** |
| Q10 | Run history storage | A: sealed, status rows plaintext; B: plaintext reports | **A** |
| Q11 | Hosting | A: two dedicated VMs; B: stay on the current VM; C: Kubernetes | **B**, hardened in C2; move on a trigger (section 5) |
| Q12 | Mail for codes and alerts | A: transactional service; B: current mail host | **A** before the paid launch (Postmark or SES) |
| Q13 | External crypto review timing | A: before paid launch; B: launch first, review later | **B**: later; the security page says no review yet |
| Q14 | Free hosted tier | A: 3 / 3 / 100 MB; B: none; C: generous | **A** |
| Q15 | Hosted SSO on the shared test server now | A: no, until Business; B: yes | **A** |
| Q16 | App Store apps and subscriptions | A: sign in only; B: IAP; C: hide Cloud | **A** (owner checks guideline 3.1.3) |
| Q17 | SOC 2 | A: after the first enterprise asks; B: start now | **B**: groundwork in C1/C2, Type 1 first |
| Q18 | Second region | A: on demand later; B: EU + US at launch | **A**: EU only at launch |

### Risks

- **Single operator.** One person runs the service and the company. Mitigation: runbook, tested
  restores, a second admin path, status page, an honest 99.5%.
- **Single shared VM.** HITOP Cloud shares `bohdan-prod-donotdelete` with the mail server, Gitea
  and other sites: one disk, one kernel, one network path. A neighbour's overload, full disk or
  compromise can take HITOP Cloud down or expose the host. Mitigation: systemd limits and
  sandboxing, Postgres on a local socket, disk alerts, off-site encrypted backups with drills,
  the off-host uptime check, the shared-host sentence on the security and SLA pages, and the move
  trigger in section 5.
- **Launching without an external review.** We take money for E2E encryption that no outside
  party has checked. A design flaw found later hurts the one claim we make. Mitigation: the
  security page says so plainly; the threat model lists residual risks; clients refuse bad keys
  (1.2 signed wraps); tests include a malicious server; the review stays an open owner item, and
  revenue from C3 is the natural budget for it.
- **Managed Payments eligibility.** Stripe decides eligibility (business type, region; Poland and
  the EU are supported) and may stop it for a product it doesn't consider a fully automated
  digital product, or for a high dispute rate. Mitigation: HITOP Cloud is SaaS with no human
  service in the plan (support is product support, as Stripe allows); entitlements are
  provider-neutral, so Stripe Billing + Stripe Tax (the owner then files tax) can replace it behind
  the same handler.
- **Managed Payments limits.** Checkout Sessions and Payment Links only (no subscription made
  through the API, no one-off invoices or invoice items on a subscription), Stripe controls payment
  methods and the checkout ("Sold through Link", no custom domain), and Stripe/Link support may
  refund within 60 days on its own. Invoice and PO payment aren't offered. Mitigation: Business
  pays by card; Enterprise is invoiced by the owner's company outside Stripe; refunds arrive as
  `charge.refunded` and end the plan like the owner's.
- **No hosted monitors.** Some teams expect them (Postman has them). Mitigation: CI-based runs
  with history and alerts, clear FAQ, the price lever in section 3, the revisit trigger in
  section 4.
- **Run uploads as an abuse vector** (storage, alert spam). Mitigation: per-plan daily ceilings,
  retention purge, one alert per status change, rate limits.
- **Promise drift.** The site today says "no accounts, no sync, no subscription". Mitigation:
  C3 rewrites every such sentence in one commit with a grep check.
- **Format v1 pressure.** Run reports and shared secrets must not become files in
  `environments/`. Mitigation: sync-only blobs by rule (section 9 header).
- **Seat gaming and plan downgrades.** Mitigation: viewers count (Q5), downgrades never delete,
  creates refused with a clear note.

---

## 11. Owner handoffs for this plan

| Date | Tranche | What the owner needs to do | Done |
| --- | --- | --- | --- |
| 2026-11-02 | plan | Answer Q1–Q18. | yes, 2026-11-02 (section 0) |
| 2026-11-02 | C0 | Legal entity and payout bank for the Stripe account (until 2026-11-03: the Lemon Squeezy store); Managed Payments enabled and accepted by Stripe's eligibility review. | |
| 2026-11-02 | C0 | In Stripe: products, prices, secret key and webhook, sandbox first; the steps are in the 2026-11-03 C3 Stripe row below. Place key and secret in a root-only env file on the VM. | |
| 2026-11-02 | C0 | Pick and create the transactional mail service (Postmark or SES); verify the sending domain; place the key. | |
| 2026-11-02 | C0 | Create an S3-compatible bucket at another provider or region for backups; generate the age key and keep the private half offline; place the bucket credentials and the public key on the VM. | |
| 2026-11-02 | C0 | DNS: `cloud.hitop.rws8.tech` and `status.hitop.rws8.tech` → the VM. | |
| 2026-11-02 | C0 | Pick an uptime checker outside the VM's provider; create the account. | |
| 2026-11-02 | C0 | Pick the SOC 2 compliance tool (Vanta, Drata, Secureframe or a spreadsheet) and the auditor; Type 1 first. | |
| 2026-11-02 | C1 | Read and approve the policies in `cloud/policies/`; confirm the first access review (`cloud/evidence/access-reviews/2026-Q4.md`: fill the "owner lists" rows, sign) and the vendor list (`cloud/policies/vendors.md`: the VM provider, DNS provider). Then quarterly access reviews. | |
| 2026-11-02 | C2 | Agree the write-freeze time for the SQLite → Postgres switch; keep the backup key offline. | |
| 2026-11-03 | C2 | For the built C2 (status in section 9): bucket with a write-only key and a 35-day lifecycle rule; an age key pair (public key to `/etc/hitop-sync/backup-recipients.txt`, private key offline); uptime checker monitors (`cloud/RUNBOOK.md`); the `status.` CNAME; an alert address; then run the first restore drill on your computer and commit its evidence. | |
| 2026-11-03 | C1b | Register the OAuth apps and place their ids and secrets in `/etc/hitop-sync/env` (or `_FILE`): **Google** (Cloud console → Credentials → OAuth client ID, Web application; consent screen with `openid`, `email`; authorized redirect URI `https://sync.hitop.rws8.tech/v1/auth/google/callback`) → `HITOP_SYNC_GOOGLE_CLIENT_ID/SECRET`; **GitHub** (Settings → Developer settings → OAuth Apps; callback URL `https://sync.hitop.rws8.tech/v1/auth/github/callback`) → `HITOP_SYNC_GITHUB_CLIENT_ID/SECRET`. Optional: a password pepper (`HITOP_SYNC_PASSWORD_PEPPER_FILE`, kept out of backups). | |
| 2026-11-02 | C3 | Lawyer review of the drafts: ToS, privacy update, DPA, sub-processor list, SLA (99.5%, shared host), refund policy. Approve the security page wording (no external review yet; shared server). Confirm the App Store position (guideline 3.1.3). | |
| 2026-11-02 | C3 | Switch to live (live products and prices with the same lookup keys, the live webhook endpoint and key, the customer portal configuration), place the live keys, give the paid-launch green light, post the launch yourself (drafts in `launch/`). | |
| 2026-11-03 | C3 | ~~Lemon Squeezy store~~: replaced by the Stripe row below (owner decision 2026-11-03). | n/a |
| 2026-11-03 | C3 | **Stripe Managed Payments (sandbox first)**: (1) Account: business details and payout bank (Poland is a supported location); Settings → Managed Payments → enable (Stripe reviews eligibility: SaaS, fully automated, sold directly). (2) Products **HITOP Cloud Pro**, **Team**, **Business**, each with tax code **SaaS – business use** (`txcd_10103001`, marked "Eligible for Managed Payments"), recurring USD prices, **tax-exclusive**, flat rate per unit (quantity = seats; Pro is bought with quantity 1): Pro $8 a month and $72 a year, Team $12 per seat a month and $120 per seat a year, Business $25 per seat a month and $240 per seat a year (the proposals; other prices: tell the agent, they also appear in `cloud/portal/src/portal-app.js` `PRICES` and `site/src/pages/pricing.js` `TIERS`), with the lookup keys `pro_monthly`, `pro_yearly`, `team_monthly`, `team_yearly`, `business_monthly`, `business_yearly`; no Stripe trial on them (our server runs the Team trial). (3) Developers → API keys: the secret key, or a restricted key with write on Checkout Sessions, Subscriptions and Customer portal and read on Prices → `HITOP_SYNC_STRIPE_SECRET_KEY`. (4) Developers → Webhooks → Add endpoint `https://sync.hitop.rws8.tech/v1/billing/stripe`, API version **2026-10-28.endive**, events `checkout.session.completed`, `customer.subscription.created`, `customer.subscription.updated`, `customer.subscription.deleted`, `customer.subscription.paused`, `customer.subscription.resumed`, `invoice.paid`, `invoice.payment_failed`, `charge.refunded`; its signing secret → `HITOP_SYNC_STRIPE_WEBHOOK_SECRET`. (5) Settings → Billing → Customer portal: on; allow updating the payment method and billing address, invoice history, cancelling (at the end of the period), and switching between the monthly and yearly price of the same product only (no switch between products; quantity changes off: seats change in the HITOP portal, never below the seats in use); business name, terms and privacy links. (6) Settings → Billing → Subscriptions and emails: Smart Retries on; when all retries fail, cancel the subscription (the org drops to Free limits; nothing is deleted); failed-payment emails on. (7) Settings → Checkout: terms of service and privacy policy links (the legal drafts once approved). Put (3) and (4) in `/etc/hitop-sync/env` (root only; `_FILE` paths allowed), never in chat or git; prices need no setting (lookup keys). (8) Then tell the agent: deploy and run the sandbox purchase (Team, 3 seats, card 4242 4242 4242 4242; a 4th seat; a failed renewal with 4000 0000 0000 0341; a refund). Sandbox products and lookup keys: set up by the owner 2026-11-03. | partly (sandbox products 2026-11-03) |
| 2026-11-03 | C3 | **Approve the drafts**: `site/content/legal/*.md` (terms: fill in the governing law; sub-processors: the VM provider and data centre, the mail service, the backup storage, the uptime checker) and the security page wording; the refund policy (proposal on the pricing page: full refund within 14 days of the first payment); the trial (14 days, 5 seats). Approval = set `DRAFT = false` in `site/src/legal-status.js` (or tell the agent). Also: DNS `cloud.hitop.rws8.tech` (the pricing page links the portal there) and the mail service before live. | |
| 2026-11-02 | later | External crypto review (Q13: not a launch gate). Scope: ROADMAP section 6 rows T23/T24/T25/1.2; `sync-server/THREAT_MODEL.md`, `cli/crates/hitop-sync-crypto`, `cli/crates/hitop-core/src/sync.rs`, `cli/crates/hitop/src/commands/sync/`, `webapp/src/sync.js`, `sync-server/src/{oidc,audit,workspaces,blobs}.rs`. Rough budget €10–25k. Tell the agent when it starts; the security page links the summary when done. | |
| 2026-11-02 | C6 | Book the SOC 2 Type 1 audit; sign the enterprise contract template; decide the US region and an own host when a customer or a trigger (section 5) asks. | |
| 2026-11-03 | C6 | **SOC 2 Type 1**: work through the owner decisions S1–S12 in `cloud/soc2-type1-readiness.md` (scope and "as of" date, compliance tool and auditor, policy approval and the signed 2026-Q4 access review, backups and the first restore drill, uptime checker, 2FA on every operator account, single-operator oversight and change review, training record / risk register / tabletop, vendor reports, review timing, pinning Actions, legal drafts); load the evidence into the tool; book the audit. | |
| 2026-11-03 | C6 | **Enterprise drafts**: fill and approve `/legal/enterprise-agreement.html` (legal entity, governing law, fees, response times, supported versions, upgrade windows, liability cap, insurance) and `/legal/security-questionnaire.html` (check each answer; it is the basis for the first customer questionnaire). Approval = `DRAFT = false` with the other drafts. | |
| 2026-11-03 | C6 | **SCIM and the image**: nothing to set on the server. Decide whether to keep the deactivation rule (out of the org and of what SCIM gave; account and sessions stay) and the `userName` = email rule. The signed image appears with the next `cli-v*` tag (agents don't tag); after it, check `cosign verify` as in `sync-server/README.md`. Decide later whether to build arm64 images. | |
| 2026-11-03 | C5 | **Approve the drafts**: `/legal/sla.html` (99.5%, shared host, credits, priority support line) and `/legal/support.html`; pick the **priority support address** (proposal `business-support@hitop.rws8.tech`: create the mailbox or alias, or tell the agent another) and whether outage answers within 4 hours are promised. Approval = `DRAFT = false` (with the C3 drafts). | |
| 2026-11-03 | C5 | **Business in Stripe** (with the C3 Stripe row): product HITOP Cloud Business, per seat, $240 per seat a year and $25 per seat a month (the proposal), lookup keys `business_yearly` and `business_monthly`. Business isn't sold until those prices exist. | sandbox: yes, 2026-11-03 |
| 2026-11-03 | C5 | **Per-org SSO on the hosted server** (only when you want Business SSO live; Q15 says not on the test server): a host key line `HITOP_SYNC_HOST_KEY=<openssl rand -hex 32>` in `/etc/hitop-sync/env` (keep a copy offline, never in backups) and a DNS-over-HTTPS resolver `HITOP_SYNC_DNS_URL` (e.g. Cloudflare; add it to the sub-processor list); RUNBOOK "Business features (C5)". Until then every per-org SSO endpoint answers 404 `org_sso_off`. | |
| 2026-11-03 | C5 | Optional: DNS `status.hitop.rws8.tech` (C0 row) also carries public audit anchors once an rws route sends `/audit-anchors/` to the sync server's `/v1/audit-anchors/` (the agent adds the route with your go). | |
