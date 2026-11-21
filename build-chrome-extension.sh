#!/bin/bash
# The HITOP Chrome extension, unpacked: the web app (webapp/, npm run build:extension) packaged by
# webapp/scripts/package-extension.mjs into chrome-extension/ (git-ignored) and hitop-chrome-extension.zip.
# Load it from chrome://extensions → Developer mode → Load unpacked → chrome-extension/.
set -e
cd "$(dirname "$0")"
(cd webapp && npm run build:extension)
node webapp/scripts/package-extension.mjs chrome chrome-extension
rm -f hitop-chrome-extension.zip
(cd chrome-extension && zip -r ../hitop-chrome-extension.zip manifest.json background.js icons app -x "*.DS_Store")
echo "📦 Extension package: hitop-chrome-extension.zip (the web app). Load unpacked: chrome-extension/"
