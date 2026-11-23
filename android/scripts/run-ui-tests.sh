#!/bin/sh
# Runs the instrumented UI tests on the one connected emulator (CI job ui-tests). Waits until the
# system services the install and the tests need answer (an emulator reports booted before
# `settings` and `package` do), then fails unless tests really ran: AGP has reported success with
# zero tests after an install the device refused.
set -eu
cd "$(dirname "$0")/.."
adb wait-for-device
i=0
until [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] \
  && adb shell settings get global device_name >/dev/null 2>&1 \
  && adb shell pm path android >/dev/null 2>&1 \
  && adb shell cmd input -h >/dev/null 2>&1; do
  i=$((i + 1))
  [ $i -gt 180 ] && { echo "the emulator's services didn't start" >&2; exit 2; }
  sleep 2
done
adb shell input keyevent 82 || true
./gradlew :app:connectedDebugAndroidTest "$@"
results=$(find app/build/outputs/androidTest-results/connected -name 'TEST-*.xml')
[ -n "$results" ] || { echo "no test results: nothing ran" >&2; exit 1; }
python3 - $results <<'PY'
import sys, xml.etree.ElementTree as ET
tests = failures = 0
for f in sys.argv[1:]:
    r = ET.parse(f).getroot()
    for s in ([r] if r.tag == "testsuite" else r.iter("testsuite")):
        tests += int(s.get("tests", 0)); failures += int(s.get("failures", 0)) + int(s.get("errors", 0))
print(f"{tests} UI tests ran, {failures} failed")
sys.exit(1 if tests == 0 or failures else 0)
PY
