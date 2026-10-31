#!/usr/bin/env bash
# Builds everything hitop.rws8.tech serves into deploy/out/:
#   out/www/          the site (site/) with the web app under /app/ (frontend/)
#   out/hitop-server  the server binary (server/): static files + POST /api/proxy
# Then: deploy/install.sh copies them to the host (see deploy/README.md).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/deploy/out"
cargo="${CARGO:-$HOME/.cargo/bin/cargo}"

rm -rf "$out" && mkdir -p "$out"

(cd "$root/site" && npm ci --no-audit --no-fund && npm run build)
(cd "$root/frontend" && npm ci --no-audit --no-fund && npm run build:web)
cp -r "$root/site/dist" "$out/www"
rm -rf "$out/www/.vite"
mkdir -p "$out/www/app"
cp -r "$root/frontend/build/." "$out/www/app/"

(cd "$root/server" && "$cargo" test --release && "$cargo" build --release)
cp "$root/server/target/release/hitop-server" "$out/hitop-server"

echo "built: $out/www ($(find "$out/www" -type f | wc -l) files) and $out/hitop-server"
