# HITOP v1.0.0 Release Notes

## 🎉 First Official Release!

We're excited to announce the first stable release of HITOP - a powerful, privacy-focused HTTP API testing tool!

## 📦 Download

### Desktop Applications

**macOS (Apple Silicon)**
- [HITOP-1.0.0-arm64.dmg](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0-arm64.dmg) (101 MB) - Installer
- [HITOP-1.0.0-arm64-mac.zip](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0-arm64-mac.zip) (97 MB) - Portable

**Windows (ARM64)**
- [HITOP Setup 1.0.0.exe](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-Setup-1.0.0.exe) (84 MB) - Installer
- [HITOP 1.0.0.exe](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0.exe) (84 MB) - Portable

**Linux (ARM64)**
- [HITOP-1.0.0-arm64.AppImage](https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0-arm64.AppImage) (111 MB) - Universal
- [hitop_1.0.0_arm64.deb](https://github.com/bohdaq/hitop/releases/download/v1.0.0/hitop_1.0.0_arm64.deb) (73 MB) - Debian/Ubuntu

### Web Application

- **Live Demo**: [https://bohdaq.github.io/hitop/app/](https://bohdaq.github.io/hitop/app/)
- **Landing Page**: [https://bohdaq.github.io/hitop/](https://bohdaq.github.io/hitop/)

### Browser Extension

- **Firefox**: [Install from Firefox Add-ons](https://addons.mozilla.org/firefox/addon/hitop/)

## ✨ Features

### Core Functionality

- ✅ **Multiple HTTP Methods**: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS
- ✅ **Request Management**: Full control over headers, body, and URL parameters
- ✅ **Response Viewer**: Syntax-highlighted JSON, XML, HTML, and text responses
- ✅ **Status Codes**: Clear display with descriptions
- ✅ **Response Headers**: View all response headers and metadata

### Collections & Organization

- ✅ **Collections**: Organize requests into collections
- ✅ **Folders**: Nested folder structure for better organization
- ✅ **Drag & Drop**: Reorder requests and folders easily
- ✅ **Import/Export**: JSON format for sharing and backup
- ✅ **Search**: Quick search across collections

### Variables & Scripting

- ✅ **Collection Variables**: Define variables at collection level
- ✅ **Variable Interpolation**: Use `${variableName}` syntax in URLs, headers, and body
- ✅ **Pre-request Scripts**: JavaScript execution before sending requests
- ✅ **Post-response Scripts**: Process responses with custom JavaScript
- ✅ **Context API**: Access and modify request/response data programmatically

### Advanced Features

- ✅ **Request History**: Automatic history tracking with search
- ✅ **Multiple Tabs**: Work on multiple requests simultaneously
- ✅ **Keyboard Shortcuts**: Efficient workflow with shortcuts
- ✅ **Dark/Light Mode**: Comfortable viewing in any environment
- ✅ **Persistent Storage**: All data saved locally
- ✅ **Privacy First**: No data sent to external servers

### Desktop App Exclusive

- ✅ **No CORS Issues**: Direct HTTP requests bypass browser restrictions
- ✅ **Native Performance**: Fast and responsive desktop experience
- ✅ **Offline Capable**: Works completely offline
- ✅ **File System Access**: Save/load collections from disk
- ✅ **System Integration**: Native menus and shortcuts

## 🚀 What's New in 1.0.0

### Desktop Applications
- **Electron-based desktop apps** for macOS, Windows, and Linux
- **Native HTTP client** that bypasses CORS restrictions
- **File-based storage** for collections and settings
- **Auto-update ready** infrastructure (can be configured)

### Web Application
- **GitHub Pages deployment** - accessible from anywhere
- **Progressive Web App** features
- **Responsive design** for mobile and tablet

### Browser Extension
- **Firefox extension** with full functionality
- **No CORS limitations** in extension mode
- **Seamless integration** with browser

### Documentation
- **Comprehensive guides** for all platforms
- **Windows build instructions** with troubleshooting
- **Scripting guide** with examples
- **Variables guide** with use cases

## 📚 Documentation

- [README.md](https://github.com/bohdaq/hitop/blob/main/README.md) - Overview and features
- [ELECTRON_GUIDE.md](https://github.com/bohdaq/hitop/blob/main/ELECTRON_GUIDE.md) - Desktop app guide
- [BUILD_ON_WINDOWS.md](https://github.com/bohdaq/hitop/blob/main/BUILD_ON_WINDOWS.md) - Windows build instructions
- [SCRIPTING_GUIDE.md](https://github.com/bohdaq/hitop/blob/main/SCRIPTING_GUIDE.md) - Custom scripts documentation
- [VARIABLES_GUIDE.md](https://github.com/bohdaq/hitop/blob/main/VARIABLES_GUIDE.md) - Variables usage guide
- [EXTENSION_GUIDE.md](https://github.com/bohdaq/hitop/blob/main/EXTENSION_GUIDE.md) - Firefox extension guide

## 🔧 Installation

### Desktop App

**macOS:**
```bash
# Download and open DMG
open HITOP-1.0.0-arm64.dmg
# Drag to Applications folder
```

**Windows:**
```powershell
# Run installer
.\HITOP-Setup-1.0.0.exe
# Or run portable version
.\HITOP-1.0.0.exe
```

**Linux:**
```bash
# AppImage
chmod +x HITOP-1.0.0-arm64.AppImage
./HITOP-1.0.0-arm64.AppImage

# Debian/Ubuntu
sudo dpkg -i hitop_1.0.0_arm64.deb
```

### Web App

Simply visit: [https://bohdaq.github.io/hitop/app/](https://bohdaq.github.io/hitop/app/)

### Firefox Extension

Install from: [Firefox Add-ons](https://addons.mozilla.org/firefox/addon/hitop/)

## 🛠️ Building from Source

### Prerequisites
- Node.js 16+
- npm 8+
- Git

### Build Desktop App

```bash
# Clone repository
git clone https://github.com/bohdaq/hitop.git
cd hitop

# Install dependencies
npm install

# Build for your platform
npm run electron:build:mac     # macOS
npm run electron:build:win     # Windows
npm run electron:build:linux   # Linux
```

### Run in Development

```bash
# Desktop app with hot reload
npm run electron:dev

# Web app
cd frontend
npm start
```

## 🔒 Privacy & Security

- ✅ **100% Local**: All data stored locally on your device
- ✅ **No Tracking**: No analytics, no telemetry, no data collection
- ✅ **No Account Required**: Use immediately without sign-up
- ✅ **Open Source**: Full transparency - review the code
- ✅ **MIT License**: Free to use, modify, and distribute

## 🌟 Use Cases

### API Development
- Test REST APIs during development
- Debug API responses
- Validate request/response formats

### QA Testing
- Automated API testing with scripts
- Regression testing with collections
- Environment-specific testing with variables

### API Documentation
- Create example requests
- Share collections with team
- Document API workflows

### Learning & Education
- Learn HTTP protocols
- Understand REST APIs
- Practice API integration

## 🐛 Known Issues

- Desktop app is unsigned (macOS/Windows may show security warnings)
- First launch may be slow (loading resources)
- Some antivirus software may flag the installer (false positive)

**Workarounds:**
- **macOS**: Right-click → Open (first time)
- **Windows**: Click "More info" → "Run anyway"
- **Antivirus**: Add exception for HITOP

## 🔮 Future Plans

- [ ] x64/Intel builds for all platforms
- [ ] Code signing for desktop apps
- [ ] Chrome extension
- [ ] GraphQL support
- [ ] WebSocket testing
- [ ] gRPC support
- [ ] Team collaboration features
- [ ] Cloud sync (optional)
- [ ] API mocking
- [ ] Performance testing

## 🤝 Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](https://github.com/bohdaq/hitop/blob/main/CONTRIBUTING.md) for guidelines.

### Ways to Contribute
- 🐛 Report bugs
- 💡 Suggest features
- 📝 Improve documentation
- 🔧 Submit pull requests
- ⭐ Star the repository

## 📄 License

MIT License - see [LICENSE](https://github.com/bohdaq/hitop/blob/main/LICENSE) for details.

Copyright (c) 2025 Bohdan Tsap

## 🙏 Acknowledgments

Built with:
- [React](https://reactjs.org/) - UI framework
- [Material-UI](https://mui.com/) - Component library
- [Electron](https://www.electronjs.org/) - Desktop framework
- [electron-builder](https://www.electron.build/) - Build tool

## 📞 Support

- **Issues**: [GitHub Issues](https://github.com/bohdaq/hitop/issues)
- **Discussions**: [GitHub Discussions](https://github.com/bohdaq/hitop/discussions)
- **Email**: bohdan.tsap@example.com

## 🔗 Links

- **Repository**: https://github.com/bohdaq/hitop
- **Website**: https://bohdaq.github.io/hitop/
- **Web App**: https://bohdaq.github.io/hitop/app/
- **Firefox Extension**: https://addons.mozilla.org/firefox/addon/hitop/

---

## Checksums (SHA256)

```
HITOP-1.0.0-arm64.dmg:           [to be generated]
HITOP-1.0.0-arm64-mac.zip:       [to be generated]
HITOP Setup 1.0.0.exe:           [to be generated]
HITOP 1.0.0.exe:                 [to be generated]
HITOP-1.0.0-arm64.AppImage:      [to be generated]
hitop_1.0.0_arm64.deb:           [to be generated]
```

To verify downloads:
```bash
# macOS/Linux
shasum -a 256 HITOP-1.0.0-arm64.dmg

# Windows
certutil -hashfile "HITOP Setup 1.0.0.exe" SHA256
```

---

**Thank you for using HITOP! 🎉**

If you find HITOP useful, please consider:
- ⭐ Starring the repository
- 🐦 Sharing on social media
- 📝 Writing a review
- 🤝 Contributing to the project

Happy API testing! 🚀
