#!/usr/bin/env bash
# HITOP Cloud C2: switch the sync server from SQLite to the Postgres cluster postgres-setup.sh made.
# Run as root ON the host, at the write-freeze time agreed with the owner. Rolls itself back (to
# SQLite, unchanged) when any check fails; rollback-postgres.sh undoes it later. The live SQLite
# file is never opened by anything but the service: every check runs on a copy.
#
#   1. Preflight, server running: the binary opens Postgres, and the database there is empty.
#   2. Write freeze: the service stops (requests fail for the copy's minute or so).
#   3. Backup: sync.db (+ -wal/-shm) copied to sync.db.pre-pg-<ts>; the env file to env.bak-<ts>.
#   4. On a copy of the frozen file: its stats, then migrate-db into Postgres (every row, equal row
#      counts and content hashes, audit chains verify on Postgres).
#   5. Switch: HITOP_SYNC_DATABASE in the env file (it overrides the unit's) = the URL from
#      postgres.url; a drop-in orders the service after (and with) its Postgres cluster.
#   6. Start; /healthz says postgres; db-stats on Postgres = the frozen file's; audit-verify;
#      over-limits.
#
# Settings (environment, defaults for the VM): BIN, ENV_FILE, URL_FILE, DATA_DIR, HEALTH_URL,
# SERVICE (hitop-sync), PG_UNIT (postgresql@<major>-hitop.service), STOP_CMD and START_CMD (tests
# replace systemctl; NO_SYSTEMD=1 skips daemon-reload and the drop-in), DROPIN_DIR, REPORT (a file the report is appended to, for the ops log).
set -euo pipefail

BIN="${BIN:-/usr/local/bin/hitop-sync-server}"
ENV_FILE="${ENV_FILE:-/etc/hitop-sync/env}"
URL_FILE="${URL_FILE:-/etc/hitop-sync/postgres.url}"
DATA_DIR="${DATA_DIR:-/var/lib/private/hitop-sync}"
HEALTH_URL="${HEALTH_URL:-http://127.0.0.1:7881/healthz}"
SERVICE="${SERVICE:-hitop-sync}"
PG_UNIT="${PG_UNIT:-postgresql@$(ls /usr/lib/postgresql 2>/dev/null | sort -n | tail -n 1)-hitop.service}"
STOP_CMD="${STOP_CMD:-systemctl stop $SERVICE}"
START_CMD="${START_CMD:-systemctl start $SERVICE}"
DROPIN_DIR="${DROPIN_DIR:-/etc/systemd/system/$SERVICE.service.d}"
REPORT="${REPORT:-/dev/null}"
ts="$(date -u +%Y%m%dT%H%M%SZ)"
db="$DATA_DIR/sync.db"

say() { echo "cutover: $*" | tee -a "$REPORT"; }
die() { say "STOPPED: $*"; exit 1; }
run_bin() { env -u HITOP_SYNC_DATABASE HITOP_SYNC_DATABASE="$1" "$BIN" "${@:2}"; }
healthy() {
  for _ in $(seq 30); do
    if out="$(curl -fsS "$HEALTH_URL" 2>/dev/null)"; then echo "$out"; return 0; fi
    sleep 1
  done
  return 1
}
# The tables part of db-stats --json (the database kind differs between the two sides).
tables() { python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["tables"], sort_keys=True))'; }
systemd() { [ -z "${NO_SYSTEMD:-}" ] && command -v systemctl >/dev/null && [ -d /run/systemd/system ]; }

[ -x "$BIN" ] || die "$BIN is missing"
[ -s "$URL_FILE" ] || die "$URL_FILE is missing: run postgres-setup.sh first"
[ -f "$db" ] || die "$db is missing"
if grep -q '^HITOP_SYNC_DATABASE=postgres' "$ENV_FILE" 2>/dev/null; then die "$ENV_FILE already points at Postgres"; fi
PG_URL="$(head -n 1 "$URL_FILE")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
say "$(date -u) start: $db → $(echo "$PG_URL" | sed -E 's#//([^:]*):[^@]*@#//\1:***@#')"

