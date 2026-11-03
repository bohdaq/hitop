# Data retention and deletion policy

Status: draft for the owner's approval (C1, 2026-11-02). Review yearly.

| Data | Where | Kept | Deleted by |
| --- | --- | --- | --- |
| Workspace ciphertext (blobs, wrapped keys) | sync database | while the workspace exists | its owner deleting it (`DELETE /v1/workspaces/:ws`); everything goes in the same transaction |
| Accounts (address, device public keys, passkeys, SSO identity links) | sync database | while the account is used | on request (mail the support address): the account, its sessions, keys, tokens and memberships |
| Organizations, members, entitlements, usage counters | sync database | while the org exists; daily usage counters 400 days | org owners delete an empty org; counters purged hourly past 400 days |
| Sessions, sign-in links, invites, access tokens | sync database (hashes only) | until they expire (links 15 min, invites 7 days, sessions 30 days, tokens at most 365 days) | expiry and the cleanup on the next sign-in or token create |
| Audit log | sync database | 365 days (`HITOP_SYNC_AUDIT_RETENTION_DAYS`) | hourly purge; IPs kept truncated (/24, /48) |
| Request logs of the front proxy | VM | as rws keeps them (no bodies, no tokens) | log rotation |
| Backups (C2) | off-site bucket, encrypted | 35 days | bucket lifecycle rule |
| Billing records (C3) | Stripe | as the law requires of the merchant of record | Stripe |

**Deletion on request** (GDPR article 17): within 30 days, including the time for backups that
still hold the data to expire (35 days at most), recorded in the ops log without the person's data.
**Export**: customers already hold their collections as files (`hitop sync pull`); account data on
request.

A downgraded or lapsed plan never deletes anything: creates are refused, reads keep working
(`sync-server/src/limits.rs`).
