/**
 * Popup script for HITOP Extension
 * Compatible with both Firefox (browser) and Chrome (chrome)
 */

// Use chrome API if available, otherwise fall back to browser (Firefox)
const extensionAPI = typeof chrome !== 'undefined' ? chrome : browser;

// Get the full app URL
const getAppUrl = () => {
  return extensionAPI.runtime.getURL('app/index.html');
};

// Open full application in new tab
document.getElementById('openFullApp').addEventListener('click', () => {
  window.open(getAppUrl(), '_blank');
  window.close();
});

console.log('HITOP popup loaded');
