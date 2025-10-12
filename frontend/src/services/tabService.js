/**
 * Tab Service
 * 
 * Manages tab creation and manipulation
 */

/**
 * Creates a new tab with default values
 * 
 * @returns {object} - New tab object
 */
export const createNewTab = () => {
  return {
    id: Date.now(),
    title: 'New Request',
    url: '',
    method: 'GET',
    headers: [{ name: '', value: '' }],
    requestBody: '',
    response: null,
    responseHeaders: null,
    statusCode: null,
    responseType: '',
    loading: false,
    loadedRequestId: null,
    loadedCollectionId: null,
    preRequestScript: '',
    postRequestScript: '',
    showExtra: false
  };
};

/**
 * Generates a tab title from URL and method
 * 
 * @param {string} url - Request URL
 * @param {string} method - HTTP method
 * @returns {string} - Generated title
 */
export const generateTabTitle = (url, method) => {
  if (!url) return 'New Request';
  
  try {
    const urlObj = new URL(url);
    const pathname = urlObj.pathname || '/';
    return `${method} ${pathname}`;
  } catch (error) {
    return 'New Request';
  }
};

/**
 * Updates a tab in the tabs array
 * 
 * @param {array} tabs - Tabs array
 * @param {number} tabIndex - Index of tab to update
 * @param {object} updates - Updates to apply
 * @returns {array} - Updated tabs array
 */
export const updateTab = (tabs, tabIndex, updates) => {
  const newTabs = [...tabs];
  newTabs[tabIndex] = { ...newTabs[tabIndex], ...updates };
  return newTabs;
};

/**
 * Closes a tab
 * 
 * @param {array} tabs - Tabs array
 * @param {number} tabIndex - Index of tab to close
 * @param {number} currentTab - Current active tab index
 * @returns {object} - { tabs: array, newCurrentTab: number }
 */
export const closeTab = (tabs, tabIndex, currentTab) => {
  // Don't close if it's the only tab
  if (tabs.length === 1) {
    return { tabs, newCurrentTab: currentTab };
  }

  const newTabs = tabs.filter((_, index) => index !== tabIndex);
  
  let newCurrentTab = currentTab;
  
  // Adjust current tab if necessary
  if (currentTab >= newTabs.length) {
    newCurrentTab = newTabs.length - 1;
  } else if (currentTab === tabIndex && currentTab > 0) {
    newCurrentTab = currentTab - 1;
  }

  return { tabs: newTabs, newCurrentTab };
};

/**
 * Adds a new tab to the tabs array
 * 
 * @param {array} tabs - Tabs array
 * @returns {object} - { tabs: array, newTabIndex: number }
 */
export const addNewTab = (tabs) => {
  const newTab = createNewTab();
  return {
    tabs: [...tabs, newTab],
    newTabIndex: tabs.length
  };
};

export default {
  createNewTab,
  generateTabTitle,
  updateTab,
  closeTab,
  addNewTab
};
