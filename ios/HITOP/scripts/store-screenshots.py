"""CI (ios.yml job store-screenshots): copies StoreScreenshotTests' attachments out of an
`xcresulttool export attachments` folder as <out>/iphone/NN-name.png and <out>/ipad/NN-name.png
(attachment "iphone-01-collections" → iphone/01-collections.png)."""
import json
import os
import shutil
import sys

src, dst = sys.argv[1:]
for test in json.load(open(os.path.join(src, "manifest.json"))):
    for a in test.get("attachments", []):
        name = (a.get("suggestedHumanReadableName") or "").split("_0_")[0].rsplit(".", 1)[0]
        device, _, rest = name.partition("-")
        if device in ("iphone", "ipad") and rest[:2].isdigit():
            os.makedirs(os.path.join(dst, device), exist_ok=True)
            shutil.copy(os.path.join(src, a["exportedFileName"]), os.path.join(dst, device, rest + ".png"))
