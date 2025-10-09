# GitHub Pages Setup Guide

## Quick Setup

### Step 1: Enable GitHub Pages

1. Go to your repository on GitHub: `https://github.com/bohdaq/hitop`
2. Click **Settings** (top right)
3. Scroll down to **Pages** section (left sidebar)
4. Under "Build and deployment":
   - **Source**: Deploy from a branch
   - **Branch**: `main`
   - **Folder**: `/docs`
5. Click **Save**

### Step 2: Wait for Deployment

- GitHub will automatically build and deploy your site
- This takes 1-2 minutes
- You'll see a green checkmark when ready

### Step 3: Access Your Site

Your site will be available at:
```
https://bohdaq.github.io/hitop/
```

## Custom Domain (Optional)

If you want to use a custom domain like `hitop.dev`:

### Step 1: Buy Domain

Buy a domain from:
- Namecheap
- GoDaddy
- Google Domains
- Cloudflare

### Step 2: Configure DNS

Add these DNS records:

**Option A: A Records (Apex domain)**
```
Type: A
Name: @
Value: 185.199.108.153

Type: A
Name: @
Value: 185.199.109.153

Type: A
Name: @
Value: 185.199.110.153

Type: A
Name: @
Value: 185.199.111.153
```

**Option B: CNAME Record (Subdomain)**
```
Type: CNAME
Name: www
Value: bohdaq.github.io
```

### Step 3: Update CNAME File

Edit `docs/CNAME` with your domain:
```
hitop.dev
```

Or remove the file if not using custom domain.

### Step 4: Configure in GitHub

1. Go to repository Settings → Pages
2. Under "Custom domain", enter: `hitop.dev`
3. Click Save
4. Wait for DNS check (can take 24-48 hours)
5. Enable "Enforce HTTPS" once DNS is verified

## Testing Locally

Before pushing, test locally:

```bash
# Navigate to docs folder
cd docs

# Start local server (Python)
python3 -m http.server 8000

# Or with Node.js
npx serve

# Open in browser
open http://localhost:8000
```

## Updating the Site

1. Edit `docs/index.html`
2. Commit changes:
   ```bash
   git add docs/
   git commit -m "Update GitHub Pages"
   git push origin main
   ```
3. GitHub automatically rebuilds (1-2 minutes)

## Adding Screenshots

To add screenshots to the landing page:

1. Take screenshots of HITOP
2. Save as `screenshot1.png`, `screenshot2.png`, etc.
3. Add to `docs/` folder
4. Update `index.html`:

```html
<div class="screenshot">
  <img src="screenshot1.png" alt="HITOP Interface">
</div>
```

## Customization

### Change Colors

Find and replace in `index.html`:
- Primary: `#667eea` → Your color
- Secondary: `#764ba2` → Your color

### Update Links

Replace all instances of:
- `bohdaq` → Your GitHub username
- `hitop` → Your repo name

### Add Sections

Copy existing section structure:

```html
<div class="section">
  <h2>Your Section Title</h2>
  <p>Your content here</p>
</div>
```

## SEO Optimization

The page already includes:
- ✅ Meta description
- ✅ Meta keywords
- ✅ Semantic HTML
- ✅ Responsive design
- ✅ Fast loading

### Add More SEO

Add to `<head>`:

```html
<!-- Open Graph (Facebook, LinkedIn) -->
<meta property="og:title" content="HITOP - HTTP API Testing Tool">
<meta property="og:description" content="A powerful HTTP API testing tool">
<meta property="og:image" content="https://yourdomain.com/og-image.png">
<meta property="og:url" content="https://yourdomain.com">

<!-- Twitter Card -->
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="HITOP - HTTP API Testing Tool">
<meta name="twitter:description" content="A powerful HTTP API testing tool">
<meta name="twitter:image" content="https://yourdomain.com/twitter-image.png">
```

## Analytics (Optional)

### Google Analytics

1. Create GA4 property
2. Get Measurement ID
3. Add before `</head>`:

```html
<script async src="https://www.googletagmanager.com/gtag/js?id=G-XXXXXXXXXX"></script>
<script>
  window.dataLayer = window.dataLayer || [];
  function gtag(){dataLayer.push(arguments);}
  gtag('js', new Date());
  gtag('config', 'G-XXXXXXXXXX');
</script>
```

### Plausible Analytics (Privacy-friendly)

```html
<script defer data-domain="yourdomain.com" src="https://plausible.io/js/script.js"></script>
```

## Troubleshooting

### Site Not Loading

1. Check GitHub Actions tab for build errors
2. Verify Settings → Pages shows green checkmark
3. Clear browser cache
4. Try incognito mode

### Custom Domain Not Working

1. Verify DNS records (use `dig yourdomain.com`)
2. Wait 24-48 hours for DNS propagation
3. Check CNAME file has correct domain
4. Disable then re-enable custom domain in Settings

### 404 Error

1. Ensure `index.html` is in `/docs` folder
2. Check branch is set to `main`
3. Check folder is set to `/docs`
4. Push changes and wait for rebuild

## Advanced: Custom 404 Page

Create `docs/404.html`:

```html
<!DOCTYPE html>
<html>
<head>
  <title>404 - Page Not Found</title>
  <style>
    body {
      font-family: sans-serif;
      text-align: center;
      padding: 100px;
    }
  </style>
</head>
<body>
  <h1>404 - Page Not Found</h1>
  <p><a href="/">Go back home</a></p>
</body>
</html>
```

## Monitoring

Check your site status:
- GitHub Actions tab (build status)
- Settings → Pages (deployment status)
- https://www.githubstatus.com/ (GitHub status)

## Next Steps

1. ✅ Enable GitHub Pages
2. ✅ Test the site
3. 📸 Add screenshots
4. 🎨 Customize colors/content
5. 🌐 (Optional) Set up custom domain
6. 📊 (Optional) Add analytics
7. 🚀 Share your site!

## Resources

- [GitHub Pages Documentation](https://docs.github.com/en/pages)
- [Custom Domain Setup](https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site)
- [Troubleshooting](https://docs.github.com/en/pages/getting-started-with-github-pages/troubleshooting-404-errors-for-github-pages-sites)

## Support

If you need help:
1. Check [GitHub Pages docs](https://docs.github.com/en/pages)
2. Open an issue on the repository
3. Ask in GitHub Discussions
