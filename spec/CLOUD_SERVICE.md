# HITOP Cloud: the paid service

Plan for a hosted service on top of HITOP, for individual developers, small teams, mid-size
companies and enterprises. Written 2026-11-02 and aligned the same day with the owner's
decisions (section 0). It is the work order for tranches C0–C7 (section 9). Every price and tier
name in this file is a **proposal** until the owner sets it in Lemon Squeezy (ROADMAP section 3:
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
| Q3 | Billing: **Lemon Squeezy** (merchant of record) | yes (was Paddle) |
| Q4 | Prices: Pro $6, Team $10 per seat, Business $20 per seat, Enterprise by quote (annual; monthly costs more); a proposal until set in Lemon Squeezy | no |
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
| Q15 | No SSO on the shared test server until the Business tier | no |
| Q16 | App Store builds: sign-in only, no purchase UI (owner checks guideline 3.1.3) | no |
| Q17 | **SOC 2 groundwork starts now** (C1/C2); owner picks a compliance tool and auditor; Type 1 first | yes (was when the first enterprise asks) |
| Q18 | EU only at launch; US region later on demand | no |

### Later decisions

| Date | Decision | What it changes |
| --- | --- | --- |
| 2026-11-03 | Normal sign-in and sign-up before C2 is applied: email + password (sign-up with email verification, password reset) **and** "Continue with Google" / "Continue with GitHub". Mail codes, passkeys and SSO stay. | New tranche C1b (after C2's code, before C2 is applied on the VM). Owner registers the Google and GitHub OAuth apps. |
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
  (hosted or self-hosted, with support). Sold through Lemon Squeezy as merchant of record.
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
| C0 | Owner decisions and accounts (parallel to C1) | Lemon Squeezy store, mail service, backup bucket, DNS names, compliance tool picked, legal drafts |
| C1 | Organizations, plans, entitlements; portal MVP; SOC 2 policies | org → plan → limits enforced; portal: sign in, org, members, usage; written policies and access review |
| C2 | Harden the current VM | Postgres on the VM, systemd resource limits, off-site encrypted backups + restore drill, recovery key, status page, off-host uptime check, runbook |
| C3 | Lemon Squeezy billing, pricing page, prompts | **paid launch of Pro and Team**; no review gate |
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

Prices are per month, billed annually; "monthly" is the month-to-month price. USD list; Lemon
Squeezy shows local currency and adds tax at checkout. All numbers are **proposals for the owner**.

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
| Billing | none | card (Lemon Squeezy) | card (Lemon Squeezy) | card, annual (invoice payment: see section 6) | contract, invoice, PO |
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

The owner may prefer round EUR prices (€5/€9/€19) for an EU company; Lemon Squeezy shows the
buyer's currency either way.

---

## 4. Features per tier: exists today vs must be built

Legend: **have** = in main today; **build** = new code; **owner** = owner handoff.

| Feature | Tier | State | Notes |
| --- | --- | --- | --- |
| Hosted sync, E2E workspaces, invites, roles, rotation | Free+ | have | `sync-server/`, clients in CLI, desktop, web; 1.2 signed wraps |
| Organizations, plans, seats, per-org limits | all | build (C1) | today: users and workspaces only; limits are server-global env (`HITOP_SYNC_MAX_*`, 50 workspaces, 10 runners, 50 invites) |
| Account portal (sign in, org, members, usage, plan, API tokens) | all | build (C1, C3) | separate small app; shares the sync-server session; billing details live in the Lemon Squeezy customer portal |
| Device recovery key | all | build (C2) | THREAT_MODEL open item 4; a paying customer who loses a laptop must not lose the workspace |
| Web app device key not in localStorage | all | build (C2, partial) | THREAT_MODEL item 5: at least an explicit warning + recovery key; a WebCrypto non-extractable key can't drive the Rust HPKE code |
| Billing (checkout, seats, trials, dunning, receipts, tax) | Pro+ | build (C3) + owner | Lemon Squeezy as merchant of record; signed webhooks → entitlements |
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
                           plan (links to the Lemon Squeezy checkout and customer portal),
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
| Secrets in root-only env files: Lemon Squeezy API key and webhook secret, mail service key, OIDC, backup public key | as today | **owner** places keys |
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

## 6. Billing: Lemon Squeezy

Lemon Squeezy is the merchant of record (Q3): it sells to the buyer, collects and remits VAT and
sales tax, and issues receipts in its name. Fees about 5% + $0.50 per transaction plus
surcharges (international cards, PayPal); checked 2026-11-02 on third-party pages, the owner
confirms on signing up. We never see card data.

### Store setup (owner)

- The owner creates the Lemon Squeezy store, completes identity and payout details, and submits
  the store for approval (Lemon Squeezy reviews SaaS stores before live payments).
- **Products and variants:** one product per tier (Pro, Team, Business), each with an annual and a
  monthly variant at the section 3 prices. Team and Business variants are **per-seat
  subscriptions** (quantity = seats). Enterprise is not sold through checkout.
- The owner creates an API key and a webhook (URL `https://sync.hitop.rws8.tech/v1/billing/lemonsqueezy`,
  the events below, a signing secret), first in **test mode**, later in live mode, and places the
  key and secret in a root-only env file on the VM.

### What the code does (C3)

- **Checkout:** the portal opens the Lemon Squeezy checkout overlay (Lemon.js) for a variant and
  seat count, or falls back to a checkout link. The checkout carries `custom_data` with the org id,
  so the webhook knows which org bought.
- **Customer portal:** the portal's "Billing" button opens the Lemon Squeezy customer portal (the
  signed URL from the subscription) for card changes, receipts, cancel and plan changes.
- **Webhooks** (`POST /v1/billing/lemonsqueezy`): the `X-Signature` header is checked as
  HMAC-SHA256 of the raw body with the signing secret (constant-time compare) before anything is
  parsed; events are idempotent by event id and ordered by `updated_at`. Events used:
  `subscription_created`, `subscription_updated`, `subscription_cancelled`, `subscription_resumed`,
  `subscription_expired`, `subscription_paused`, `subscription_unpaused`,
  `subscription_payment_success`, `subscription_payment_failed`, `subscription_payment_recovered`,
  `subscription_payment_refunded`, `order_created`, `order_refunded`. They update one
  `entitlements` row per org (plan, seats, status, period end, Lemon Squeezy ids). Entitlements
  are the only thing the rest of the server reads.
- **Trials:** 14 days of Team for a new org, no card, run by our server (not Lemon Squeezy). After
  the trial the org drops to Free limits with nothing deleted (reads always work; creates beyond
  Free are refused with the plan note).
- **Seat changes:** the portal shows seats used vs bought. Adding a member beyond the bought seats
  asks the owner to add seats; the server updates the subscription item quantity through the Lemon
  Squeezy API (prorated, charged at once). Removing members lowers the quantity at the next renewal,
  never mid-term (simple, predictable).
- **Dunning:** Lemon Squeezy retries failed payments and mails the buyer. We show a "payment
  failed" banner while the subscription is `past_due`; when it becomes `unpaid` or `expired` the
  org drops to Free limits, nothing deleted.
- **Refunds:** issued by the owner in the Lemon Squeezy dashboard; the refund webhook lowers or
  ends the entitlement as the refund says. Policy text on the pricing page (owner's call; proposal:
  full refund within 14 days of the first payment).
- **Invoices for Business:** Lemon Squeezy issues receipts and tax invoices for card payments. Pay
  by bank transfer or PO isn't its strength; until a deal needs it, Business annual pays by card and
  Enterprise is invoiced by the owner's company outside Lemon Squeezy.
- **Tests:** `sync-server/tests/billing.rs` replays recorded test-mode webhook bodies (every event
  above, a bad signature, a replay, out-of-order events); entitlements enforcement in
  `tests/limits.rs`. The whole flow runs in test mode before the owner switches to live.
- **App Store:** subscriptions are sold on the web only; the Mac and iOS apps sign in to an account
  and never show a buy button. The owner checks whether guideline 3.1.3(b) (multiplatform
  services) applies; if it does, the MAS/iOS builds either add IAP for the same plans or hide the
  Cloud features (compile-time flag, like telemetry).

---

## 7. Legal and compliance (owner)

Everything here is the owner's: agents draft text, the owner signs.

- **Legal entity and bank**: the owner's company (or a new one) as the Lemon Squeezy store owner
  and payout account.
- **Terms of Service**: none exist today (`/terms/*.html` is the guides path and must keep
  working). New path `/legal/terms.html`, `/legal/privacy.html` (the current `/privacy.html` stays
  and links there), `/legal/dpa.html`, `/legal/sla.html`, `/legal/subprocessors.html`. Drafts by an
  agent, reviewed by a lawyer before the paid launch.
- **Privacy**: rewrite the privacy page for accounts, billing (Lemon Squeezy), run uploads (what the
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
    database, the backup bucket, Lemon Squeezy, the mail service, DNS, GitHub and the App Store,
    reviewed and recorded in `cloud/evidence/access-reviews/`.
  - **Change management = git history + CI**: every change lands as a signed-off commit on `main`
    with CI green; deploys are recorded with the commit they ship (`cloud/ops-log.md`). The policy
    says exactly that; no extra tooling.
  - **Backup evidence** (C2): each restore drill's output and date in `cloud/evidence/restore-drills/`.
  - **Vendor list** (`cloud/policies/vendors.md`, C1): hosting provider of the VM, backup
    object-storage provider, Lemon Squeezy, the transactional mail service, the uptime checker,
    GitHub, DNS provider; what data each sees and its own compliance report.
  - **Incident log** and status page history (C2).
  - The owner picks the **compliance tool** (Vanta, Drata, Secureframe or a spreadsheet) and the
    **auditor**; rough budget €15–30k for the first Type 1 with a tool. Until the report exists the
    security page says "SOC 2: groundwork in progress, no report yet".
- **Trademark**: `TRADEMARK.md` exists; "HITOP Cloud" falls under it. Optional EU trademark
  filing (owner).
- **Export/sanctions**: Lemon Squeezy screens buyers as merchant of record. Nothing more needed at
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
without the owner's figures (prices live in Lemon Squeezy; code reads variant ids), change
DNS/App Store/money, post publicly, touch the VM beyond what ROADMAP allows and C2 lists, store
secrets anywhere but root-only env files. Code for the service goes in `cloud/` (portal, policies,
runbook) and `sync-server/` (orgs, entitlements, billing webhooks, run status, alerts), all AGPL;
client changes stay MIT. Format v1 stays additive-only: every new thing is a sync-only blob or
server-side row, never a new file in `environments/`.

| ID | Tranche | Status | Needs |
| --- | --- | --- | --- |
| C0 | Owner decisions and accounts | decisions done; accounts open | owner |
| C1 | Organizations, plans, entitlements; portal MVP; SOC 2 policies and first access review | **deployed** to the test server 2026-11-02 (2049e95, CI green; `cloud/ops-log.md`); left: the `cloud.` route after the owner's DNS, and the owner's approvals | the 1.2 sync server deployed to the test server first (ROADMAP section 6, "1.2 signed key wraps") so migration 4 and the 1.2 deploy don't collide |
| C2 | Harden the current VM: Postgres, limits, backups + drill, recovery key, status page, runbook | **built** 2026-11-03, CI green, **not applied** to the VM (status below; apply: `cloud/RUNBOOK.md` "Apply C2") | C0 (bucket, backup key, uptime checker, `status.` DNS), C1 |
| C3 | Lemon Squeezy billing, pricing page, prompts, legal and security pages: **paid launch of Pro and Team** | open | C1, C2, C0 (store approved, mail service, legal drafts reviewed) |
| C4 | Run history and alerts from your own runners; shared environment secrets | open | C3 (can start after C1; ships after C3) |
| C5 | Business tier: hosted SSO per org, audit export + anchoring, admin console | open | C3 |
| C6 | Enterprise: SCIM, self-host support offer, SOC 2 Type 1 readiness; second region and own host on demand | open | C5, owner (contracts, auditor) |
| C7 | Deferred: hosted monitors (until confidential computing), mock servers, docs publishing, comments | not planned | revisit with the triggers in section 4 and demand evidence |

### C0 Owner decisions and accounts (parallel with C1)

- Goal: everything only the owner can do, started now so C2 and C3 aren't blocked.
- Scope (owner): Q1–Q18 answered (done 2026-11-02); legal entity and payout for Lemon Squeezy;
  Lemon Squeezy store, products and variants, API key and webhook secret (test mode first), store
  approval; backup bucket at another provider and the age key; DNS `cloud.` and `status.`; uptime
  checker account; transactional mail service (Postmark or SES); compliance tool and auditor;
  lawyer review of the legal drafts.
- Scope (agent): draft ToS, privacy update, DPA, sub-processor list, SLA (99.5%, shared host
  stated) and the security page under `site/src/pages/legal/` behind a `draft` flag (not linked
  until the owner approves).
- **Done when:** the owner has ticked each C0 row in section 11; `curl -sI
  https://cloud.hitop.rws8.tech/healthz` answers from the VM; a test-mode webhook from Lemon
  Squeezy reaches the server and passes the signature check.
- Owner handoffs: section 11, rows C0.

### C1 Organizations, plans, entitlements; portal MVP; SOC 2 policies

- Goal: the data model and enforcement that every paid feature hangs on, running on the test
  server with Free limits, so nothing is sold before it's enforced; and the first SOC 2 documents.
- Scope:
  - sync-server migration 4: `orgs` (id, name, plan, region, created, billing ref),
    `org_members` (role owner/admin/member), `workspaces.org_id`, `entitlements` (plan, seats,
    limits JSON, status, period end, Lemon Squeezy customer/subscription/variant ids), `usage`
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
  - SOC 2: the policies, the vendor list (VM host, backup storage, Lemon Squeezy, mail service,
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

### C3 Lemon Squeezy billing, pricing page, prompts: paid launch of Pro and Team

- Goal: a person can buy Pro or a team can buy Team, and the limits lift at once. This is the paid
  launch; it does **not** wait for an external review (Q13).
- Scope: Lemon Squeezy checkout overlay and links in the portal (variant + seat quantity +
  `custom_data` org id); `POST /v1/billing/lemonsqueezy` with signature verification →
  entitlements; customer portal link; seat changes through the API; the 14-day Team trial; dunning
  banner and downgrade to Free limits; refunds; pricing page and home copy (section 8); Sync page
  limit notes and "Manage plan"; CLI limit line; `/legal/*` pages linked; privacy page updated;
  the security page with the "no external review yet" and "shared server" sentences;
  THREAT_MODEL "Hosted service" section; the Cloud guide; `sync-server/README.md` status line
  changes from "for testing" to the real status; mail through the transactional service.
- **Done when:** `sync-server/tests/billing.rs` replays every recorded test-mode event (bad
  signature refused before parsing, replay ignored, out-of-order handled) and the entitlements
  match; an e2e in test mode buys Team with 3 seats, adds a 4th member (quantity 4 in Lemon
  Squeezy), and a `subscription_payment_failed` then `subscription_expired` replay shows the banner
  and drops to Free with no data loss (pull still works); a refund replay ends the entitlement;
  `site` build has no "no subscription" sentence left (`grep -r "Is there a subscription? No"
  site/src` empty); the security page contains the no-review sentence (a site test checks it);
  the owner's switch to live mode and green light are recorded in section 11.
- Needs: C1, C2, C0 (store approved, live keys, mail service, legal drafts reviewed).
- Owner handoffs: set the prices in Lemon Squeezy; approve the legal pages and the security page
  wording; switch the store to live and place the live keys; give the launch green light; post the
  launch.

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
| Q3 | Billing provider | A: Paddle; B: Lemon Squeezy; C: Stripe Billing + Stripe Tax | **B** Lemon Squeezy (merchant of record) |
| Q4 | Prices | A: $6 / $10 / $20; B: lower; C: higher | **A**, a proposal until set in Lemon Squeezy |
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
- **Lemon Squeezy approval.** The store may be refused or delayed, or its terms may change (it is
  owned by Stripe). Mitigation: apply in C0, run everything in test mode meanwhile; entitlements
  are provider-neutral, so Paddle or Stripe + Stripe Tax can replace it behind the same webhook
  handler shape.
- **Lemon Squeezy B2B limits.** Invoice and PO payment are weak. Mitigation: Business pays by
  card; Enterprise is invoiced by the owner's company outside Lemon Squeezy.
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
| 2026-11-02 | C0 | Legal entity and payout bank for the Lemon Squeezy store; create the store and submit it for approval. | |
| 2026-11-02 | C0 | In Lemon Squeezy: products Pro, Team, Business with annual and monthly variants (Team and Business per seat) at your prices; an API key and a webhook to `https://sync.hitop.rws8.tech/v1/billing/lemonsqueezy` with its signing secret, test mode first. Place key and secret in a root-only env file on the VM. | |
| 2026-11-02 | C0 | Pick and create the transactional mail service (Postmark or SES); verify the sending domain; place the key. | |
| 2026-11-02 | C0 | Create an S3-compatible bucket at another provider or region for backups; generate the age key and keep the private half offline; place the bucket credentials and the public key on the VM. | |
| 2026-11-02 | C0 | DNS: `cloud.hitop.rws8.tech` and `status.hitop.rws8.tech` → the VM. | |
| 2026-11-02 | C0 | Pick an uptime checker outside the VM's provider; create the account. | |
| 2026-11-02 | C0 | Pick the SOC 2 compliance tool (Vanta, Drata, Secureframe or a spreadsheet) and the auditor; Type 1 first. | |
| 2026-11-02 | C1 | Read and approve the policies in `cloud/policies/`; confirm the first access review (`cloud/evidence/access-reviews/2026-Q4.md`: fill the "owner lists" rows, sign) and the vendor list (`cloud/policies/vendors.md`: the VM provider, DNS provider). Then quarterly access reviews. | |
| 2026-11-02 | C2 | Agree the write-freeze time for the SQLite → Postgres switch; keep the backup key offline. | |
| 2026-11-03 | C2 | For the built C2 (status in section 9): bucket with a write-only key and a 35-day lifecycle rule; an age key pair (public key to `/etc/hitop-sync/backup-recipients.txt`, private key offline); uptime checker monitors (`cloud/RUNBOOK.md`); the `status.` CNAME; an alert address; then run the first restore drill on your computer and commit its evidence. | |
| 2026-11-02 | C3 | Lawyer review of the drafts: ToS, privacy update, DPA, sub-processor list, SLA (99.5%, shared host), refund policy. Approve the security page wording (no external review yet; shared server). Confirm the App Store position (guideline 3.1.3). | |
| 2026-11-02 | C3 | Switch the Lemon Squeezy store to live, place the live keys, give the paid-launch green light, post the launch yourself (drafts in `launch/`). | |
| 2026-11-02 | later | External crypto review (Q13: not a launch gate). Scope: ROADMAP section 6 rows T23/T24/T25/1.2; `sync-server/THREAT_MODEL.md`, `cli/crates/hitop-sync-crypto`, `cli/crates/hitop-core/src/sync.rs`, `cli/crates/hitop/src/commands/sync/`, `webapp/src/sync.js`, `sync-server/src/{oidc,audit,workspaces,blobs}.rs`. Rough budget €10–25k. Tell the agent when it starts; the security page links the summary when done. | |
| 2026-11-02 | C6 | Book the SOC 2 Type 1 audit; sign the enterprise contract template; decide the US region and an own host when a customer or a trigger (section 5) asks. | |
