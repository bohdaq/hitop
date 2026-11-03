# Backup and recovery policy

Status: draft for the owner's approval (C1, 2026-11-02). Review yearly. What exists today and what
C2 adds are marked.

## Today (C1)

- `sync-server/deploy/install.sh` makes a consistent copy of the SQLite database
  (`sqlite3 .backup`) next to it before every deploy, and keeps the previous binary and env file
  (`.bak-<timestamp>`).
- The database holds ciphertext and metadata only; customers also keep their collections as files
  on their own devices and in their git repositories, so the server is not the only copy of a
  collection.

## From C2 (built 2026-11-03, applied when the owner's bucket and age key exist)

- Postgres on the VM in its own socket-only cluster (`sync-server/deploy/postgres-setup.sh`);
  the switch from SQLite is `cutover-postgres.sh` (write freeze, backup, `migrate-db`, row counts
  and content hashes compared, `audit-verify`), undone by `rollback-postgres.sh`.
- **Hourly** encrypted off-site backups (`sync-server/deploy/backup.sh`, `hitop-sync-backup.timer`):
  `hitop-sync-server dump` (a SQLite snapshot, or `pg_dump` in the same snapshot as the manifest of
  row counts and hashes), encrypted with `age` to the owner's public key (the private half stays
  offline), uploaded to an S3-compatible bucket at **another provider or region**. 35-day retention
  by a lifecycle rule on the bucket; the VM's key has no delete right, so a compromised VM can't
  erase backups. Hourly dumps replace WAL archiving (deviation, spec C2 status).
- **Restore drill** monthly with `sync-server/deploy/restore-drill.sh`, run where the private key
  is: the newest backup into a scratch database, row counts and hashes equal to its manifest,
  `audit-verify` passing; the evidence file it writes is committed under
  `../evidence/restore-drills/`.
- Alerts (`cloud/ops/disk-alert.sh`, every 15 minutes): a disk over 80%, the Postgres data over
  its size limit, the last good backup older than 3 hours.
- Recovery objectives: data loss at most 1 hour (hourly dumps), restore within 4 hours on the same
  or a new VM (`../RUNBOOK.md`, "Restore from a backup").

## Customer-side recovery

A lost device doesn't lose a workspace while another member's device has it; a recovery phrase (C2,
`hitop sync recovery`, the Sync page) covers a person with one device. HITOP can't recover a workspace key: it never has one.
