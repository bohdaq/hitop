#!/usr/bin/env bash
# Builds everything hitop.rws8.tech serves into deploy/out/:
#   out/www/          the site (site/) with the web app under /app/ (webapp/) and the HITOP
#                     Cloud portal under /cloud/ (cloud/portal/)
#   out/hitop-server  the server binary (server/): static files + POST /api/proxy
# Then: deploy/install.sh copies them to the host (see deploy/README.md).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/deploy/out"
cargo="${CARGO:-$HOME/.cargo/bin/cargo}"

rm -rf "$out" && mkdir -p "$out"

(cd "$root/site" && npm ci --no-audit --no-fund && npm run build)
(cd "$root/webapp" && npm ci --no-audit --no-fund && npm run build)
cp -r "$root/site/dist" "$out/www"
rm -rf "$out/www/.vite"
mkdir -p "$out/www/app"
cp -r "$root/webapp/dist/." "$out/www/app/"
# The HITOP Cloud portal (cloud/portal, AGPL): www/cloud/, served at cloud.hitop.rws8.tech (the
# front rws adds the /cloud prefix) and at hitop.rws8.tech/cloud/.
(cd "$root/cloud/portal" && npm ci --no-audit --no-fund && HITOP_COMMIT="$(git -C "$root" rev-parse --short=12 HEAD 2>/dev/null || echo main)" npm run build)
mkdir -p "$out/www/cloud"
cp -r "$root/cloud/portal/dist/." "$out/www/cloud/"

(cd "$root/server" && "$cargo" test --release && "$cargo" build --release)
cp "$root/server/target/release/hitop-server" "$out/hitop-server"

echo "built: $out/www ($(find "$out/www" -type f | wc -l) files) and $out/hitop-server"
