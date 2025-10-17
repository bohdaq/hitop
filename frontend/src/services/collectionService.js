/**
 * Collection Service
 * 
 * Manages collection import, export, and manipulation
 */

/**
 * Converts HITOP collection to Bruno format
 * 
 * @param {object} collection - HITOP collection
 * @returns {object} - Bruno collection
 */
const convertToBrunoFormat = (collection) => {
  return {
    version: "1",
    name: collection.name,
    type: "collection",
    items: collection.requests.map(req => ({
      uid: `hitop-${req.id}`,
      name: req.name,
      type: "http-request",
      request: {
        url: req.url,
        method: req.method,
        headers: req.headers.map(h => ({
          name: h.key,
          value: h.value,
          enabled: true
        })),
        body: req.body ? {
          mode: "json",
          json: req.body
        } : undefined,
        script: {
          req: req.preRequestScript || "",
          res: req.postRequestScript || ""
        }
      }
    })),
    environments: Object.keys(collection.variables || {}).length > 0 ? [{
      name: "Default",
      variables: Object.entries(collection.variables || {}).map(([key, value]) => ({
        name: key,
        value: value,
        enabled: true
      }))
    }] : []
  };
};

/**
 * Converts HITOP collection to Postman v2.1 format
 * 
 * @param {object} collection - HITOP collection
 * @returns {object} - Postman collection
 */
const convertToPostmanFormat = (collection) => {
  return {
    info: {
      name: collection.name,
      _postman_id: `hitop-${collection.id}`,
      description: `Exported from HITOP`,
      schema: "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
    },
    item: collection.requests.map(req => ({
      name: req.name,
      request: {
        method: req.method,
        header: req.headers.map(h => ({
          key: h.key,
          value: h.value,
          type: "text"
        })),
        body: req.body ? {
          mode: "raw",
          raw: req.body,
          options: {
            raw: {
              language: "json"
            }
          }
        } : undefined,
        url: {
          raw: req.url,
          protocol: req.url.split('://')[0],
          host: req.url.split('://')[1]?.split('/')[0]?.split('.') || [],
          path: req.url.split('://')[1]?.split('/').slice(1) || []
        }
      },
      event: [
        ...(req.preRequestScript ? [{
          listen: "prerequest",
          script: {
            type: "text/javascript",
            exec: req.preRequestScript.split('\n')
          }
        }] : []),
        ...(req.postRequestScript ? [{
          listen: "test",
          script: {
            type: "text/javascript",
            exec: req.postRequestScript.split('\n')
          }
        }] : [])
      ]
    })),
    variable: Object.entries(collection.variables || {}).map(([key, value]) => ({
      key,
      value,
      type: "string"
    }))
  };
};

/**
 * Exports collections to specified format
 * 
 * @param {array} collections - Collections array
 * @param {string} format - Export format ('postman', 'bruno', or 'hitop')
 * @returns {string} - JSON string in specified format
 */
export const exportCollections = (collections, format = 'postman') => {
  if (format === 'postman') {
    const postmanCollections = collections.map(convertToPostmanFormat);
    return JSON.stringify(postmanCollections, null, 2);
  }
  if (format === 'bruno') {
    const brunoCollections = collections.map(convertToBrunoFormat);
    return JSON.stringify(brunoCollections, null, 2);
  }
  // HITOP native format
  return JSON.stringify(collections, null, 2);
};

/**
 * Converts Bruno collection to HITOP format
 * 
 * @param {object} brunoCollection - Bruno collection
 * @returns {object} - HITOP collection
 */
const convertFromBrunoFormat = (brunoCollection) => {
  // Check if it's a Bruno collection
  if (brunoCollection.type === 'collection' && brunoCollection.items) {
    const variables = {};
    if (brunoCollection.environments && brunoCollection.environments.length > 0) {
      brunoCollection.environments[0].variables?.forEach(v => {
        variables[v.name] = v.value;
      });
    }

    return {
      id: Date.now() + Math.random(),
      name: brunoCollection.name || 'Imported Collection',
      requests: brunoCollection.items.map((item, index) => {
        const request = item.request || {};
        
        return {
          id: Date.now() + index,
          name: item.name || 'Untitled Request',
          url: request.url || '',
          method: request.method || 'GET',
          headers: (request.headers || []).map(h => ({
            key: h.name,
            value: h.value
          })),
          body: request.body?.json || request.body?.text || '',
          preRequestScript: request.script?.req || '',
          postRequestScript: request.script?.res || ''
        };
      }),
      variables
    };
  }
  
  // If not Bruno format, return as-is
  return brunoCollection;
};

/**
 * Converts Postman collection to HITOP format
 * 
 * @param {object} postmanCollection - Postman collection
 * @returns {object} - HITOP collection
 */
const convertFromPostmanFormat = (postmanCollection) => {
  // Check if it's a Postman collection
  if (postmanCollection.info && postmanCollection.item) {
    const variables = {};
    if (postmanCollection.variable) {
      postmanCollection.variable.forEach(v => {
        variables[v.key] = v.value;
      });
    }

    return {
      id: Date.now() + Math.random(),
      name: postmanCollection.info.name || 'Imported Collection',
      requests: postmanCollection.item.map((item, index) => {
        const request = item.request || {};
        const url = typeof request.url === 'string' ? request.url : (request.url?.raw || '');
        
        // Extract scripts
        let preRequestScript = '';
        let postRequestScript = '';
        if (item.event) {
          item.event.forEach(event => {
            if (event.listen === 'prerequest' && event.script) {
              preRequestScript = Array.isArray(event.script.exec) 
                ? event.script.exec.join('\n') 
                : event.script.exec || '';
            }
            if (event.listen === 'test' && event.script) {
              postRequestScript = Array.isArray(event.script.exec) 
                ? event.script.exec.join('\n') 
                : event.script.exec || '';
            }
          });
        }

        return {
          id: Date.now() + index,
          name: item.name || 'Untitled Request',
          url: url,
          method: request.method || 'GET',
          headers: (request.header || []).map(h => ({
            key: h.key,
            value: h.value
          })),
          body: request.body?.raw || '',
          preRequestScript,
          postRequestScript
        };
      }),
      variables
    };
  }
  
  // If not Postman format, assume it's HITOP format
  return postmanCollection;
};

/**
 * Imports collections from JSON string (supports HITOP, Postman, and Bruno formats)
 * 
 * @param {string} jsonString - JSON string to parse
 * @returns {array} - Parsed collections array in HITOP format
 * @throws {Error} - If JSON is invalid
 */
export const importCollections = (jsonString) => {
  const parsed = JSON.parse(jsonString);
  
  // Handle array of collections
  if (Array.isArray(parsed)) {
    return parsed.map(col => {
      // Try Bruno format first
      if (col.type === 'collection') {
        return convertFromBrunoFormat(col);
      }
      // Then Postman format
      if (col.info) {
        return convertFromPostmanFormat(col);
      }
      // Otherwise assume HITOP format
      return col;
    });
  }
  
  // Handle single collection - detect format
  if (parsed.type === 'collection') {
    // Bruno format
    return [convertFromBrunoFormat(parsed)];
  }
  
  if (parsed.info) {
    // Postman format
    return [convertFromPostmanFormat(parsed)];
  }
  
  if (parsed.id || parsed.name) {
    // HITOP format
    return [parsed];
  }
  
  throw new Error('Invalid format: Expected a collection or array of collections');
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
