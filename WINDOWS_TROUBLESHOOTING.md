# Windows Build Troubleshooting

## Common Errors and Solutions

### ❌ Error: FFmpeg error when opening app

**Error Message:**
```
Error loading ffmpeg
Failed to load library
```

**Cause:** Electron hardware acceleration issues on Windows.

**Solution:** This has been fixed in the latest version. Rebuild or download new version.

#### Quick Fix for Existing Build:

```powershell
# Run with GPU disabled
.\HITOP.exe --disable-gpu --no-sandbox
```

Or set environment variable:
```powershell
$env:ELECTRON_DISABLE_HARDWARE_ACCELERATION = 1
.\HITOP.exe
```

#### Permanent Fix:

Rebuild with latest code (fix already applied):
```powershell
git pull
npm run electron:build:win
```

See [WINDOWS_FFMPEG_FIX.md](./WINDOWS_FFMPEG_FIX.md) for detailed information.

---

### ❌ Error: "windows-build-tools" process.env descriptor

**Error Message:**
```
Error process.env only accepts a configurable writable and enumerable data descriptor
```

**Cause:** `windows-build-tools` package is deprecated and incompatible with modern Node.js versions.

**Solution:** You don't need `windows-build-tools` for HITOP! Skip it and build directly.

#### Quick Fix:

```powershell
# Just build the app - no build tools needed
npm install
npm run electron:build:win
```

#### If You Really Need Build Tools:

**Option 1: Visual Studio Build Tools (Best)**

1. Download: https://visualstudio.microsoft.com/downloads/
2. Scroll to "All Downloads" → "Tools for Visual Studio"
3. Download "Build Tools for Visual Studio 2022"
4. Run installer
5. Select "Desktop development with C++"
6. Install (takes ~10 minutes)

**Option 2: Chocolatey (Package Manager)**

```powershell
# Install Chocolatey first (if not installed)
Set-ExecutionPolicy Bypass -Scope Process -Force; [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))

# Then install build tools
choco install visualstudio2022buildtools visualstudio2022-workload-vctools
```

**Option 3: Manual Python + VS**

```powershell
# 1. Install Python 3.11
winget install Python.Python.3.11

# 2. Install Visual Studio Build Tools (Option 1 above)
```

---

## Other Common Errors

### ❌ "node is not recognized"

**Solution:**
```powershell
# Install Node.js from nodejs.org
# Then restart PowerShell
```

### ❌ "npm install" fails with permission error

**Solution:**
```powershell
# Run PowerShell as Administrator
# Right-click PowerShell → "Run as Administrator"
```

### ❌ "electron-builder: command not found"

**Solution:**
```powershell
# Use npx instead
npx electron-builder --win
```

Or update package.json scripts (already done in HITOP).

### ❌ Antivirus blocks build

**Solution:**
```powershell
# Add folder to Windows Defender exclusions
Add-MpPreference -ExclusionPath "C:\Users\YourUsername\Documents\hitop"
```

Or temporarily disable antivirus during build.

### ❌ "EPERM: operation not permitted"

**Solutions:**

1. **Close all apps** that might be using files (VS Code, file explorer)
2. **Run as Administrator**
3. **Disable antivirus** temporarily
4. **Delete and retry:**
   ```powershell
   Remove-Item -Recurse -Force node_modules
   Remove-Item -Recurse -Force dist
   npm install
   npm run electron:build:win
   ```

### ❌ "Python not found"

**Solution:**
```powershell
# Install Python
winget install Python.Python.3.11

# Verify
python --version
```

### ❌ "MSBuild not found"

**Solution:** Install Visual Studio Build Tools (see Option 1 above)

### ❌ Build hangs or freezes

**Solutions:**

1. **Check internet connection** (first build downloads ~100MB)
2. **Wait longer** (first build takes 5-10 minutes)
3. **Kill and restart:**
   ```powershell
   # Press Ctrl+C to stop
   # Then retry
   npm run electron:build:win
   ```

### ❌ "Cannot find module 'electron'"

**Solution:**
```powershell
# Reinstall dependencies
Remove-Item -Recurse -Force node_modules
npm install
```

### ❌ "Out of memory" error

**Solutions:**

1. **Close other apps** to free RAM
2. **Increase Node memory:**
   ```powershell
   $env:NODE_OPTIONS="--max-old-space-size=4096"
   npm run electron:build:win
   ```

### ❌ Long path names error

