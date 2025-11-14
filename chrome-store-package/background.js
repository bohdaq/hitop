// Background service worker for Chrome extension
// Opens HITOP in a new tab when extension icon is clicked

chrome.action.onClicked.addListener((tab) => {
  chrome.tabs.create({
    url: chrome.runtime.getURL('index.html')
  });
});

// Handle installation
chrome.runtime.onInstalled.addListener((details) => {
  if (details.reason === 'install') {
    console.log('HITOP HTTP Client installed');
    // Optionally open welcome page
    chrome.tabs.create({
      url: chrome.runtime.getURL('index.html')
    });
  } else if (details.reason === 'update') {
    console.log('HITOP HTTP Client updated to version ' + chrome.runtime.getManifest().version);
  }
});
