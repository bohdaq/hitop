# Changelog

All notable changes to HITOP will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-01-10

### Added

#### Desktop Applications
- Electron-based desktop applications for macOS, Windows, and Linux
- Native HTTP client that bypasses CORS restrictions
- File-based storage for collections and settings in app data directory
- System integration with native menus and shortcuts
- Offline capability - works without internet connection
- Auto-update infrastructure (configurable)

#### Web Application
- GitHub Pages deployment at https://bohdaq.github.io/hitop/
- Progressive Web App (PWA) features
- Responsive design for mobile and tablet devices
- Landing page with feature showcase

#### Browser Extension
- Firefox extension with full HITOP functionality
- No CORS limitations in extension mode
- Seamless browser integration

#### Core Features
- Multiple HTTP methods: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS
- Request management with full control over headers, body, and URL parameters
- Response viewer with syntax highlighting for JSON, XML, HTML, and text
- Status code display with descriptions
- Response headers viewer

#### Collections & Organization
- Collections for organizing requests
- Nested folder structure
- Drag & drop reordering of requests and folders
- Import/Export in JSON format
- Search functionality across collections

#### Variables & Scripting
- Collection-level variables
- Variable interpolation using `${variableName}` syntax
- Pre-request scripts with JavaScript execution
- Post-response scripts for processing responses
- Context API for accessing and modifying request/response data

#### Advanced Features
- Automatic request history tracking with search
- Multiple tabs for working on multiple requests simultaneously
- Keyboard shortcuts for efficient workflow
- Dark/Light mode support
- Persistent storage with all data saved locally
- Privacy-first approach - no data sent to external servers

#### Documentation
- Comprehensive README with feature overview
- Electron desktop app guide (ELECTRON_GUIDE.md)
- Windows build instructions (BUILD_ON_WINDOWS.md)
- Windows quick start guide (WINDOWS_QUICK_START.md)
- Windows troubleshooting guide (WINDOWS_TROUBLESHOOTING.md)
- Custom scripting guide (SCRIPTING_GUIDE.md)
- Variables usage guide (VARIABLES_GUIDE.md)
- Firefox extension guide (EXTENSION_GUIDE.md)
- GitHub Pages setup guide (GITHUB_PAGES_SETUP.md)
- DNS configuration guide (DNS_SETUP.md)

#### Build Tools
- Build scripts for macOS, Windows, and Linux
- Automated build script (build-electron.sh)
- Windows batch build script (build-electron.bat)
- GitHub Pages deployment script (deploy-to-pages.sh)
- Firefox extension build script (build-extension.sh)

### Changed
- N/A (Initial release)

### Deprecated
- N/A (Initial release)

### Removed
- N/A (Initial release)

### Fixed
- N/A (Initial release)

### Security
- All data stored locally - no external server communication
- No tracking or analytics
- No account required
- Open source with MIT License

## [Unreleased]

### Planned Features
- x64/Intel builds for all platforms
- Code signing for desktop applications
- Chrome extension
- GraphQL support
- WebSocket testing
- gRPC support
- Team collaboration features
- Optional cloud sync
- API mocking capabilities
- Performance testing tools

---

## Release Links

- [1.0.0] - https://github.com/bohdaq/hitop/releases/tag/v1.0.0

## Version History

| Version | Date | Description |
|---------|------|-------------|
| 1.0.0 | 2025-01-10 | First official release |

## Migration Guides

### Upgrading to 1.0.0
This is the first release - no migration needed.

## Breaking Changes

### 1.0.0
No breaking changes (initial release).

---

For more details on each release, see the [Releases page](https://github.com/bohdaq/hitop/releases).
