#!/usr/bin/env bash
# HITOP Cloud C2: applies the host hardening to the shared VM, one reversible part at a time. Run
# as root ON the host from an unpacked bundle of this repo's files (cloud/RUNBOOK.md, "Apply C2"):
#
#   tar czf hitop-ops.tgz cloud/ops sync-server/deploy deploy/hitop-server.service
#
#   install.sh hardening     the sandboxed hitop-sync and hitop-server units + resource-limit
#                            drop-ins; restarts each and checks it answers, else puts it back
#   install.sh pg-limits     resource limits for the hitop Postgres cluster (after postgres-setup.sh)
#   install.sh disk-alert    disk / Postgres size / backup age checks every 15 minutes
#   install.sh backup        hourly encrypted off-site backups (needs the owner's bucket and age key
#                            in /etc/hitop-sync/backup.env and backup-recipients.txt first)
#   install.sh status        what is installed, the limits systemd applies, the exposure scores
#   install.sh remove <part> undoes one part (units and drop-ins back as they were)
#
# Every file replaced is kept as <file>.bak-hitop-<UTC time> first. Nothing outside these units,
# /usr/local/lib/hitop-sync and their drop-ins changes.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
SYSTEMD="${SYSTEMD_DIR:-/etc/systemd/system}"
LIB=/usr/local/lib/hitop-sync
ts="$(date -u +%Y%m%dT%H%M%SZ)"
[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 2; }
say() { echo "install: $*"; }
pg_unit() { echo "postgresql@$(ls /usr/lib/postgresql 2>/dev/null | sort -n | tail -n 1)-hitop.service"; }

backup_file() { if [ -e "$1" ]; then cp -a "$1" "$1.bak-hitop-$ts"; say "kept $1.bak-hitop-$ts"; fi; }
put() { backup_file "$2"; install -D -m "${3:-0644}" "$1" "$2"; }
answers() {
  for _ in $(seq 20); do curl -fsS -o /dev/null "$1" && return 0; sleep 1; done
  return 1
}

# Replace a unit and its drop-in, restart, check; put both back if it doesn't answer.
harden() {
  local unit="$1" file="$2" dropin="$3" probe="$4"
  local target="$SYSTEMD/$unit" dir="$SYSTEMD/$unit.d"
  local had_unit=0; [ -e "$target" ] && had_unit=1
  put "$file" "$target"
  put "$dropin" "$dir/10-hitop-limits.conf"
  systemctl daemon-reload
  systemctl restart "${unit%.service}"
  if systemctl is-active -q "${unit%.service}" && answers "$probe"; then
    say "$unit: active and answering $probe"
  else
    say "$unit: NOT answering after the change, putting it back"
    journalctl -u "${unit%.service}" -n 30 --no-pager || true
    if [ "$had_unit" = 1 ]; then cp -a "$target.bak-hitop-$ts" "$target"; fi
    if [ -e "$dir/10-hitop-limits.conf.bak-hitop-$ts" ]; then cp -a "$dir/10-hitop-limits.conf.bak-hitop-$ts" "$dir/10-hitop-limits.conf"; else rm -f "$dir/10-hitop-limits.conf"; fi
    systemctl daemon-reload
    systemctl restart "${unit%.service}"
    answers "$probe" && say "$unit: back as it was, answering" || say "$unit: STILL DOWN, see cloud/RUNBOOK.md"
    return 1
  fi
}

show() {
  for u in hitop-sync hitop-server "$(pg_unit)" hitop-sync-backup hitop-disk-alert; do
    systemctl cat "$u" >/dev/null 2>&1 || continue
    echo "== $u"
    systemctl show "$u" -p ActiveState -p MemoryHigh -p MemoryMax -p CPUQuotaPerSecUSec -p CPUWeight -p IOWeight -p TasksMax -p LimitNOFILE -p ProtectSystem -p NoNewPrivileges -p PrivateTmp | sed 's/^/   /'
    systemd-analyze security "$u" --no-pager 2>/dev/null | tail -n 1 | sed 's/^/   /' || true
  done
  systemctl list-timers 'hitop-*' --no-pager 2>/dev/null || true
}

