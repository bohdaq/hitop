# HITOP Electron Desktop App Guide

## Overview

HITOP is now available as a native desktop application for macOS, Windows, and Linux using Electron!

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

#### Windows

1. Download `HITOP-Setup-1.0.0.exe` from releases
2. Run the installer
3. Follow installation wizard
4. Launch HITOP from Start Menu or Desktop

#### Linux

**AppImage:**
```bash
# Download HITOP-1.0.0.AppImage
chmod +x HITOP-1.0.0.AppImage
./HITOP-1.0.0.AppImage
```

**Debian/Ubuntu:**
```bash
sudo dpkg -i HITOP_1.0.0_amd64.deb
hitop
```

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

#### Build for Current Platform

```bash
# Quick build script
./build-electron.sh

# Or use npm
npm run electron:build
```

#### Build for Specific Platform

```bash
# macOS
npm run electron:build:mac

# Windows
npm run electron:build:win

# Linux
npm run electron:build:linux
```

#### Build Output

Built applications will be in `dist/` directory:

- **macOS**: `HITOP-1.0.0.dmg`, `HITOP-1.0.0-mac.zip`
- **Windows**: `HITOP Setup 1.0.0.exe`, `HITOP 1.0.0.exe` (portable)
- **Linux**: `HITOP-1.0.0.AppImage`, `hitop_1.0.0_amd64.deb`

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
- **Windows**: `%APPDATA%/hitop/hitop-data.json`
- **Linux**: `~/.config/hitop/hitop-data.json`

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

### Windows Code Signing

```bash
# Get code signing certificate
# Set environment variables
export CSC_LINK=/path/to/certificate.pfx
export CSC_KEY_PASSWORD=your_password

# Build with signing
npm run electron:build:win
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
- **Windows**: Delete `%APPDATA%\hitop\`
- **Linux**: `rm -rf ~/.config/hitop/`

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

1. Build for all platforms
2. Create GitHub release
3. Upload built files:
   - `HITOP-1.0.0.dmg` (macOS)
   - `HITOP-Setup-1.0.0.exe` (Windows)
   - `HITOP-1.0.0.AppImage` (Linux)

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

### Snap (Linux)

Create `snap/snapcraft.yaml` for Snap Store distribution.

### Microsoft Store (Windows)

Use `electron-builder` with `appx` target for Microsoft Store.

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

A: Cross-platform desktop app with single codebase, no CORS issues, native features.

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
