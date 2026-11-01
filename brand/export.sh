#!/bin/sh
# Regenerates every HITOP icon in the repository from brand/logo.svg (see brand/BRAND.md).
#   brand/export.sh           write them
#   brand/export.sh --check   only compare (what CI runs); exit 1 when a file differs
#   brand/export.sh --tauri   also regenerate desktop/src-tauri/icons (icns, ico, Windows and mobile sets)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
[ -d "$here/node_modules/@resvg/resvg-js" ] || npm --prefix "$here" ci --no-audit --no-fund
node "$here/export.mjs" "$@"
case " $* " in
  *" --tauri "*)
    # Tauri's own generator: every size and format tauri.conf.json and the bundlers need.
    (cd "$here/../desktop" && npx --yes @tauri-apps/cli icon "$here/png/logo-1024.png" -o src-tauri/icons)
    ;;
esac
