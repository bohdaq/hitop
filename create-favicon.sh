#!/bin/bash

# Create favicon in multiple sizes from SVG

echo "🎨 Creating favicons..."

# Check if ImageMagick or rsvg-convert is available
if command -v rsvg-convert &> /dev/null; then
    echo "Using rsvg-convert..."
    rsvg-convert -w 16 -h 16 site/public-assets/favicon.svg -o site/public-assets/favicon-16x16.png
    rsvg-convert -w 32 -h 32 site/public-assets/favicon.svg -o site/public-assets/favicon-32x32.png
    rsvg-convert -w 180 -h 180 site/public-assets/favicon.svg -o site/public-assets/apple-touch-icon.png
    echo "✅ Favicons created with rsvg-convert"
elif command -v convert &> /dev/null; then
    echo "Using ImageMagick..."
    convert -background none -resize 16x16 site/public-assets/favicon.svg site/public-assets/favicon-16x16.png
    convert -background none -resize 32x32 site/public-assets/favicon.svg site/public-assets/favicon-32x32.png
    convert -background none -resize 180x180 site/public-assets/favicon.svg site/public-assets/apple-touch-icon.png
    echo "✅ Favicons created with ImageMagick"
elif command -v sips &> /dev/null; then
    echo "Using macOS sips (SVG not fully supported)..."
    echo "⚠️  Please use online tool or install rsvg-convert"
    echo "   brew install librsvg"
else
    echo "⚠️  No image converter found"
    echo "   Install one of:"
    echo "   - brew install librsvg (macOS)"
    echo "   - brew install imagemagick (macOS)"
    echo "   - Or use online tool: https://realfavicongenerator.net/"
fi

echo ""
echo "📝 Manual alternative:"
echo "1. Open site/public-assets/favicon.svg in browser"
echo "2. Take screenshot"
echo "3. Resize to 32x32, 16x16, 180x180"
echo "4. Save as PNG files"
