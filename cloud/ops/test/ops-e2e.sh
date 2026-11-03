#!/usr/bin/env bash
# HITOP Cloud C2 ops scripts end to end, without a VM (CI job `ops`; locally too):
#
#   - backup.sh → restore-drill.sh on SQLite, into a directory target, with a throwaway age key:
#     the drill passes and writes its evidence; a wrong key, a tampered backup and a missing age
#     recipient are refused; the backup holds no plaintext;
#   - the same against an S3 API (LocalStack S3 on CI) when S3_ENDPOINT is set: upload, list, prune;
#   - on Postgres when HITOP_SYNC_TEST_POSTGRES and HITOP_SYNC_TEST_PG_DUMP/_PG_RESTORE are set:
#     dump in one snapshot, restore into a scratch database, rows and hashes equal;
#   - disk-alert.sh over and under its thresholds, a stale backup, the alert sent once;
#   - status/check.sh against a live server and a dead port;
#   - systemd-analyze verify and security on the units with their drop-ins.
#
#   BIN=sync-server/target/debug/hitop-sync-server cloud/ops/test/ops-e2e.sh
set -euo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="$(cd "$(dirname "${BIN:?BIN: the hitop-sync-server binary (built with --features postgres)}")" && pwd)/$(basename "$BIN")"
export BIN
deploy="$repo/sync-server/deploy"
t="$(mktemp -d)"
pids=()
cleanup() { for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done; rm -rf "$t"; }
trap cleanup EXIT
pass=0
ok() { pass=$((pass + 1)); echo "PASS $*"; }
fail() { echo "FAIL $*"; exit 1; }
# Locally a missing tool skips its part; on CI (CI set) nothing may be skipped.
skip() { if [ -n "${CI:-}" ]; then fail "skipped on CI: $*"; fi; echo "SKIP $*"; }
free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'; }

# A sync server on $1 (database URL), mail to its log; prints "url log".
start_server() {
  local db="$1" port; port="$(free_port)"
  local log="$t/server-$port.log"
  HITOP_SYNC_DATABASE="$db" RWS_CONFIG_IP=127.0.0.1 RWS_CONFIG_PORT="$port" HITOP_SYNC_PUBLIC_URL="http://127.0.0.1:$port" \
    HITOP_SYNC_RP_ID=localhost HITOP_SYNC_RP_ORIGINS="http://localhost:$port" HITOP_SYNC_MAIL=log "$BIN" 2>"$log" >/dev/null &
  pids+=($!)
  for _ in $(seq 100); do curl -fs "http://127.0.0.1:$port/healthz" >/dev/null 2>&1 && break; sleep 0.1; done
  echo "http://127.0.0.1:$port $log"
}

age-keygen -o "$t/id.txt" 2>/dev/null
age-keygen -y "$t/id.txt" > "$t/recipients.txt"
age-keygen -o "$t/other.txt" 2>/dev/null

