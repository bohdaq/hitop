#!/usr/bin/env bash
# HITOP Cloud C2 off-site backup of the sync server's database: run by hitop-sync-backup.service
# (timer: hourly) as the service's own dynamic user, or by hand as root.
#
#   1. hitop-sync-server dump: the data (SQLite snapshot, or pg_dump in one snapshot) and
#      manifest.json with the row counts and content hashes of exactly that data.
#   2. tar | age -R <recipients>: encrypted to the owner's age public key(s) before it leaves the
#      host. The private key stays offline with the owner; nothing on the VM can decrypt a backup.
#   3. Upload to BACKUP_TARGET (another provider or region; s3.sh). Name:
#      hitop-sync-<UTC time>.tar.age, plus a sha256 line in the status file.
#   4. Optional pruning (BACKUP_KEEP_DAYS > 0, needs delete rights). The recommended setup gives the
#      VM's key no delete right and lets a bucket lifecycle rule expire objects after 35 days, so a
#      compromised VM can't erase its backups.
#
# Settings: /etc/hitop-sync/backup.env (root only; see backup.env.example) and the database from
# /etc/hitop-sync/env (HITOP_SYNC_DATABASE). Exit 0 and a status file on success; non-zero and a
# journal line otherwise (the disk/backup alert, cloud/ops/disk-alert.sh, mails when the last good
# backup is too old).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=s3.sh
. "$here/s3.sh"

BIN="${BIN:-/usr/local/bin/hitop-sync-server}"
AGE="${AGE:-age}"
: "${HITOP_SYNC_DATABASE:?HITOP_SYNC_DATABASE is not set (from /etc/hitop-sync/env)}"
# Under systemd the recipients come in as a credential (LoadCredential=, the unit), since
# /etc/hitop-sync is root only.
if [ -n "${CREDENTIALS_DIRECTORY:-}" ] && [ -f "$CREDENTIALS_DIRECTORY/recipients" ]; then AGE_RECIPIENTS_FILE="$CREDENTIALS_DIRECTORY/recipients"; fi
: "${AGE_RECIPIENTS_FILE:=/etc/hitop-sync/backup-recipients.txt}"
STATUS_FILE="${STATUS_FILE:-/var/lib/hitop-sync/backup-status.json}"
BACKUP_KEEP_DAYS="${BACKUP_KEEP_DAYS:-0}"
target_kind >/dev/null

[ -s "$AGE_RECIPIENTS_FILE" ] || { echo "backup: no age recipients in $AGE_RECIPIENTS_FILE: refusing to make an unencrypted backup" >&2; exit 2; }
grep -q '^age1' "$AGE_RECIPIENTS_FILE" || { echo "backup: $AGE_RECIPIENTS_FILE has no age1... public key" >&2; exit 2; }

started="$(date -u +%s)"
name="hitop-sync-$(date -u +%Y%m%dT%H%M%SZ).tar.age"
work="$(mktemp -d "${TMPDIR:-/var/tmp}/hitop-backup.XXXXXX")"
trap 'rm -rf "$work"' EXIT
chmod 0700 "$work"

"$BIN" dump "$work/b" > /dev/null
rows="$(python3 -c 'import json,sys; m=json.load(open(sys.argv[1])); print(sum(t["rows"] for t in m["stats"]["tables"]))' "$work/b/manifest.json")"
tar -C "$work/b" -cf - . | "$AGE" -R "$AGE_RECIPIENTS_FILE" -o "$work/$name"
rm -rf "$work/b"
bytes="$(stat -c %s "$work/$name")"
sum="$(sha256sum "$work/$name" | cut -d' ' -f1)"
put_object "$work/$name" "$name"

pruned=0
if [ "$BACKUP_KEEP_DAYS" -gt 0 ]; then
  cutoff="$(date -u -d "-$BACKUP_KEEP_DAYS days" +%Y%m%dT%H%M%SZ)"
  for o in $(list_objects | grep '^hitop-sync-.*\.tar\.age$'); do
    stamp="${o#hitop-sync-}"; stamp="${stamp%.tar.age}"
    if [[ "$stamp" < "$cutoff" ]]; then delete_object "$o"; pruned=$((pruned + 1)); fi
  done
fi

finished="$(date -u +%s)"
status="$(python3 -c 'import json,sys; a=sys.argv[1:]; print(json.dumps({"ok": True, "object": a[0], "bytes": int(a[1]), "sha256": a[2], "rows": int(a[3]), "at": int(a[4]), "seconds": int(a[4]) - int(a[5]), "pruned": int(a[6])}))' "$name" "$bytes" "$sum" "$rows" "$finished" "$started" "$pruned")"
mkdir -p "$(dirname "$STATUS_FILE")"
echo "$status" > "$STATUS_FILE.tmp" && mv "$STATUS_FILE.tmp" "$STATUS_FILE"
echo "backup: $status"
