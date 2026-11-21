#!/bin/bash
# The HITOP Chrome Web Store package: the web app (webapp/, npm run build:extension) packaged by
# webapp/scripts/package-extension.mjs into chrome-store-package/ (git-ignored) and hitop-chrome-store.zip.
# Nothing is uploaded: the owner submits the zip (or release-extensions.yml does, on an ext-v tag).
set -e
cd "$(dirname "$0")"
(cd webapp && npm run build:extension)
rm -rf chrome-store-package
node webapp/scripts/package-extension.mjs store chrome-store-package
rm -f hitop-chrome-store.zip
(cd chrome-store-package && zip -r ../hitop-chrome-store.zip . -x "*.DS_Store")
echo "📦 Chrome Web Store package: hitop-chrome-store.zip (the web app)"