# ---- SQLite, directory target -----------------------------------------------------------
read -r url log < <(start_server "sqlite:$t/sync.db")
python3 "$repo/cloud/ops/test/populate.py" "$url" "$log" ada@example.test > /dev/null
python3 "$repo/cloud/ops/test/populate.py" "$url" "$log" bob@example.test > /dev/null
common=(HITOP_SYNC_DATABASE="sqlite:$t/sync.db" AGE_RECIPIENTS_FILE="$t/recipients.txt" STATUS_FILE="$t/status.json" TMPDIR="$t")
env "${common[@]}" BACKUP_TARGET="file://$t/target" "$deploy/backup.sh" > "$t/b1.out"
obj="$(ls "$t/target")"
[[ "$obj" == hitop-sync-*.tar.age ]] && ok "backup.sh writes an encrypted object ($obj)" || fail "no object: $(cat "$t/b1.out")"
grep -q '"ok": true' "$t/status.json" && ok "backup.sh writes its status file" || fail "status $(cat "$t/status.json")"
! grep -q "ada@example.test" "$t/target/$obj" && head -c 21 "$t/target/$obj" | grep -q "age-encryption.org" && ok "the backup is age ciphertext, no plaintext address in it" || fail "plaintext in the backup"
env BACKUP_TARGET="file://$t/target" "$deploy/restore-drill.sh" --identity "$t/id.txt" --evidence-dir "$t/evidence" > "$t/drill.out" 2>&1 || { cat "$t/drill.out"; fail "restore drill"; }
ev="$(ls "$t/evidence"/*.md)"
grep -q "PASSED" "$ev" && grep -q "manifest: \*\*yes\*\*" "$ev" && grep -q " 0 broken" "$ev" && ok "restore-drill.sh restores, row counts and hashes equal, audit chains intact, evidence written" || { cat "$ev"; fail "evidence"; }
grep -q "| users | 2 | 2 | yes |" "$ev" && ok "the evidence lists each table" || { cat "$ev"; fail "evidence table"; }
if env BACKUP_TARGET="file://$t/target" "$deploy/restore-drill.sh" --identity "$t/other.txt" --evidence-dir "$t/evidence2" > /dev/null 2>&1; then fail "a wrong age key restored"; else ok "a wrong age key restores nothing"; fi
cp "$t/target/$obj" "$t/tampered.tar.age"; printf 'x' | dd of="$t/tampered.tar.age" bs=1 seek=300 conv=notrunc 2>/dev/null
mkdir -p "$t/target-bad"; cp "$t/tampered.tar.age" "$t/target-bad/hitop-sync-20990101T000000Z.tar.age"
if env BACKUP_TARGET="file://$t/target-bad" "$deploy/restore-drill.sh" --identity "$t/id.txt" --evidence-dir "$t/evidence3" > /dev/null 2>&1; then fail "a tampered backup restored"; else ok "a tampered backup is refused"; fi
if env "${common[@]}" AGE_RECIPIENTS_FILE="$t/none.txt" BACKUP_TARGET="file://$t/target" "$deploy/backup.sh" > /dev/null 2>&1; then fail "a backup without recipients"; else ok "no age recipient, no backup (never unencrypted)"; fi

