#!/usr/bin/env bash
# Mac App Store build + signed .pkg for Transporter (owner's Mac). Proven 2026-11-18 (1.3.0 build 1.3.1).
# Usage: desktop/scripts/build-mas.sh <build-number> [path/to/profile.provisionprofile]
#   build-number: CFBundleVersion, must be higher than every earlier upload (even failed ones), e.g. 1.3.2
# Needs: rustup (not Homebrew rust) with aarch64/x86_64-apple-darwin + wasm32-unknown-unknown targets,
#   wasm-bindgen-cli 0.2.126, `npm ci` in webapp/ and desktop/, the "3rd Party Mac Developer Application"
#   and "... Installer" certificates, and the Mac App Store profile for tech.rws8.hitop.
set -euo pipefail
BUILD="${1:?build number, e.g. 1.3.2}"
PROFILE="${2:-$HOME/Downloads/HITOP_Provisioning_Profile.provisionprofile}"
TEAM="Bogdan Tsap (XD7JHZ5DB3)"
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
# Own target dir: an editor's rust-analyzer writing src-tauri/target with another toolchain breaks proc macros.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/hitop-mas-target}"
npx tauri build --target universal-apple-darwin --config "{\"bundle\":{\"macOS\":{\"bundleVersion\":\"$BUILD\"}}}"
APP="$CARGO_TARGET_DIR/universal-apple-darwin/release/bundle/macos/HITOP.app"
scripts/check-store-binary.sh store "$APP/Contents/MacOS/"*
cp "$PROFILE" "$APP/Contents/embedded.provisionprofile"
xattr -cr "$APP"   # downloaded files carry com.apple.quarantine: App Store processing refuses it (91109)
codesign --force --sign "3rd Party Mac Developer Application: $TEAM" --entitlements src-tauri/Entitlements.mas.plist "$APP"
codesign -d --entitlements - "$APP" 2>/dev/null | grep -q "XD7JHZ5DB3.tech.rws8.hitop"
lipo -archs "$APP/Contents/MacOS/"*
rm -f HITOP.pkg
productbuild --component "$APP" /Applications --sign "3rd Party Mac Developer Installer: $TEAM" HITOP.pkg
echo "HITOP.pkg ready: $(defaults read "$APP/Contents/Info.plist" CFBundleShortVersionString) ($BUILD). Drag it into Transporter."
