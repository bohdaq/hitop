#!/usr/bin/env bash
# HITOP Cloud C2 restore drill: restores a backup into a scratch database and proves it whole. Run
# it where the age PRIVATE key is (the owner's computer, never the VM), monthly, and commit the
# evidence file it writes (SOC 2: cloud/policies/backup-recovery.md).
#
#   restore-drill.sh --identity <age identity file> [--object <name>] [--config backup.env]
#                    [--scratch-url postgres://...] [--evidence-dir <dir>]
#
#   1. The newest hitop-sync-*.tar.age in BACKUP_TARGET (or --object), downloaded, its sha256 noted.
#   2. age -d with the identity, untar: the data and manifest.json.
#   3. Into a scratch database: a SQLite backup is opened as is; a pg_dump is restored with
#      pg_restore into --scratch-url (an EMPTY database), or into a throwaway postgres:17-alpine
#      container when there is none (needs docker).
#   4. hitop-sync-server db-stats on it must equal the manifest (row counts and content hashes of
#      every table), and audit-verify must pass.
#   5. Writes <evidence-dir>/<UTC date>-<object>.md (default cloud/evidence/restore-drills/ of the
#      checkout this script is in) and exits 0 only if every check passed.
#
# Needs: age, curl, python3, the hitop-sync-server binary (BIN, or on PATH; built with
# --features postgres for a Postgres backup), pg_restore of the server's major (PG_RESTORE, may be
# "docker run --rm -i --network host -e PGPASSWORD postgres:17-alpine pg_restore").
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=s3.sh
. "$here/s3.sh"

identity="" object="" config="" scratch="" evidence="$here/../../cloud/evidence/restore-drills"
while [ $# -gt 0 ]; do
  case "$1" in
    --identity) identity="$2"; shift 2;;
    --object) object="$2"; shift 2;;
    --config) config="$2"; shift 2;;
    --scratch-url) scratch="$2"; shift 2;;
    --evidence-dir) evidence="$2"; shift 2;;
    *) echo "unknown argument $1 (see the header of $0)" >&2; exit 2;;
  esac
done
[ -n "$config" ] && { set -a; . "$config"; set +a; }
[ -f "$identity" ] || { echo "--identity <age identity file> is required (the private key; it never goes to the VM)" >&2; exit 2; }
BIN="${BIN:-$(command -v hitop-sync-server || echo /usr/local/bin/hitop-sync-server)}"
AGE="${AGE:-age}"
PG_RESTORE="${PG_RESTORE:-pg_restore}"
target_kind >/dev/null
started="$(date -u +%s)"
work="$(mktemp -d)"
container=""
cleanup() { [ -n "$container" ] && docker rm -f "$container" >/dev/null 2>&1; rm -rf "$work"; }
trap cleanup EXIT

[ -n "$object" ] || object="$(list_objects | grep '^hitop-sync-.*\.tar\.age$' | tail -n 1)"
[ -n "$object" ] || { echo "no backup in $BACKUP_TARGET" >&2; exit 1; }
get_object "$object" "$work/backup.tar.age"
sum="$(sha256sum "$work/backup.tar.age" | cut -d' ' -f1)"
mkdir "$work/b"
"$AGE" -d -i "$identity" "$work/backup.tar.age" | tar -C "$work/b" -xf -
manifest="$work/b/manifest.json"
format="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["format"])' "$manifest")"

