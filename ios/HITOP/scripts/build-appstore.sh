#!/bin/bash
# App Store build of HITOP for iOS 26 (owner's Mac): archive, check, export an .ipa for Transporter
# or upload it straight to App Store Connect. Owner steps: ios/APP_STORE.md.
#
#   ios/HITOP/scripts/build-appstore.sh <build-number>            # → ios/HITOP/build/export/HITOP.ipa
#   ios/HITOP/scripts/build-appstore.sh <build-number> --upload   # uploads to App Store Connect instead
#   ios/HITOP/scripts/build-appstore.sh <build-number> --unsigned # CI: archive without signing, checks, no export
#
# CI (.github/workflows/ios-testflight.yml): with ASC_KEY_PATH (the .p8), ASC_KEY_ID and
# ASC_ISSUER_ID set, Xcode signs in to App Store Connect with that API key (role Admin) instead of
# an Apple ID: it makes the certificates (cloud-managed) and profiles itself, and uploads with it.
#
# build-number: CFBundleVersion, higher than every earlier upload of version 2.0.0 (failed uploads use
#   theirs up too), e.g. 2, 3, …  The version (2.0.0) is MARKETING_VERSION in ios/HITOP/project.yml.
# Needs: Xcode 26 (xcode-select), rustup's Rust (not Homebrew's; this adds the two iOS targets),
#   XcodeGen (brew install xcodegen), Xcode signed in to the team's Apple ID (Settings → Accounts):
#   automatic signing makes the distribution certificate and the App Store profiles for the app and
#   the widget extension (-allowProvisioningUpdates). The app group group.hitop.rws8.tech.HiTop is
#   registered (2026-11-19).
set -euo pipefail
BUILD="${1:?build number, e.g. 2}"
MODE="${2:-export}"
case "$BUILD" in *[!0-9.]*|"") echo "build number must be digits and dots, e.g. 2" >&2; exit 2 ;; esac
case "$MODE" in export|--upload|--unsigned) ;; *) echo "unknown option $MODE (--upload or --unsigned)" >&2; exit 2 ;; esac
TEAM=XD7JHZ5DB3
auth=()
if [ -n "${ASC_KEY_PATH:-}" ]; then
  : "${ASC_KEY_ID:?ASC_KEY_ID with ASC_KEY_PATH}" "${ASC_ISSUER_ID:?ASC_ISSUER_ID with ASC_KEY_PATH}"
  auth=(-authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID")
fi
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

for tool in xcodebuild xcodegen cargo rustup; do
  command -v "$tool" >/dev/null || { echo "missing $tool (see ios/APP_STORE.md, one-time setup)" >&2; exit 1; }
done
xcodebuild -version | head -1 | grep -q '^Xcode 2[6-9]' || { echo "needs Xcode 26 or later: $(xcodebuild -version | head -1)" >&2; exit 1; }
VERSION=$(sed -n 's/^ *MARKETING_VERSION: *"\{0,1\}\([0-9.]*\)"\{0,1\}.*/\1/p' project.yml | head -1)
test -n "$VERSION" || { echo "no MARKETING_VERSION in project.yml" >&2; exit 1; }

# The Rust library (SyncFFI: sync plan, end-to-end crypto, gRPC), then the project.
scripts/build-sync-ffi.sh xcframework >/dev/null
xcodegen generate --quiet

rm -rf build/HITOP.xcarchive build/export
mkdir -p build
if [ "$MODE" = --unsigned ]; then
  signing=(CODE_SIGNING_ALLOWED=NO)
else
  signing=(-allowProvisioningUpdates ${auth[@]+"${auth[@]}"} CODE_SIGN_STYLE=Automatic DEVELOPMENT_TEAM=$TEAM)
fi
xcodebuild archive -project HITOPApp.xcodeproj -scheme HITOP -configuration Release \
  -destination generic/platform=iOS -archivePath build/HITOP.xcarchive \
  CURRENT_PROJECT_VERSION="$BUILD" "${signing[@]}" 2>&1 | tee build/archive.log \
  | grep -E 'error:|warning: .*(HITOP|Widgets)|ARCHIVE (SUCCEEDED|FAILED)' || true
grep -q 'ARCHIVE SUCCEEDED' build/archive.log || { echo "archive failed: see ios/HITOP/build/archive.log" >&2; exit 1; }

APP=build/HITOP.xcarchive/Products/Applications/HITOP.app
EXT="$APP/PlugIns/HITOPWidgets.appex"
plist() { /usr/libexec/PlistBuddy -c "Print :$2" "$1/Info.plist"; }
for b in "$APP" "$EXT"; do
  test -d "$b" || { echo "missing $b" >&2; exit 1; }
  test "$(plist "$b" CFBundleShortVersionString)" = "$VERSION" || { echo "$b: version $(plist "$b" CFBundleShortVersionString), want $VERSION" >&2; exit 1; }
  test "$(plist "$b" CFBundleVersion)" = "$BUILD" || { echo "$b: build $(plist "$b" CFBundleVersion), want $BUILD" >&2; exit 1; }
done
test "$(plist "$APP" CFBundleIdentifier)" = hitop.rws8.tech.HiTop || { echo "wrong bundle id: $(plist "$APP" CFBundleIdentifier)" >&2; exit 1; }
test "$(plist "$APP" MinimumOSVersion)" = 26.0 || { echo "MinimumOSVersion $(plist "$APP" MinimumOSVersion), want 26.0" >&2; exit 1; }
lipo -archs "$APP/HITOP" | grep -qx arm64 || { echo "HITOP is not arm64 only: $(lipo -archs "$APP/HITOP")" >&2; exit 1; }
scripts/check-store-app.sh "$APP"

if [ "$MODE" = --unsigned ]; then
  echo "Unsigned archive checked: HITOP $VERSION ($BUILD), ios/HITOP/$APP. No export (CI)."
  exit 0
fi

# Signed with the app group (widgets, intents) in both targets.
for b in "$APP" "$EXT"; do
  codesign -d --entitlements - "$b" 2>/dev/null | grep -q group.hitop.rws8.tech.HiTop \
    || { echo "$b is not signed with the app group group.hitop.rws8.tech.HiTop" >&2; exit 1; }
done

dest=export
[ "$MODE" = --upload ] && dest=upload
cat > build/ExportOptions.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>method</key><string>app-store-connect</string>
	<key>destination</key><string>$dest</string>
	<key>teamID</key><string>$TEAM</string>
	<key>signingStyle</key><string>automatic</string>
	<key>uploadSymbols</key><true/>
	<key>manageAppVersionAndBuildNumber</key><false/>
</dict>
</plist>
EOF
xcodebuild -exportArchive -archivePath build/HITOP.xcarchive -exportPath build/export \
  -exportOptionsPlist build/ExportOptions.plist -allowProvisioningUpdates ${auth[@]+"${auth[@]}"} 2>&1 | tee build/export.log \
  | grep -E 'error:|EXPORT (SUCCEEDED|FAILED)|Upload' || true
grep -q 'EXPORT SUCCEEDED' build/export.log || { echo "export failed: see ios/HITOP/build/export.log" >&2; exit 1; }

if [ "$dest" = upload ]; then
  echo "HITOP $VERSION ($BUILD) uploaded. It shows in App Store Connect → TestFlight after processing (minutes)."
else
  ls -la build/export/*.ipa
  echo "ios/HITOP/build/export/HITOP.ipa ready: HITOP $VERSION ($BUILD). Drag it into Transporter and Deliver."
fi
