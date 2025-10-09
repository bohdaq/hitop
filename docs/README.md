# HITOP GitHub Pages

This directory contains the GitHub Pages site for HITOP.

## Setup

1. **Enable GitHub Pages:**
   - Go to your repository settings
   - Navigate to "Pages" section
   - Source: Deploy from a branch
   - Branch: `main`
   - Folder: `/docs`
   - Click Save

2. **Custom Domain (Optional):**
   - If you own `hitop.dev` or another domain:
   - Update `CNAME` file with your domain
   - Configure DNS:
     - Add A records pointing to GitHub Pages IPs:
       - 185.199.108.153
       - 185.199.109.153
       - 185.199.110.153
       - 185.199.111.153
     - Or add CNAME record: `yourusername.github.io`

3. **Access:**
   - Without custom domain: `https://yourusername.github.io/hitop`
   - With custom domain: `https://hitop.dev`

## Files

- `index.html` - Main landing page
- `CNAME` - Custom domain configuration (optional)
- `README.md` - This file

## Local Development

To test locally:

```bash
# Simple HTTP server
cd docs
python3 -m http.server 8000

# Or with Node.js
npx serve
```

Then open `http://localhost:8000` in your browser.

## Updating

To update the site:

1. Edit `index.html`
2. Commit and push changes
3. GitHub Pages will automatically rebuild

## Features

The landing page includes:
- Hero section with download buttons
- Feature showcase
- Statistics
- Use cases
- Call-to-action sections
- Footer with links

## Customization

Edit `index.html` to customize:
- Colors (search for `#667eea` and `#764ba2`)
- Content (update text in HTML)
- Links (update GitHub URLs)
- Add screenshots (add `<img>` tags in screenshot section)

## SEO

The page includes:
- Meta description
- Meta keywords
- Semantic HTML
- Responsive design
- Fast loading

## Analytics (Optional)

To add Google Analytics:

1. Get your GA tracking ID
2. Add before `</head>` in `index.html`:

```html
<!-- Google Analytics -->
<script async src="https://www.googletagmanager.com/gtag/js?id=GA_MEASUREMENT_ID"></script>
<script>
  window.dataLayer = window.dataLayer || [];
  function gtag(){dataLayer.push(arguments);}
  gtag('js', new Date());
  gtag('config', 'GA_MEASUREMENT_ID');
</script>
```

## License

Same as main project - MIT License
