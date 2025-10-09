#!/bin/bash

# Build HITOP Electron App

echo "🚀 Building HITOP Electron App..."

# Step 1: Install dependencies
echo "📦 Installing dependencies..."
if [ ! -d "node_modules" ]; then
  npm install
fi

# Step 2: Build React app
echo "⚛️  Building React application..."
cd frontend
npm run build
cd ..

# Step 3: Build Electron app
echo "🖥️  Building Electron application..."

# Detect platform
if [[ "$OSTYPE" == "darwin"* ]]; then
  echo "Building for macOS..."
  npx electron-builder --mac
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
  echo "Building for Linux..."
  npx electron-builder --linux
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
  echo "Building for Windows..."
  npx electron-builder --win
else
  echo "Building for all platforms..."
  npx electron-builder
fi

echo "✅ Build complete!"
echo ""
echo "📦 Output directory: dist/"
echo ""
echo "Built files:"
ls -lh dist/

echo ""
echo "🎉 HITOP Electron app is ready!"
