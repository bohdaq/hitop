# HITOP Firefox Extension Guide

## Overview

The HITOP Firefox Extension brings the full power of HITOP API testing tool directly to your browser. Access all features with a single click from your toolbar.

## Quick Start

### Installation

1. **Build the extension:**
   ```bash
   ./build-extension.sh
   ```

2. **Install in Firefox:**
   - Open `about:debugging` in Firefox
   - Click "This Firefox"
   - Click "Load Temporary Add-on"
   - Navigate to `extension/` folder and select `manifest.json`

### First Use

1. Click the HITOP icon in your Firefox toolbar
2. Choose "Open HITOP" (opens in new tab)
3. Start testing your APIs!

**Tip**: Use "Open in Background Tab" to open HITOP without leaving your current page.

## Features

### ✅ All Web App Features
- Multiple HTTP methods
- Request collections
- Collection variables
- Pre/post-request scripts
- Request history
- Import/Export
- Multiple tabs

### 🔒 Extension Benefits
- **Persistent Storage**: Data saved in browser storage
- **Quick Access**: One-click from toolbar
- **Cross-Origin Requests**: No CORS issues
- **Offline Capable**: Works without internet (for saved data)

## Building the Extension

### Prerequisites
- Node.js and npm installed
- Firefox browser (version 58.0 or higher)

### Important Configuration

The `frontend/package.json` must include:
```json
"homepage": "."
```

This ensures React builds with relative paths, which is required for the extension to load correctly.

### Build Steps

```bash
# 1. Clone the repository
git clone https://github.com/yourusername/hitop.git
cd hitop

# 2. Install dependencies
cd frontend
npm install
cd ..

# 3. Build the extension
./build-extension.sh
```

This will:
1. Build the React application
2. Copy files to `extension/app/`
3. Create `hitop-firefox-extension.zip`

### Manual Build

If you prefer to build manually:

```bash
# Build React app
cd frontend
npm run build
cd ..

# Copy to extension
rm -rf extension/app
mkdir -p extension/app
cp -r frontend/build/* extension/app/

# Package extension
cd extension
zip -r ../hitop-firefox-extension.zip * -x "*.DS_Store"
cd ..
```

## Development

### Hot Reload During Development

For development, you can use the React dev server:

1. Start the dev server:
   ```bash
   cd frontend
   npm start
   ```

2. Access at `http://localhost:3000`

3. Make changes and see them live

### Testing Extension Changes

1. Make changes to extension files
2. Rebuild if needed: `./build-extension.sh`
3. Go to `about:debugging` in Firefox
4. Click "Reload" next to HITOP extension

## Extension Architecture

### File Structure

```
extension/
├── manifest.json          # Extension configuration
├── background.js          # Background service worker
├── popup/
│   ├── popup.html        # Toolbar popup UI
│   └── popup.js          # Popup behavior
├── icons/
│   ├── icon-48.png       # Small icon
│   └── icon-96.png       # Large icon
└── app/                  # Built React app
    ├── index.html
    └── static/
```

### Components

#### manifest.json
Defines extension metadata, permissions, and configuration:
- **manifest_version**: 2 (Firefox standard)
- **permissions**: <all_urls> (for API requests)
- **browser_action**: Toolbar button configuration
- **content_security_policy**: Allows script execution

#### background.js
Background service worker that:
- Initializes default storage on install
- Handles HTTP requests from the app
- Manages extension lifecycle

#### popup/
Small popup UI that appears when clicking toolbar icon:
- Quick access buttons
- Feature overview
- Links to full app

#### app/
The full HITOP React application:
- All API testing features
- Runs in a full browser tab
- Uses browser storage API

## Storage

### Data Persistence

The extension uses `browser.storage.local` to store:
- **Collections**: All your request collections
- **Variables**: Collection-specific variables
- **History**: Request history (last 50)
- **Contexts**: Runtime context data
- **Tabs**: Open tab states

### Storage Limits

Firefox storage limits:
- **Local storage**: ~10MB by default
- **Can be increased**: Request more quota if needed

### Accessing Storage

From browser console:
```javascript
// View all stored data in console
Object.keys(localStorage).forEach(key => {
  console.log(key, localStorage.getItem(key));
});

// Clear all data
localStorage.clear();
```

## Permissions Explained