# 1. Preflight while the server still runs.
run_bin "$PG_URL" db-stats --json > "$work/target.json" 2>"$work/err" || die "$BIN can't open Postgres: $(cat "$work/err") (a build without --features postgres?)"
rows="$(python3 -c 'import json,sys; print(sum(t["rows"] for t in json.load(open(sys.argv[1]))["tables"]))' "$work/target.json")"
[ "$rows" = 0 ] || die "the Postgres database has $rows rows already; migrate-db only copies into an empty one"
say "preflight ok: Postgres answers and is empty"

# 2. Write freeze.
say "write freeze: $STOP_CMD"
$STOP_CMD
frozen_at="$(date -u +%s)"

rollback() {
  say "rolling back to SQLite: $1"
  [ -f "$ENV_FILE.bak-$ts" ] && cp -a "$ENV_FILE.bak-$ts" "$ENV_FILE"
  rm -f "$DROPIN_DIR/20-postgres.conf"
  if systemd; then systemctl daemon-reload; fi
  $START_CMD || true
  if healthy >/dev/null; then
    say "rolled back: the server runs on SQLite, unchanged (Postgres may hold a partial copy: drop and recreate its database before trying again)"
  else
    say "ROLLBACK FAILED: the server doesn't answer; see cloud/RUNBOOK.md"
  fi
  exit 1
}

# 3. Backup of the frozen database and the env file.
for f in "$db" "$db-wal" "$db-shm"; do if [ -f "$f" ]; then cp -a "$f" "${f/sync.db/sync.db.pre-pg-$ts}"; fi; done
cp -a "$ENV_FILE" "$ENV_FILE.bak-$ts"
say "backup: $db.pre-pg-$ts, $ENV_FILE.bak-$ts"

# 4. Stats and copy, from a copy of the frozen file.
cp "$db" "$work/frozen.db"
if [ -f "$db-wal" ]; then cp "$db-wal" "$work/frozen.db-wal"; fi
before="$(run_bin "sqlite:$work/frozen.db" db-stats --json | tables)" || rollback "db-stats on the frozen copy failed"
if ! run_bin "sqlite:$work/frozen.db" migrate-db --from "sqlite:$work/frozen.db" --to "$PG_URL" > "$work/migrate.out" 2>&1; then
  cat "$work/migrate.out" | tee -a "$REPORT"
  rollback "migrate-db failed"
fi
tee -a "$REPORT" < "$work/migrate.out"

# 5. Switch.
if grep -q '^HITOP_SYNC_DATABASE=' "$ENV_FILE"; then
  sed -i "s#^HITOP_SYNC_DATABASE=.*#HITOP_SYNC_DATABASE=$PG_URL#" "$ENV_FILE"
else
  echo "HITOP_SYNC_DATABASE=$PG_URL" >> "$ENV_FILE"
fi
if systemd; then
  mkdir -p "$DROPIN_DIR"
  printf '[Unit]\n# cutover-postgres.sh: the database is the hitop Postgres cluster.\nAfter=%s\nRequires=%s\n' "$PG_UNIT" "$PG_UNIT" > "$DROPIN_DIR/20-postgres.conf"
  systemctl daemon-reload
fi

# 6. Start and verify.
$START_CMD || rollback "the service didn't start"
health="$(healthy)" || rollback "no answer on $HEALTH_URL"
echo "$health" | grep -q '"database":"postgres"' || rollback "/healthz doesn't say postgres: $health"
after="$(run_bin "$PG_URL" db-stats --json | tables)" || rollback "db-stats on Postgres failed"
[ "$before" = "$after" ] || rollback "the rows on Postgres differ from the frozen SQLite file"
run_bin "$PG_URL" audit-verify > "$work/audit.out" || true
tail -n 1 "$work/audit.out" | tee -a "$REPORT" | grep -q ' 0 broken' || rollback "audit-verify failed on Postgres"
run_bin "$PG_URL" over-limits | tail -n 1 | tee -a "$REPORT" || true
say "done: on Postgres, $(python3 -c 'import json,sys; print(sum(t["rows"] for t in json.loads(sys.argv[1])))' "$after") rows; write freeze $(( $(date -u +%s) - frozen_at )) s; sync.db stays as it was (rollback-postgres.sh)"
