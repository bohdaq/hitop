#!/usr/bin/env bash
# Deletes the sync server's database copies on the host once they are 35 days old, as the privacy
# page and cloud/policies/data-retention.md promise: the SQLite copies install.sh makes
# (/var/lib/private/hitop-sync/sync.db.bak-*) and the pre-deploy dumps (/root/pre-deploy-*,
# RUNBOOK "Deploy"). Run daily by hitop-db-copies-expire.timer. Prints each path it deletes.
# DAYS (35) and the two locations can be overridden for tests.
set -euo pipefail
DAYS="${DAYS:-35}"
SYNC_DIR="${SYNC_DIR:-/var/lib/private/hitop-sync}"
DUMP_DIR="${DUMP_DIR:-/root}"
min=$((DAYS * 1440))
gone=0
if [ -d "$SYNC_DIR" ]; then
  while IFS= read -r -d '' f; do echo "deleted $f"; rm -f -- "$f"; gone=$((gone + 1)); done \
    < <(find "$SYNC_DIR" -maxdepth 1 -type f -name 'sync.db.bak-*' -mmin +"$min" -print0)
fi
if [ -d "$DUMP_DIR" ]; then
  # A dump directory counts by its newest file, so one written into later stays.
  while IFS= read -r -d '' d; do
    if [ -z "$(find "$d" -mindepth 1 -type f -mmin -"$min" -print -quit)" ]; then echo "deleted $d"; rm -rf -- "$d"; gone=$((gone + 1)); fi
  done < <(find "$DUMP_DIR" -mindepth 1 -maxdepth 1 -type d -name 'pre-deploy-*' -print0)
fi
echo "database copies older than $DAYS days deleted: $gone"
