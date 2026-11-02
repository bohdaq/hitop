# HITOP Cloud operations log

Every deploy, configuration change on the VM, access grant or removal, restore drill and incident,
newest last (change management and incident response policies, `policies/`). One entry each:
date (UTC), who, what, the commit, the checks run after it.

## 2026-11-02 C1 built, not deployed yet

- Who: agent. What: HITOP Cloud C1 on `main` (migration 4: organizations, plans, entitlements,
  usage, access tokens; `/v1/orgs`; plan limits behind `HITOP_SYNC_LIMITS=plans`; the portal in
  `cloud/portal/`; these policies). Not deployed: the deploy of the 1.2 sync server comes first
  (spec/CLOUD_SERVICE.md C1 "Needs").
- Deploy steps (for the entry below): spec/CLOUD_SERVICE.md section 9, C1 status.

<!-- Next entry: the C1 deploy. Record: commit, backup file names (sync.db.bak-<ts>), the row
counts before and after (users, workspaces, members; orgs with personal_of <> '' = users with
runner_of = ''), `hitop-sync-server over-limits` output (0 over a limit), curl of /healthz on sync
and on hitop.rws8.tech/cloud/healthz. -->
