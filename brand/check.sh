#!/bin/sh
# CI check (job "brand"): every icon is the render brand/export.sh makes from brand/logo.svg, byte
# for byte, and no file shows the product under an old name. See brand/BRAND.md.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

"$root/brand/export.sh" --check

# Old app names (exact, case-sensitive). ROADMAP.md and BRAND.md record them as history.
old_names='HITOP - HTTP Client
HITOP - HTTP API Testing Tool
HITOP — HTTP API Testing Tool
HITOP - API Tester
HITOP API Tester
HITOP HTTP Client
<string>HiTop</string>
<string>hitop</string>
navigationTitle("HiTop")
navigationTitle("hitop")
HiTop Collections
"hitop runs"
"Run hitop collection"'
pattern_file=$(mktemp)
trap 'rm -f "$pattern_file"' EXIT
printf '%s\n' "$old_names" > "$pattern_file"
if git grep -n -I -F -f "$pattern_file" -- . ':!ROADMAP.md' ':!brand/BRAND.md' ':!brand/check.sh'; then
  echo "Old product names above; the app is called HITOP (brand/BRAND.md)." >&2
  exit 1
fi
echo "no old product names"