case "$format" in
  sqlite) url="sqlite:$work/b/sync.db";;
  pg_dump-custom)
    if [ -z "$scratch" ]; then
      port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
      pw="$(head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n')"
      container="hitop-drill-$$"
      docker run -d --name "$container" -e POSTGRES_PASSWORD="$pw" -p "127.0.0.1:$port:5432" postgres:17-alpine >/dev/null
      for _ in $(seq 60); do docker exec "$container" pg_isready -U postgres >/dev/null 2>&1 && break; sleep 1; done
      sleep 1
      scratch="postgres://postgres:$pw@127.0.0.1:$port/postgres"
    fi
    pw_of="$(python3 -c 'import sys,urllib.parse as u; print(u.unquote(u.urlsplit(sys.argv[1]).password or ""))' "$scratch")"
    bare="$(python3 -c 'import sys,urllib.parse as u; p=u.urlsplit(sys.argv[1]); n=p.netloc.rsplit("@",1); h=n[-1]; us=n[0].split(":")[0] if len(n)==2 else ""; print(u.urlunsplit((p.scheme,(us+"@" if us else "")+h,p.path,p.query,p.fragment)))' "$scratch")"
    # shellcheck disable=SC2086
    PGPASSWORD="$pw_of" $PG_RESTORE --no-owner --no-privileges --exit-on-error "--dbname=$bare" < "$work/b/sync.pgdump"
    url="$scratch";;
  *) echo "unknown backup format $format" >&2; exit 1;;
esac

HITOP_SYNC_DATABASE="$url" "$BIN" db-stats --json > "$work/restored.json"
HITOP_SYNC_DATABASE="$url" "$BIN" audit-verify > "$work/audit.out" && audit_ok=1 || audit_ok=0
report="$(python3 - "$manifest" "$work/restored.json" <<'PY'
import json, sys
m = json.load(open(sys.argv[1])); r = json.load(open(sys.argv[2]))
want = {t["table"]: t for t in m["stats"]["tables"]}; got = {t["table"]: t for t in r["tables"]}
ok = want.keys() == got.keys() and all(want[k] == got[k] for k in want)
lines = [f"| {k} | {want[k]['rows']} | {got.get(k, {}).get('rows', '-')} | {'yes' if want[k] == got.get(k) else 'NO'} |" for k in want]
print(json.dumps({"ok": ok, "rows": sum(t["rows"] for t in m["stats"]["tables"]), "created": m["createdAt"], "format": m["format"], "schema": m["stats"]["schema"], "table": "\n".join(lines)}))
PY
)"
ok="$(echo "$report" | python3 -c 'import json,sys; print("yes" if json.load(sys.stdin)["ok"] else "no")')"
field() { echo "$report" | python3 -c "import json,sys; print(json.load(sys.stdin)['$1'])"; }
created="$(field created)"
mkdir -p "$evidence"
out="$evidence/$(date -u +%Y-%m-%d)-${object%.tar.age}.md"
result="PASSED"
[ "$ok" = yes ] && [ "$audit_ok" = 1 ] || result="FAILED"
{
  echo "# Restore drill $(date -u +%Y-%m-%d): $result"
  echo
  echo "- Run: $(date -u +'%Y-%m-%d %H:%M:%S UTC') by $(id -un)@$(hostname -s), $(( $(date -u +%s) - started )) s"
  echo "- Backup: \`$object\` from \`$BACKUP_TARGET\`, sha256 \`$sum\`, $(stat -c %s "$work/backup.tar.age") bytes"
  echo "- Made: $(date -u -d "@$created" +'%Y-%m-%d %H:%M:%S UTC') ($(( (started - created) / 3600 )) h before the drill), format \`$(field format)\`, schema $(field schema), $(field rows) rows"
  echo "- Restored into: $( [ "$format" = sqlite ] && echo "a scratch SQLite file" || echo "a scratch Postgres database ($( [ -n "$container" ] && echo "throwaway postgres:17-alpine container" || echo "given --scratch-url"))")"
  echo "- Row counts and content hashes equal to the backup's manifest: **$ok**"
  echo "- \`hitop-sync-server audit-verify\`: \`$(tail -n 1 "$work/audit.out")\`"
  echo
  echo "| table | rows in the manifest | rows restored | hash equal |"
  echo "| --- | --- | --- | --- |"
  field table
} > "$out"
cat "$out"
echo
echo "evidence: $out"
[ "$result" = PASSED ]
