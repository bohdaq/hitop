#!/bin/sh
# Google Play needs native libraries aligned for 16 KB pages (Android 15+ devices with 16 KB pages).
#   scripts/check-16k.sh lib.so...      every LOAD segment of each library aligned to >= 16 KB
#   scripts/check-16k.sh --apk app.apk  the .so files inside the APK stored 16 KB aligned (zipalign -P 16)
set -eu
sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
if [ "${1:-}" = "--apk" ]; then
  zipalign=$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)/zipalign
  "$zipalign" -c -P 16 -v 4 "$2" | grep -E '\.so|Verification' || true
  "$zipalign" -c -P 16 4 "$2"
  echo "16 KB aligned in the APK: $2"
  exit 0
fi
ndk=${ANDROID_NDK_HOME:-$(ls -d "$sdk"/ndk/* | sort -V | tail -1)}
readelf=$(ls "$ndk"/toolchains/llvm/prebuilt/*/bin/llvm-readelf | head -1)
status=0
for lib in "$@"; do
  # Alignment column of every LOAD segment, as 0x... hex.
  for align in $("$readelf" -lW "$lib" | awk '$1 == "LOAD" { print $NF }'); do
    if [ $((align)) -lt 16384 ]; then
      echo "$lib: LOAD segment aligned to $align (< 16 KB)" >&2
      status=1
    fi
  done
  [ $status -eq 0 ] && echo "16 KB aligned: $lib"
done
exit $status
