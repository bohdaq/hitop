# HITOP Chrome Extension

HITOP is a powerful HTTP client Chrome extension for testing REST APIs directly from your browser.

## Features

- 🚀 **Multiple HTTP Methods**: GET, POST, PUT, DELETE, PATCH, HEAD, OPTIONS
- 📁 **Collections**: Organize requests into collections
- 💾 **Save & Load**: Save requests and reload them anytime
- 🔄 **Import/Export**: Backup and share your collections
- 📝 **Custom Headers**: Add custom headers to your requests
- 📤 **Request Body**: Send JSON, form data, or raw text
- 🎨 **Syntax Highlighting**: Beautiful code highlighting for responses
- 📊 **Response Details**: View status codes, headers, and formatted responses
- 🗂️ **Multiple Tabs**: Work with multiple requests simultaneously

## Installation

### From Source

1. **Clone the repository:**
   ```bash
   git clone https://github.com/bohdaq/hitop.git
   cd hitop
   ```

2. **Install dependencies:**
   ```bash
   cd frontend
   npm install
   ```

3. **Build the extension:**
   ```bash
   npm run build:extension
   ```

4. **Load in Chrome:**
   - Open Chrome and navigate to `chrome://extensions/`
   - Enable "Developer mode" (toggle in the top right)
   - Click "Load unpacked"
   - Select the `frontend/build` directory
   - HITOP extension is now installed!

## Usage

1. Click the HITOP icon in your Chrome toolbar
2. Enter the URL you want to test
3. Select the HTTP method
4. Add headers if needed
5. Add request body for POST/PUT requests
6. Click "Make Request"
7. View the formatted response

### Saving Requests

1. Configure your request
2. Click the Save icon
3. Enter a name and select a collection
4. The request is saved and can be reloaded anytime

### Managing Collections

- **Create**: Click the + icon next to "Collections"
- **Rename**: Click the edit icon on a collection
- **Delete**: Click "Delete Collection" in the rename modal
- **Export**: Click "Export" at the bottom of the sidebar
- **Import**: Click "Import" to restore collections from JSON

## Permissions

The extension requires the following permissions:

- **storage**: To save your collections and requests locally
- **host_permissions (<all_urls>)**: To make HTTP requests to any URL you specify

## Privacy

- All data is stored locally in your browser
- No data is sent to external servers
- Your API keys and requests remain private

## Development

To contribute or modify:

1. Make changes in `frontend/src`
2. Run `npm run build:extension`
3. Reload the extension in `chrome://extensions/`

## License

Open source - see LICENSE file for details

## Support

For issues or feature requests, please visit: https://github.com/bohdaq/hitop/issues
