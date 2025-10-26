# Google Analytics Setup

## Overview

Google Analytics is **only** injected into production builds during deployment. The source code remains clean and analytics-free during development.

## How It Works

1. **Development**: Run `npm start` - No analytics tracking
2. **Regular Build**: Run `npm run build` - No analytics tracking
3. **Production Build**: Run `npm run build:production` - Analytics injected automatically

## Deployment

When deploying to GitHub Pages, use the deployment script:

```bash
./deploy-to-pages.sh
```

This script automatically:
1. Builds the React app with `npm run build:production`
2. Injects Google Analytics (ID: `G-TYQC208YN0`)
3. Copies the build to `docs/app/`

## Manual Deployment

If you need to build manually:

```bash
cd frontend
npm run build:production
cd ..
cp -r frontend/build/* docs/app/
```

## Changing Analytics ID

To update the Google Analytics tracking ID:

1. Edit `/frontend/scripts/inject-analytics.js`
2. Change the `GA_ID` constant:
   ```javascript
   const GA_ID = 'G-YOUR-NEW-ID';
   ```

## Verification

After deployment, check that analytics is working:

1. Open the deployed app: https://hitop.rws8.tech/app/
2. Open browser DevTools → Network tab
3. Look for requests to `googletagmanager.com/gtag/js`
4. Check the Google Analytics Real-Time dashboard

## Benefits

✅ **Clean source code** - No tracking code in development  
✅ **Privacy-friendly** - Analytics only in production  
✅ **Easy maintenance** - Single script to update  
✅ **Automated** - Injection happens during build  
✅ **Flexible** - Can easily disable by using `npm run build` instead
