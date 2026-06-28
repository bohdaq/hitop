/**
 * Storage Service
 * 
 * Handles localStorage operations for persisting data
 */

const STORAGE_KEYS = {
  COLLECTIONS: 'hitop_collections',
  TABS: 'hitop_tabs',
  CURRENT_TAB: 'hitop_current_tab',
  HISTORY: 'hitop_history',
  CONTEXTS: 'hitop_contexts'
};

/**
 * Saves collections to localStorage
 * 
 * @param {array} collections - Collections array
 */
export const saveCollections = (collections) => {
  try {
    localStorage.setItem(STORAGE_KEYS.COLLECTIONS, JSON.stringify(collections));
  } catch (error) {
    console.error('Failed to save collections:', error);
  }
};

/**
 * Loads collections from localStorage
 * 
 * @returns {array|null} - Collections array or null if not found
 */
export const loadCollections = () => {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.COLLECTIONS);
    return stored ? JSON.parse(stored) : null;
  } catch (error) {
    console.error('Failed to load collections:', error);
    return null;
  }
};

/**
 * Saves tabs to localStorage
 * 
 * @param {array} tabs - Tabs array
 */
export const saveTabs = (tabs) => {
  try {
    localStorage.setItem(STORAGE_KEYS.TABS, JSON.stringify(tabs));
  } catch (error) {
    console.error('Failed to save tabs:', error);
  }
};

/**
 * Loads tabs from localStorage
 * 
 * @returns {array|null} - Tabs array or null if not found
 */
export const loadTabs = () => {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.TABS);
    return stored ? JSON.parse(stored) : null;
  } catch (error) {
    console.error('Failed to load tabs:', error);
    return null;
  }
};

/**
 * Saves current tab index to localStorage
 * 
 * @param {number} tabIndex - Current tab index
 */
export const saveCurrentTab = (tabIndex) => {
  try {
    localStorage.setItem(STORAGE_KEYS.CURRENT_TAB, tabIndex.toString());
  } catch (error) {
    console.error('Failed to save current tab:', error);
  }
};

/**
 * Loads current tab index from localStorage
 * 
 * @returns {number|null} - Tab index or null if not found
 */
export const loadCurrentTab = () => {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.CURRENT_TAB);
    return stored ? parseInt(stored, 10) : null;
  } catch (error) {
    console.error('Failed to load current tab:', error);
    return null;
  }
};

/**
 * Saves request history to localStorage
 * 
 * @param {array} history - History array
 */
export const saveHistory = (history) => {
  try {
    localStorage.setItem(STORAGE_KEYS.HISTORY, JSON.stringify(history));
  } catch (error) {
    console.error('Failed to save history:', error);
  }
};

/**
 * Loads request history from localStorage
 * 
 * @returns {array|null} - History array or null if not found
 */
export const loadHistory = () => {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.HISTORY);
    return stored ? JSON.parse(stored) : null;
  } catch (error) {
    console.error('Failed to load history:', error);
    return null;
  }
};

/**
 * Saves contexts to localStorage
 * 
 * @param {object} contexts - Contexts object
 */
export const saveContexts = (contexts) => {
  try {
    localStorage.setItem(STORAGE_KEYS.CONTEXTS, JSON.stringify(contexts));
  } catch (error) {
    console.error('Failed to save contexts:', error);
  }
};

/**
 * Loads contexts from localStorage
 * 
 * @returns {object|null} - Contexts object or null if not found
 */
export const loadContexts = () => {
  try {
    const stored = localStorage.getItem(STORAGE_KEYS.CONTEXTS);
    return stored ? JSON.parse(stored) : null;
  } catch (error) {
    console.error('Failed to load contexts:', error);
    return null;
  }
};

/**
 * Clears all stored data
 */
export const clearAllStorage = () => {
  try {
    Object.values(STORAGE_KEYS).forEach(key => {
      localStorage.removeItem(key);
    });
  } catch (error) {
    console.error('Failed to clear storage:', error);
  }
};

/**
 * Gets storage usage information
 * 
 * @returns {object} - Storage info { used: number, available: number }
 */
export const getStorageInfo = () => {
  try {
    let used = 0;
    Object.values(STORAGE_KEYS).forEach(key => {
      const item = localStorage.getItem(key);
      if (item) {
        used += item.length;
      }
    });
    
    return {
      used: used,
      usedKB: (used / 1024).toFixed(2),
      usedMB: (used / 1024 / 1024).toFixed(2)
    };
  } catch (error) {
    console.error('Failed to get storage info:', error);
    return { used: 0, usedKB: '0', usedMB: '0' };
  }
};

export default {
  saveCollections,
  loadCollections,
  saveTabs,
  loadTabs,
  saveCurrentTab,
  loadCurrentTab,
  saveHistory,
  loadHistory,
  saveContexts,
  loadContexts,
  clearAllStorage,
  getStorageInfo,
  STORAGE_KEYS
};
