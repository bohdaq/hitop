#!/bin/bash
# Store-mode check of a built HITOP.app (iOS), the iOS side of desktop/scripts/check-store-binary.sh:
# the app sells nothing (owner 2026-11-17, launch/app-store-free.md), so neither the app nor its
# widget extension may link StoreKit or carry purchase wording.
#
#   ios/HITOP/scripts/check-store-app.sh path/to/HITOP.app
#
# A backstop, not the guard: optimised Swift keeps string literals of 15 bytes or fewer inside the
# code, not in the string section, so a short word on a button ("Buy", "Upgrade") can't be found
# in the binary. The UI tests are the real guard (they assert no purchase words on the screens,
# RunFlowTests, CloudFlowTests, StoreScreenshotTests). What this checks:
#   - no StoreKit (linked library, or StoreKit / SKPayment symbols)
#   - no purchase phrase longer than 15 bytes
#   - a control string every build has (longer than 15 bytes), so a wrong file can't pass
# It also prints, without failing, where short words such as "Upgrade" or "Pricing" occur, for a
# person to read (the Rust sync library has HTTP "upgrade" strings; the in-app test double
# FakeCloud has the sync server's own 402 texts, which the app strips before showing).
set -euo pipefail
app=${1:?usage: check-store-app.sh path/to/HITOP.app}
test -d "$app" || { echo "no such app: $app" >&2; exit 1; }

bins="$app/HITOP"
for ext in "$app"/PlugIns/*.appex; do
  [ -d "$ext" ] || continue
  bins="$bins $ext/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$ext/Info.plist")"
done

# Purchase phrases (all longer than 15 bytes, so a literal would be in the string section).
phrases='In-App Purchase
Restore Purchases
Start free trial
Start your free trial
Upgrade to Team
Upgrade to Business
Upgrade your plan
Choose a plan to
per seat per month
Buy HITOP Cloud
billing portal
/checkout/session
apps.apple.com/account/subscriptions'

found=0
for bin in $bins; do
  test -s "$bin" || { echo "missing binary: $bin" >&2; exit 1; }
  name=$(basename "$bin")
  if otool -L "$bin" | grep -q -i 'storekit'; then
    echo "$name links StoreKit:" >&2; otool -L "$bin" | grep -i storekit >&2; found=1
  fi
  if nm -u "$bin" 2>/dev/null | grep -q -E 'SKPayment|SKProduct|8StoreKit'; then
    echo "$name uses StoreKit symbols:" >&2; nm -u "$bin" | grep -E 'SKPayment|SKProduct|8StoreKit' | head -5 >&2; found=1
  fi
  while IFS= read -r p; do
    [ -n "$p" ] || continue
    if grep -q -a -F -i -e "$p" "$bin"; then echo "$name contains '$p'" >&2; found=1; fi
  done <<EOF
$phrases
EOF
  echo "-- $name: short words, for reading (not a failure):"
  strings -a "$bin" | grep -E 'Upgrade|Purchase|Subscribe|Pricing|Checkout|Billing|[Tt]rial' | cut -c1-120 | sort -u | head -20 | sed 's/^/   /' || true
done

# Control: the default sync server address is a literal in the app (Core/Cloud.swift).
grep -q -a -F 'https://sync.hitop.rws8.tech' "$app/HITOP" \
  || { echo "$app/HITOP lacks 'https://sync.hitop.rws8.tech': not the HITOP app, nothing was checked" >&2; exit 1; }

test "$found" = 0 || exit 1
echo "$app: no StoreKit, no purchase phrase (app + extensions)"
