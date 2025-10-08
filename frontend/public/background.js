// Background service worker for HITOP Chrome Extension
// Opens the app in a new tab when the extension icon is clicked

chrome.action.onClicked.addListener((tab) => {
  chrome.tabs.create({
    url: chrome.runtime.getURL('index.html')
  });
});
