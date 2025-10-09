# Windows 11 Quick Start Guide

## 🚀 Fast Track (5 Minutes)

### 1. Install Prerequisites

**Node.js** (Required):
```
Download: https://nodejs.org/
Install: Run installer → Next → Next → Install
Verify: Open PowerShell → node --version
```

**Git** (Required):
```
Download: https://git-scm.com/download/win
Install: Run installer → Use defaults
Verify: Open PowerShell → git --version
```

**Build Tools** (NOT Required):
```
⚠️ SKIP windows-build-tools - it's deprecated and causes errors
✅ HITOP builds fine without any build tools
```

### 2. Get the Code

Open **PowerShell** and run:

```powershell
# Navigate to your projects folder
cd C:\Users\YourUsername\Documents

# Clone repository
git clone https://github.com/bohdaq/hitop.git

# Enter directory
cd hitop
```

### 3. Build the App

**Option A: Using Batch File (Easiest)**
```powershell
.\build-electron.bat
```

**Option B: Using npm**
```powershell
# Install dependencies
npm install

# Build for Windows
npm run electron:build:win
```

**Option C: Using Git Bash**
```bash
./build-electron.sh
```

### 4. Find Your App

Built files are in `dist\` folder:
- `HITOP Setup 1.0.0.exe` - Installer (84 MB)
- `HITOP 1.0.0.exe` - Portable (84 MB)

### 5. Test It

```powershell
# Run the portable version
.\dist\"HITOP 1.0.0.exe"
```

## ⚡ One-Line Build

```powershell
git clone https://github.com/bohdaq/hitop.git && cd hitop && npm install && npm run electron:build:win
```

## 🛠️ Development Mode

```powershell
# Run with hot reload
npm run electron:dev
```

Opens app with DevTools for debugging.

## 📋 Common Commands

```powershell
# Install dependencies
npm install

# Build Windows app
npm run electron:build:win

# Build macOS app (from Windows)
npm run electron:build:mac

# Build Linux app (from Windows)
npm run electron:build:linux

# Build all platforms
npm run electron:build

# Development mode
npm run electron:dev

# Run built app
npm run electron
```

## 🐛 Quick Fixes

### "windows-build-tools" error
→ **IGNORE IT!** You don't need windows-build-tools for HITOP
→ Just run: `npm install` then `npm run electron:build:win`

### "node is not recognized"
→ Install Node.js from nodejs.org

### "npm install" fails
→ Run PowerShell as Administrator

### Antivirus blocks build
→ Add exception for `hitop` folder

### Build is slow
→ Normal for first build (downloads ~100MB)

### "electron-builder not found"
→ Use `npx electron-builder --win` instead

### More issues?
→ See [WINDOWS_TROUBLESHOOTING.md](./WINDOWS_TROUBLESHOOTING.md)

## 📁 File Locations

```
Source Code:
C:\Users\YourUsername\Documents\hitop\

Built Apps:
C:\Users\YourUsername\Documents\hitop\dist\

After Installation:
C:\Users\YourUsername\AppData\Local\Programs\HITOP\

App Data:
C:\Users\YourUsername\AppData\Roaming\hitop\
```

## 🎯 Build Times

- **First build**: 5-10 minutes (downloads Electron)
- **Subsequent builds**: 1-2 minutes
- **Development mode**: 30 seconds startup

## 💾 Disk Space

- Source code: ~500 MB
- node_modules: ~400 MB
- Built app: ~200 MB
- **Total needed**: ~1.5 GB

## ✅ System Requirements

**For Building:**
- Windows 10/11
- 8GB RAM (16GB recommended)
- 5GB free space
- Internet connection

**For Running:**
- Windows 10/11
- 4GB RAM
- 500MB space

## 🎓 Next Steps

1. **Customize**: Edit `package.json` with your info
2. **Sign**: Get code signing certificate (optional)
3. **Distribute**: Upload to GitHub Releases
4. **Update**: Bump version and rebuild

## 📚 Full Documentation

See `BUILD_ON_WINDOWS.md` for detailed instructions.

## 🆘 Need Help?

- [Full Windows Guide](./BUILD_ON_WINDOWS.md)
- [Electron Guide](./ELECTRON_GUIDE.md)
- [GitHub Issues](https://github.com/bohdaq/hitop/issues)

---

**Quick Test:**
```powershell
node --version  # Should show v16+ or v18+
npm --version   # Should show 8+ or 9+
git --version   # Should show 2.x
```

If all three work, you're ready to build! 🚀
