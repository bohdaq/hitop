// Writes to localStorage under the React app's keys and in its shapes (as
// frontend/src/services/storageService.jsx writes them), but saying whether
// the write worked: a full storage (QuotaExceededError) is reported to the
// person by the store instead of only in the console.
const KEYS = {
  collections: "hitop_collections",
  tabs: "hitop_tabs",
  currentTab: "hitop_current_tab",
  history: "hitop_history",
  contexts: "hitop_contexts",
};

function write(key, text) {
  try {
    localStorage.setItem(key, text);
    return true;
  } catch (e) {
    console.error(`Couldn't save ${key}:`, e);
    return false;
  }
}

const json = (key) => (value) => write(key, JSON.stringify(value));

// Each returns true when saved, false when the browser refused (storage full or blocked).
export const localSaves = {
  saveCollections: json(KEYS.collections),
  saveTabs: json(KEYS.tabs),
  saveCurrentTab: (index) => write(KEYS.currentTab, String(index)),
  saveHistory: json(KEYS.history),
  saveContexts: json(KEYS.contexts),
};

