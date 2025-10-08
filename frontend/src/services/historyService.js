/**
 * History Service
 * 
 * Manages request history tracking and retrieval
 */

/**
 * Creates a history item from request data
 * 
 * @param {object} requestData - Request data to save
 * @returns {object} - History item object
 */
export const createHistoryItem = (requestData) => {
  return {
    id: Date.now(),
    timestamp: new Date().toISOString(),
    url: requestData.url,
    method: requestData.method,
    headers: requestData.headers,
    body: requestData.requestBody,
    statusCode: requestData.statusCode,
    success: requestData.statusCode >= 200 && requestData.statusCode < 300,
    preRequestScript: requestData.preRequestScript || '',
    postRequestScript: requestData.postRequestScript || ''
  };
};

/**
 * Creates a history item for a collection run request
 * 
 * @param {object} request - Request object
 * @param {object} requestData - Actual request data used
 * @param {object} response - Response object
 * @param {string} collectionName - Name of the collection
 * @param {number} index - Request index in collection
 * @returns {object} - History item object
 */
export const createCollectionRunHistoryItem = (request, requestData, response, collectionName, index) => {
  return {
    id: Date.now() + index,
    timestamp: new Date().toISOString(),
    url: requestData.url,
    method: request.method,
    headers: requestData.headers,
    body: requestData.body,
    statusCode: response.status,
    success: response.ok,
    collectionName: collectionName,
    requestName: request.name,
    isCollectionRun: true,
    preRequestScript: request.preRequestScript || '',
    postRequestScript: request.postRequestScript || ''
  };
};

/**
 * Adds an item to history, maintaining a maximum size
 * 
 * @param {array} currentHistory - Current history array
 * @param {object} historyItem - Item to add
 * @param {number} maxSize - Maximum history size (default: 50)
 * @returns {array} - Updated history array
 */
export const addToHistory = (currentHistory, historyItem, maxSize = 50) => {
  return [...currentHistory, historyItem].slice(-maxSize);
};

/**
 * Clears all history
 * 
 * @returns {array} - Empty array
 */
export const clearHistory = () => {
  return [];
};

/**
 * Filters history by success status
 * 
 * @param {array} history - History array
 * @param {boolean} successOnly - If true, return only successful requests
 * @returns {array} - Filtered history
 */
export const filterHistoryBySuccess = (history, successOnly = true) => {
  return history.filter(item => item.success === successOnly);
};

/**
 * Filters history by method
 * 
 * @param {array} history - History array
 * @param {string} method - HTTP method to filter by
 * @returns {array} - Filtered history
 */
export const filterHistoryByMethod = (history, method) => {
  return history.filter(item => item.method === method);
};

/**
 * Filters history by collection runs
 * 
 * @param {array} history - History array
 * @param {boolean} collectionRunsOnly - If true, return only collection run items
 * @returns {array} - Filtered history
 */
export const filterHistoryByCollectionRuns = (history, collectionRunsOnly = true) => {
  return history.filter(item => item.isCollectionRun === collectionRunsOnly);
};

export default {
  createHistoryItem,
  createCollectionRunHistoryItem,
  addToHistory,
  clearHistory,
  filterHistoryBySuccess,
  filterHistoryByMethod,
  filterHistoryByCollectionRuns
};
