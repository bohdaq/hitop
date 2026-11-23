#!/bin/sh
# Builds the release AAB and APK, signed with the upload key, and checks them (check-release.sh).
# Used by android.yml (a throwaway key), android-play.yml and release-android.yml (the owner's
# upload key from secrets, android/PLAY_STORE.md). Needs the NDK, cargo-ndk and BUNDLETOOL.
#   HITOP_UPLOAD_KEYSTORE=<file.jks> HITOP_UPLOAD_KEYSTORE_PASSWORD=... [HITOP_UPLOAD_KEY_ALIAS=upload]
#   [HITOP_UPLOAD_KEY_PASSWORD=...] [HITOP_VERSION_CODE=n] scripts/build-release.sh [--skip-ffi]
# Outputs: app/build/outputs/bundle/release/app-release.aab, app/build/outputs/apk/release/app-release.apk
set -eu
cd "$(dirname "$0")/.."
[ -n "${HITOP_UPLOAD_KEYSTORE:-}" ] && [ -f "$HITOP_UPLOAD_KEYSTORE" ] || { echo "HITOP_UPLOAD_KEYSTORE must name the upload keystore" >&2; exit 2; }
[ -n "${HITOP_UPLOAD_KEYSTORE_PASSWORD:-}" ] || { echo "HITOP_UPLOAD_KEYSTORE_PASSWORD is not set" >&2; exit 2; }
if [ "${1:-}" != "--skip-ffi" ]; then
  scripts/build-ffi.sh android arm64-v8a x86_64
fi
scripts/check-16k.sh build/ffi/jniLibs/*/libhitop_ffi.so
./gradlew --stacktrace :app:bundleRelease :app:assembleRelease
scripts/check-release.sh app/build/outputs/bundle/release/app-release.aab app/build/outputs/apk/release/app-release.apk