### <all_urls>
- **Why**: Make HTTP requests to any API
- **Access**: Only when you initiate requests
- **Privacy**: No automatic requests
- **Storage**: Data saved in localStorage (no storage permission needed)

## Troubleshooting

### Extension Won't Load

**Problem**: Error loading manifest

**Solutions**:
1. Check Firefox version (need 57+)
2. Validate manifest.json syntax
3. Check console for specific errors

### Requests Failing

**Problem**: HTTP requests return errors

**Solutions**:
1. Check API URL is correct
2. Verify API is accessible
3. Check for CORS issues (extension bypasses most)
4. Review network tab in DevTools

### Data Not Saving

**Problem**: Collections/history not persisting

**Solutions**:
1. Verify localStorage is enabled in browser
2. Check storage quota not exceeded
3. Check browser console for errors
4. Try clearing and re-saving

### Build Fails

**Problem**: `./build-extension.sh` errors

**Solutions**:
1. Ensure Node.js and npm installed
2. Run `npm install` in frontend/
3. Check for build errors in console
4. Verify file permissions

## Publishing

### Prepare for Release

1. **Update version** in `manifest.json`
2. **Test thoroughly** in Firefox
3. **Create icons** (48x48 and 96x96)
4. **Write release notes**

### Submit to Firefox Add-ons

1. **Create account** at [addons.mozilla.org](https://addons.mozilla.org)
2. **Package extension**: Already done by build script
3. **Submit for review**:
   - Upload `hitop-firefox-extension.zip`
   - Fill in metadata
   - Add screenshots
   - Submit for review

4. **Wait for approval** (usually 1-2 weeks)

### Self-Distribution

You can also distribute the `.xpi` file directly:

```bash
# Rename zip to xpi
mv hitop-firefox-extension.zip hitop-firefox-extension.xpi
```

Users can install by:
1. Downloading the `.xpi` file
2. Opening in Firefox
3. Clicking "Add" when prompted

## Security Considerations

### Content Security Policy

The extension uses:
```json
"content_security_policy": "script-src 'self' 'unsafe-eval'; object-src 'self'"
```

- **'unsafe-eval'**: Required for custom script execution
- **'self'**: Only load scripts from extension

### Data Privacy

- All data stored locally
- No external servers contacted
- No analytics or tracking
- No data collection

### API Security

- Use HTTPS APIs when possible
- Don't store sensitive tokens in variables
- Be cautious with custom scripts
- Review scripts before running collections

## Advanced Usage

### Custom Icons

Replace default icons:
```bash
# Add your custom icons
cp your-icon-48.png extension/icons/icon-48.png
cp your-icon-96.png extension/icons/icon-96.png

# Rebuild
./build-extension.sh
```

### Modify Popup

Edit `extension/popup/popup.html` and `popup.js` to customize:
- Add quick action buttons
- Change styling
- Add shortcuts

### Background Script

Modify `extension/background.js` to:
- Add custom request interceptors
- Implement caching
- Add logging

## Best Practices

### Development
1. Use React dev server for UI changes
2. Test in temporary extension mode
3. Check console for errors
4. Use Firefox DevTools

### Production
1. Minimize bundle size
2. Optimize images
3. Test on multiple Firefox versions
4. Provide clear documentation

### User Experience
1. Keep popup simple
2. Provide helpful error messages
3. Save state frequently
4. Handle edge cases gracefully

## FAQ

**Q: Can I use this in Chrome?**
A: The extension is Firefox-specific, but can be adapted for Chrome with manifest v3 changes.

**Q: Does it work offline?**
A: Yes, the extension works offline for viewing saved data. API requests require internet.

**Q: Is my data synced?**
A: No, data is stored locally. Use Export/Import to transfer between browsers.

**Q: Can I automate API tests?**
A: Yes, use collection runs with pre/post-request scripts for automation.

**Q: How do I update the extension?**
A: Rebuild with `./build-extension.sh` and reload in about:debugging.

## Support

- **Issues**: Open on GitHub
- **Documentation**: See main README.md
- **Guides**: SCRIPTING_GUIDE.md, VARIABLES_GUIDE.md

## Contributing

Contributions welcome! To contribute:
1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

Copyright (c) 2025 Bohdan Tsap

### What MIT License Means

✅ **You can:**
- Use commercially
- Modify the code
- Distribute
- Use privately
- Sublicense

⚠️ **You must:**
- Include the license and copyright notice

❌ **No warranty:**
- Software provided "as is"
