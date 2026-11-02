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

## From C2 (planned, spec/CLOUD_SERVICE.md section 5)

- Postgres on the VM; nightly `pg_dump` plus WAL archiving to an S3-compatible bucket at **another
  provider or region**, encrypted with an age key whose private half the owner keeps offline;
  35-day retention.
- **Restore drill** monthly with `deploy/restore-drill.sh`: last night's backup into a scratch
  database, row counts compared, `audit-verify` passing; the output kept in
  `../evidence/restore-drills/<date>.txt`.
- Recovery objectives: data loss at most 24 hours (nightly dump; less with WAL), restore within 4
  hours on the same or a new VM.

## Customer-side recovery

A lost device doesn't lose a workspace while another member's device has it; a recovery key (C2)
covers a person with one device. HITOP can't recover a workspace key: it never has one.
