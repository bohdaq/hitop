# Windows FFmpeg Error Fix

## Problem

Windows build shows error related to FFmpeg when trying to open the app.

## Root Cause

Electron on Windows sometimes has issues with hardware acceleration and GPU rendering, which can manifest as FFmpeg-related errors even though HITOP doesn't use FFmpeg directly.

## Solution Applied

The following fixes have been applied to the codebase:

### 1. Disabled Hardware Acceleration

In `electron/main.js`:
```javascript
app.disableHardwareAcceleration();
```

### 2. Added Command Line Switches

```javascript
app.commandLine.appendSwitch('disable-gpu');
app.commandLine.appendSwitch('disable-software-rasterizer');
app.commandLine.appendSwitch('disable-gpu-compositing');
app.commandLine.appendSwitch('no-sandbox');
```

## Rebuild Required

After applying these fixes, you need to rebuild the Windows app:

```powershell
# Clean previous build
Remove-Item -Recurse -Force dist

# Rebuild
npm run electron:build:win
```

## For Users with Existing Build

If you already have the app installed and it's not working:

### Option 1: Run with Flags (Temporary)

Create a shortcut with these flags:
```
"C:\Path\To\HITOP.exe" --disable-gpu --no-sandbox
```

### Option 2: Set Environment Variable

```powershell
# Set environment variable
$env:ELECTRON_DISABLE_HARDWARE_ACCELERATION = 1

# Then run the app
.\HITOP.exe
```

### Option 3: Download New Build

Wait for the updated build with fixes included.

## Testing

After rebuilding, test on Windows:

1. **Clean install:**
   ```powershell
   # Uninstall old version
   # Install new version
   .\HITOP-Setup-1.0.0.exe
   ```

2. **Run and verify:**
   - App should open without errors
   - Check if all features work
   - Test HTTP requests
   - Test collections

3. **Check logs:**
   ```powershell
   # App data location
   cd $env:APPDATA\hitop
   
   # Check for error logs
   dir
   ```

## Alternative Solutions

If the above doesn't work:

### 1. Install Visual C++ Redistributables

Download and install:
- [Microsoft Visual C++ 2015-2022 Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)

```powershell
# Or via winget
winget install Microsoft.VCRedist.2015+.x64
```

### 2. Update Windows

```powershell
# Check for Windows updates
# Settings → Windows Update → Check for updates
```

### 3. Update Graphics Drivers

Update your GPU drivers from:
- NVIDIA: https://www.nvidia.com/Download/index.aspx
- AMD: https://www.amd.com/en/support
- Intel: https://www.intel.com/content/www/us/en/download-center/home.html

### 4. Run in Compatibility Mode

Right-click HITOP.exe → Properties → Compatibility:
- Check "Run this program in compatibility mode for:"
- Select "Windows 8"
- Check "Disable fullscreen optimizations"
- Apply

## Verification

To verify the fix is applied in your build:

1. Open the app
2. Open DevTools (if in development mode)
3. Check console for GPU-related messages
4. Should see: "GPU process disabled"

## For Developers

### Build with Verbose Logging

```powershell
# Enable debug logging
$env:DEBUG = "electron-builder"
npm run electron:build:win
```

### Test in Different Environments

Test on:
- Windows 10 (various versions)
- Windows 11
- Different GPU vendors (NVIDIA, AMD, Intel)
- Virtual machines
- Systems with/without GPU

## Known Limitations

With hardware acceleration disabled:
- ❌ Slightly slower rendering (not noticeable for HITOP)
- ❌ No GPU-accelerated video (HITOP doesn't use video)
- ✅ Better compatibility
- ✅ More stable on various systems
- ✅ Lower GPU usage

## Performance Impact

For HITOP specifically:
- **No noticeable impact** - HITOP is a text-based app
- UI rendering is still fast
- HTTP requests unaffected
- Collections and variables work normally

## Future Improvements

Potential future solutions:
1. Conditional hardware acceleration (detect and enable if safe)
2. User setting to toggle hardware acceleration
3. Better error handling and fallback
4. Automatic detection of problematic systems

## Reporting Issues

If you still experience issues after applying these fixes:

1. **Collect information:**
   ```powershell
   # Windows version
   winver
   
   # System info
   systeminfo | findstr /B /C:"OS Name" /C:"OS Version"
   
   # GPU info
   wmic path win32_VideoController get name
   ```

2. **Check logs:**
   ```powershell
   # App logs location
   cd $env:APPDATA\hitop
   type hitop-data.json
   ```

3. **Open GitHub issue:**
   - Include Windows version
   - Include GPU information
   - Include error message
   - Include steps to reproduce

## Quick Fix Summary

**For Developers:**
```bash
# Already applied in codebase
# Just rebuild:
npm run electron:build:win
```

**For Users:**
```powershell
# Download new build with fixes
# Or run with flags:
HITOP.exe --disable-gpu --no-sandbox
```

## Status

- ✅ Fix applied to codebase
- ⏳ Rebuild required
- ⏳ Testing on Windows systems
- ⏳ New release with fix

## Related Issues

- Electron hardware acceleration on Windows
- FFmpeg codec errors in Electron
- GPU process crashes
- Windows compatibility issues

## References

- [Electron Command Line Switches](https://www.electronjs.org/docs/latest/api/command-line-switches)
- [Electron Hardware Acceleration](https://www.electronjs.org/docs/latest/tutorial/offscreen-rendering)
- [Windows Compatibility](https://www.electronjs.org/docs/latest/tutorial/windows-store-guide)

---

**Last Updated:** 2025-01-10
**Status:** Fixed - Rebuild Required
