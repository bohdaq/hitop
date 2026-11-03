#!/usr/bin/env bash
# HITOP Cloud C2 host checks, every 15 minutes (hitop-disk-alert.timer): a full disk is the most
# likely outage on a shared host, and a backup that silently stopped is the most likely loss.
#
#   - every file system in DISK_PATHS over DISK_ALERT_PERCENT (80) full;
#   - the hitop Postgres data directory over PG_MAX_BYTES (default 10 GiB), when it exists;
#   - the last good backup (backup.sh's status file) older than BACKUP_MAX_AGE_HOURS (3), once
#     backups are set up (/etc/hitop-sync/backup.env exists).
#
# A problem goes to the journal (priority warning, tag hitop-alert) and, when ALERT_TO is set, as a
# mail through the host's own submission port (SMTP_URL-like settings below), at most once every
# ALERT_REPEAT_HOURS (6) per problem. Exit 1 while anything is wrong, 0 otherwise.
#
# Settings: /etc/hitop-sync/alert.env (optional, root only): ALERT_TO, ALERT_FROM, ALERT_SMTP
# (curl URL, default smtp://127.0.0.1:587), ALERT_SMTP_USER, ALERT_SMTP_PASSWORD; and the
# thresholds above. ALERT_CMD replaces the mail (tests).
set -uo pipefail

DISK_PATHS="${DISK_PATHS:-/ /var/lib/postgresql /var/lib/private/hitop-sync}"
DISK_ALERT_PERCENT="${DISK_ALERT_PERCENT:-80}"
PG_DATA="${PG_DATA:-$(ls -d /var/lib/postgresql/*/hitop 2>/dev/null | tail -n 1)}"
PG_MAX_BYTES="${PG_MAX_BYTES:-10737418240}"
BACKUP_STATUS="${BACKUP_STATUS:-/var/lib/private/hitop-sync/backup-status.json}"
BACKUP_CONFIG="${BACKUP_CONFIG:-/etc/hitop-sync/backup.env}"
BACKUP_MAX_AGE_HOURS="${BACKUP_MAX_AGE_HOURS:-3}"
ALERT_REPEAT_HOURS="${ALERT_REPEAT_HOURS:-6}"
STATE_DIR="${STATE_DIR:-/var/lib/hitop-alert}"
ALERT_SMTP="${ALERT_SMTP:-smtp://127.0.0.1:587}"
ALERT_FROM="${ALERT_FROM:-HITOP alerts <noreply@rws8.tech>}"
now="$(date -u +%s)"
host="$(hostname -s 2>/dev/null || echo host)"
problems=()

for p in $DISK_PATHS; do
  [ -e "$p" ] || continue
  use="$(df -P "$p" | awk 'NR == 2 { gsub("%", "", $5); print $5 }')"
  if [ -n "$use" ] && [ "$use" -ge "$DISK_ALERT_PERCENT" ]; then
    problems+=("disk:$(df -P "$p" | awk 'NR == 2 { print $6 }')|$(df -P "$p" | awk 'NR == 2 { print $6 }') is ${use}% full (alert at ${DISK_ALERT_PERCENT}%): see cloud/RUNBOOK.md, Disk full")
  fi
done

if [ -n "$PG_DATA" ] && [ -d "$PG_DATA" ]; then
  size="$(du -sb "$PG_DATA" 2>/dev/null | cut -f1)"
  if [ -n "$size" ] && [ "$size" -ge "$PG_MAX_BYTES" ]; then
    problems+=("pgsize|the hitop Postgres data directory $PG_DATA holds $((size / 1048576)) MiB (alert at $((PG_MAX_BYTES / 1048576)) MiB)")
  fi
fi

if [ -f "$BACKUP_CONFIG" ]; then
  last="$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(d["at"] if d.get("ok") else 0)' "$BACKUP_STATUS" 2>/dev/null || echo 0)"
  age_h=$(( (now - last) / 3600 ))
  if [ "$age_h" -ge "$BACKUP_MAX_AGE_HOURS" ]; then
    problems+=("backup|the last good off-site backup is $([ "$last" = 0 ] && echo "missing" || echo "${age_h} h old") (alert at ${BACKUP_MAX_AGE_HOURS} h): journalctl -u hitop-sync-backup")
  fi
fi

[ -f /etc/hitop-sync/alert.env ] && { set -a; . /etc/hitop-sync/alert.env; set +a; }
mkdir -p "$STATE_DIR"
send() {
  local subject="$1" body="$2"
  if [ -n "${ALERT_CMD:-}" ]; then
    printf '%s\n\n%s\n' "$subject" "$body" | $ALERT_CMD
    return
  fi
  [ -n "${ALERT_TO:-}" ] || return 0
  local msg; msg="$(mktemp)"
  printf 'From: %s\r\nTo: %s\r\nSubject: %s\r\nDate: %s\r\n\r\n%s\r\n' "$ALERT_FROM" "$ALERT_TO" "$subject" "$(date -R)" "$body" > "$msg"
  local from_addr; from_addr="$(echo "$ALERT_FROM" | sed -E 's/.*<([^>]*)>.*/\1/')"
  printf 'user = "%s:%s"\n' "${ALERT_SMTP_USER:-}" "${ALERT_SMTP_PASSWORD:-}" | curl -sS --config - --ssl-reqd --url "$ALERT_SMTP" --mail-from "$from_addr" --mail-rcpt "$ALERT_TO" -T "$msg" \
    || logger -p user.err -t hitop-alert "could not mail the alert to $ALERT_TO"
  rm -f "$msg"
}

for entry in "${problems[@]}"; do
  key="${entry%%|*}"; text="${entry#*|}"
  logger -p user.warning -t hitop-alert "$text" 2>/dev/null || true
  echo "ALERT $text"
  stamp="$STATE_DIR/$(echo "$key" | tr -c 'a-zA-Z0-9' '_')"
  if [ ! -f "$stamp" ] || [ $(( now - $(cat "$stamp") )) -ge $(( ALERT_REPEAT_HOURS * 3600 )) ]; then
    send "[HITOP $host] $text" "$text

Host: $host, $(date -u)
Runbook: https://github.com/bohdaq/hitop/blob/main/cloud/RUNBOOK.md"
    echo "$now" > "$stamp"
  fi
done
# A problem that went away may alert again at once next time.
for stamp in "$STATE_DIR"/*; do
  [ -f "$stamp" ] || continue
  name="$(basename "$stamp")"
  still=0
  for entry in "${problems[@]}"; do [ "$(echo "${entry%%|*}" | tr -c 'a-zA-Z0-9' '_')" = "$name" ] && still=1; done
  [ "$still" = 1 ] || rm -f "$stamp"
done

if [ "${#problems[@]}" -gt 0 ]; then exit 1; fi
echo "ok: disks under ${DISK_ALERT_PERCENT}%$([ -n "$PG_DATA" ] && echo ", Postgres data under the limit")$([ -f "$BACKUP_CONFIG" ] && echo ", last backup within ${BACKUP_MAX_AGE_HOURS} h")"
