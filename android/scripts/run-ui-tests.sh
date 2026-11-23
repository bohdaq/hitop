#!/bin/sh
# Runs the instrumented UI tests on the one connected emulator (CI job ui-tests). Waits until the
# system services the install and the tests need answer (an emulator reports booted before
# `settings` and `package` do), then fails unless tests really ran: AGP has reported success with
# zero tests after an install the device refused.
set -eu
cd "$(dirname "$0")/.."
adb wait-for-device
ready() {
  # Also the user's credential-encrypted storage (/data/user/0): apps can't write files before it.
  [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ] \
    && [ "$(adb shell getprop sys.user.0.ce_available 2>/dev/null | tr -d '\r')" = true ] \
    && adb shell settings get global device_name >/dev/null 2>&1 \
    && adb shell pm path android >/dev/null 2>&1 \
    && adb shell cmd input -h >/dev/null 2>&1
}
# Ready five checks in a row (~10 s): a first boot may restart the runtime after reporting booted.
i=0
ok=0
while [ $ok -lt 5 ]; do
  if ready; then ok=$((ok + 1)); else ok=0; fi
  i=$((i + 1))
  [ $i -gt 240 ] && { echo "the emulator's services didn't start" >&2; exit 2; }
  sleep 2
done
adb shell input keyevent 82 || true
status=0
./gradlew :app:connectedDebugAndroidTest "$@" || status=$?
python3 - <<'PY'
import glob, sys, xml.etree.ElementTree as ET
tests = failures = 0
files = glob.glob("app/build/outputs/androidTest-results/connected/**/TEST-*.xml", recursive=True)
if not files:
    print("no test results: nothing ran (the emulator refused the install)", file=sys.stderr)
    sys.exit(2)
for f in files:
    r = ET.parse(f).getroot()
    for s in ([r] if r.tag == "testsuite" else r.iter("testsuite")):
        tests += int(s.get("tests", 0)); failures += int(s.get("failures", 0)) + int(s.get("errors", 0))
print(f"{tests} UI tests ran, {failures} failed")
# Nothing ran = the emulator, not the tests (exit 2: the workflow tries a fresh emulator).
sys.exit(2 if tests == 0 else 1 if failures else 0)
PY
exit $status
