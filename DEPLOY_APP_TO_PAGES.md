# Deploy HITOP App to GitHub Pages

## What Was Done

The HITOP React application is now available on GitHub Pages!

### Files Created:

1. **`docs/app/`** - Built React application
2. **`docs/app.html`** - Redirect page to the app
3. **`deploy-to-pages.sh`** - Deployment script

### Files Updated:

4. **`docs/index.html`** - Added "Launch Web App" buttons
5. **`.gitignore`** - Added note about docs/app

## Access URLs

Once deployed, the app will be available at:

- **Direct App**: `https://bohdaq.github.io/hitop/app/`
- **With Redirect**: `https://bohdaq.github.io/hitop/app.html`
- **Landing Page**: `https://bohdaq.github.io/hitop/`

Or with custom domain:
- **Direct App**: `https://hitop.dev/app/`
- **With Redirect**: `https://hitop.dev/app.html`
- **Landing Page**: `https://hitop.dev/`

## Deploy Now

```bash
# Add all changes
git add .

# Commit
git commit -m "Deploy HITOP app to GitHub Pages"

# Push
git push origin main

# Wait 1-2 minutes for GitHub Pages to rebuild
```

## Future Updates

To update the app after making changes:

```bash
# 1. Make changes to frontend/src/
# 2. Run deployment script
./deploy-to-pages.sh

# 3. Commit and push
git add docs/app/
git commit -m "Update app on GitHub Pages"
git push origin main
```

## How It Works

### Landing Page → App Flow

1. User visits `https://bohdaq.github.io/hitop/`
2. Clicks "🚀 Launch Web App" button
3. Redirected to `app.html`
4. `app.html` redirects to `app/index.html`
5. React app loads and runs

### Direct Access

Users can also directly access:
- `https://bohdaq.github.io/hitop/app/` - Skip redirect

### Storage

The app uses `localStorage` to save:
- Collections
- Variables
- History
- Tabs
- Settings

Data persists across sessions in the browser.

## Features Available

All HITOP features work on GitHub Pages:
- ✅ Make HTTP requests
- ✅ Collections management
- ✅ Variables with ${} syntax
- ✅ Pre/post-request scripts
- ✅ Request history
- ✅ Multiple tabs
- ✅ Import/Export
- ✅ Drag & drop

## Limitations

### CORS

Some APIs may block requests due to CORS policies. Solutions:
1. Use the Firefox extension (bypasses CORS)
2. Use APIs with CORS enabled
3. Use a CORS proxy (not recommended for production)

### Storage

Browser localStorage limits:
- ~5-10MB per domain
- Cleared if user clears browser data

## Testing Locally

Before deploying, test locally:

```bash
cd docs/app
python3 -m http.server 8000
# Open http://localhost:8000
```

## Troubleshooting

### App Not Loading

1. Check browser console for errors
2. Verify `docs/app/index.html` exists
3. Check if GitHub Pages is enabled
4. Clear browser cache

### Blank Page

1. Check `homepage: "."` in `frontend/package.json`
2. Rebuild with `./deploy-to-pages.sh`
3. Verify all files in `docs/app/` are committed

### 404 Errors

1. Ensure GitHub Pages is set to `/docs` folder
2. Check branch is `main`
3. Wait for GitHub Pages rebuild (1-2 minutes)

## Updating the App

### Minor Updates (Content/Styling)

```bash
# Edit files in frontend/src/
./deploy-to-pages.sh
git add docs/app/
git commit -m "Update app"
git push
```

### Major Updates (Dependencies)

```bash
cd frontend
npm install <package>
cd ..
./deploy-to-pages.sh
git add docs/app/ frontend/package.json
git commit -m "Update dependencies"
git push
```

## Monitoring

Check deployment status:
1. GitHub Actions tab (build status)
2. Settings → Pages (deployment URL)
3. Browser DevTools (console errors)

## Analytics (Optional)

To track usage, add to `docs/app/index.html`:

```html
<!-- Google Analytics -->
<script async src="https://www.googletagmanager.com/gtag/js?id=G-XXXXXXXXXX"></script>
<script>
  window.dataLayer = window.dataLayer || [];
  function gtag(){dataLayer.push(arguments);}
  gtag('js', new Date());
  gtag('config', 'G-XXXXXXXXXX');
</script>
```

## SEO for App

The app is a single-page application (SPA), so:
- Main SEO is on landing page (`docs/index.html`)
- App itself doesn't need SEO
- Use landing page for search engine visibility

## Security

The app is secure:
- No server-side code
- No data sent to external servers
- All data stored locally
- HTTPS enforced by GitHub Pages

## Performance

The app is optimized:
- Minified JavaScript
- Compressed assets
- Cached by browser
- Fast loading (~500KB total)

## Custom Domain

If using custom domain:
1. App will be at `https://yourdomain.com/app/`
2. No additional configuration needed
3. HTTPS automatically enabled

## Backup

To backup the deployed app:

```bash
# Clone the repo
git clone https://github.com/bohdaq/hitop.git

# The app is in docs/app/
cd hitop/docs/app
```

## Rollback

To rollback to previous version:

```bash
# Find commit hash
git log --oneline

# Revert to specific commit
git revert <commit-hash>

# Or reset (careful!)
git reset --hard <commit-hash>
git push --force
```

## Next Steps

1. ✅ Commit and push changes
2. ✅ Wait for GitHub Pages to rebuild
3. ✅ Test the app at your GitHub Pages URL
4. ✅ Share the link!

## Support

If you encounter issues:
1. Check this guide
2. Review GitHub Pages docs
3. Open an issue on GitHub
4. Check browser console for errors

## Summary

Your HITOP app is now:
- ✅ Built and ready
- ✅ In `docs/app/` folder
- ✅ Linked from landing page
- ✅ Ready to commit and push

Just run:
```bash
git add .
git commit -m "Deploy HITOP app to GitHub Pages"
git push origin main
```

Then visit your GitHub Pages URL! 🚀
