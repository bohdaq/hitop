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
# jarsigner exits 0 for an unsigned file too: read what it says.
jarsigner -verify "$aab" 2>&1 | grep -q "^jar verified" && echo "the bundle is signed" || { echo "the bundle isn't signed" >&2; exit 1; }
if [ -n "$apk" ]; then
  tools=$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)
  "$tools/apksigner" verify --print-certs "$apk" | head -3
  "$(dirname "$0")/check-16k.sh" --apk "$apk"
fi
