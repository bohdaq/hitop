#!/bin/sh
# Builds ios/HITOP/SyncFFI (Rust: hitop-core's sync plan and folder format + hitop-sync-crypto
# behind one C function, module CHitopSync) for the app or for the SwiftPM tests.
#
#   ios/HITOP/scripts/build-sync-ffi.sh xcframework   # macOS: Vendor/HitopSyncFFI.xcframework (device + simulator)
#   ios/HITOP/scripts/build-sync-ffi.sh host          # this machine: prints the folder with libhitop_sync_ffi.a
#
# Needs rustup's cargo (cli/rust-toolchain.toml's stable). The xcframework is what project.yml
# links: run this once before `xcodegen generate`, and again after a change to SyncFFI,
# hitop-core or hitop-sync-crypto. SIM_ARCHS="arm64 x86_64" adds an Intel simulator slice.
# CARGO_TARGET_DIR is honoured (CI keeps it out of the tree). SYNC_FFI_CARGO_FLAGS adds cargo flags
# (CI measures the app without gRPC with "--no-default-features"). The xcframework holds the libraries
# only: the module (SyncFFI/include/module.modulemap) comes in through SWIFT_INCLUDE_PATHS, so
# the app and its unit tests see one definition of CHitopSync.
set -eu
# The app's minimum (project.yml deploymentTarget): without it rustc builds for the SDK's own iOS
# version (27.2 on a newer Xcode) and the library wouldn't load on iOS 26.
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-26.0}"
here=$(cd "$(dirname "$0")/.." && pwd)
crate="$here/SyncFFI"
cargo=${CARGO:-cargo}
command -v "$cargo" >/dev/null 2>&1 || cargo="$HOME/.cargo/bin/cargo"
target_dir=${CARGO_TARGET_DIR:-"$crate/target"}
mode=${1:-xcframework}

build() {
  # $1: target triple, or "" for this machine
  if [ -n "$1" ]; then
    (cd "$crate" && "$cargo" build --locked --release ${SYNC_FFI_CARGO_FLAGS:-} --target "$1" --target-dir "$target_dir") >&2
  else
    (cd "$crate" && "$cargo" build --locked --release ${SYNC_FFI_CARGO_FLAGS:-} --target-dir "$target_dir") >&2
  fi
}

case "$mode" in
host)
  build ""
  echo "$target_dir/release"
  ;;
xcframework)
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim >&2
  build aarch64-apple-ios
  sims=""
  for arch in ${SIM_ARCHS:-arm64}; do
    case "$arch" in
    arm64) t=aarch64-apple-ios-sim ;;
    x86_64) t=x86_64-apple-ios; rustup target add "$t" >&2 ;;
    *) echo "unknown simulator arch $arch" >&2; exit 2 ;;
    esac
    build "$t"
    sims="$sims $target_dir/$t/release/libhitop_sync_ffi.a"
  done
  sim_lib="$target_dir/sim/libhitop_sync_ffi.a"
  mkdir -p "$target_dir/sim"
  # shellcheck disable=SC2086
  lipo -create $sims -output "$sim_lib"
  out="$here/Vendor/HitopSyncFFI.xcframework"
  rm -rf "$out"
  mkdir -p "$here/Vendor"
  xcodebuild -create-xcframework \
    -library "$target_dir/aarch64-apple-ios/release/libhitop_sync_ffi.a" \
    -library "$sim_lib" \
    -output "$out" >&2
  echo "$out"
  ;;
*)
  echo "usage: $0 xcframework|host" >&2
  exit 2
  ;;
esac
