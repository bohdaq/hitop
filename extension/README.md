## Overview

HITOP Extension packages the HITOP web application as a browser extension for Chrome and Firefox.

## Features

- 🚀 Quick access to HITOP from browser toolbar
- 📦 Full-featured API testing tool
{{ ... }}
   - Navigate to `chrome://extensions/`
   - Enable "Developer mode"
   - Click "Load unpacked"
   - Select the extracted extension folder

### Firefox

1. Download the latest release `.zip` file
2. Open Firefox
3. Go to `about:addons`
4. Click the gear icon
5. Select "Install Add-on From File"
6. Choose the downloaded `.xpi` filefox-extension.zip`

## Usage

### Quick Access
1. Click the HITOP icon in the browser toolbar
2. The full application opens directly in a new tab

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

- **`<all_urls>`**: To make HTTP requests to any URL for API testing

Data is saved using localStorage (no storage permission required).

Note: `tabs.create()` doesn't require the tabs permission - it's only needed for reading tab data.

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

The extension uses browser's `localStorage` API to persist:
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
- Verify localStorage is enabled in browser
- Check storage quota is not exceeded
- Check for browser storage errors in console

## Known Limitations

- Content Security Policy restrictions on inline scripts
- Some APIs may block requests from extensions
- Storage is limited by browser quota

## Contributing

Contributions are welcome! Please see the main project README for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](../LICENSE) file for details.

Copyright (c) 2025 Bohdan Tsap

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
