# Access control policy

Status: draft for the owner's approval (C1, 2026-11-02). Review yearly.

## Systems and who may access them

The list of systems is the one the access review checks (template:
`../evidence/access-reviews/`): the VM, the sync database, the backup bucket (from C2),
Stripe (from C3), the transactional mail service, DNS, GitHub (repository, Actions secrets,
releases), crates.io, ghcr.io, App Store Connect.

- The **owner** holds every account himself, with a unique password from a password manager and
  two-factor authentication wherever the service offers it.
- **Agents** get access only through the owner: on the VM through the tunnel described in
  `ROADMAP.md` section 3 (a pod the agent starts and deletes, `ubuntu` user over SSH with the
  owner's key), and to GitHub through the owner's `gh` session on his workstation. Agents have no
  accounts of their own at Stripe, the mail service, DNS, crates.io tokens beyond the CI
  secret, or App Store Connect.
- **Customers** reach only the public API and the portal, with their own sessions.

## Granting and removing access

- Access is granted by the owner only, recorded in `../ops-log.md` (who, what, why, date).
- Access is removed the same day it is no longer needed (a contractor leaves, a key is lost) and
  recorded the same way. A lost SSH key or token is revoked first, then investigated.
- Shared credentials are not used; service credentials (SMTP, API keys) live in root-only env
  files and are rotated when someone with access to them leaves or on suspicion.

## Customer accounts

- Sign-in by mailed one-time code, passkey or SSO; sessions expire (30 days; runner credentials 1
  year, revocable by removing the runner). Access tokens have an expiry (at most 365 days),
  scopes and are revocable in the portal.
- Workspace roles (owner, editor, viewer) and org roles (owner, admin, member) are enforced by the
  server (`tests/roles.rs`, `tests/orgs.rs`). An org role never opens a workspace: that needs the
  workspace key, which only members' clients hold.

## Reviews

Quarterly, the owner (with an agent preparing the list) reviews every account on the systems above
and records the result in `../evidence/access-reviews/<year>-Q<n>.md`: each account, its owner, why
it is needed, kept or removed. The first review is `2026-Q4.md`.
