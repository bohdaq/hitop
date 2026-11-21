"""CI (ios.yml job store-screenshots): makes the two App Store screenshot simulators from their
device types on the newest iOS 26 runtime and prints STORE_PHONE_ID/NAME, STORE_PAD_ID/NAME for
$GITHUB_ENV. Fails, listing what there is, when no 6.9" iPhone or 13" iPad type exists.

App Store Connect sizes: iPhone 6.9" 1320 x 2868 (Pro Max 16/17) or 1290 x 2796; iPad 13"
2064 x 2752 (Pro M4/M5) or 2048 x 2732 (Air 13-inch)."""
import json
import subprocess
import sys


def simctl(*args):
    return json.loads(subprocess.check_output(["xcrun", "simctl", "list", *args, "-j"]))


runtimes = [r for r in simctl("runtimes")["runtimes"]
            if r["identifier"].startswith("com.apple.CoreSimulator.SimRuntime.iOS-26") and r.get("isAvailable")]
if not runtimes:
    sys.exit("no iOS 26 simulator runtime")
runtimes.sort(key=lambda r: [int(x) for x in r["version"].split(".")])
runtime = runtimes[-1]
types = {t["name"]: t["identifier"] for t in simctl("devicetypes")["devicetypes"]}
supported = {t["name"] for t in runtime.get("supportedDeviceTypes", [])} or set(types)


def make(prefs, var):
    for name in prefs:
        if name in types and name in supported:
            udid = subprocess.check_output(
                ["xcrun", "simctl", "create", f"Store {name}", types[name], runtime["identifier"]], text=True).strip()
            print(f"{var}_ID={udid}")
            print(f"{var}_NAME={name}")
            return
    sys.exit(f"no device type among {prefs} for {runtime['name']}; there are: {sorted(supported)}")


make(["iPhone 17 Pro Max", "iPhone 16 Pro Max"], "STORE_PHONE")
make(["iPad Pro 13-inch (M5)", "iPad Pro 13-inch (M4)", "iPad Air 13-inch (M3)", "iPad Air 13-inch (M2)"], "STORE_PAD")
print(f"STORE_RUNTIME={runtime['name']}")
