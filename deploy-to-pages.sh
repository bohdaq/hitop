#!/bin/bash

# Deploy HITOP to GitHub Pages

echo "🚀 Deploying HITOP to GitHub Pages..."

# Step 1: Build React app
echo "📦 Building React application..."
cd frontend
npm run build
cd ..

# Step 2: Copy build to docs/app
echo "📋 Copying build to docs/app..."
rm -rf docs/app
mkdir -p docs/app
cp -r frontend/build/* docs/app/

# Step 3: Update index.html link
echo "🔗 Updating landing page..."
# The landing page already has links, no changes needed

echo "✅ Deployment complete!"
echo ""
echo "📍 Your app will be available at:"
echo "   https://bohdaq.github.io/hitop/app/"
echo "   or"
echo "   https://bohdaq.github.io/hitop/app.html (with redirect)"
echo ""
echo "Next steps:"
echo "1. git add docs/"
echo "2. git commit -m 'Deploy app to GitHub Pages'"
echo "3. git push origin main"
echo "4. Wait 1-2 minutes for GitHub Pages to rebuild"
