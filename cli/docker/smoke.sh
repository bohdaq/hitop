#!/usr/bin/env bash
# Runs the smoke collection with the given image against a local static server.
#   cli/docker/smoke.sh ghcr.io/bohdaq/hitop:<version>
set -euo pipefail
image="$1"
here="$(cd "$(dirname "$0")" && pwd)"
python3 -m http.server 8099 --bind 127.0.0.1 --directory "$here/smoke/www" >/dev/null 2>&1 &
server=$!
trap 'kill $server' EXIT
for _ in $(seq 20); do curl -fs http://127.0.0.1:8099/ping.json >/dev/null && break; sleep 0.25; done
docker run --rm --network host -v "$here/smoke:/work/smoke:ro" "$image" run ./smoke
echo "image smoke test passed"
