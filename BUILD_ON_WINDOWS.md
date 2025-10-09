# Building HITOP Electron App on Windows 11

## Prerequisites

### 1. Install Node.js

Download and install Node.js 16+ from [nodejs.org](https://nodejs.org/)

```powershell
# Verify installation
node --version
npm --version
```

### 2. Install Git

Download from [git-scm.com](https://git-scm.com/download/win)

```powershell
# Verify installation
git --version
```

### 3. Install Build Tools (Optional but Recommended)

**IMPORTANT**: `windows-build-tools` is deprecated. Use one of these methods instead:

#### Option A: Visual Studio Build Tools (Recommended)

1. Download from [visualstudio.microsoft.com](https://visualstudio.microsoft.com/downloads/)
2. Run installer
3. Select **"Desktop development with C++"**
4. Click Install

#### Option B: Automated Install via npm

```powershell
# Run PowerShell as Administrator
npm install --global --production windows-build-tools@4.0.0
```

**Note**: If you get errors, use Option A instead.

#### Option C: Install Python + Visual Studio separately

```powershell
# 1. Install Python 3.x from python.org
# 2. Install Visual Studio Build Tools (Option A above)
```

**For HITOP**: Build tools are **optional**. The app will build fine without them on most systems.

## Clone Repository

```powershell
# Open PowerShell or Command Prompt
cd C:\Users\YourUsername\Documents
git clone https://github.com/bohdaq/hitop.git
cd hitop
```

## Install Dependencies

```powershell
# Install root dependencies
npm install

# This will also install frontend dependencies via postinstall script
```

## Build Commands

### Build for Windows

```powershell
# Build Windows installer and portable
npm run electron:build:win
```

This creates:
- `dist\HITOP Setup 1.0.0.exe` - Installer
- `dist\HITOP 1.0.0.exe` - Portable version

### Build for macOS (from Windows)

```powershell
npm run electron:build:mac
```

**Note**: macOS builds from Windows have limitations (no code signing)

### Build for Linux (from Windows)

```powershell
npm run electron:build:linux
```

### Build for All Platforms

```powershell
npm run electron:build
```

## Development Mode

Run the app in development with hot reload:

```powershell
npm run electron:dev
```

This will:
1. Start React dev server on port 3000
2. Launch Electron with DevTools open
3. Auto-reload on code changes

## Using build-electron.sh on Windows

### Option 1: Git Bash

If you have Git for Windows installed:

```bash
# Open Git Bash
cd /c/Users/YourUsername/Documents/hitop
./build-electron.sh
```

### Option 2: WSL (Windows Subsystem for Linux)

```bash
# Install WSL if not already installed
wsl --install

# In WSL terminal
cd /mnt/c/Users/YourUsername/Documents/hitop
./build-electron.sh
```

### Option 3: Create Windows Batch Script

Create `build-electron.bat`:

```batch
@echo off
echo Building HITOP Electron App...

echo Installing dependencies...
if not exist "node_modules" (
  npm install
)

echo Building React application...
cd frontend
call npm run build
cd ..

echo Building Electron application...
echo Building for Windows...
npx electron-builder --win

echo Build complete!
echo.
echo Output directory: dist\
echo.
dir dist\*.exe

echo.
echo HITOP Electron app is ready!
pause
```

Then run:
```powershell
.\build-electron.bat
```

## Troubleshooting

### Error: "electron-builder: command not found"

```powershell
# Use npx instead
npx electron-builder --win
```

### Error: "EPERM: operation not permitted"

Run PowerShell as Administrator or disable antivirus temporarily.

### Error: "Python not found"

Install Python 3.x from [python.org](https://www.python.org/downloads/)

```powershell
# Verify
python --version
```

### Error: "MSBuild not found"

Install Visual Studio Build Tools:
1. Download from [visualstudio.microsoft.com](https://visualstudio.microsoft.com/downloads/)
2. Select "Desktop development with C++"
3. Install

### Build is Slow

First build downloads Electron binaries (~100MB) and takes 5-10 minutes.
Subsequent builds are faster (1-2 minutes).

### Antivirus Blocking

Windows Defender or antivirus may block electron-builder:
1. Add exception for `node_modules` folder
2. Add exception for `dist` folder
3. Temporarily disable during build

## Build Output

After successful build, find files in `dist\` folder:

```
dist\
├── HITOP Setup 1.0.0.exe       (Installer - 84 MB)
├── HITOP 1.0.0.exe             (Portable - 84 MB)
├── win-unpacked\               (Unpacked app folder)
└── builder-effective-config.yaml
```

## Testing the Built App

### Test Installer

```powershell
# Run the installer
.\dist\"HITOP Setup 1.0.0.exe"

# App installs to:
# C:\Users\YourUsername\AppData\Local\Programs\HITOP
```

### Test Portable

```powershell
# Run directly
.\dist\"HITOP 1.0.0.exe"
```

### Test Unpacked

```powershell
# Run from unpacked folder
.\dist\win-unpacked\HITOP.exe
```

## Code Signing (Optional)

To sign the Windows executable:

### 1. Get Code Signing Certificate

Purchase from:
- Sectigo
- DigiCert
- GlobalSign

### 2. Set Environment Variables

```powershell
# In PowerShell
$env:CSC_LINK = "C:\path\to\certificate.pfx"
$env:CSC_KEY_PASSWORD = "your_password"

# Then build
npm run electron:build:win
```

### 3. Or Add to package.json

```json
"win": {
  "certificateFile": "path/to/certificate.pfx",
  "certificatePassword": "your_password"
}
```

## Architecture-Specific Builds

### Build for x64 (Intel/AMD)

```powershell
npx electron-builder --win --x64
```

### Build for ARM64

```powershell
npx electron-builder --win --arm64
```

### Build for Both

```powershell
npx electron-builder --win --x64 --arm64
```

## CI/CD on Windows

### GitHub Actions

Create `.github/workflows/build-windows.yml`:

```yaml
name: Build Windows

on:
  push:
    branches: [ main ]

jobs:
  build:
    runs-on: windows-latest
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Setup Node.js
      uses: actions/setup-node@v3
      with:
        node-version: '18'
    
    - name: Install dependencies
      run: npm install
    
    - name: Build
      run: npm run electron:build:win
    
    - name: Upload artifacts
      uses: actions/upload-artifact@v3
      with:
        name: windows-build
        path: dist/*.exe
```

## Performance Tips

### 1. Use SSD

Build on SSD drive for faster compilation.

### 2. Exclude from Antivirus

Add these folders to antivirus exclusions:
- `node_modules\`
- `dist\`
- `frontend\build\`

### 3. Close Other Apps

Close heavy applications during build for better performance.

### 4. Use PowerShell 7

Install PowerShell 7 for better performance:
```powershell
winget install Microsoft.PowerShell
```

## Common Windows-Specific Issues

### Long Path Names

Enable long paths in Windows:

```powershell
# Run as Administrator
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force
```

### Permission Issues

Run PowerShell as Administrator:
```powershell
# Right-click PowerShell → "Run as Administrator"
```

### Network Issues

If npm install fails:
```powershell
# Use different registry
npm config set registry https://registry.npmjs.org/

# Or use proxy
npm config set proxy http://proxy.company.com:8080
npm config set https-proxy http://proxy.company.com:8080
```

## Quick Start Commands

```powershell
# Complete build process
git clone https://github.com/bohdaq/hitop.git
cd hitop
npm install
npm run electron:build:win

# Development
npm run electron:dev

# Build only (if already installed)
npm run electron:build:win
```

## File Locations

### Source Code
```
C:\Users\YourUsername\Documents\hitop\
```

### Built Apps
```
C:\Users\YourUsername\Documents\hitop\dist\
```

### Installed App (after installation)
```
C:\Users\YourUsername\AppData\Local\Programs\HITOP\
```

### App Data (user data)
```
C:\Users\YourUsername\AppData\Roaming\hitop\
```

## System Requirements

### For Building:
- Windows 10/11
- 8GB RAM minimum (16GB recommended)
- 5GB free disk space
- Node.js 16+
- Internet connection (first build)

### For Running Built App:
- Windows 10/11
- 4GB RAM
- 500MB disk space

## Next Steps

1. ✅ Install Node.js and Git
2. ✅ Clone repository
3. ✅ Run `npm install`
4. ✅ Run `npm run electron:build:win`
5. ✅ Test `dist\HITOP Setup 1.0.0.exe`
6. 🎉 Distribute your app!

## Support

- [Electron Builder Docs](https://www.electron.build/)
- [Node.js Windows Guide](https://nodejs.org/en/download/package-manager/)
- [GitHub Issues](https://github.com/bohdaq/hitop/issues)

## Additional Resources

- [Windows Terminal](https://aka.ms/terminal) - Better terminal experience
- [Visual Studio Code](https://code.visualstudio.com/) - Recommended IDE
- [Git for Windows](https://gitforwindows.org/) - Git with Bash
- [nvm-windows](https://github.com/coreybutler/nvm-windows) - Node version manager

---

**Note**: Replace `bohdaq` with your GitHub username and `bohdan.tsap@example.com` with your email in `package.json` before building.
