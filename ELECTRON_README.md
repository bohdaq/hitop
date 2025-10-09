# HITOP Electron Desktop App - Quick Start

## What is it?

HITOP Desktop is a native desktop application for macOS, Windows, and Linux that brings all HITOP features to your desktop without browser limitations.

## Quick Start

### Install Dependencies

```bash
npm install
```

### Run in Development

```bash
npm run electron:dev
```

This starts the React dev server and launches Electron with hot reload.

### Build for Distribution

```bash
# Build for current platform
./build-electron.sh

# Or build for specific platform
npm run electron:build:mac     # macOS
npm run electron:build:win     # Windows
npm run electron:build:linux   # Linux
```

Built apps will be in `dist/` directory.

## Features

✅ **No CORS Issues** - Direct HTTP requests
✅ **Native App** - Runs as desktop application
✅ **Offline** - Works completely offline
✅ **Persistent Storage** - Data saved to disk
✅ **All HITOP Features** - Collections, variables, scripts, history

## File Structure

```
electron/
├── main.js      # Main process (Node.js)
└── preload.js   # Preload script (IPC bridge)

frontend/
└── build/       # React app (built)

package.json     # Electron configuration
```

## Scripts

- `npm run electron` - Run built app
- `npm run electron:dev` - Development mode with hot reload
- `npm run electron:build` - Build for current platform
- `npm run electron:build:mac` - Build for macOS
- `npm run electron:build:win` - Build for Windows
- `npm run electron:build:linux` - Build for Linux

## Storage Location

App data is stored in:
- **macOS**: `~/Library/Application Support/hitop/`
- **Windows**: `%APPDATA%/hitop/`
- **Linux**: `~/.config/hitop/`

## Documentation

See [ELECTRON_GUIDE.md](./ELECTRON_GUIDE.md) for comprehensive documentation.

## Troubleshooting

### Build fails?
```bash
rm -rf node_modules
npm install
```

### App won't start?
- macOS: Right-click → Open
- Windows: Click "More info" → "Run anyway"
- Linux: `chmod +x HITOP-*.AppImage`

## Support

- [Full Documentation](./ELECTRON_GUIDE.md)
- [Report Issues](https://github.com/bohdaq/hitop/issues)
- [GitHub Repository](https://github.com/bohdaq/hitop)

## License

MIT License - see [LICENSE](LICENSE) file.
