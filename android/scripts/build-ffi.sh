#!/bin/sh
# Builds android/hitop-ffi (android/GOAL.md):
#   scripts/build-ffi.sh android [abi...]   cargo-ndk → android/build/ffi/jniLibs/<abi>/libhitop_ffi.so
#                                           (default abis: arm64-v8a x86_64; the app's jniLibs)
#   scripts/build-ffi.sh host               → android/build/ffi/host/libhitop_ffi.so (core's JVM tests)
# Needs rustup's toolchain (cli/rust-toolchain.toml), the Android targets and cargo-ndk for
# `android`; ANDROID_NDK_HOME or ANDROID_HOME/ndk/<version>. CARGO_TARGET_DIR is honoured (one
# shared target dir keeps the disk use down). PROFILE=debug for a faster, larger build.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
crate="$here/hitop-ffi"
out="$here/build/ffi"
profile=${PROFILE:-release}
flag=--release
[ "$profile" = debug ] && flag=
export PATH="$HOME/.cargo/bin:$PATH"
target_dir=${CARGO_TARGET_DIR:-$crate/target}
mode=${1:-android}
[ $# -gt 0 ] && shift
case "$mode" in
  android)
    abis=${*:-arm64-v8a x86_64}
    if [ -z "${ANDROID_NDK_HOME:-}" ]; then
      sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
      ANDROID_NDK_HOME=$(ls -d "$sdk"/ndk/* | sort -V | tail -1)
      export ANDROID_NDK_HOME
    fi
    set --
    for abi in $abis; do set -- "$@" -t "$abi"; done
    # Android 12 (API 31) is the minimum; 16 KB pages come from NDK r28's default (CI checks).
    (cd "$crate" && cargo ndk --platform 31 "$@" build --locked $flag)
    for abi in $abis; do
      case "$abi" in
        arm64-v8a) triple=aarch64-linux-android ;;
        x86_64) triple=x86_64-linux-android ;;
        *) echo "unknown abi $abi" >&2; exit 2 ;;
      esac
      mkdir -p "$out/jniLibs/$abi"
      # Only the one library: cargo-ndk's -o would also copy dependencies' own .so files.
      cp "$target_dir/$triple/$profile/libhitop_ffi.so" "$out/jniLibs/$abi/"
    done
    ;;
  host)
    (cd "$crate" && cargo build --locked $flag)
    mkdir -p "$out/host"
    cp "$target_dir/$profile/libhitop_ffi.so" "$out/host/" 2>/dev/null || cp "$target_dir/$profile/libhitop_ffi.dylib" "$out/host/"
    ;;
  *) echo "usage: $0 android [abi...] | host" >&2; exit 2 ;;
esac
