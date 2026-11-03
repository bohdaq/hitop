# SOC 2 Type 1 readiness: controls → evidence (HITOP Cloud C6)

Status: prepared by the agent on 2026-11-03 for the owner (decision Q17: SOC 2 groundwork now,
Type 1 first). It maps each control an auditor will test to the evidence that exists in this
repository since C1, and lists the gaps. Every gap row needs an owner decision or an owner action;
nothing here is evidence by itself until the owner loads it into the compliance tool he picks
(Vanta, Drata, Secureframe or a spreadsheet) and the policies are approved.

Scope proposal (owner decides, row S1): the **Security** criteria (CC1–CC9, required), plus
**Availability** (A1) and **Confidentiality** (C1), for the HITOP Cloud sync service and portal
on the one VM, as of a date the owner sets. Out of scope: the free clients (they run on customers'
devices), the website, the App Store apps, the other sites on the shared VM (carve-out: the VM
provider and the shared host's other tenants), and the on-demand US region (Q18, not built).

State: **in place** = done and evidenced; **built** = exists in code or text, not yet operating
(waits on an owner item); **gap** = missing.

## Control environment, communication, risk (CC1–CC3)

| Control | Evidence (path) | State | Gap / owner decision |
| --- | --- | --- | --- |
| CC1.1 Integrity and ethics | `cloud/policies/acceptable-use.md`, `CODE_OF_CONDUCT.md` | built | Owner approves the policies (spec section 11, C1 row). |
| CC1.2 Oversight | single operator; `cloud/policies/information-security.md` (roles) | gap | No board or independent oversight. Decision: state the single-operator structure in the system description; a yearly self-review note signed by the owner as the compensating control. |
| CC1.3 Structure, authority | `information-security.md` (owner = operator; agents act under `ROADMAP.md` section 3 rules) | built | Approve; describe the agents' role (tools acting for the operator, no own access). |
| CC1.4 Competence | none | gap | Decision: keep a short yearly security training record (what was read or attended) in `cloud/evidence/training/`. |
| CC1.5 Accountability | commit sign-off (DCO) on every change, `ops-log.md` | in place | |
| CC2.1 Information for control | `cloud/ops-log.md` (every deploy and operator action), audit log (hash chain), `db-stats`, `support-bundle` | in place | |
| CC2.2 Internal communication | `cloud/policies/`, `cloud/RUNBOOK.md`, `spec/CLOUD_SERVICE.md` | in place | |
| CC2.3 External communication | `SECURITY.md`; drafts behind `site/src/legal-status.js` DRAFT: terms, privacy, DPA, SLA, support, sub-processors, security page, security questionnaire, enterprise agreement template | built | Owner (and lawyer) approve the drafts (DRAFT = false); `status.` DNS for the status page (C0). |
| CC3.1–CC3.4 Risk assessment, fraud, change | `spec/CLOUD_SERVICE.md` section 10 (risks), `sync-server/THREAT_MODEL.md` | built | Gap: no dated, signed yearly risk assessment. Decision: a one-page risk register in `cloud/evidence/risk/2026.md` reviewed yearly (the agent can draft it from section 10). |

## Monitoring and control activities (CC4–CC5)

| Control | Evidence (path) | State | Gap / owner decision |
| --- | --- | --- | --- |
| CC4.1 Ongoing evaluation | CI on every commit (`.github/workflows/ci.yml`: tests, clippy, e2e, compat with released clients), `licenses.yml` (licences and advisories), `fuzz.yml` | in place | |
| CC4.2 Deficiencies communicated | `ops-log.md` incident entries; GitHub issues | built | Decision: where deficiencies are tracked (issues with a label, or the compliance tool). |
| CC5.1–CC5.3 Control activities and policies | `cloud/policies/*.md` (8 policies, mapped to criteria in `cloud/policies/README.md`) | built | Approve; yearly review date in each file. |

## Logical and physical access (CC6)

| Control | Evidence (path) | State | Gap / owner decision |
| --- | --- | --- | --- |
| CC6.1 Access security (infrastructure) | `cloud/policies/access-control.md`; SSH keys only; Postgres on a local socket (`sync-server/deploy/postgres-setup.sh`); service on 127.0.0.1 behind the front proxy; systemd sandboxing (`cloud/ops/systemd/`) | built (applied in C2 to the test server) | Owner confirms 2FA on GitHub, crates.io, the registrar/DNS, Stripe, the VM provider and mail; record it in the access review. |
| CC6.1 Access security (customers) | passwords (Argon2id auth key), passkeys, per-org SSO with verified domains and "SSO required" (C5), access tokens with scopes and expiry, rate limits (`ratelimit.rs`) | in place in code | |
| CC6.2 Provisioning | access review `cloud/evidence/access-reviews/2026-Q4.md`; customers: org roles, invites, SCIM (C6, `sync-server/src/scim.rs`) | built | Owner fills the "owner lists" rows of 2026-Q4 and signs it. |
| CC6.3 Removal, least privilege | quarterly review (template `cloud/evidence/access-reviews/TEMPLATE.md`); the tunnel pod deleted after each agent session; customers: SCIM deactivation, admin device removal, rotation requests (C5/C6) | built | Quarterly reviews actually run (first one unsigned). |
| CC6.4 Physical access | VM provider's data centre | gap | Carve-out: collect the VM provider's SOC 2 or ISO 27001 report (vendor list). Owner names the provider in `cloud/policies/vendors.md`. |
| CC6.5 Disposal | `cloud/policies/data-retention.md` (deletion on request, purges) | built | Decision: how a retired disk/VM is wiped (provider's process; record it). |
| CC6.6 Boundary protection | front rws with TLS, one route per service (`sync-server/deploy/rws-routes.toml`), body limits, rate limits, CORS allowlist | in place | |
| CC6.7 Transmission and storage | E2E encryption (THREAT_MODEL), TLS, host-key sealing of SSO secrets (C5), age-encrypted backups (C2) | built | Backups wait for the owner's bucket and age key (C2 row). |
| CC6.8 Unauthorized or malicious software | `cargo deny` advisories, `npm audit`, pinned dependencies and lock files checked `--locked`; signed sync server image with SBOM (C6, `.github/workflows/release-sync-server.yml`) | built | Decision: pin third-party GitHub Actions by commit SHA (today by major tag). |

## System operations, change, risk mitigation (CC7–CC9)

| Control | Evidence (path) | State | Gap / owner decision |
| --- | --- | --- | --- |
| CC7.1 Vulnerability detection | advisories in CI, fuzzing, `THREAT_MODEL.md` open items | built | Gap: no external crypto review or pen test (Q13: later). Decision: when, and whether the Type 1 report says it is planned. |
| CC7.2 Anomaly monitoring | audit log hash chain + `audit-verify`, signed anchors (C5), disk/backup-age alerts (`cloud/ops/disk-alert.sh`), `/healthz` | built | Off-host uptime checker and an alert address (C0, C2 rows). |
| CC7.3–CC7.5 Incident response and recovery | `cloud/policies/incident-response.md`, RUNBOOK "Incident", ops-log template | built | Decision: one tabletop exercise before the audit date, recorded in the ops log. |
| CC8.1 Change management | `cloud/policies/change-management.md`; DCO check (`dco.yml`); CI green before deploy; deploys recorded in `ops-log.md`; upgrade notes (`sync-server/UPGRADE_NOTES.md`); release tags; signed images | in place | Gap: changes aren't reviewed by a second person. Decision: accept with compensating controls (CI gates, recorded deploys, post-deploy checks), or a second reviewer for releases. |
| CC9.1 Business disruption | `backup-recovery.md` (RPO 1 h, RTO 4 h), RUNBOOK "Move to another host", "Restore from a backup" | built | Decision: insurance (cyber/professional liability), needed for Enterprise indemnity anyway. |
| CC9.2 Vendors | `cloud/policies/vendor-management.md`, `vendors.md`, site sub-processor page | built | Owner fills the VM provider and DNS rows; collects Stripe's and the VM provider's reports; picks the mail service and backup storage (C0). |

## Availability (A1) and Confidentiality (C1)

| Control | Evidence (path) | State | Gap / owner decision |
| --- | --- | --- | --- |
| A1.1 Capacity | plan limits (`sync-server/src/limits.rs`), systemd CPU/memory limits (`cloud/ops/systemd/`), disk alerts | built | |
| A1.2 Backups and environment | `sync-server/deploy/backup.sh`, `hitop-sync-backup.timer`, 35-day bucket lifecycle | built | Owner: bucket with a write-only key, age key pair (C2 row). |
| A1.3 Recovery testing | `sync-server/deploy/restore-drill.sh` (writes evidence for `cloud/evidence/restore-drills/`) | gap | First restore drill run by the owner and committed; then monthly. |
| C1.1 Identify and protect confidential information | THREAT_MODEL "What the server stores"; E2E; truncated IPs; support bundle never holds customer data (`sync-server/tests/support_bundle.rs`) | in place | |
| C1.2 Disposal | `data-retention.md`, audit and run retention purges, workspace deletion in one transaction | built | Deletion-on-request procedure tested once and recorded. |

## Owner decisions and actions (summary)

| # | Decision or action | Blocks |
| --- | --- | --- |
| S1 | Scope (Security + Availability + Confidentiality proposed) and the "as of" date | the audit booking |
| S2 | Compliance tool and auditor; book the Type 1 (C0, C6 rows) | loading this evidence |
| S3 | Approve the policies; sign the 2026-Q4 access review | CC1, CC5, CC6.2 |
| S4 | Backups live (bucket, age key) and the first restore drill committed | A1.2, A1.3, CC6.7 |
| S5 | Uptime checker, alert address, `status.` DNS | CC7.2, CC2.3 |
| S6 | 2FA confirmed on every operator account | CC6.1 |
| S7 | Single-operator oversight and change review: accept with compensating controls, or add a reviewer | CC1.2, CC8.1 |
| S8 | Training record, risk register, tabletop exercise (the agent can draft the files) | CC1.4, CC3, CC7.4 |
| S9 | Vendor reports (VM provider, Stripe, mail, backup storage) and names in `vendors.md` | CC6.4, CC9.2 |
| S10 | External review / pen test timing (Q13) and how the report mentions it | CC7.1 |
| S11 | Pin GitHub Actions by SHA (the agent can do it on a go) | CC6.8 |
| S12 | Legal drafts approved (DRAFT = false), incl. the security questionnaire and enterprise template | CC2.3 |

Not in scope until a customer asks (Q18): a US region (second host, org pinned at creation; the
`orgs.region` column exists and is `eu` everywhere), and a move to an own host (spec section 5
trigger).
