#!/usr/bin/env bash
# HITOP Cloud status page feeder: probes each component from wherever it runs (never the VM: the
# VM can't report its own outage) and writes status.json for index.html beside it. Run it every
# 5 minutes from an off-host scheduler (cloud/RUNBOOK.md, "Uptime and status page"), then publish
# the directory (index.html, status.json, incidents.json) to the static host behind
# status.hitop.rws8.tech.
#
#   check.sh [out dir]       (default: this directory)
#
# CHECKS: "name|url" lines (default: the sync API, the site, the portal). A component is up when
# its URL answers 2xx within TIMEOUT seconds (10). status.json keeps the last HISTORY (288 = a day
# at 5 minutes) results per component. Exit 1 when anything is down.
set -uo pipefail
out="${1:-$(cd "$(dirname "$0")" && pwd)}"
TIMEOUT="${TIMEOUT:-10}"
HISTORY="${HISTORY:-288}"
CHECKS="${CHECKS:-sync|https://sync.hitop.rws8.tech/healthz
site|https://hitop.rws8.tech/
portal|https://hitop.rws8.tech/cloud/healthz}"
mkdir -p "$out"
results="$(mktemp)"
trap 'rm -f "$results"' EXIT
down=0
while IFS='|' read -r name url; do
  [ -n "$name" ] || continue
  r="$(curl -s -o /dev/null -m "$TIMEOUT" -w '%{http_code} %{time_total}' "$url" 2>/dev/null || true)"
  code="${r%% *}"; secs="${r##* }"
  case "$code" in 2??) ok=true;; *) ok=false; down=1;; esac
  printf '%s\t%s\t%s\t%s\t%s\n' "$name" "$url" "$ok" "${code:-000}" "${secs:-0}" >> "$results"
done <<< "$CHECKS"
python3 - "$out/status.json" "$results" "$HISTORY" <<'PY'
import json, sys, time
path, results, keep = sys.argv[1], sys.argv[2], int(sys.argv[3])
try:
    old = json.load(open(path))
except Exception:
    old = {}
history = old.get("history", {})
now = int(time.time())
components = []
for line in open(results):
    name, url, ok, code, secs = line.rstrip("\n").split("\t")
    entry = {"at": now, "ok": ok == "true", "code": int(code or 0), "ms": int(float(secs or 0) * 1000)}
    h = (history.get(name, []) + [entry])[-keep:]
    history[name] = h
    up = sum(1 for e in h if e["ok"])
    components.append({"name": name, "url": url, **entry, "uptime": round(100.0 * up / len(h), 2), "checks": len(h)})
json.dump({"updated": now, "components": components, "history": history}, open(path + ".tmp", "w"))
import os
os.replace(path + ".tmp", path)
PY
[ -f "$out/incidents.json" ] || echo '{"incidents": []}' > "$out/incidents.json"
[ "$down" = 0 ]
