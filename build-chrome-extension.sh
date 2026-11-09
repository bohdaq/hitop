#!/bin/bash

# Build script for HITOP Chrome Extension
# Since HITOP Next A8 the extension is the web app (webapp/, npm run build:extension); the React
# UI (frontend/) stays buildable with HITOP_EXTENSION_UI=react until the store listings switch.
if [ "${HITOP_EXTENSION_UI:-web}" != "react" ]; then
  set -e
  (cd webapp && npm run build:extension)
  node webapp/scripts/package-extension.mjs chrome chrome-extension
  rm -f hitop-chrome-extension.zip
  (cd chrome-extension && zip -r ../hitop-chrome-extension.zip manifest.json background.js icons app -x "*.DS_Store")
  echo "📦 Extension package: hitop-chrome-extension.zip (the web app). Load unpacked: chrome-extension/"
  exit 0
fi

echo "🚀 Building HITOP Chrome Extension..."

# Step 1: Build React app
echo "📦 Building React application..."
cd frontend
npm run build
cd ..

# Step 2: Create extension directory structure
echo "📁 Creating extension directory..."
rm -rf chrome-extension/app
mkdir -p chrome-extension/app
mkdir -p chrome-extension/icons

# Step 3: Copy React build to extension
echo "📋 Copying React build files..."
cp -r frontend/build/* chrome-extension/app/

# Step 4: Copy and resize icons
echo "🎨 Copying icons..."
# Renders of brand/logo.svg (brand/export.sh)
cp brand/png/logo-16.png chrome-extension/icons/icon-16.png
cp brand/png/logo-48.png chrome-extension/icons/icon-48.png
cp brand/png/logo-128.png chrome-extension/icons/icon-128.png

# Step 5: Copy manifest for Chrome
echo "🔧 Copying Chrome manifest..."
if [ -f "public/manifest_chrome.json" ]; then
  cp public/manifest_chrome.json chrome-extension/manifest.json
else
  echo "⚠️  Warning: manifest_chrome.json not found, creating default..."
  cat > chrome-extension/manifest.json << 'EOF'
{
  "manifest_version": 3,
  "name": "HITOP",
  "version": "4.0.0",
  "description": "The API client your terminal, your CI and your AI agent can all drive.",
  "action": {
    "default_title": "HITOP"
  },
  "icons": {
    "16": "icons/icon-16.png",
    "48": "icons/icon-48.png",
    "128": "icons/icon-128.png"
  },
  "background": {
    "service_worker": "background.js"
  },
  "host_permissions": [
    "<all_urls>"
  ],
  "web_accessible_resources": [
    {
      "resources": ["app/sandbox.html"],
      "matches": ["<all_urls>"]
    }
  ],
  "sandbox": {
    "pages": ["app/sandbox.html"]
  }
}
EOF
fi

# Step 6: Create background script
echo "📝 Creating background script..."
cat > chrome-extension/background.js << 'EOF'
// Background script for Chrome extension
chrome.action.onClicked.addListener((tab) => {
  chrome.tabs.create({
    url: chrome.runtime.getURL('app/index.html')
  });
});
EOF

# Step 7: Remove React's manifest.json to avoid conflict
echo "🔧 Updating manifest..."
if [ -f "chrome-extension/app/manifest.json" ]; then
  rm chrome-extension/app/manifest.json
  echo "✓ Removed conflicting React manifest.json"
fi

# Step 8: Create zip package
echo "📦 Creating extension package..."
cd chrome-extension
zip -r ../hitop-chrome-extension.zip * -x "*.DS_Store"
cd ..

echo "✅ Build complete!"
echo "📦 Extension package: hitop-chrome-extension.zip"
echo ""
echo "To install in Chrome:"
echo "1. Open Chrome and go to chrome://extensions/"
echo "2. Enable 'Developer mode' (toggle in top right)"
echo "3. Click 'Load unpacked'"
echo "4. Select the chrome-extension directory"
echo ""
echo "Or install the packaged extension:"
echo "1. Go to chrome://extensions/"
echo "2. Enable 'Developer mode'"
echo "3. Drag and drop hitop-chrome-extension.zip onto the page"
echo ""
