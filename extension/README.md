# HITOP Firefox Extension

A Firefox extension for the HITOP HTTP API Testing Tool.

## Features

- 🚀 Quick access to HITOP from browser toolbar
- 📦 Full-featured API testing tool
- 💾 Data persistence using browser storage
- 🔒 Secure cross-origin requests
- 📝 All features from the web app

## Installation

### Development Installation

**Requirements:**
- Firefox 58.0 or higher

1. **Build the extension:**
   ```bash
   ./build-extension.sh
   ```

2. **Load in Firefox:**
   - Open Firefox
   - Navigate to `about:debugging`
   - Click "This Firefox"
   - Click "Load Temporary Add-on"
   - Select `extension/manifest.json`

### Production Installation

1. **Install from file:**
   - Open Firefox
   - Navigate to `about:addons`
   - Click the gear icon ⚙️
   - Select "Install Add-on From File"
   - Choose `hitop-firefox-extension.zip`

## Usage

### Quick Access
1. Click the HITOP icon in the Firefox toolbar
2. Choose:
   - **"Open HITOP"** - Opens in a new tab and switches to it
   - **"Open in Background Tab"** - Opens in a new tab without switching

### Features Available

#### Core Features
- Multiple HTTP methods (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS)
- Request/response viewer with syntax highlighting
- Custom headers and request body
- Multiple tabs for concurrent requests

#### Collections
- Organize requests into collections
- Save and load requests
- Run entire collections sequentially
- Drag & drop request reordering
- Import/Export collections as JSON

#### Variables
- Collection-level variables
- Variable interpolation with `${variableName}` syntax
- Use in URLs, headers, and request bodies

#### Custom Scripting
- Pre-request scripts (modify request before sending)
- Post-request scripts (extract data from responses)
- Context sharing between requests
- Access to variables in scripts

#### History
- Track all requests and collection runs
- Reload previous requests
- Filter by success/failure

## File Structure

```
extension/
├── manifest.json          # Extension manifest
├── background.js          # Background service worker
├── popup/
│   ├── popup.html        # Extension popup UI
│   └── popup.js          # Popup logic
├── icons/
│   ├── icon-48.png       # 48x48 icon
│   └── icon-96.png       # 96x96 icon
├── app/                  # Built React application
│   ├── index.html
│   ├── static/
│   └── ...
└── README.md             # This file
```

## Permissions

The extension requires the following permissions:

- **`storage`**: To save collections, history, and settings
- **`activeTab`**: To interact with the current tab
- **`<all_urls>`**: To make HTTP requests to any URL for API testing

## Development

### Building from Source

```bash
# Install dependencies
cd frontend
npm install

# Build the extension
cd ..
./build-extension.sh
```

### Making Changes

1. Modify the React app in `frontend/src/`
2. Run `./build-extension.sh` to rebuild
3. Reload the extension in Firefox (about:debugging)

## Storage

The extension uses Firefox's `browser.storage.local` API to persist:
- Collections and requests
- Variables and contexts
- Request history
- Tab states

Data is stored locally in the browser and is not synced across devices.

## Troubleshooting

### Extension doesn't load
- Check Firefox version (requires Firefox 57+)
- Verify manifest.json is valid
- Check browser console for errors

### Requests fail
- Check that the target URL is accessible
- Verify CORS settings on the API
- Check network tab in browser DevTools

### Data not persisting
- Check browser storage permissions
- Verify storage quota is not exceeded
- Check for browser storage errors in console

## Known Limitations

- Content Security Policy restrictions on inline scripts
- Some APIs may block requests from extensions
- Storage is limited by browser quota

## Contributing

Contributions are welcome! Please see the main project README for guidelines.

## License

MIT License - See LICENSE file for details

## Support

For issues and questions:
- Open an issue on GitHub
- Check the main HITOP documentation
- Review the SCRIPTING_GUIDE.md and VARIABLES_GUIDE.md

## Version History

### 1.0.0 (Initial Release)
- Full HITOP functionality in Firefox extension
- Collection management
- Variable support
- Custom scripting
- Request history
- Import/Export
