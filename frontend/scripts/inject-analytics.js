#!/usr/bin/env node

/**
 * Inject Google Analytics and Google Ads conversion tracking into the built index.html
 * This script runs after the build to add tracking only to production builds
 */

const fs = require('fs');
const path = require('path');

const GA_ID = 'G-TYQC208YN0';
const ADS_ID = 'AW-17667429231';
const BUILD_DIR = path.join(__dirname, '../build');
const INDEX_PATH = path.join(BUILD_DIR, 'index.html');

const GA_SCRIPT = `
  <!-- Google tag (gtag.js) - Analytics & Ads Conversion Tracking -->
  <script async src="https://www.googletagmanager.com/gtag/js?id=${GA_ID}"></script>
  <script>
    window.dataLayer = window.dataLayer || [];
    function gtag(){dataLayer.push(arguments);}
    gtag('js', new Date());
    gtag('config', '${GA_ID}');
    gtag('config', '${ADS_ID}');
    gtag('event', 'conversion', {'send_to': 'AW-17667429231/i0_dCO6fprQbEO-mvuhB'});
  </script>
`;

try {
  // Read the built index.html
  let html = fs.readFileSync(INDEX_PATH, 'utf8');
  
  // Inject GA script right after <head> tag
  html = html.replace('<head>', `<head>${GA_SCRIPT}`);
  
  // Write back to index.html
  fs.writeFileSync(INDEX_PATH, html, 'utf8');
  
  console.log('✅ Google Analytics & Ads conversion tracking injected successfully');
} catch (error) {
  console.error('❌ Error injecting Google Analytics:', error.message);
  process.exit(1);
}
