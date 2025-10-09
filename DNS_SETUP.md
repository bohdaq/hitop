# DNS Setup for HITOP GitHub Pages

## Overview

To use a custom domain like `hitop.dev` with your GitHub Pages site, you need to configure DNS records.

## Prerequisites

1. ✅ Own a domain (e.g., `hitop.dev`)
2. ✅ Have access to domain registrar's DNS settings
3. ✅ GitHub Pages enabled with `/docs` folder

## DNS Configuration Options

### Option 1: Apex Domain (Recommended)

Use root domain: `hitop.dev`

#### DNS Records:

Add these **A records** pointing to GitHub Pages servers:

```
Type: A
Name: @
TTL: 3600
Value: 185.199.108.153

Type: A
Name: @
TTL: 3600
Value: 185.199.109.153

Type: A
Name: @
TTL: 3600
Value: 185.199.110.153

Type: A
Name: @
TTL: 3600
Value: 185.199.111.153
```

#### Optional: Add www subdomain

```
Type: CNAME
Name: www
TTL: 3600
Value: bohdaq.github.io
```

### Option 2: Subdomain Only

Use subdomain: `www.hitop.dev` or `app.hitop.dev`

#### DNS Records:

```
Type: CNAME
Name: www (or app)
TTL: 3600
Value: bohdaq.github.io
```

## Step-by-Step Setup

### Step 1: Update CNAME File

The `docs/CNAME` file should contain your domain:

```
hitop.dev
```

Or for subdomain:
```
www.hitop.dev
```

**Already done!** ✅ Your CNAME file is set to `hitop.dev`

### Step 2: Configure DNS at Your Registrar

#### For Namecheap:

1. Log in to Namecheap
2. Go to **Domain List**
3. Click **Manage** next to your domain
4. Go to **Advanced DNS** tab
5. Add the A records:

| Type | Host | Value | TTL |
|------|------|-------|-----|
| A Record | @ | 185.199.108.153 | Automatic |
| A Record | @ | 185.199.109.153 | Automatic |
| A Record | @ | 185.199.110.153 | Automatic |
| A Record | @ | 185.199.111.153 | Automatic |
| CNAME Record | www | bohdaq.github.io. | Automatic |

6. **Save All Changes**

#### For GoDaddy:

1. Log in to GoDaddy
2. Go to **My Products**
3. Click **DNS** next to your domain
4. Add records:

| Type | Name | Value | TTL |
|------|------|-------|-----|
| A | @ | 185.199.108.153 | 1 Hour |
| A | @ | 185.199.109.153 | 1 Hour |
| A | @ | 185.199.110.153 | 1 Hour |
| A | @ | 185.199.111.153 | 1 Hour |
| CNAME | www | bohdaq.github.io | 1 Hour |

5. **Save**

#### For Cloudflare:

1. Log in to Cloudflare
2. Select your domain
3. Go to **DNS** tab
4. Add records:

| Type | Name | Content | Proxy status | TTL |
|------|------|---------|--------------|-----|
| A | @ | 185.199.108.153 | DNS only | Auto |
| A | @ | 185.199.109.153 | DNS only | Auto |
| A | @ | 185.199.110.153 | DNS only | Auto |
| A | @ | 185.199.111.153 | DNS only | Auto |
| CNAME | www | bohdaq.github.io | DNS only | Auto |

5. **Save**

**Important:** Set proxy status to "DNS only" (gray cloud) initially

#### For Google Domains:

1. Log in to Google Domains
2. Select your domain
3. Click **DNS** in left menu
4. Scroll to **Custom resource records**
5. Add records:

| Name | Type | TTL | Data |
|------|------|-----|------|
| @ | A | 1h | 185.199.108.153 |
| @ | A | 1h | 185.199.109.153 |
| @ | A | 1h | 185.199.110.153 |
| @ | A | 1h | 185.199.111.153 |
| www | CNAME | 1h | bohdaq.github.io. |

6. **Add**

### Step 3: Configure GitHub Pages

1. Go to your repository: `https://github.com/bohdaq/hitop`
2. Click **Settings**
3. Scroll to **Pages** section
4. Under **Custom domain**, enter: `hitop.dev`
5. Click **Save**
6. Wait for DNS check (can take a few minutes to 24 hours)
7. Once verified, check **Enforce HTTPS**

### Step 4: Wait for DNS Propagation

DNS changes take time to propagate:
- **Minimum**: 15 minutes
- **Typical**: 1-4 hours
- **Maximum**: 24-48 hours

## Verification

### Check DNS Propagation

Use these tools to verify:

1. **dig command** (Terminal):
```bash
dig hitop.dev +short
# Should show GitHub IPs:
# 185.199.108.153
# 185.199.109.153
# 185.199.110.153
# 185.199.111.153
```

2. **nslookup** (Terminal):
```bash
nslookup hitop.dev
```

3. **Online Tools**:
- https://www.whatsmydns.net/
- https://dnschecker.org/
- https://mxtoolbox.com/SuperTool.aspx

