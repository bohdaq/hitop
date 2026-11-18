#!/bin/sh
# App/Core and its unit tests with SwiftPM, no Xcode (Linux or macOS): a scratch package whose
# targets are symlinks to App/Core (module HITOP) and UnitTests (HITOPTests). SwiftPM won't take
# target paths outside a package, hence the scratch folder. App/Core links SyncFFI (Rust, module
# CHitopSync): built here first for this machine with rustup's cargo (scripts/build-sync-ffi.sh host).
#
#   ios/HITOP/scripts/core-tests.sh [swift test arguments]
#   HITOP_SYNC_SERVER_BIN=sync-server/target/debug/hitop-sync-server ios/HITOP/scripts/core-tests.sh --filter CloudEndToEndTests
#
# HITOP_CORE_PKG: the scratch folder to use (kept, so builds are incremental); default a temp
# folder, removed afterwards. HITOP_SYNC_FFI_LIB: a folder that already has libhitop_sync_ffi.a
# (skips the cargo build). HITOP_SYNC_VECTORS (default cli/crates/hitop-sync-crypto/vectors) is
# where the crypto tests read the published test vectors.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
kit=$(cd "$here/../HitopKit" && pwd)
repo=$(cd "$here/../.." && pwd)
work=${HITOP_CORE_PKG:-}
cleanup=0
if [ -z "$work" ]; then
  work=$(mktemp -d)
  cleanup=1
fi
lib=${HITOP_SYNC_FFI_LIB:-}
if [ -z "$lib" ]; then
  lib=$("$here/scripts/build-sync-ffi.sh" host)
fi
export HITOP_SYNC_VECTORS=${HITOP_SYNC_VECTORS:-"$repo/cli/crates/hitop-sync-crypto/vectors"}
mkdir -p "$work/Sources" "$work/Tests" "$work/Sources/CHitopSync"
ln -sfn "$here/App/Core" "$work/Sources/HITOP"
ln -sfn "$here/UnitTests" "$work/Tests/HITOPTests"
cat > "$work/Sources/CHitopSync/module.modulemap" <<EOF
module CHitopSync {
    header "$here/SyncFFI/include/hitop_sync.h"
    link "hitop_sync_ffi"
    export *
}
EOF
cat > "$work/Package.swift" <<EOF
// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "HITOPCore",
    platforms: [.macOS(.v14)],
    dependencies: [.package(path: "$kit")],
    targets: [
        .systemLibrary(name: "CHitopSync", path: "Sources/CHitopSync"),
        .target(name: "HITOP", dependencies: ["CHitopSync", .product(name: "HitopKit", package: "HitopKit")], path: "Sources/HITOP"),
        .testTarget(name: "HITOPTests", dependencies: ["HITOP"], path: "Tests/HITOPTests"),
    ]
)
EOF
status=0
(cd "$work" && swift test -Xlinker -L"$lib" "$@") || status=$?
if [ "$cleanup" = 1 ]; then rm -rf "$work"; fi
exit $status
