# HITOP Chrome Extension Setup

## Building the Extension

1. **Build the React app:**
   ```bash
   cd frontend
   npm run build
   ```

2. **Prepare the extension:**
   ```bash
   cd build
   # Replace the PWA manifest with the Chrome extension manifest
   cp ../public/manifest_extension.json manifest.json
   ```

3. **Load the extension in Chrome:**
   - Open Chrome and go to `chrome://extensions/`
   - Enable "Developer mode" (toggle in top right)
   - Click "Load unpacked"
   - Select the `frontend/build` directory
   - The HITOP extension should now appear in your extensions

## Using the Extension

- Click the HITOP icon in your Chrome toolbar to open the HTTP client
- The extension will open in a new tab (not as a popup)
- All features (collections, requests, export/import) work the same as the web version
- You can have multiple HITOP tabs open simultaneously

## Permissions

The extension requires:
- **storage**: To save collections and requests locally
- **tabs**: To open the app in a new tab
- **host_permissions (<all_urls>)**: To make HTTP requests to any URL

## Notes

- Collections are stored in Chrome's local storage
- The extension works offline once installed
- You can pin the extension to your toolbar for quick access

## Development

To make changes:
1. Modify the source code in `frontend/src`
2. Run `npm run build`
3. Go to `chrome://extensions/` and click the refresh icon on the HITOP extension
