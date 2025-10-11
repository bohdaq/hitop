#!/bin/bash

# Build script for HITOP Firefox Extension

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
if [ -f "frontend/public/logo192.png" ]; then
  # Copy logo192 for 96x96 icon (will be scaled by browser)
  cp frontend/public/logo192.png extension/icons/icon-96.png
  
  # Use sips to resize for 48x48 icon (macOS only)
  if command -v sips &> /dev/null; then
    sips -z 48 48 frontend/public/logo192.png --out extension/icons/icon-48.png > /dev/null 2>&1
    echo "✓ Icons resized with sips"
  else
    # Fallback: just copy the same file
    cp frontend/public/logo192.png extension/icons/icon-48.png
    echo "⚠️  sips not found, using original size (browser will scale)"
  fi
else
  echo "⚠️  Warning: Icons not found, please add icons manually"
fi

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
