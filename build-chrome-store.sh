#!/bin/bash

# Build script for HITOP Chrome Web Store Submission

echo "🚀 Building HITOP Chrome Extension for Web Store..."

# Step 1: Build React app
echo "📦 Building React application..."
cd frontend
npm run build
cd ..

# Step 2: Create extension directory structure
echo "📁 Creating extension directory..."
rm -rf chrome-store-package
mkdir -p chrome-store-package
mkdir -p chrome-store-package/icons
mkdir -p chrome-store-package/screenshots

# Step 3: Copy React build to extension
echo "📋 Copying React build files..."
cp -r frontend/build/* chrome-store-package/

# Step 4: Create and copy icons in all required sizes
echo "🎨 Creating icons for Chrome Web Store..."
if [ -f "frontend/public/logo512.png" ]; then
  # Chrome Web Store requires: 16, 48, 128
  if command -v sips &> /dev/null; then
    sips -z 16 16 frontend/public/logo512.png --out chrome-store-package/icons/icon-16.png > /dev/null 2>&1
    sips -z 48 48 frontend/public/logo512.png --out chrome-store-package/icons/icon-48.png > /dev/null 2>&1
    sips -z 128 128 frontend/public/logo512.png --out chrome-store-package/icons/icon-128.png > /dev/null 2>&1
    echo "✓ Icons created: 16x16, 48x48, 128x128"
  else
    echo "⚠️  sips not found, please create icons manually"
  fi
else
  echo "⚠️  Warning: logo512.png not found"
fi

# Step 5: Create Chrome Web Store manifest
echo "🔧 Creating Chrome Web Store manifest..."
cat > chrome-store-package/manifest.json << 'EOF'
{
  "manifest_version": 3,
  "name": "HITOP - HTTP Client",
  "version": "3.0.0",
  "description": "A powerful HTTP client for testing and debugging APIs. Features include collections, variables, pre/post request scripts, and request history.",
  "author": "HITOP Team",
  "action": {
    "default_title": "Open HITOP HTTP Client"
  },
  "icons": {
    "16": "icons/icon-16.png",
    "48": "icons/icon-48.png",
    "128": "icons/icon-128.png"
  },
  "background": {
    "service_worker": "background.js"
  },
  "permissions": [
    "storage"
  ],
  "host_permissions": [
    "<all_urls>"
  ]
}
EOF

# Step 6: Create background service worker
echo "📝 Creating background service worker..."
cat > chrome-store-package/background.js << 'EOF'
// Background service worker for Chrome extension
// Opens HITOP in a new tab when extension icon is clicked

chrome.action.onClicked.addListener((tab) => {
  chrome.tabs.create({
    url: chrome.runtime.getURL('index.html')
  });
});

// Handle installation
chrome.runtime.onInstalled.addListener((details) => {
  if (details.reason === 'install') {
    console.log('HITOP HTTP Client installed');
    // Optionally open welcome page
    chrome.tabs.create({
      url: chrome.runtime.getURL('index.html')
    });
  } else if (details.reason === 'update') {
    console.log('HITOP HTTP Client updated to version ' + chrome.runtime.getManifest().version);
  }
});
EOF

# Step 7: Remove conflicting files
echo "🔧 Cleaning up conflicting files..."
if [ -f "chrome-store-package/manifest.json.bak" ]; then
  rm chrome-store-package/manifest.json.bak
fi
if [ -f "chrome-store-package/manifest_extension.json" ]; then
  rm chrome-store-package/manifest_extension.json
fi

# Step 8: Create privacy policy
echo "📄 Creating privacy policy..."
cat > chrome-store-package/PRIVACY.md << 'EOF'
# Privacy Policy for HITOP HTTP Client

Last updated: November 2025

## Data Collection and Storage

HITOP HTTP Client does NOT collect, transmit, or share any personal data. All data is stored locally in your browser.

### What We Store Locally:
- HTTP request collections
- Request history
- Collection variables
- User preferences

### What We DON'T Do:
- We do NOT send your data to any external servers
- We do NOT track your usage
- We do NOT collect analytics
- We do NOT share data with third parties

### Permissions Explained:

**storage**: Used to save your collections and requests locally in Chrome's storage
**host_permissions (<all_urls>)**: Required to make HTTP requests to any API endpoint you specify

All data remains on your device and is never transmitted to our servers or any third party.

## Contact

For questions about this privacy policy, please contact: [your-email@example.com]
EOF

# Step 9: Create README for store
echo "📄 Creating store README..."
cat > chrome-store-package/README.txt << 'EOF'
HITOP - HTTP Client Chrome Extension

A powerful HTTP client for testing and debugging APIs directly in your browser.

FEATURES:
- Create and organize requests in collections
- Support for GET, POST, PUT, DELETE, PATCH, and more
- Collection variables for dynamic values
- Pre-request and post-request scripts
- Request history
- Import/Export collections
- Drag and drop to reorder requests
- Dark mode support

USAGE:
Click the HITOP icon in your Chrome toolbar to open the HTTP client in a new tab.

PRIVACY:
All data is stored locally in your browser. No data is sent to external servers.

PERMISSIONS:
- storage: To save your collections locally
- host_permissions: To make HTTP requests to APIs

For more information, visit: https://github.com/bohdaq/hitop
EOF

# Step 10: Create zip package for Chrome Web Store
echo "📦 Creating Chrome Web Store package..."
cd chrome-store-package
zip -r ../hitop-chrome-store.zip * -x "*.DS_Store" -x "screenshots/*"
cd ..

echo "✅ Build complete!"
echo ""
echo "📦 Chrome Web Store package: hitop-chrome-store.zip"
echo "📁 Package directory: chrome-store-package/"
echo ""
echo "📋 NEXT STEPS FOR CHROME WEB STORE SUBMISSION:"
echo ""
echo "1. Create promotional images (required):"
echo "   - Small tile: 440x280 pixels"
echo "   - Large tile: 920x680 pixels (optional)"
echo "   - Marquee tile: 1400x560 pixels (optional)"
echo "   - Screenshots: 1280x800 or 640x400 pixels (at least 1 required)"
echo ""
echo "2. Go to Chrome Web Store Developer Dashboard:"
echo "   https://chrome.google.com/webstore/devconsole"
echo ""
echo "3. Click 'New Item' and upload hitop-chrome-store.zip"
echo ""
echo "4. Fill in the store listing:"
echo "   - Detailed description"
echo "   - Category: Developer Tools"
echo "   - Language: English"
echo "   - Upload promotional images"
echo "   - Upload screenshots"
echo ""
echo "5. Set pricing (Free recommended)"
echo ""
echo "6. Submit for review"
echo ""
echo "Note: First-time publishers need to pay a one-time $5 developer registration fee"
echo ""
