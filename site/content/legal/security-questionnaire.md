---
path: "/legal/security-questionnaire.html"
title: "Security questionnaire answers · HITOP Cloud"
description: "Answers to the questions security teams ask before they approve HITOP Cloud: encryption, access, hosting, backups, incidents, compliance, and what is not done yet."
---

# Security questionnaire answers

The answers your security team usually asks for, in one place. Each one says what is true
today; where something is planned or not done, it says so. Send further questions to the support
address of your plan. The code is open (AGPL server, MIT clients), so every answer can be checked
in the [repository](https://github.com/bohdaq/hitop).

**No external security review of HITOP's encryption has been done yet. HITOP Cloud runs on a
single server shared with other sites.**

## Company and service

| Question | Answer |
| --- | --- |
| What is the service? | HITOP Cloud: hosted sync for HITOP API collections between team members, with organizations, single sign-on, SCIM, audit export and run history. The clients (CLI, desktop, web app) are free and work without it. |
| Who operates it? | One operator (the owner), with documented runbooks. There are no other employees with production access. |
| Can we run it ourselves? | Yes: the same server is open source (AGPL-3.0-or-later), with signed Docker images, upgrade notes per release and a support bundle. Enterprise support and indemnity are offered by contract, without a separate licence. |
| Certifications | None yet. SOC 2 Type 1 is in preparation (policies, access reviews and a control map are in the repository); no report exists. |
| Penetration test or external review | Not yet. Planned after launch; this page and the security page link the summary when done. |

## Data and encryption

| Question | Answer |
| --- | --- |
| What customer data does the server hold? | Collections are end-to-end encrypted on the members' devices (XChaCha20-Poly1305 under a workspace key; the key is wrapped per device with HPKE, X25519). The server stores ciphertext, wrapped keys and public keys, plus metadata: account addresses, device names, who is in which workspace, sizes and times, organizations and plans, audit logs, run status rows. |
| Can the operator read our collections? | No. The server program contains no decryption code (a test fails the build if it links any) and never holds a workspace key. Clients accept keys only from devices their user pinned. |
| Encryption in transit | TLS 1.2+ at the front proxy for every API call; the service listens on localhost behind it. |
| Encryption at rest | Collections: end-to-end, above. Per-organization SSO client secrets: AES-256-GCM under a host key kept outside the database and backups. Backups: encrypted with age before they leave the server (the private key is offline). |
| Passwords | Never sent: the app derives an auth key with Argon2id and the server keeps only a slow hash of that. |
| Secrets in requests (API keys in environments) | Stay on the devices; shared environment values are opt-in per environment and sealed end-to-end for chosen devices. |
| Data location | EU only. A US region is offered later on request (not available today). |
| Data retention and deletion | Workspaces until their owner deletes them; accounts on request; audit log 365 days; sessions and invites until they expire. Details in the [data retention policy](https://github.com/bohdaq/hitop/blob/main/cloud/policies/data-retention.md). |
| Sub-processors | Listed on the [sub-processor page](/legal/subprocessors.html). |

## Access and identity

| Question | Answer |
| --- | --- |
| How do our people sign in? | Email and password, mail codes, passkeys, Google, GitHub, or your own identity provider (OpenID Connect) on Business and Enterprise, which can be made the only way for your verified domains. |
| Provisioning and deprovisioning | SCIM 2.0 on Business and Enterprise: your identity provider adds and removes members and their groups; a group gives organization and workspace roles. Removal takes effect on the server at once; the workspace owners are asked to rotate the key. |
| Roles | Organization: owner, admin, member. Workspace: owner, editor, viewer. An organization role never opens a workspace: that needs the workspace key, which only members' devices can hand on. |
| MFA | Passkeys, or your identity provider's MFA through single sign-on. |
| Operator access | SSH keys to one VM, root only on the VM; reviewed quarterly ([access reviews](https://github.com/bohdaq/hitop/tree/main/cloud/evidence/access-reviews)). |

## Logging and monitoring

| Question | Answer |
| --- | --- |
| Audit log | Per workspace, account and organization: sign-ins, members, invites, roles, key rotations, SSO, SCIM and admin changes; hash-chained; IPs truncated. Business and Enterprise get a signed export and daily signed anchors mailed to the owners, so a rewritten log shows. |
| Monitoring | Disk, database size and backup-age alerts every 15 minutes on the server; an off-host uptime check and a public status page are being set up before the paid launch. |
| Vulnerability management | Every commit runs tests, clippy, dependency licence and advisory checks (cargo deny, npm audit) and fuzzing of the parsers; release images are signed (cosign keyless) with an SBOM. |

## Availability and recovery

| Question | Answer |
| --- | --- |
| Availability target | 99.5% a month on Business ([SLA](/legal/sla.html)); no higher figure on a shared server. Enterprise can get a dedicated or self-hosted setup. |
| Backups | Hourly, encrypted, to storage at another provider, 35 days, tested by restore drills. (Built; switched on when the backup storage is set up before the paid launch.) |
| Recovery objectives | Data loss at most 1 hour, restore within 4 hours. Collections also live on the members' devices and in your git repositories. |

## Incidents

| Question | Answer |
| --- | --- |
| Incident process | Written [incident response policy](https://github.com/bohdaq/hitop/blob/main/cloud/policies/incident-response.md): detect, record, contain, tell customers, recover, learn. |
| Breach notification | Affected organization owners by mail within 72 hours of knowing, with what is known. |
| Reporting a vulnerability | See [SECURITY.md](https://github.com/bohdaq/hitop/blob/main/SECURITY.md). |

## Legal

| Question | Answer |
| --- | --- |
| Terms and DPA | [Terms](/legal/terms.html), [DPA](/legal/dpa.html) (EU standard contractual clauses where data leaves the EU). |
| Licence | Clients MIT, server AGPL-3.0-or-later. Enterprise: support, upgrade windows and indemnity by contract ([template](/legal/enterprise-agreement.html)); no dual licence. |
