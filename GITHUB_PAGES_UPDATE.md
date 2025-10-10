# GitHub Pages Update - Downloads Section

## Changes Made

Updated the GitHub Pages landing page (docs/index.html) to include a comprehensive downloads section.

## What's New

### 1. Downloads Section

Added a new prominent "Download HITOP" section with:

- **Desktop App Downloads**
  - macOS (ARM64) - DMG installer
  - Windows (ARM64) - EXE installer
  - Linux (ARM64) - AppImage
  - Link to all downloads on GitHub Releases

- **Web App**
  - Direct link to launch web app
  - Browser compatibility info

- **Browser Extension**
  - Link to Firefox extension
  - Note about Chrome extension coming soon

### 2. System Requirements

Added system requirements box showing:
- macOS 10.12+ (Apple Silicon)
- Windows 10/11 (ARM64)
- Linux Ubuntu 18.04+ or equivalent
- Note about x64/Intel builds coming soon

### 3. Navigation Updates

- Added "Download Desktop App" button in header
- Updated CTA section with download button
- Added version badge (v1.0.0) in header
- Smooth scrolling for anchor links

### 4. Visual Improvements

- Three-column layout for download options
- Icons for each platform
- Consistent button styling
- Clear call-to-action buttons

## File Modified

- `docs/index.html` - Main landing page

## Preview

The downloads section includes:

```
📥 Download HITOP
Available as desktop app, web app, and browser extension

[Desktop App]        [Web App]           [Browser Extension]
🖥️                   🌐                  🦊
- macOS (ARM64)      Launch Web App      Get Extension
- Windows (ARM64)    Works in all        Firefox (Chrome soon)
- Linux (ARM64)      browsers
View all downloads →

📋 System Requirements
macOS: 10.12+ (Apple Silicon)
Windows: 10/11 (ARM64)
Linux: Ubuntu 18.04+
Note: x64/Intel builds coming soon
```

## Deploy

To deploy the updated page:

```bash
# Commit changes
git add docs/index.html
git commit -m "Add downloads section to GitHub Pages"
git push origin main

# GitHub Pages will auto-deploy in ~1 minute
```

## Verify

After deployment, check:
1. Visit: https://bohdaq.github.io/hitop/
2. Click "Download Desktop App" button
3. Verify downloads section appears
4. Test all download links
5. Verify smooth scrolling works

## Download Links

All links point to GitHub Releases:
- https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0-arm64.dmg
- https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-Setup-1.0.0.exe
- https://github.com/bohdaq/hitop/releases/download/v1.0.0/HITOP-1.0.0-arm64.AppImage
- https://github.com/bohdaq/hitop/releases/latest (for all downloads)

## Future Updates

When releasing new versions:

1. Update version badge in header:
   ```html
   <span class="badge">v1.1.0</span>
   ```

2. Update download links to new version:
   ```html
   href="https://github.com/bohdaq/hitop/releases/download/v1.1.0/..."
   ```

3. Update system requirements if needed

4. Add new platforms when available (x64 builds, Chrome extension, etc.)

## Testing Checklist

- [ ] Page loads correctly
- [ ] Download buttons work
- [ ] Links point to correct releases
- [ ] Smooth scrolling works
- [ ] Mobile responsive
- [ ] All sections visible
- [ ] Version badge shows v1.0.0

## Mobile View

The downloads section is responsive and stacks vertically on mobile devices for better usability.

---

**Status:** Ready to deploy
**Last Updated:** 2025-01-10
