#!/bin/sh
# App/Core and its unit tests with SwiftPM, no Xcode (Linux or macOS): a scratch package whose
# targets are symlinks to App/Core (module HITOP) and UnitTests (HITOPTests). SwiftPM won't take
# target paths outside a package, hence the scratch folder.
#
#   ios/HITOP/scripts/core-tests.sh [swift test arguments]
#   HITOP_SYNC_SERVER_BIN=sync-server/target/debug/hitop-sync-server ios/HITOP/scripts/core-tests.sh --filter CloudEndToEndTests
#
# HITOP_CORE_PKG: the scratch folder to use (kept, so builds are incremental); default a temp
# folder, removed afterwards.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
kit=$(cd "$here/../HitopKit" && pwd)
work=${HITOP_CORE_PKG:-}
cleanup=0
if [ -z "$work" ]; then
  work=$(mktemp -d)
  cleanup=1
fi
mkdir -p "$work/Sources" "$work/Tests"
ln -sfn "$here/App/Core" "$work/Sources/HITOP"
ln -sfn "$here/UnitTests" "$work/Tests/HITOPTests"
cat > "$work/Package.swift" <<EOF
// swift-tools-version:6.0
import PackageDescription

let package = Package(
    name: "HITOPCore",
    platforms: [.macOS(.v14)],
    dependencies: [.package(path: "$kit")],
    targets: [
        .target(name: "HITOP", dependencies: [.product(name: "HitopKit", package: "HitopKit")], path: "Sources/HITOP"),
        .testTarget(name: "HITOPTests", dependencies: ["HITOP"], path: "Tests/HITOPTests"),
    ]
)
EOF
status=0
(cd "$work" && swift test "$@") || status=$?
if [ "$cleanup" = 1 ]; then rm -rf "$work"; fi
exit $status
