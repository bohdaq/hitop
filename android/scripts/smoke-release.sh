#!/bin/sh
# Starts the minified release build (signed with the debug key) on the running emulator: it must
# draw, load hitop-ffi (an import goes through it) and not crash. Needs :app:assembleSmoke.
set -eu
cd "$(dirname "$0")/.."
apk=app/build/outputs/apk/smoke/app-smoke.apk
pkg=tech.rws8.hitop
adb uninstall $pkg >/dev/null 2>&1 || true
adb install -r "$apk"
adb logcat -c || true
# wait_for TEXT: until the screen shows it.
wait_for() {
  i=0
  while [ $i -lt 60 ]; do
    adb exec-out uiautomator dump /dev/tty 2>/dev/null | grep -q "$1" && return 0
    i=$((i + 1)); sleep 2
  done
  echo "the release build never showed: $1" >&2
  adb logcat -d -t 400 '*:E' >&2 || true
  return 1
}
adb shell am start -W -n $pkg/.MainActivity
wait_for "Collections"
adb shell am start -W -n $pkg/.MainActivity -a android.intent.action.SEND -t text/plain --es android.intent.extra.TEXT "'[{\"id\":1,\"name\":\"SmokeShop\",\"requests\":[]}]'"
wait_for "SmokeShop"
if adb logcat -d | grep -E "FATAL EXCEPTION|UnsatisfiedLinkError|NoSuchMethodError|ClassNotFoundException" | grep -v "^$" >/dev/null; then
  if adb logcat -d | grep -A12 "FATAL EXCEPTION" | grep -q "$pkg"; then
    adb logcat -d | grep -A30 "FATAL EXCEPTION" >&2
    exit 1
  fi
fi
adb uninstall $pkg >/dev/null 2>&1 || true
echo "the release build starts and runs"
