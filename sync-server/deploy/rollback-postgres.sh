#!/usr/bin/env bash
# HITOP Cloud C2: back from Postgres to SQLite after cutover-postgres.sh. Run as root ON the host.
#
#   rollback-postgres.sh            keeps every write made on Postgres: stops the service, copies
#                                   Postgres into a new SQLite file (migrate-db, verified), puts it
#                                   in place of sync.db (the old one is kept as sync.db.before-
#                                   rollback-<ts>), points HITOP_SYNC_DATABASE back, starts, checks.
#   rollback-postgres.sh --snapshot <sync.db.pre-pg-TS>
#                                   goes back to the SQLite file the cutover froze, losing what was
#                                   written on Postgres since (when Postgres itself is broken).
#
# Postgres and its data are not touched (drop them by hand once you're sure). Settings as in
# cutover-postgres.sh.
set -euo pipefail

BIN="${BIN:-/usr/local/bin/hitop-sync-server}"
ENV_FILE="${ENV_FILE:-/etc/hitop-sync/env}"
DATA_DIR="${DATA_DIR:-/var/lib/private/hitop-sync}"
HEALTH_URL="${HEALTH_URL:-http://127.0.0.1:7881/healthz}"
SERVICE="${SERVICE:-hitop-sync}"
STOP_CMD="${STOP_CMD:-systemctl stop $SERVICE}"
START_CMD="${START_CMD:-systemctl start $SERVICE}"
DROPIN_DIR="${DROPIN_DIR:-/etc/systemd/system/$SERVICE.service.d}"
SQLITE_URL="${SQLITE_URL:-sqlite:/var/lib/hitop-sync/sync.db}"
REPORT="${REPORT:-/dev/null}"
ts="$(date -u +%Y%m%dT%H%M%SZ)"
db="$DATA_DIR/sync.db"

say() { echo "rollback: $*" | tee -a "$REPORT"; }
die() { say "STOPPED: $*"; exit 1; }
run_bin() { env -u HITOP_SYNC_DATABASE HITOP_SYNC_DATABASE="$1" "$BIN" "${@:2}"; }

snapshot=""
if [ "${1:-}" = "--snapshot" ]; then
  snapshot="${2:-}"
  [ -f "$snapshot" ] || die "--snapshot needs the sync.db.pre-pg-<ts> file the cutover kept"
fi
PG_URL="$(sed -n 's/^HITOP_SYNC_DATABASE=//p' "$ENV_FILE" | tail -n 1)"
case "$PG_URL" in postgres://*|postgresql://*) ;; *) die "$ENV_FILE doesn't point at Postgres (HITOP_SYNC_DATABASE=$PG_URL)";; esac

say "$(date -u) start$([ -n "$snapshot" ] && echo " (to the snapshot $snapshot, writes since the cutover are lost)")"
$STOP_CMD
cp -a "$ENV_FILE" "$ENV_FILE.bak-$ts"
work="$(mktemp -d "$DATA_DIR/rollback-$ts.XXXX")" # same file system: the mv below is atomic
trap 'rm -rf "$work"' EXIT
if [ -n "$snapshot" ]; then
  cp -a "$snapshot" "$work/sync.db"
  [ -f "$snapshot-wal" ] && cp -a "$snapshot-wal" "$work/sync.db-wal"
else
  if ! run_bin "$PG_URL" migrate-db --from "$PG_URL" --to "sqlite:$work/sync.db" 2>&1 | tee -a "$REPORT"; then
    $START_CMD || true
    die "the copy out of Postgres failed; the service runs on Postgres again"
  fi
fi
# Checked before it goes in place (the live file is only ever opened by the service).
run_bin "sqlite:$work/sync.db" audit-verify > "$work/audit.out" || true
tail -n 1 "$work/audit.out" | tee -a "$REPORT" | grep -q ' 0 broken' || { $START_CMD || true; die "audit-verify failed on the SQLite copy; nothing changed"; }
rm -f "$work/sync.db-shm"
for f in "$db" "$db-wal" "$db-shm"; do [ -f "$f" ] && mv "$f" "${f/sync.db/sync.db.before-rollback-$ts}"; done
mv "$work/sync.db" "$db"
[ -f "$work/sync.db-wal" ] && mv "$work/sync.db-wal" "$db-wal"
# DynamicUser: the state directory belongs to the service's user; so must the database.
chown --reference="$DATA_DIR" "$db" "$db"-* 2>/dev/null || chown --reference="$DATA_DIR" "$db"
chmod 0600 "$db"
sed -i "s#^HITOP_SYNC_DATABASE=.*#HITOP_SYNC_DATABASE=$SQLITE_URL#" "$ENV_FILE"
rm -f "$DROPIN_DIR/20-postgres.conf"
if [ -z "${NO_SYSTEMD:-}" ] && command -v systemctl >/dev/null && [ -d /run/systemd/system ]; then systemctl daemon-reload; fi
$START_CMD
for _ in $(seq 30); do health="$(curl -fsS "$HEALTH_URL" 2>/dev/null)" && break; sleep 1; done
echo "${health:-}" | grep -q '"database":"sqlite"' || die "/healthz doesn't say sqlite: ${health:-no answer}; the env backup is $ENV_FILE.bak-$ts"
say "done: on SQLite again ($db); the previous file is sync.db.before-rollback-$ts; Postgres untouched"
