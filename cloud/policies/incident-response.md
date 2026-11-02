# Incident response policy

Status: draft for the owner's approval (C1, 2026-11-02). Review yearly. The step-by-step runbook
is `cloud/RUNBOOK.md` (C2).

## What is an incident

Anything that harms or may harm the confidentiality, integrity or availability of HITOP Cloud:
the service down or failing, data lost or corrupted, a leaked credential or token, unauthorized
access to the VM or an account, a vulnerability reported in the code, a neighbour site on the
shared VM misbehaving.

## Severity

| Sev | Meaning | First response |
| --- | --- | --- |
| 1 | Service down, data at risk, or a credential known leaked | at once, within 4 business hours for Enterprise |
| 2 | Degraded (errors, slow), a vulnerability without known exploitation | same business day |
| 3 | Minor, no customer impact | next working day |

## Steps

1. **Detect**: the off-host uptime check (C2), CI, customer mail (security address in
   `SECURITY.md`), the audit log, the operator noticing.
2. **Record**: open an entry in `../ops-log.md` with time, what is seen, severity.
3. **Contain**: revoke leaked tokens or sessions, stop the affected service, block an abusive
   account or IP at the front rws, roll back a deploy (`.bak-<ts>` binaries and env files).
4. **Tell customers**: the status page (C2) for availability; for a security incident affecting
   customer data, mail the affected org owners within 72 hours of knowing, with what is known
   (GDPR article 33/34 duties are the owner's as controller of account data).
5. **Recover**: restore from backup if needed (backup policy), verify with `hitop-sync-server
   audit-verify` and `over-limits`.
6. **Learn**: a short post-incident note in the ops log within a week: cause, fix, what changes
   (a test, a policy, a check).

Because customer collections are end-to-end encrypted, a server compromise exposes metadata (the
threat model lists it) but not collection contents; clients refuse keys the server forges (1.2
signed wraps). Incident notes say this only when it is true for that incident.
