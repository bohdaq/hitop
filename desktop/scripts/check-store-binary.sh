#!/bin/sh
# Store builds (Mac App Store, Microsoft Store) carry no usage ping and no self-update: the desktop crate
# without the `telemetry` and `updater` features, and no update endpoint or key. This greps a built
# binary for the strings those bring in. One list for every caller: CI job `desktop` (the MAS
# configuration on Linux, and the GitHub configuration as the control) and release-msstore.yml (the exe
# inside the MSIX package).
#
#   desktop/scripts/check-store-binary.sh store  <binary> [label]   fails if any string is there
#   desktop/scripts/check-store-binary.sh github <binary> [label]   fails if any is missing (the control)
#
# A store check also needs a string every build has (the bundle id), so a wrong file can't pass.
set -eu
mode=${1:?usage: check-store-binary.sh store|github <binary> [label]}
bin=${2:?usage: check-store-binary.sh store|github <binary> [label]}
label=${3:-$bin}
here=$(cd "$(dirname "$0")" && pwd)
test -s "$bin" || { echo "$label: no such binary: $bin" >&2; exit 1; }

# The updater's public key, as the GitHub configuration embeds it.
pubkey=$(sed -n 's/.*"pubkey": *"\([^"]*\)".*/\1/p' "$here/../src-tauri/tauri.github.conf.json")
test -n "$pubkey" || { echo "no updater pubkey in tauri.github.conf.json" >&2; exit 1; }

# Usage ping (hitop-core telemetry) and self-update (tauri-plugin-updater, its endpoint and key).
ping='hitop.rws8.tech/api/ping
telemetry.json'
store_only='/api/ping
HITOP_TELEMETRY_URL'
updater='tauri-plugin-updater
plugin:updater
minisign
releases/latest/download/latest.json'

has() { grep -q -a -F -e "$1" "$bin"; }

case "$mode" in
  store)
    has 'tech.rws8.hitop' || { echo "$label: not a HITOP desktop binary (no bundle id): nothing was checked" >&2; exit 1; }
    found=0
    printf '%s\n%s\n%s\n%s\n' "$ping" "$store_only" "$updater" "$pubkey" > "${TMPDIR:-/tmp}/hitop-store-strings.$$"
    while IFS= read -r s; do
      if has "$s"; then echo "$label contains '$s' (usage ping or updater in a store build)" >&2; found=1; fi
    done < "${TMPDIR:-/tmp}/hitop-store-strings.$$"
    rm -f "${TMPDIR:-/tmp}/hitop-store-strings.$$"
    test "$found" = 0 || exit 1
    echo "$label: no usage ping or updater code, endpoint or key"
    ;;
  github)
    missing=0
    printf '%s\n%s\n%s\n' "$ping" "$updater" "$pubkey" > "${TMPDIR:-/tmp}/hitop-store-strings.$$"
    while IFS= read -r s; do
      if ! has "$s"; then echo "control: $label lacks '$s'" >&2; missing=1; fi
    done < "${TMPDIR:-/tmp}/hitop-store-strings.$$"
    rm -f "${TMPDIR:-/tmp}/hitop-store-strings.$$"
    test "$missing" = 0 || exit 1
    echo "$label: usage ping, updater, endpoint and key present (control)"
    ;;
  *)
    echo "usage: check-store-binary.sh store|github <binary> [label]" >&2
    exit 2
    ;;
esac