# ---- S3 API (LocalStack S3 on CI; any S3 endpoint) ------------------------------------------
if [ -n "${S3_ENDPOINT:-}" ]; then
  # shellcheck source=../../../sync-server/deploy/s3.sh
  . "$deploy/s3.sh"
  export S3_ENDPOINT S3_REGION="${S3_REGION:-us-east-1}" S3_ACCESS_KEY_ID S3_SECRET_ACCESS_KEY
  bucket="hitop-ops-$$"
  printf 'user = "%s:%s"\n' "$S3_ACCESS_KEY_ID" "$S3_SECRET_ACCESS_KEY" | curl -fsS --config - --aws-sigv4 "aws:amz:$S3_REGION:s3" -X PUT "${S3_ENDPOINT%/}/$bucket" -o /dev/null
  export BACKUP_TARGET="s3://$bucket/sync"
  echo old > "$t/old"; put_object "$t/old" "hitop-sync-20200101T000000Z.tar.age"
  env "${common[@]}" "$deploy/backup.sh" > /dev/null
  [ "$(list_objects | grep -c '^hitop-sync-')" = 2 ] && ok "S3: upload and list (SigV4 by curl)" || fail "S3 list: $(list_objects)"
  "$deploy/restore-drill.sh" --identity "$t/id.txt" --evidence-dir "$t/evidence-s3" > "$t/drill-s3.out" 2>&1 && grep -q PASSED "$t"/evidence-s3/*.md && ok "S3: the restore drill takes the newest backup and passes" || { cat "$t/drill-s3.out"; fail "S3 drill"; }
  sleep 1
  env "${common[@]}" BACKUP_KEEP_DAYS=35 "$deploy/backup.sh" > "$t/b3.out"
  grep -q '"pruned": 1' "$t/b3.out" && ! list_objects | grep -q 20200101 && [ "$(list_objects | grep -c '^hitop-sync-')" = 2 ] && ok "S3: BACKUP_KEEP_DAYS prunes only what is older" || fail "prune: $(cat "$t/b3.out") $(list_objects)"
  unset BACKUP_TARGET
else
  skip "S3 (S3_ENDPOINT not set)"
fi

# ---- Postgres -------------------------------------------------------------------------------
if [ -n "${HITOP_SYNC_TEST_POSTGRES:-}" ] && [ -n "${HITOP_SYNC_TEST_PG_DUMP:-}" ] && [ -n "${HITOP_SYNC_TEST_PG_RESTORE:-}" ]; then
  base="${HITOP_SYNC_TEST_POSTGRES%/*}"
  src="ops_src_$$"; dst="ops_dst_$$"
  # Two empty databases, made with psql (HITOP_SYNC_TEST_PSQL, default: the pg_dump command's psql).
  psql_cmd="${HITOP_SYNC_TEST_PSQL:-${HITOP_SYNC_TEST_PG_DUMP% pg_dump} psql}"
  pw="$(python3 -c 'import sys,urllib.parse as u; print(u.urlsplit(sys.argv[1]).password or "")' "$HITOP_SYNC_TEST_POSTGRES")"
  bare="$(python3 -c 'import sys,urllib.parse as u; p=u.urlsplit(sys.argv[1]); print(f"{p.scheme}://{p.username}@{p.hostname}:{p.port}{p.path}")' "$HITOP_SYNC_TEST_POSTGRES")"
  for d in "$src" "$dst"; do PGPASSWORD="$pw" $psql_cmd "$bare" -qc "CREATE DATABASE $d" >/dev/null; done
  read -r pgurl pglog < <(start_server "$base/$src")
  python3 "$repo/cloud/ops/test/populate.py" "$pgurl" "$pglog" cy@example.test > /dev/null
  env "${common[@]}" HITOP_SYNC_DATABASE="$base/$src" HITOP_SYNC_PG_DUMP="$HITOP_SYNC_TEST_PG_DUMP" BACKUP_TARGET="file://$t/target-pg" "$deploy/backup.sh" > /dev/null
  env BACKUP_TARGET="file://$t/target-pg" PG_RESTORE="$HITOP_SYNC_TEST_PG_RESTORE" "$deploy/restore-drill.sh" --identity "$t/id.txt" --scratch-url "$base/$dst" --evidence-dir "$t/evidence-pg" > "$t/drill-pg.out" 2>&1 \
    && grep -q PASSED "$t"/evidence-pg/*.md && grep -q "pg_dump-custom" "$t"/evidence-pg/*.md && ok "Postgres: pg_dump in one snapshot, pg_restore into a scratch database, rows and hashes equal" || { cat "$t/drill-pg.out"; fail "Postgres drill"; }
  for d in "$src" "$dst"; do PGPASSWORD="$pw" $psql_cmd "$bare" -qc "DROP DATABASE IF EXISTS $d WITH (FORCE)" >/dev/null 2>&1 || true; done
else
  skip "Postgres backup (HITOP_SYNC_TEST_POSTGRES / _PG_DUMP / _PG_RESTORE not set)"
fi

# ---- disk-alert.sh ----------------------------------------------------------------------------
alerts="$t/alerts.log"
da=(env STATE_DIR="$t/alert-state" ALERT_CMD="tee -a $alerts" DISK_PATHS="$t" PG_DATA="" BACKUP_CONFIG="$t/none" BACKUP_STATUS="$t/status.json")
"${da[@]}" DISK_ALERT_PERCENT=100 "$repo/cloud/ops/disk-alert.sh" > "$t/da1.out" && ok "disk-alert: under the threshold, exit 0" || fail "disk-alert under: $(cat "$t/da1.out")"
if "${da[@]}" DISK_ALERT_PERCENT=1 "$repo/cloud/ops/disk-alert.sh" > "$t/da2.out"; then fail "disk-alert over"; fi
grep -q "full (alert at 1%)" "$t/da2.out" && grep -q "full (alert at 1%)" "$alerts" && ok "disk-alert: over the threshold, exit 1, alert sent" || fail "disk-alert: $(cat "$t/da2.out")"
"${da[@]}" DISK_ALERT_PERCENT=1 "$repo/cloud/ops/disk-alert.sh" > /dev/null || true
[ "$(grep -c 'full (alert at 1%)' "$alerts")" = 2 ] && ok "disk-alert: the same problem is mailed once per ALERT_REPEAT_HOURS" || fail "repeat: $(cat "$alerts")"
touch "$t/backup.env"
if env STATE_DIR="$t/alert-state2" ALERT_CMD="tee -a $alerts" DISK_PATHS="$t" DISK_ALERT_PERCENT=100 PG_DATA="" BACKUP_CONFIG="$t/backup.env" BACKUP_STATUS="$t/status.json" BACKUP_MAX_AGE_HOURS=0 "$repo/cloud/ops/disk-alert.sh" > "$t/da3.out"; then fail "stale backup"; fi
grep -q "last good off-site backup" "$t/da3.out" && ok "disk-alert: a backup older than BACKUP_MAX_AGE_HOURS alerts" || fail "backup age: $(cat "$t/da3.out")"
mkdir -p "$t/pgdata"; head -c 4096 /dev/zero > "$t/pgdata/f"
if "${da[@]}" DISK_ALERT_PERCENT=100 PG_DATA="$t/pgdata" PG_MAX_BYTES=1024 "$repo/cloud/ops/disk-alert.sh" > "$t/da4.out"; then fail "pg size"; fi
grep -q "Postgres data directory" "$t/da4.out" && ok "disk-alert: the Postgres data directory over PG_MAX_BYTES alerts" || fail "pg size: $(cat "$t/da4.out")"

# ---- status page feeder -----------------------------------------------------------------------
dead="$(free_port)"
if CHECKS="sync|$url/healthz
portal|http://127.0.0.1:$dead/" "$repo/cloud/status/check.sh" "$t/status-page" > /dev/null; then fail "check.sh with a dead component"; fi
python3 - "$t/status-page/status.json" <<'PY' && ok "status/check.sh: status.json with up and down components and history" || fail "status.json"
import json, sys
s = json.load(open(sys.argv[1]))
c = {x["name"]: x for x in s["components"]}
assert c["sync"]["ok"] and c["sync"]["code"] == 200 and not c["portal"]["ok"], c
assert len(s["history"]["sync"]) == 1
PY
CHECKS="sync|$url/healthz" "$repo/cloud/status/check.sh" "$t/status-page" > /dev/null && [ "$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["history"]["sync"]))' "$t/status-page/status.json")" = 2 ] && ok "status/check.sh: history grows, exit 0 when all are up" || fail "status history"
grep -q 'fetch("status.json"' "$repo/cloud/status/index.html" && ok "the status page reads status.json" || fail "index.html"

# ---- systemd units -----------------------------------------------------------------------------
if command -v systemd-analyze >/dev/null; then
  u="$t/units"; mkdir -p "$u"
  cp "$deploy/hitop-sync.service" "$deploy/hitop-sync-backup.service" "$deploy/hitop-sync-backup.timer" "$repo/deploy/hitop-server.service" "$repo/cloud/ops/systemd/hitop-disk-alert.service" "$repo/cloud/ops/systemd/hitop-disk-alert.timer" "$u/"
  # The drop-ins merged into a copy of each unit (what systemd applies).
  for n in hitop-sync hitop-server; do cat "$u/$n.service" "$repo/cloud/ops/systemd/$n.service.d/10-hitop-limits.conf" > "$u/$n-merged.service"; done
  # ExecStart binaries must exist for verify: stand-ins under a root of our own.
  root="$t/root"; mkdir -p "$root/usr/local/bin" "$root/usr/local/lib/hitop-sync"
  for b in usr/local/bin/hitop-sync-server usr/local/bin/hitop-server usr/local/lib/hitop-sync/backup.sh usr/local/lib/hitop-sync/disk-alert.sh; do printf '#!/bin/sh\n' > "$root/$b"; chmod +x "$root/$b"; done
  out="$(cd "$u" && systemd-analyze verify --root="$root" --man=no --generators=no ./*.service ./*.timer 2>&1 | grep -v -e 'is not executable' -e 'No such file' -e 'Failed to prepare filename' -e 'Unit configuration has fatal error' -e 'not found' -e 'Command .* is not executable' || true)"
  if echo "$out" | grep -qi -e 'unknown key' -e 'unknown section' -e 'invalid' -e 'failed to parse'; then echo "$out"; fail "systemd-analyze verify"; fi
  ok "systemd-analyze verify: no unknown keys or invalid values in the units"
  for n in hitop-sync hitop-server; do
    score="$(systemd-analyze security --offline=true "$u/$n-merged.service" 2>/dev/null | grep 'Overall exposure level' | grep -oE '[0-9]+\.[0-9]+' | tail -n 1)"
    python3 -c "import sys; sys.exit(0 if float('$score') <= 3.0 else 1)" && ok "systemd-analyze security $n: exposure $score (at most 3.0, 'OK')" || fail "$n exposure $score"
  done
else
  skip "systemd-analyze (not installed)"
fi

echo "ops-e2e: $pass checks passed"
