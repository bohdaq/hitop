/**
 * Context Service
 * 
 * Manages collection-specific runtime contexts
 */

/**
 * Gets context for a specific collection
 * 
 * @param {object} contexts - All contexts object
 * @param {number} collectionId - Collection ID
 * @returns {object} - Context object for the collection
 */
export const getCollectionContext = (contexts, collectionId) => {
  if (!collectionId) return {};
  return contexts[collectionId] || {};
};

/**
 * Updates a context value for a specific collection
 * 
 * @param {object} contexts - All contexts object
 * @param {number} collectionId - Collection ID
 * @param {string} key - Context key
 * @param {any} value - Context value
 * @returns {object} - Updated contexts object
 */
export const updateCollectionContext = (contexts, collectionId, key, value) => {
  if (!collectionId) return contexts;
  
  return {
    ...contexts,
    [collectionId]: {
      ...(contexts[collectionId] || {}),
      [key]: value
    }
  };
};

/**
 * Clears context for a specific collection
 * 
 * @param {object} contexts - All contexts object
 * @param {number} collectionId - Collection ID
 * @returns {object} - Updated contexts object
 */
export const clearCollectionContext = (contexts, collectionId) => {
  const newContexts = { ...contexts };
  delete newContexts[collectionId];
  return newContexts;
};

/**
 * Clears all contexts
 * 
 * @returns {object} - Empty contexts object
 */
export const clearAllContexts = () => {
  return {};
};

/**
 * Gets all context keys for a collection
 * 
 * @param {object} contexts - All contexts object
 * @param {number} collectionId - Collection ID
 * @returns {array} - Array of context keys
 */
export const getCollectionContextKeys = (contexts, collectionId) => {
  const context = getCollectionContext(contexts, collectionId);
  return Object.keys(context);
};

/**
 * Exports contexts to JSON string
 * 
 * @param {object} contexts - All contexts object
 * @returns {string} - JSON string
 */
export const exportContexts = (contexts) => {
  return JSON.stringify(contexts, null, 2);
};

/**
 * Imports contexts from JSON string
 * 
 * @param {string} jsonString - JSON string to parse
 * @returns {object} - Parsed contexts object
 */
export const importContexts = (jsonString) => {
  return JSON.parse(jsonString);
};

export default {
  getCollectionContext,
  updateCollectionContext,
  clearCollectionContext,
  clearAllContexts,
  getCollectionContextKeys,
  exportContexts,
  importContexts
};
