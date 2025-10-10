# HITOP Electron Desktop App - Quick Start

## What is it?

HITOP Desktop is a native desktop application for macOS (Apple Silicon) that brings all HITOP features to your desktop without browser limitations.

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
# Build for macOS ARM64
./build-electron.sh

# Or build directly
npm run electron:build:mac
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
- `npm run electron:build:mac` - Build for macOS ARM64

## Storage Location

App data is stored in:
- **macOS**: `~/Library/Application Support/hitop/`

## Documentation

See [ELECTRON_GUIDE.md](./ELECTRON_GUIDE.md) for comprehensive documentation.

## Troubleshooting

### Build fails?
```bash
rm -rf node_modules
npm install
```

### App won't start?
- macOS: Right-click → Open (if unsigned)

## Support

- [Full Documentation](./ELECTRON_GUIDE.md)
- [Report Issues](https://github.com/bohdaq/hitop/issues)
- [GitHub Repository](https://github.com/bohdaq/hitop)

## License

MIT License - see [LICENSE](LICENSE) file.