part="${1:-}"
case "$part" in
  hardening)
    harden hitop-sync.service "$repo/sync-server/deploy/hitop-sync.service" "$here/systemd/hitop-sync.service.d/10-hitop-limits.conf" http://127.0.0.1:7881/healthz
    harden hitop-server.service "$repo/deploy/hitop-server.service" "$here/systemd/hitop-server.service.d/10-hitop-limits.conf" http://127.0.0.1:7880/
    show;;
  pg-limits)
    u="$(pg_unit)"
    systemctl cat "$u" >/dev/null 2>&1 || { echo "no $u: run sync-server/deploy/postgres-setup.sh first" >&2; exit 1; }
    put "$here/systemd/postgresql-hitop.service.d/10-hitop-limits.conf" "$SYSTEMD/$u.d/10-hitop-limits.conf"
    systemctl daemon-reload
    systemctl restart "$u"
    systemctl is-active -q "$u" && say "$u active with limits" || { say "$u NOT active"; exit 1; }
    if systemctl is-active -q hitop-sync; then answers http://127.0.0.1:7881/healthz && say "hitop-sync answers"; fi;;
  disk-alert)
    put "$here/disk-alert.sh" "$LIB/disk-alert.sh" 0755
    put "$here/systemd/hitop-disk-alert.service" "$SYSTEMD/hitop-disk-alert.service"
    put "$here/systemd/hitop-disk-alert.timer" "$SYSTEMD/hitop-disk-alert.timer"
    systemctl daemon-reload
    systemctl enable --now hitop-disk-alert.timer
    systemctl start hitop-disk-alert.service || true
    journalctl -u hitop-disk-alert -n 5 --no-pager || true
    [ -f /etc/hitop-sync/alert.env ] || say "no /etc/hitop-sync/alert.env yet: alerts go to the journal only (set ALERT_TO there for mail)";;
  backup)
    command -v age >/dev/null || { echo "age is not installed: apt-get install -y age" >&2; exit 1; }
    put "$repo/sync-server/deploy/backup.sh" "$LIB/backup.sh" 0755
    put "$repo/sync-server/deploy/s3.sh" "$LIB/s3.sh" 0644
    put "$repo/sync-server/deploy/hitop-sync-backup.service" "$SYSTEMD/hitop-sync-backup.service"
    put "$repo/sync-server/deploy/hitop-sync-backup.timer" "$SYSTEMD/hitop-sync-backup.timer"
    systemctl daemon-reload
    if [ -s /etc/hitop-sync/backup.env ] && grep -q '^age1' /etc/hitop-sync/backup-recipients.txt 2>/dev/null; then
      systemctl start hitop-sync-backup.service && say "first backup done: $(cat /var/lib/private/hitop-sync/backup-status.json)"
      systemctl enable --now hitop-sync-backup.timer
    else
      say "installed but not enabled: the owner's bucket (/etc/hitop-sync/backup.env, from backup.env.example) and age public key (/etc/hitop-sync/backup-recipients.txt) come first"
    fi;;
  status) show;;
  remove)
    what="${2:-}"
    newest_bak() { ls -1 "$1".bak-hitop-* 2>/dev/null | sort | tail -n 1; }
    restore() { local b; b="$(newest_bak "$1")"; if [ -n "$b" ]; then cp -a "$b" "$1"; say "restored $1 from $b"; else rm -f "$1"; say "removed $1"; fi; }
    case "$what" in
      hardening)
        for u in hitop-sync hitop-server; do
          restore "$SYSTEMD/$u.service"; restore "$SYSTEMD/$u.service.d/10-hitop-limits.conf"
        done
        systemctl daemon-reload; systemctl restart hitop-sync hitop-server;;
      pg-limits) restore "$SYSTEMD/$(pg_unit).d/10-hitop-limits.conf"; systemctl daemon-reload; systemctl restart "$(pg_unit)";;
      disk-alert) systemctl disable --now hitop-disk-alert.timer || true; rm -f "$SYSTEMD/hitop-disk-alert.service" "$SYSTEMD/hitop-disk-alert.timer" "$LIB/disk-alert.sh"; systemctl daemon-reload;;
      backup) systemctl disable --now hitop-sync-backup.timer || true; rm -f "$SYSTEMD/hitop-sync-backup.service" "$SYSTEMD/hitop-sync-backup.timer" "$LIB/backup.sh" "$LIB/s3.sh"; systemctl daemon-reload;;
      *) echo "remove hardening|pg-limits|disk-alert|backup" >&2; exit 2;;
    esac;;
  *) sed -n '2,20p' "$0"; exit 2;;
esac