**Solution:**
```powershell
# Enable long paths (Run as Administrator)
New-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem" -Name "LongPathsEnabled" -Value 1 -PropertyType DWORD -Force

# Restart PowerShell
```

### ❌ Network/proxy issues

**Solution:**
```powershell
# Configure npm proxy
npm config set proxy http://proxy.company.com:8080
npm config set https-proxy http://proxy.company.com:8080

# Or use different registry
npm config set registry https://registry.npmjs.org/
```

### ❌ "gyp ERR!" errors

**Cause:** Missing build tools (but HITOP doesn't need them)

**Solution:** Ignore these warnings if build completes successfully. Or install Visual Studio Build Tools.

---

## Verification Commands

Check if everything is installed correctly:

```powershell
# Node.js
node --version
# Should show: v16.x.x or v18.x.x or higher

# npm
npm --version
# Should show: 8.x.x or 9.x.x or higher

# Git
git --version
# Should show: 2.x.x

# Python (optional)
python --version
# Should show: 3.x.x

# Check if electron-builder is available
npx electron-builder --version
# Should show: 24.x.x
```

---

## Clean Build

If all else fails, start fresh:

```powershell
# 1. Delete everything
Remove-Item -Recurse -Force node_modules
Remove-Item -Recurse -Force frontend\node_modules
Remove-Item -Recurse -Force frontend\build
Remove-Item -Recurse -Force dist
Remove-Item package-lock.json
Remove-Item frontend\package-lock.json

# 2. Reinstall
npm install

# 3. Build
npm run electron:build:win
```

---

## Performance Issues

### Build is very slow

**Normal times:**
- First build: 5-10 minutes (downloads Electron)
- Subsequent builds: 1-2 minutes

**Speed up:**

1. **Use SSD** instead of HDD
2. **Close other apps** to free resources
3. **Disable antivirus** during build
4. **Use wired internet** for downloads
5. **Upgrade RAM** (16GB recommended)

### High CPU/Memory usage

**Normal:** Electron builds use lots of resources temporarily.

**If problematic:**
```powershell
# Limit Node memory
$env:NODE_OPTIONS="--max-old-space-size=2048"
npm run electron:build:win
```

---

## Windows-Specific Tips

### Use Windows Terminal

Better than PowerShell:
```powershell
# Install via Microsoft Store or:
winget install Microsoft.WindowsTerminal
```

### Use PowerShell 7

Newer and faster:
```powershell
winget install Microsoft.PowerShell
```

### Use Package Manager

Install tools easily:
```powershell
# Winget (built into Windows 11)
winget install NodeJS.NodeJS
winget install Git.Git

# Or Chocolatey
choco install nodejs git
```

---

## Still Having Issues?

### 1. Check System Requirements

- Windows 10/11 (64-bit)
- 8GB RAM minimum (16GB recommended)
- 5GB free disk space
- Internet connection

### 2. Update Everything

```powershell
# Update npm
npm install -g npm@latest

# Update Node.js
# Download latest from nodejs.org

# Update Windows
# Settings → Windows Update → Check for updates
```

### 3. Try Different Method

If `npm run electron:build:win` fails:

```powershell
# Method 1: Direct npx
cd frontend
npm run build
cd ..
npx electron-builder --win

# Method 2: Use batch file
.\build-electron.bat

# Method 3: Use Git Bash
bash ./build-electron.sh
```

### 4. Get Help

- Check [BUILD_ON_WINDOWS.md](./BUILD_ON_WINDOWS.md)
- Check [Electron Builder docs](https://www.electron.build/)
- Open [GitHub Issue](https://github.com/bohdaq/hitop/issues)
- Search error message on Google/Stack Overflow

---

## Quick Reference

### Minimal Build (No Build Tools)

```powershell
# This works on most Windows systems
npm install
npm run electron:build:win
```

### Full Build (With Build Tools)

```powershell
# 1. Install Visual Studio Build Tools
# 2. Then build
npm install
npm run electron:build:win
```

### Development Build

```powershell
# Faster, for testing
npm run electron:dev
```

---

## Success Checklist

✅ Node.js 16+ installed
✅ Git installed
✅ Repository cloned
✅ `npm install` completed
✅ `npm run electron:build:win` completed
✅ Files in `dist\` folder
✅ App runs: `.\dist\"HITOP 1.0.0.exe"`

If all checked, you're done! 🎉

---

**Remember:** For HITOP, you **don't need** `windows-build-tools`. Just Node.js and Git are enough!
