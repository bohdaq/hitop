# HITOP Electron Desktop App Guide

## Overview

HITOP is now available as a native desktop application for macOS (Apple Silicon) using Electron!

## Features

### ✅ Desktop Application Benefits

- **Native App Experience** - Runs as a standalone desktop application
- **No CORS Issues** - Direct HTTP requests without browser restrictions
- **File System Access** - Save/load collections from disk
- **Persistent Storage** - Data stored in app data directory
- **Offline Capable** - Works completely offline
- **System Integration** - Dock/taskbar icon, native menus
- **Auto-Updates** - Built-in update mechanism (can be configured)

### ✅ All HITOP Features

- Multiple HTTP methods (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS)
- Request collections
- Collection variables
- Pre/post-request scripts
- Request history
- Multiple tabs
- Import/Export
- Syntax highlighting

## Installation

### For Users

#### macOS

1. Download `HITOP-1.0.0.dmg` from releases
2. Open the DMG file
3. Drag HITOP to Applications folder
4. Launch HITOP from Applications

**Note**: On first launch, you may need to:
- Right-click → Open (if unsigned)
- Go to System Preferences → Security & Privacy → Allow

**Note**: Windows and Linux builds are available on [GitHub Releases](https://github.com/bohdaq/hitop/releases/latest) but are not officially supported in this version.

## Development

### Prerequisites

- Node.js 16+ and npm
- Git

### Setup

```bash
# Clone repository
git clone https://github.com/bohdaq/hitop.git
cd hitop

# Install dependencies
npm install

# This will also install frontend dependencies via postinstall script
```

### Running in Development

```bash
# Start development mode (hot reload)
npm run electron:dev
```

This will:
1. Start React dev server on port 3000
2. Wait for server to be ready
3. Launch Electron with dev tools open

### Building

```bash
# Quick build script
./build-electron.sh

# Or build directly
npm run electron:build:mac
```

#### Build Output

Built applications will be in `dist/` directory:

- **macOS ARM64**: `HITOP-1.0.0-arm64.dmg`, `HITOP-1.0.0-arm64-mac.zip`

## Architecture

### File Structure

```
hitop/
├── electron/
│   ├── main.js          # Main process (Node.js)
│   ├── preload.js       # Preload script (bridge)
│   └── build/           # Build resources (icons, etc.)
├── frontend/
│   ├── src/             # React app source
│   └── build/           # Built React app
├── package.json         # Electron config
└── build-electron.sh    # Build script
```

### Process Architecture

```
Main Process (Node.js)
    ↓
Preload Script (Bridge)
    ↓
Renderer Process (React App)
```

### Storage

Data is stored in:
- **macOS**: `~/Library/Application Support/hitop/hitop-data.json`

### IPC Communication

The app uses IPC (Inter-Process Communication) for:
- **Storage**: Save/load collections, history, settings
- **HTTP Requests**: Bypass CORS restrictions
- **File System**: Import/export collections

## Configuration

### electron/main.js

Main process configuration:
- Window size and behavior
- Security settings
- IPC handlers
- Storage management

### package.json

Build configuration:
- App ID and name
- Icons and resources
- Platform-specific settings
- Output formats

## Security

### Content Security Policy

```javascript
'Content-Security-Policy': ["script-src 'self' 'unsafe-eval'"]
```

- `'self'`: Only load scripts from app
- `'unsafe-eval'`: Required for custom scripts feature

### Context Isolation

- ✅ Enabled - Renderer process isolated from Node.js
- ✅ Preload script - Safe bridge between processes
- ✅ No Node Integration - Renderer can't access Node.js directly

## Customization

### Change App Icon

1. Replace `frontend/public/logo512.png`
2. Update icon paths in `package.json` build config
3. Rebuild app

### Change App Name

1. Update `productName` in `package.json`
2. Update `title` in `electron/main.js`
3. Rebuild app

### Change Window Size

Edit `electron/main.js`:
```javascript
new BrowserWindow({
  width: 1400,  // Change this
  height: 900,  // Change this
  // ...
})
```

## Distribution

### Code Signing (macOS)

For distribution outside App Store:

```bash
# Get Developer ID certificate from Apple
# Set environment variables
export CSC_LINK=/path/to/certificate.p12
export CSC_KEY_PASSWORD=your_password

# Build with signing
npm run electron:build:mac
```


### Auto-Updates

To enable auto-updates, configure in `package.json`:

```json
"publish": {
  "provider": "github",
  "owner": "bohdaq",
  "repo": "hitop"
}
```

Then use `electron-updater` in main process.

## Troubleshooting

### App Won't Start

**macOS**: "App is damaged"
- Right-click → Open
- System Preferences → Security & Privacy → Allow

**Windows**: SmartScreen warning
- Click "More info" → "Run anyway"

**Linux**: Permission denied
```bash
chmod +x HITOP-1.0.0.AppImage
```

### Build Fails

**Missing dependencies:**
```bash
rm -rf node_modules
npm install
```

**React build fails:**
```bash
cd frontend
rm -rf node_modules build
npm install
npm run build
```

### Storage Issues

Clear app data:
- **macOS**: `rm -rf ~/Library/Application\ Support/hitop/`

### DevTools Not Opening

In development mode, DevTools should open automatically.

To toggle manually, add to `electron/main.js`:
```javascript
mainWindow.webContents.openDevTools();
```

## Performance

### Optimization Tips

1. **Reduce Bundle Size**
   - Remove unused dependencies
   - Use production build
   - Enable compression

2. **Faster Startup**
   - Lazy load components
   - Optimize images
   - Cache resources

3. **Memory Usage**
   - Limit history size
   - Clear old data
   - Use pagination

## Testing

### Manual Testing

```bash
# Development mode
npm run electron:dev

# Production build
npm run electron:build
# Then test the built app in dist/
```

### Automated Testing

Add to `package.json`:
```json
"devDependencies": {
  "spectron": "^19.0.0"
}
```

Create tests in `test/` directory.

## Deployment

### GitHub Releases

1. Build for macOS ARM64
2. Create GitHub release
3. Upload built files:
   - `HITOP-1.0.0-arm64.dmg` (macOS ARM64)
   - `HITOP-1.0.0-arm64-mac.zip` (macOS ARM64 zip)

### Homebrew (macOS)

Create a Homebrew cask:
```ruby
cask "hitop" do
  version "1.0.0"
  sha256 "..."
  
  url "https://github.com/bohdaq/hitop/releases/download/v#{version}/HITOP-#{version}.dmg"
  name "HITOP"
  desc "HTTP API Testing Tool"
  homepage "https://github.com/bohdaq/hitop"
  
  app "HITOP.app"
end
```


## Updates

### Version Bumping

```bash
# Update version in package.json
npm version patch  # 1.0.0 -> 1.0.1
npm version minor  # 1.0.0 -> 1.1.0
npm version major  # 1.0.0 -> 2.0.0

# Rebuild
npm run electron:build
```

### Changelog

Maintain `CHANGELOG.md` with version history.

## FAQ

### Q: Why Electron?

A: Native desktop app with no CORS issues and offline support.

### Q: App size?

A: ~100-150MB (includes Chromium and Node.js runtime)

### Q: Can I use with web version?

A: Yes! Data can be exported/imported between versions.

### Q: Offline support?

A: Yes, fully functional offline. Only API requests need internet.

### Q: Auto-updates?

A: Can be configured using `electron-updater`.

### Q: Open source?

A: Yes! MIT License. See [LICENSE](LICENSE).

## Resources

- [Electron Documentation](https://www.electronjs.org/docs)
- [Electron Builder](https://www.electron.build/)
- [HITOP Repository](https://github.com/bohdaq/hitop)
- [Report Issues](https://github.com/bohdaq/hitop/issues)

## Support

Need help?
- Open an issue on GitHub
- Check documentation
- Review example code

## Contributing

Contributions welcome! To contribute:
1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

## License

MIT License - see [LICENSE](LICENSE) file for details.

Copyright (c) 2025 Bohdan Tsap

## Next Steps

1. ✅ Install dependencies: `npm install`
2. ✅ Run in development: `npm run electron:dev`
3. ✅ Build for distribution: `./build-electron.sh`
4. 🎉 Share your desktop app!
