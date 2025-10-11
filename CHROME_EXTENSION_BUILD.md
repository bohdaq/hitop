# Building HITOP for Chrome - Local Development

## Quick Start

### 1. Build the Extension

```bash
# Run the build script
./build-extension.sh
```

This creates the `extension/` directory with all necessary files.

### 2. Load in Chrome

1. **Open Chrome Extensions page:**
   - Go to `chrome://extensions/`
   - Or: Menu → More Tools → Extensions

2. **Enable Developer Mode:**
   - Toggle the "Developer mode" switch in the top-right corner

3. **Load the extension:**
   - Click "Load unpacked"
   - Navigate to the `extension/` folder in your HITOP directory
   - Click "Select Folder"

4. **Test the extension:**
   - Click the HITOP icon in the Chrome toolbar
   - The popup should open with the full HITOP interface

### 3. Making Changes

After modifying code:

```bash
# Rebuild
./build-extension.sh

# In Chrome:
# - Go to chrome://extensions/
# - Click the refresh icon on the HITOP extension card
```

## Chrome vs Firefox Differences

### Current Manifest (V3)

The current `manifest.json` uses **Manifest V3**, which is the modern standard.

**Compatibility:**
- ✅ **Chrome**: Full support (required for Chrome Web Store)
- ✅ **Firefox**: Supports V3 (Firefox 109+)
- ✅ **Future-proof**: Latest standard

### Manifest V3 Features

```json
{
  "manifest_version": 3,
  "action": { ... },
  "host_permissions": ["<all_urls>"],
  "background": {
    "service_worker": "background.js"
  },
  "content_security_policy": {
    "extension_pages": "script-src 'self' 'wasm-unsafe-eval'; ..."
  }
}
```

**Key Changes from V2:**
- `browser_action` → `action`
- `permissions` → `host_permissions` (for URL access)
- Background scripts → Service worker
- CSP now uses object format
- `'unsafe-eval'` → `'wasm-unsafe-eval'`

## Testing in Chrome (Current Setup)

### What Works:
- ✅ Extension loads and runs
- ✅ All HITOP features functional
- ✅ HTTP requests work
- ✅ localStorage works (no permission needed)
- ✅ Collections, variables, scripts all work

### What You'll See:
- ✅ No deprecation warnings
- ✅ Clean load in Chrome
- ✅ Manifest V3 compliant

## Building for Chrome Web Store

The extension is now **Manifest V3** and ready for Chrome Web Store submission!

```bash
# Build
./build-extension.sh

# Package
cd extension
zip -r ../hitop-chrome-extension.zip * -x "*.DS_Store" -x "README.md" -x "test-icon.html"
cd ..
```

Upload `hitop-chrome-extension.zip` to Chrome Web Store.

**Features:**
- ✅ Manifest V3 compliant
- ✅ No deprecation warnings
- ✅ Works in Chrome and Firefox (109+)
- ✅ Future-proof
- ✅ **Minimal permissions** (`<all_urls>` only!)
- ✅ No popup - opens app directly in new tab
- ✅ Uses `tabs.create()` without tabs permission

## Current Status

The extension now uses **Manifest V3** and is ready for both local testing and Chrome Web Store submission!

### For Local Testing:
```bash
./build-extension.sh
# Load extension/ folder in Chrome
```

### For Chrome Web Store:
```bash
./build-extension.sh
cd extension
zip -r ../hitop-chrome-extension.zip * -x "*.DS_Store" -x "README.md" -x "test-icon.html"
cd ..
# Upload hitop-chrome-extension.zip
```

## Troubleshooting

### Extension doesn't load
- Check Chrome version (need Chrome 88+)
- Verify all files in `extension/` folder
- Check console for errors

### Service worker errors
- Check that background.js is compatible with service workers
- Service workers don't have DOM access
- Use chrome.* APIs instead of browser.* in Chrome

### Requests fail
- Check `<all_urls>` permission granted
- Verify API is accessible
- Check network tab in DevTools

### Popup doesn't open
- Check `popup/popup.html` exists
- Verify React build in `app/` folder
- Check browser console for errors

## File Structure

After building:

```
extension/
├── manifest.json          # Manifest V2 (works in Chrome & Firefox)
├── background.js          # Background script
├── popup/
│   └── popup.html        # Popup page
├── icons/
│   ├── icon-48.png
│   └── icon-96.png
└── app/                  # Built React app
    ├── index.html
    ├── static/
    └── ...
```

## Summary

**To build and test in Chrome right now:**

```bash
# 1. Build
./build-extension.sh

# 2. Load in Chrome
# - Go to chrome://extensions/
# - Enable "Developer mode"
# - Click "Load unpacked"
# - Select the extension/ folder

# 3. Test
# - Click HITOP icon in toolbar
# - Should open and work perfectly
```

**For Chrome Web Store:**
- Current build works but shows warnings
- Consider upgrading to Manifest V3 for long-term support
- See "Option 2" above for V3 migration

---

**Questions?**
- Check [Chrome Extension docs](https://developer.chrome.com/docs/extensions/)
- See [Manifest V3 migration guide](https://developer.chrome.com/docs/extensions/mv3/intro/)
