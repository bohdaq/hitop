#!/bin/bash

# Build script for HITOP Firefox Extension
# Since HITOP Next A8 the extension is the web app (webapp/, npm run build:extension); the React
# UI (frontend/) stays buildable with HITOP_EXTENSION_UI=react until the store listings switch.
if [ "${HITOP_EXTENSION_UI:-web}" != "react" ]; then
  set -e
  (cd webapp && npm run build:extension)
  node webapp/scripts/package-extension.mjs firefox extension
  rm -f hitop-firefox-extension.zip
  (cd extension && zip -r ../hitop-firefox-extension.zip manifest.json background.js icons app -x "*.DS_Store")
  echo "📦 Extension package: hitop-firefox-extension.zip (the web app)"
  exit 0
fi

echo "🚀 Building HITOP Firefox Extension..."

# Step 1: Build React app
echo "📦 Building React application..."
cd frontend
npm run build
cd ..

# Step 2: Create extension directory structure
echo "📁 Creating extension directory..."
rm -rf extension/app
mkdir -p extension/app
mkdir -p extension/icons

# Step 3: Copy React build to extension
echo "📋 Copying React build files..."
cp -r frontend/build/* extension/app/

# Step 4: Copy and resize icons
echo "🎨 Copying icons..."
# Renders of brand/logo.svg (brand/export.sh); icon-96.png has always been 192 x 192
cp brand/png/logo-48.png extension/icons/icon-48.png
cp brand/png/logo-192.png extension/icons/icon-96.png

# Step 5: Update manifest for extension compatibility
echo "🔧 Updating manifest..."
# Remove React's manifest.json to avoid conflict with extension manifest
if [ -f "extension/app/manifest.json" ]; then
  rm extension/app/manifest.json
  echo "✓ Removed conflicting React manifest.json"
fi

# Step 6: Create zip package
echo "📦 Creating extension package..."
cd extension
zip -r ../hitop-firefox-extension.zip * -x "*.DS_Store"
cd ..

echo "✅ Build complete!"
echo "📦 Extension package: hitop-firefox-extension.zip"
echo ""
echo "To install in Firefox:"
echo "1. Open Firefox and go to about:debugging"
echo "2. Click 'This Firefox'"
echo "3. Click 'Load Temporary Add-on'"
echo "4. Select the manifest.json file from the extension directory"
echo ""
echo "Or install the packaged extension:"
echo "1. Go to about:addons"
echo "2. Click the gear icon"
echo "3. Select 'Install Add-on From File'"
echo "4. Select hitop-firefox-extension.zip"
