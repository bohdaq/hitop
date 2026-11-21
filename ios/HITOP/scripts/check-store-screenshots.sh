#!/bin/bash
# CI (ios.yml job store-screenshots): the App Store set is complete and every file has a size App
# Store Connect takes, without alpha. <dir>/iphone: 6 portrait 6.9" (1320 x 2868 or 1290 x 2796);
# <dir>/ipad: 4 landscape 13" (2752 x 2064 or 2732 x 2048). Prints a Markdown table.
set -euo pipefail
dir=${1:?usage: check-store-screenshots.sh <dir>}
want_iphone='01-collections 02-request 03-response 04-run-result 05-websocket 06-explain'
want_ipad='01-request 02-explain 03-run-result 04-graphql'
bad=0
echo "| File | Size | Alpha |"
echo "| --- | --- | --- |"
for device in iphone ipad; do
  if [ $device = iphone ]; then want=$want_iphone; sizes='1320x2868 1290x2796'; else want=$want_ipad; sizes='2752x2064 2732x2048'; fi
  have=$(cd "$dir/$device" 2>/dev/null && ls -1 *.png 2>/dev/null | sed 's/\.png$//' | tr '\n' ' ' | sed 's/ $//' || true)
  if [ "$have" != "$want" ]; then echo "$device: files '$have', want '$want'" >&2; bad=1; fi
  for f in "$dir/$device"/*.png; do
    [ -f "$f" ] || continue
    w=$(sips -g pixelWidth "$f" | awk '/pixelWidth/ {print $2}')
    h=$(sips -g pixelHeight "$f" | awk '/pixelHeight/ {print $2}')
    alpha=$(sips -g hasAlpha "$f" | awk '/hasAlpha/ {print $2}')
    echo "| $device/$(basename "$f") | ${w}x${h} | $alpha |"
    case " $sizes " in *" ${w}x${h} "*) ;; *) echo "$f: ${w}x${h}, want one of $sizes" >&2; bad=1 ;; esac
    if [ "$alpha" != no ]; then echo "$f has alpha" >&2; bad=1; fi
  done
done
exit $bad