### Test Your Site

Once DNS propagates:

1. Visit `http://hitop.dev` (will redirect to HTTPS)
2. Visit `https://hitop.dev` (should load your site)
3. Visit `https://www.hitop.dev` (should redirect to hitop.dev)

## Troubleshooting

### DNS Not Resolving

**Problem**: Domain doesn't resolve to GitHub Pages

**Solutions**:
1. Wait longer (up to 48 hours)
2. Verify A records are correct
3. Check for typos in DNS settings
4. Clear DNS cache:
   ```bash
   # macOS
   sudo dscacheutil -flushcache; sudo killall -HUP mDNSResponder
   
   # Windows
   ipconfig /flushdns
   
   # Linux
   sudo systemd-resolve --flush-caches
   ```

### Certificate Error

**Problem**: HTTPS shows certificate error

**Solutions**:
1. Wait for GitHub to provision certificate (can take 24 hours)
2. Disable and re-enable "Enforce HTTPS" in GitHub settings
3. Verify DNS is correctly configured
4. Check CNAME file has correct domain

### www Not Working

**Problem**: www subdomain doesn't work

**Solutions**:
1. Add CNAME record for www
2. Verify CNAME points to `bohdaq.github.io`
3. Wait for DNS propagation

### Site Shows 404

**Problem**: Domain resolves but shows 404

**Solutions**:
1. Verify CNAME file is in `/docs` folder
2. Check CNAME file contains correct domain
3. Verify GitHub Pages is set to `/docs` folder
4. Push changes and wait for rebuild

## Advanced Configuration

### Subdomain for App

To use `app.hitop.dev` for the web app:

1. Add CNAME record:
```
Type: CNAME
Name: app
Value: bohdaq.github.io
```

2. Create `docs/app/CNAME` with:
```
app.hitop.dev
```

3. Update links in `docs/index.html`

### Email Setup

If you want email at your domain:

1. Add MX records from your email provider
2. Example (Google Workspace):
```
Type: MX
Priority: 1
Value: aspmx.l.google.com

Type: MX
Priority: 5
Value: alt1.aspmx.l.google.com
```

### CAA Records (Optional)

For additional security:

```
Type: CAA
Name: @
Value: 0 issue "letsencrypt.org"
```

## DNS Record Summary

Here's what your final DNS should look like:

```
hitop.dev.        A       185.199.108.153
hitop.dev.        A       185.199.109.153
hitop.dev.        A       185.199.110.153
hitop.dev.        A       185.199.111.153
www.hitop.dev.    CNAME   bohdaq.github.io.
```

## Common Registrars Quick Links

- **Namecheap**: https://ap.www.namecheap.com/
- **GoDaddy**: https://dcc.godaddy.com/
- **Cloudflare**: https://dash.cloudflare.com/
- **Google Domains**: https://domains.google.com/
- **Hover**: https://www.hover.com/
- **Name.com**: https://www.name.com/

## Security

### HTTPS

GitHub Pages automatically provides HTTPS via Let's Encrypt:
- ✅ Free SSL certificate
- ✅ Auto-renewal
- ✅ Enforced HTTPS (after enabling)

### DNSSEC (Optional)

Enable DNSSEC at your registrar for additional security:
1. Check if registrar supports DNSSEC
2. Enable in DNS settings
3. Add DS records if required

## Monitoring

### Check Site Status

Monitor your site:
- https://www.githubstatus.com/ (GitHub status)
- https://uptimerobot.com/ (uptime monitoring)
- https://www.pingdom.com/ (performance monitoring)

### DNS Monitoring

Monitor DNS:
- https://www.dnsperf.com/
- https://dnschecker.org/

## Cost

- **GitHub Pages**: Free
- **Domain**: $10-15/year (varies by registrar)
- **SSL Certificate**: Free (via Let's Encrypt)
- **Total**: ~$10-15/year

## Timeline

Expected timeline:
- ✅ **Now**: DNS records configured
- ⏱️ **15 min - 4 hours**: DNS propagates
- ⏱️ **1-24 hours**: HTTPS certificate issued
- ✅ **Done**: Site live with HTTPS

## Next Steps

1. ✅ CNAME file created (`docs/CNAME`)
2. ⏳ **Configure DNS** at your registrar (follow steps above)
3. ⏳ **Add custom domain** in GitHub Pages settings
4. ⏳ **Wait** for DNS propagation
5. ✅ **Enable HTTPS** once DNS verified
6. 🎉 **Done!** Your site is live

## Support

Need help?
- GitHub Pages Docs: https://docs.github.com/en/pages
- DNS Help: Contact your registrar's support
- GitHub Community: https://github.community/

## Quick Reference

### GitHub Pages IPs:
```
185.199.108.153
185.199.109.153
185.199.110.153
185.199.111.153
```

### Your Settings:
- **Domain**: hitop.dev
- **GitHub Username**: bohdaq
- **Repository**: hitop
- **CNAME**: bohdaq.github.io
