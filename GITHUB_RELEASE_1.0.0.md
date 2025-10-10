# 🎉 HITOP v1.0.0 - First Official Release

A powerful, privacy-focused HTTP API testing tool available as desktop app, web app, and browser extension!

## 📦 Downloads

### Desktop Applications

| Platform | Download | Size |
|----------|----------|------|
| **macOS (ARM64)** | [HITOP-1.0.0-arm64.dmg](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0-arm64.dmg) | 101 MB |
| **macOS (ARM64)** | [HITOP-1.0.0-arm64-mac.zip](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0-arm64-mac.zip) (Portable) | 97 MB |
| **Windows (ARM64)** | [HITOP-Setup-1.0.0.exe](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-Setup-1.0.0.exe) | 84 MB |
| **Windows (ARM64)** | [HITOP-1.0.0.exe](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0.exe) (Portable) | 84 MB |
| **Linux (ARM64)** | [HITOP-1.0.0-arm64.AppImage](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0-arm64.AppImage) | 111 MB |
| **Linux (ARM64)** | [hitop_1.0.0_arm64.deb](https://github.com/bohdaq/hitop/releases/download/v1.0.0/hitop_1.0.0_arm64.deb) | 73 MB |

### Other Platforms

- **Web App**: [Launch Online](https://bohdaq.github.io/hitop/app/)
- **Firefox Extension**: [Install from Firefox Add-ons](https://addons.mozilla.org/firefox/addon/hitop/)

## ✨ Key Features

### Core Functionality
- ✅ Multiple HTTP methods (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS)
- ✅ Collections with folders and drag & drop
- ✅ Variables with `${variable}` interpolation
- ✅ Pre/post-request JavaScript scripting
- ✅ Request history with search
- ✅ Multiple tabs
- ✅ Import/Export collections
- ✅ Syntax-highlighted responses

### Desktop App Benefits
- ✅ **No CORS issues** - Direct HTTP requests
- ✅ **Native performance** - Fast and responsive
- ✅ **Offline capable** - Works without internet
- ✅ **Privacy first** - All data stored locally

## 🚀 What's New

- **Desktop applications** for macOS, Windows, and Linux
- **Web application** hosted on GitHub Pages
- **Firefox extension** with full functionality
- **Comprehensive documentation** for all platforms
- **Build tools** for Windows, macOS, and Linux

## 📚 Documentation

- [README](https://github.com/bohdaq/hitop/blob/main/README.md) - Overview
- [Electron Guide](https://github.com/bohdaq/hitop/blob/main/ELECTRON_GUIDE.md) - Desktop app
- [Windows Build Guide](https://github.com/bohdaq/hitop/blob/main/BUILD_ON_WINDOWS.md) - Build on Windows
- [Scripting Guide](https://github.com/bohdaq/hitop/blob/main/SCRIPTING_GUIDE.md) - Custom scripts
- [Variables Guide](https://github.com/bohdaq/hitop/blob/main/VARIABLES_GUIDE.md) - Using variables

## 🔧 Quick Start

### Desktop App

**macOS:**
```bash
open HITOP-1.0.0-arm64.dmg
# Drag to Applications
# Right-click → Open (first time)
```

**Windows:**
```powershell
.\HITOP-Setup-1.0.0.exe
# Click "More info" → "Run anyway" if needed
```

**Linux:**
```bash
chmod +x HITOP-1.0.0-arm64.AppImage
./HITOP-1.0.0-arm64.AppImage
```

### Build from Source

```bash
git clone https://github.com/bohdaq/hitop.git
cd hitop
npm install
npm run electron:build:mac     # or :win or :linux
```

## 🔒 Privacy & Security

- ✅ 100% local - no data sent to servers
- ✅ No tracking or analytics
- ✅ No account required
- ✅ Open source - MIT License

## 🐛 Known Issues

- Apps are unsigned (may show security warnings on first launch)
- ARM64 builds only (x64/Intel builds coming soon)

**Workarounds:**
- **macOS**: Right-click → Open
- **Windows**: Click "More info" → "Run anyway"

## 🔮 Coming Soon

- x64/Intel builds
- Code signing
- Chrome extension
- GraphQL support
- WebSocket testing

## 🤝 Contributing

Contributions welcome! See [CONTRIBUTING.md](https://github.com/bohdaq/hitop/blob/main/CONTRIBUTING.md)

## 📄 License

MIT License - Copyright (c) 2025 Bohdan Tsap

## 🙏 Built With

React • Material-UI • Electron • electron-builder

---

**Full release notes**: [RELEASE_NOTES_1.0.0.md](https://github.com/bohdaq/hitop/blob/main/RELEASE_NOTES_1.0.0.md)

**Questions?** Open an [issue](https://github.com/bohdaq/hitop/issues) or [discussion](https://github.com/bohdaq/hitop/discussions)

⭐ **Star the repo** if you find HITOP useful!

Happy API testing! 🚀
