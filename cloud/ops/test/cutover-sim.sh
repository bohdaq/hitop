#!/usr/bin/env bash
# cutover-postgres.sh and rollback-postgres.sh against a local server and Postgres, the way the VM
# runs them but with stand-ins for systemctl (STOP_CMD/START_CMD). Checks: the switch keeps every
# row and the audit chains, a refused second cutover, writes on Postgres survive a rollback, and a
# cutover into a non-empty database stops before the write freeze.
#
#   BIN=sync-server/target/debug/hitop-sync-server HITOP_SYNC_TEST_POSTGRES=postgres://... \
#     HITOP_SYNC_TEST_PSQL="docker run --rm -i --network host -e PGPASSWORD postgres:17-alpine psql" \
#     cloud/ops/test/cutover-sim.sh
set -euo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="$(cd "$(dirname "${BIN:?BIN: hitop-sync-server built with --features postgres}")" && pwd)/$(basename "$BIN")"
: "${HITOP_SYNC_TEST_POSTGRES:?HITOP_SYNC_TEST_POSTGRES}" "${HITOP_SYNC_TEST_PSQL:?HITOP_SYNC_TEST_PSQL}"
t="$(mktemp -d)"
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
dbname="cutover_$$"
pw="$(python3 -c 'import sys,urllib.parse as u; print(u.urlsplit(sys.argv[1]).password or "")' "$HITOP_SYNC_TEST_POSTGRES")"
bare="$(python3 -c 'import sys,urllib.parse as u; p=u.urlsplit(sys.argv[1]); print(f"{p.scheme}://{p.username}@{p.hostname}:{p.port}{p.path}")' "$HITOP_SYNC_TEST_POSTGRES")"
psql() { PGPASSWORD="$pw" $HITOP_SYNC_TEST_PSQL "$bare" -qAt -c "$1"; }
cleanup() { [ -f "$t/pid" ] && kill "$(cat "$t/pid")" 2>/dev/null; psql "DROP DATABASE IF EXISTS $dbname WITH (FORCE)" >/dev/null 2>&1 || true; rm -rf "$t"; }
trap cleanup EXIT
pass=0
ok() { pass=$((pass + 1)); echo "PASS $*"; }
fail() { echo "FAIL $*"; exit 1; }

mkdir -p "$t/data"
cat > "$t/env" <<EOF
HITOP_SYNC_DATABASE=sqlite:$t/data/sync.db
HITOP_SYNC_PUBLIC_URL=http://127.0.0.1:$port
HITOP_SYNC_RP_ID=localhost
HITOP_SYNC_RP_ORIGINS=http://localhost:$port
HITOP_SYNC_MAIL=log
RWS_CONFIG_IP=127.0.0.1
RWS_CONFIG_PORT=$port
EOF
cat > "$t/start" <<EOF
#!/usr/bin/env bash
set -a; . "$t/env"; set +a
"$BIN" >/dev/null 2>>"$t/server.log" &
echo \$! > "$t/pid"
for _ in \$(seq 100); do curl -fs "http://127.0.0.1:$port/healthz" >/dev/null && exit 0; sleep 0.1; done
exit 1
EOF
printf '#!/usr/bin/env bash\nkill "$(cat %s/pid)" 2>/dev/null; while kill -0 "$(cat %s/pid)" 2>/dev/null; do sleep 0.1; done; rm -f %s/pid\n' "$t" "$t" "$t" > "$t/stop"
chmod +x "$t/start" "$t/stop"
psql "CREATE DATABASE $dbname" >/dev/null
echo "${HITOP_SYNC_TEST_POSTGRES%/*}/$dbname" > "$t/postgres.url"
common=(NO_SYSTEMD=1 BIN="$BIN" ENV_FILE="$t/env" URL_FILE="$t/postgres.url" DATA_DIR="$t/data" HEALTH_URL="http://127.0.0.1:$port/healthz" STOP_CMD="$t/stop" START_CMD="$t/start" DROPIN_DIR="$t/dropin" PG_UNIT=none.service SQLITE_URL="sqlite:$t/data/sync.db")

"$t/start"
python3 "$repo/cloud/ops/test/populate.py" "http://127.0.0.1:$port" "$t/server.log" ada@example.test > /dev/null
python3 "$repo/cloud/ops/test/populate.py" "http://127.0.0.1:$port" "$t/server.log" bob@example.test > /dev/null
rows_sqlite="$(HITOP_SYNC_DATABASE="sqlite:$t/data/sync.db" "$BIN" db-stats | tail -n 1)"

env "${common[@]}" REPORT="$t/report" "$repo/sync-server/deploy/cutover-postgres.sh" > "$t/cutover.out" 2>&1 || { cat "$t/cutover.out"; fail "cutover"; }
grep -q '"database":"postgres"' <(curl -fsS "http://127.0.0.1:$port/healthz") && ok "cutover: the server runs on Postgres" || fail "healthz"
grep -q "row counts and content hashes equal" "$t/report" && grep -q " 0 broken" "$t/report" && ok "cutover: rows, hashes and audit chains checked, in the report" || { cat "$t/report"; fail "report"; }
[ "$(HITOP_SYNC_DATABASE="$(cat "$t/postgres.url")" "$BIN" db-stats | tail -n 1)" = "$rows_sqlite" ] && ok "cutover: Postgres has as many rows as SQLite had ($rows_sqlite)" || fail "rows"
ls "$t/data" | grep -q '^sync.db.pre-pg-' && grep -q "^HITOP_SYNC_DATABASE=postgres" "$t/env" && ls "$t" | grep -q '^env.bak-' && ok "cutover: SQLite and env file backed up, env switched" || fail "backups"
if env "${common[@]}" "$repo/sync-server/deploy/cutover-postgres.sh" > /dev/null 2>&1; then fail "a second cutover ran"; else ok "a second cutover is refused"; fi

# Writes on Postgres, then roll back keeping them.
python3 "$repo/cloud/ops/test/populate.py" "http://127.0.0.1:$port" "$t/server.log" cy@example.test > /dev/null
rows_pg="$(HITOP_SYNC_DATABASE="$(cat "$t/postgres.url")" "$BIN" db-stats | tail -n 1)"
env "${common[@]}" "$repo/sync-server/deploy/rollback-postgres.sh" > "$t/rollback.out" 2>&1 || { cat "$t/rollback.out"; fail "rollback"; }
grep -q '"database":"sqlite"' <(curl -fsS "http://127.0.0.1:$port/healthz") && grep -q "^HITOP_SYNC_DATABASE=sqlite:" "$t/env" && ok "rollback: on SQLite again" || fail "rollback health"
[ "$(HITOP_SYNC_DATABASE="sqlite:$t/data/sync.db" "$BIN" db-stats | tail -n 1)" = "$rows_pg" ] && ok "rollback: the writes made on Postgres are kept ($rows_pg)" || fail "rollback rows"

# A cutover into a database that isn't empty stops before the freeze; the server stays up.
if env "${common[@]}" "$repo/sync-server/deploy/cutover-postgres.sh" > "$t/again.out" 2>&1; then fail "cutover into a non-empty database"; fi
grep -q "rows already" "$t/again.out" && curl -fs "http://127.0.0.1:$port/healthz" | grep -q sqlite && ok "a non-empty target stops the cutover before the write freeze" || { cat "$t/again.out"; fail "non-empty"; }
echo "cutover-sim: $pass checks passed"
