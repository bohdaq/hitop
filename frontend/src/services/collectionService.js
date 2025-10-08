/**
 * Collection Service
 * 
 * Manages collection import, export, and manipulation
 */

/**
 * Exports collections to JSON string
 * 
 * @param {array} collections - Collections array
 * @returns {string} - JSON string
 */
export const exportCollections = (collections) => {
  return JSON.stringify(collections, null, 2);
};

/**
 * Imports collections from JSON string
 * 
 * @param {string} jsonString - JSON string to parse
 * @returns {array} - Parsed collections array
 * @throws {Error} - If JSON is invalid or not an array
 */
export const importCollections = (jsonString) => {
  const parsed = JSON.parse(jsonString);
  
  if (!Array.isArray(parsed)) {
    throw new Error('Invalid format: Expected an array of collections');
  }
  
  return parsed;
};

/**
 * Validates collection structure
 * 
 * @param {object} collection - Collection to validate
 * @returns {boolean} - True if valid
 */
export const validateCollection = (collection) => {
  if (!collection || typeof collection !== 'object') {
    return false;
  }
  
  if (!collection.id || !collection.name) {
    return false;
  }
  
  if (!Array.isArray(collection.requests)) {
    return false;
  }
  
  return true;
};

/**
 * Creates a new collection
 * 
 * @param {string} name - Collection name
 * @returns {object} - New collection object
 */
export const createCollection = (name) => {
  return {
    id: Date.now(),
    name: name.trim(),
    requests: [],
    variables: {}
  };
};

/**
 * Creates a new request
 * 
 * @param {string} name - Request name
 * @param {object} requestData - Request data (url, method, headers, body, scripts)
 * @returns {object} - New request object
 */
export const createRequest = (name, requestData) => {
  return {
    id: Date.now(),
    name: name.trim(),
    url: requestData.url,
    method: requestData.method,
    headers: requestData.headers,
    body: requestData.body,
    preRequestScript: requestData.preRequestScript || '',
    postRequestScript: requestData.postRequestScript || ''
  };
};

/**
 * Adds a request to a collection
 * 
 * @param {array} collections - Collections array
 * @param {number} collectionId - Collection ID
 * @param {object} request - Request to add
 * @returns {array} - Updated collections array
 */
export const addRequestToCollection = (collections, collectionId, request) => {
  return collections.map(col =>
    col.id === collectionId
      ? { ...col, requests: [...col.requests, request] }
      : col
  );
};

/**
 * Updates a request in a collection
 * 
 * @param {array} collections - Collections array
 * @param {number} collectionId - Collection ID
 * @param {number} requestId - Request ID
 * @param {object} updatedRequest - Updated request data
 * @returns {array} - Updated collections array
 */
export const updateRequestInCollection = (collections, collectionId, requestId, updatedRequest) => {
  return collections.map(col => {
    if (col.id === collectionId) {
      return {
        ...col,
        requests: col.requests.map(req =>
          req.id === requestId ? { ...req, ...updatedRequest } : req
        )
      };
    }
    return col;
  });
};

/**
 * Deletes a request from a collection
 * 
 * @param {array} collections - Collections array
 * @param {number} collectionId - Collection ID
 * @param {number} requestId - Request ID
 * @returns {array} - Updated collections array
 */
export const deleteRequestFromCollection = (collections, collectionId, requestId) => {
  return collections.map(col =>
    col.id === collectionId
      ? { ...col, requests: col.requests.filter(req => req.id !== requestId) }
      : col
  );
};

/**
 * Renames a collection
 * 
 * @param {array} collections - Collections array
 * @param {number} collectionId - Collection ID
 * @param {string} newName - New collection name
 * @returns {array} - Updated collections array
 */
export const renameCollection = (collections, collectionId, newName) => {
  return collections.map(col =>
    col.id === collectionId ? { ...col, name: newName.trim() } : col
  );
};

/**
 * Deletes a collection
 * 
 * @param {array} collections - Collections array
 * @param {number} collectionId - Collection ID
 * @returns {array} - Updated collections array
 */
export const deleteCollection = (collections, collectionId) => {
  return collections.filter(col => col.id !== collectionId);
};

/**
 * Updates collection variables
 * 
 * @param {array} collections - Collections array
 * @param {number} collectionId - Collection ID
 * @param {object} variables - Variables object
 * @returns {array} - Updated collections array
 */
export const updateCollectionVariables = (collections, collectionId, variables) => {
  return collections.map(col =>
    col.id === collectionId ? { ...col, variables } : col
  );
};

/**
 * Gets a collection by ID
 * 
 * @param {array} collections - Collections array
 * @param {number} collectionId - Collection ID
 * @returns {object|null} - Collection object or null if not found
 */
export const getCollectionById = (collections, collectionId) => {
  return collections.find(col => col.id === collectionId) || null;
};

/**
 * Gets collection variables
 * 
 * @param {array} collections - Collections array
 * @param {number} collectionId - Collection ID
 * @returns {object} - Variables object
 */
export const getCollectionVariables = (collections, collectionId) => {
  if (!collectionId) return {};
  const collection = collections.find(col => col.id === collectionId);
  return collection?.variables || {};
};

/**
 * Reorders a request within a collection
 * 
 * @param {array} collections - Collections array
 * @param {number} sourceCollectionId - Source collection ID
 * @param {number} targetCollectionId - Target collection ID
 * @param {object} request - Request to move
 * @param {number} targetRequestId - Target request ID (to insert before)
 * @returns {array} - Updated collections array
 */
export const reorderRequest = (collections, sourceCollectionId, targetCollectionId, request, targetRequestId) => {
  // Remove from source
  let updatedCollections = collections.map(col => {
    if (col.id === sourceCollectionId) {
      return {
        ...col,
        requests: col.requests.filter(req => req.id !== request.id)
      };
    }
    return col;
  });

  // Add to target
  updatedCollections = updatedCollections.map(col => {
    if (col.id === targetCollectionId) {
      const targetIndex = col.requests.findIndex(req => req.id === targetRequestId);
      const newRequests = [...col.requests];
      
      if (targetIndex >= 0) {
        newRequests.splice(targetIndex, 0, request);
      } else {
        newRequests.push(request);
      }
      
      return { ...col, requests: newRequests };
    }
    return col;
  });

  return updatedCollections;
};

export default {
  exportCollections,
  importCollections,
  validateCollection,
  createCollection,
  createRequest,
  addRequestToCollection,
  updateRequestInCollection,
  deleteRequestFromCollection,
  renameCollection,
  deleteCollection,
  updateCollectionVariables,
  getCollectionById,
  getCollectionVariables,
  reorderRequest
};
