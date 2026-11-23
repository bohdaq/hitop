#!/bin/sh
# Checks a release build (CI job release-build, android-play.yml, release-android.yml):
#   scripts/check-release.sh <app.aab> [app.apk]
# bundletool validate (and the AAB holds both ABIs' libhitop_ffi.so), the APK's signature
# (apksigner verify) and 16 KB alignment. BUNDLETOOL = the bundletool jar.
set -eu
aab=$1
apk=${2:-}
sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
bt=${BUNDLETOOL:?set BUNDLETOOL to the bundletool jar}
java -jar "$bt" validate --bundle="$aab" > /dev/null
echo "bundletool validate: ok ($aab)"
for abi in arm64-v8a x86_64; do
  unzip -l "$aab" | grep -q "base/lib/$abi/libhitop_ffi.so" || { echo "the bundle has no $abi library" >&2; exit 1; }
done
echo "both ABIs in the bundle"
# No usage ping in the app (privacy policy, Data safety): hitop-core's telemetry endpoint is in the CLI, never here.
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
unzip -q -o "$aab" 'base/lib/*' -d "$tmp"
if grep -rlq "api/ping" "$tmp"; then echo "the bundle's native code holds the usage ping endpoint" >&2; exit 1; fi
echo "no usage ping in the native code"
# jarsigner exits 0 for an unsigned file too: read what it says.
jarsigner -verify "$aab" 2>&1 | grep -q "^jar verified" && echo "the bundle is signed" || { echo "the bundle isn't signed" >&2; exit 1; }
if [ -n "$apk" ]; then
  tools=$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)
  "$tools/apksigner" verify --print-certs "$apk" | head -3
  "$(dirname "$0")/check-16k.sh" --apk "$apk"
fi
