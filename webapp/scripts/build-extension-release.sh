#!/bin/bash
# Builds the released browser extensions from the AMO source package, at fixed paths, so an
# addons.mozilla.org reviewer following BUILD.md gets the same bytes: rustc writes source paths
# (checkout and cargo home) into the WebAssembly, and Vite names files by content hash.
# release-extensions.yml runs it; BUILD.md (scripts/extension-source.mjs) tells reviewers to.
#   webapp/scripts/build-extension-release.sh <source.zip> <out dir>
# Leaves <out dir>/chrome and <out dir>/firefox (package-extension.mjs store|firefox).
set -euo pipefail
src_zip=$(realpath "$1")
out=$(realpath -m "$2")
SRC=/tmp/hitop-ext-src
export CARGO_HOME=/tmp/hitop-ext-cargo
rm -rf "$SRC"
mkdir -p "$SRC" "$CARGO_HOME" "$out"
unzip -q "$src_zip" -d "$SRC"
cd "$SRC/webapp"
npm ci --no-audit --no-fund
npm run build:extension
rm -rf "$out/chrome" "$out/firefox"
node scripts/package-extension.mjs store "$out/chrome"
node scripts/package-extension.mjs firefox "$out/firefox"
