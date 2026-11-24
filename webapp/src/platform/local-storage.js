// Writes to localStorage under the old React app's keys and in its shapes (as
// shared/services/storageService.js writes them), but saying whether
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

// Stored text that can't be read: copied to `<key>_damaged` before anything replaces it. Returns
// which of collections, environments and globals are damaged (the library pulls them again).
const CHECKED = { collections: "hitop_collections", environments: "hitop_environments", globals: "hitop_globals" };

export function findDamaged(storage = globalThis.localStorage) {
  const damaged = new Set();
  for (const [name, key] of Object.entries(CHECKED)) {
    let text;
    try {
      text = storage.getItem(key);
    } catch {
      continue; // storage blocked: nothing is stored at all
    }
    if (text === null) continue;
    let ok = false;
    try {
      const v = JSON.parse(text);
      ok = name === "globals" ? v === null || (typeof v === "object" && !Array.isArray(v)) : Array.isArray(v) || v === null;
    } catch {
      // not JSON
    }
    if (ok) continue;
    damaged.add(name);
    try {
      storage.setItem(`${key}_damaged`, text);
    } catch {
      // storage full: the damaged text stays under its own key until something is saved
    }
  }
  return damaged;
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

