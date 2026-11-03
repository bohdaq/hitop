#!/bin/sh
# Smoke test of a hitop sync server image (CI and release-sync-server.yml): a container on a fresh
# volume answers /healthz, and `support-bundle` runs inside it (the self-host support path).
# Usage: image-smoke.sh <image>
set -eu
image=$1
name="hitop-sync-smoke-$$"
docker run -d --name "$name" -p 127.0.0.1:17881:7881 -e HITOP_SYNC_PUBLIC_URL=https://sync.example.com "$image" >/dev/null
cleanup() {
  docker logs "$name" >&2 2>&1 || true
  docker rm -f "$name" >/dev/null 2>&1 || true
}
trap cleanup EXIT
ok=
for _ in $(seq 1 60); do
  if out=$(curl -sf http://127.0.0.1:17881/healthz); then
    echo "$out"
    case "$out" in *'"ok":true'*'"schema":'*) ok=1; break ;; esac
  fi
  sleep 1
done
[ -n "$ok" ] || { echo "no healthy /healthz" >&2; exit 1; }
# The support bundle, written into the volume and copied out (the image has no shell).
docker exec "$name" /usr/local/bin/hitop-sync-server support-bundle --out /data/bundle.json --no-journal
docker cp "$name:/data/bundle.json" ./bundle.json
grep -q '"schemaInDatabase"' bundle.json
grep -q '"database": "sqlite"' bundle.json
rm -f bundle.json
echo "image ok: /healthz and support-bundle"
