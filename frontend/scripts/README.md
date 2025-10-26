# Build Scripts

## inject-analytics.js

This script injects Google Analytics tracking code into the built React application.

### Purpose
- Keeps the source code clean (no analytics in development)
- Only adds Google Analytics to production builds
- Runs automatically during the `build:production` script

### How it works
1. Reads the built `build/index.html` file
2. Injects the Google Analytics script tag after the `<head>` tag
3. Saves the modified HTML back to the build directory

### Usage

**For production deployment (with analytics):**
```bash
npm run build:production
```

**For regular build (no analytics):**
```bash
npm run build
```

### Configuration
The Google Analytics ID is defined in the script:
```javascript
const GA_ID = 'G-TYQC208YN0';
```

To change the GA ID, edit the `GA_ID` constant in `inject-analytics.js`.
