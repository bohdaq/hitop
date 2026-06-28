import { invoke } from '@tauri-apps/api/core';

// Collections
export const saveCollections = async (collections) => {
  try {
    await invoke('save_collections', { collections });
  } catch (error) {
    console.error('Failed to save collections:', error);
  }
};

export const loadCollections = async () => {
  try {
    return await invoke('load_collections');
  } catch (error) {
    console.error('Failed to load collections:', error);
    return null;
  }
};

// Tabs — kept in memory / sessionStorage (not persisted to disk, desktop reopens fresh)
export const saveTabs = (tabs) => {
  try {
    sessionStorage.setItem('hitop_tabs', JSON.stringify(tabs));
  } catch (_) {}
};

export const loadTabs = () => {
  try {
    const stored = sessionStorage.getItem('hitop_tabs');
    return stored ? JSON.parse(stored) : null;
  } catch (_) {
    return null;
  }
};

export const saveCurrentTab = (tabIndex) => {
  try {
    sessionStorage.setItem('hitop_current_tab', tabIndex.toString());
  } catch (_) {}
};

export const loadCurrentTab = () => {
  try {
    const stored = sessionStorage.getItem('hitop_current_tab');
    return stored !== null ? parseInt(stored, 10) : null;
  } catch (_) {
    return null;
  }
};

// History
export const saveHistory = async (history) => {
  try {
    await invoke('save_history', { history });
  } catch (error) {
    console.error('Failed to save history:', error);
  }
};

export const loadHistory = async () => {
  try {
    return await invoke('load_history');
  } catch (error) {
    console.error('Failed to load history:', error);
    return null;
  }
};

// Contexts
export const saveContexts = async (contexts) => {
  try {
    await invoke('save_contexts', { contexts });
  } catch (error) {
    console.error('Failed to save contexts:', error);
  }
};

export const loadContexts = async () => {
  try {
    return await invoke('load_contexts');
  } catch (error) {
    console.error('Failed to load contexts:', error);
    return null;
  }
};

export const clearAllStorage = async () => {
  try {
    await invoke('save_collections', { collections: [] });
    await invoke('save_history', { history: [] });
    await invoke('save_contexts', { contexts: {} });
    sessionStorage.clear();
  } catch (error) {
    console.error('Failed to clear storage:', error);
  }
};

export const getStorageInfo = () => ({ used: 0, usedKB: 'n/a', usedMB: 'n/a' });

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
};
