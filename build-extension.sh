#!/bin/bash
# The HITOP Firefox extension: the web app (webapp/, npm run build:extension) packaged by
# webapp/scripts/package-extension.mjs into extension/ (git-ignored) and hitop-firefox-extension.zip.
# Load it from about:debugging → This Firefox → Load Temporary Add-on → extension/manifest.json.
set -e
cd "$(dirname "$0")"
(cd webapp && npm run build:extension)
node webapp/scripts/package-extension.mjs firefox extension
rm -f hitop-firefox-extension.zip
(cd extension && zip -r ../hitop-firefox-extension.zip manifest.json background.js icons app -x "*.DS_Store")
echo "📦 Extension package: hitop-firefox-extension.zip (the web app)"
