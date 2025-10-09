/**
 * Popup script for HITOP Firefox Extension
 */

// Get the full app URL
const getAppUrl = () => {
  return browser.runtime.getURL('app/index.html');
};

// Open full application in new tab (default behavior)
document.getElementById('openFullApp').addEventListener('click', () => {
  browser.tabs.create({ url: getAppUrl(), active: true });
  window.close();
});

// Open in background tab (doesn't switch to it)
document.getElementById('openInNewTab').addEventListener('click', () => {
  browser.tabs.create({ url: getAppUrl(), active: false });
  window.close();
});

console.log('HITOP popup loaded');
