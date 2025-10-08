import './App.css';
import { useState, useEffect } from 'react';
import Tabs from '@mui/material/Tabs';
import Tab from '@mui/material/Tab';
import IconButton from '@mui/material/IconButton';
import AddIcon from '@mui/icons-material/Add';
import CloseIcon from '@mui/icons-material/Close';
import hljs from 'highlight.js';
import 'highlight.js/styles/github-dark.css';

// Components
import Sidebar from './components/Sidebar';
import RequestPanel from './components/RequestPanel';

// Modal Components
import AddCollectionModal from './components/AddCollectionModal';
import RenameCollectionModal from './components/RenameCollectionModal';
import SaveRequestModal from './components/SaveRequestModal';
import DeleteRequestModal from './components/DeleteRequestModal';
import DeleteCollectionModal from './components/DeleteCollectionModal';
import ExportModal from './components/ExportModal';
import ImportModal from './components/ImportModal';
import RunCollectionModal from './components/RunCollectionModal';
import HistoryModal from './components/HistoryModal';
import CollectionVariablesModal from './components/CollectionVariablesModal';

// Services
import { interpolateVariables } from './services/variableInterpolation';

const getStatusText = (statusCode) => {
  const statusTexts = {
    // 2xx Success
    200: 'OK',
    201: 'Created',
    202: 'Accepted',
    203: 'Non-Authoritative Information',
    204: 'No Content',
    205: 'Reset Content',
    206: 'Partial Content',
    // 3xx Redirection
    300: 'Multiple Choices',
    301: 'Moved Permanently',
    302: 'Found',
    303: 'See Other',
    304: 'Not Modified',
    307: 'Temporary Redirect',
    308: 'Permanent Redirect',
    // 4xx Client Errors
    400: 'Bad Request',
    401: 'Unauthorized',
    402: 'Payment Required',
    403: 'Forbidden',
    404: 'Not Found',
    405: 'Method Not Allowed',
    406: 'Not Acceptable',
    407: 'Proxy Authentication Required',
    408: 'Request Timeout',
    409: 'Conflict',
    410: 'Gone',
    411: 'Length Required',
    412: 'Precondition Failed',
    413: 'Payload Too Large',
    414: 'URI Too Long',
    415: 'Unsupported Media Type',
    416: 'Range Not Satisfiable',
    417: 'Expectation Failed',
    418: "I'm a teapot",
    422: 'Unprocessable Entity',
    429: 'Too Many Requests',
    // 5xx Server Errors
    500: 'Internal Server Error',
    501: 'Not Implemented',
    502: 'Bad Gateway',
    503: 'Service Unavailable',
    504: 'Gateway Timeout',
    505: 'HTTP Version Not Supported',
  };

  return statusTexts[statusCode] || 'Unknown Status';
};

const createNewTab = () => ({
  id: Date.now(),
  title: 'New Request',
  url: '',
  method: 'GET',
  loading: false,
  response: null,
  responseType: '',
  headers: [{ name: '', value: '' }],
  responseHeaders: null,
  requestBody: '',
  statusCode: null,
  loadedRequestId: null,
  loadedCollectionId: null,
  preRequestScript: '',
  postRequestScript: '',
});

function App() {
  const [tabs, setTabs] = useState([createNewTab()]);
  const [currentTab, setCurrentTab] = useState(0);
  const [collections, setCollections] = useState([{ id: 1, name: 'Default', requests: [] }]);
  const [isRenameModalOpen, setIsRenameModalOpen] = useState(false);
  const [isAddCollectionModalOpen, setIsAddCollectionModalOpen] = useState(false);
  const [isSaveRequestModalOpen, setIsSaveRequestModalOpen] = useState(false);
  const [isDeleteRequestModalOpen, setIsDeleteRequestModalOpen] = useState(false);
  const [isDeleteCollectionModalOpen, setIsDeleteCollectionModalOpen] = useState(false);
  const [isExportModalOpen, setIsExportModalOpen] = useState(false);
  const [isImportModalOpen, setIsImportModalOpen] = useState(false);
  const [isRunCollectionModalOpen, setIsRunCollectionModalOpen] = useState(false);
  const [isHistoryModalOpen, setIsHistoryModalOpen] = useState(false);
  const [newCollectionName, setNewCollectionName] = useState('');
  const [editingCollectionId, setEditingCollectionId] = useState(null);
  const [selectedCollectionId, setSelectedCollectionId] = useState(null);
  const [requestName, setRequestName] = useState('');  const [isOverwriting, setIsOverwriting] = useState(false);
  const [requestToDelete, setRequestToDelete] = useState(null);
  const [collectionToDelete, setCollectionToDelete] = useState(null);
  const [importJson, setImportJson] = useState('');
  const [runningCollection, setRunningCollection] = useState(null);
  const [runResults, setRunResults] = useState([]);
  const [isRunning, setIsRunning] = useState(false);
  const [draggedRequest, setDraggedRequest] = useState(null);
  const [draggedCollectionId, setDraggedCollectionId] = useState(null);
  const [requestHistory, setRequestHistory] = useState([]);
  const [collectionContexts, setCollectionContexts] = useState({}); // { collectionId: context }
  const [isVariablesModalOpen, setIsVariablesModalOpen] = useState(false);
  const [selectedVariablesCollection, setSelectedVariablesCollection] = useState(null);

  const currentTabData = tabs[currentTab];

  const updateTabData = (updates) => {
    const newTabs = [...tabs];
    newTabs[currentTab] = { ...newTabs[currentTab], ...updates };
    setTabs(newTabs);
  };

  const addNewTab = () => {
    const newTab = createNewTab();
    setTabs([...tabs, newTab]);
    setCurrentTab(tabs.length);
  };

  const handleOpenRenameModal = (event, collectionId, currentName) => {
    event.stopPropagation();
    setEditingCollectionId(collectionId);
    setNewCollectionName(currentName);
    setIsRenameModalOpen(true);
  };

  const handleCloseRenameModal = () => {
    setIsRenameModalOpen(false);
    setNewCollectionName('');
    setEditingCollectionId(null);
  };

  const handleSaveCollectionName = () => {
    if (newCollectionName.trim() && editingCollectionId) {
      setCollections(collections.map(col => 
        col.id === editingCollectionId 
          ? { ...col, name: newCollectionName.trim() } 
          : col
      ));
    }
    handleCloseRenameModal();
  };

  const handleOpenAddCollectionModal = (event) => {
    event.stopPropagation();
    setNewCollectionName('');
    setIsAddCollectionModalOpen(true);
  };

  const handleCloseAddCollectionModal = () => {
    setIsAddCollectionModalOpen(false);
    setNewCollectionName('');
  };

  const handleAddCollection = () => {
    if (newCollectionName.trim()) {
      const newCollection = {
        id: Date.now(),
        name: newCollectionName.trim(),
        requests: []
      };
      setCollections([...collections, newCollection]);
    }
    handleCloseAddCollectionModal();
  };

  const handleOpenSaveRequestModal = () => {
    // Check if this is a loaded request
    if (currentTabData.loadedRequestId && currentTabData.loadedCollectionId) {
      // Find the loaded request to get its name
      const collection = collections.find(col => col.id === currentTabData.loadedCollectionId);
      const request = collection?.requests.find(req => req.id === currentTabData.loadedRequestId);
      
      if (request) {
        setRequestName(request.name);
        setSelectedCollectionId(currentTabData.loadedCollectionId);
        setIsOverwriting(true);
      } else {
        setRequestName('');
        setSelectedCollectionId(collections.length > 0 ? collections[0].id : null);
        setIsOverwriting(false);
      }
    } else {
      setRequestName('');
      setSelectedCollectionId(collections.length > 0 ? collections[0].id : null);
      setIsOverwriting(false);
    }
    setIsSaveRequestModalOpen(true);
  };

  const handleCloseSaveRequestModal = () => {
    setIsSaveRequestModalOpen(false);
    setRequestName('');
    setSelectedCollectionId(null);
    setIsOverwriting(false);
  };

  const handleSaveRequest = () => {
    if (requestName.trim() && selectedCollectionId) {
      if (isOverwriting && currentTabData.loadedRequestId) {
        // Update existing request
        setCollections(collections.map(col => {
          if (col.id === selectedCollectionId) {
            return {
              ...col,
              requests: col.requests.map(req => 
                req.id === currentTabData.loadedRequestId
                  ? {
                      ...req,
                      name: requestName.trim(),
                      url: currentTabData.url,
                      method: currentTabData.method,
                      headers: currentTabData.headers,
                      body: currentTabData.requestBody,
                      preRequestScript: currentTabData.preRequestScript,
                      postRequestScript: currentTabData.postRequestScript
                    }
                  : req
              )
            };
          }
          return col;
        }));
        
        // Update tab to reflect new name if changed
        updateTabData({
          loadedRequestId: currentTabData.loadedRequestId,
          loadedCollectionId: selectedCollectionId
        });
      } else {
        // Create new request
        const newRequest = {
          id: Date.now(),
          name: requestName.trim(),
          url: currentTabData.url,
          method: currentTabData.method,
          headers: currentTabData.headers,
          body: currentTabData.requestBody,
          preRequestScript: currentTabData.preRequestScript,
          postRequestScript: currentTabData.postRequestScript
        };
        
        setCollections(collections.map(col => 
          col.id === selectedCollectionId
            ? { ...col, requests: [...col.requests, newRequest] }
            : col
        ));
        
        // Update tab to track this as a loaded request
        updateTabData({
          loadedRequestId: newRequest.id,
          loadedCollectionId: selectedCollectionId
        });
      }
    }
    handleCloseSaveRequestModal();
  };

  const handleLoadRequest = (request, collectionId) => {
    updateTabData({
      url: request.url,
      method: request.method,
      headers: request.headers,
      requestBody: request.body,
      response: null,
      responseHeaders: null,
      statusCode: null,
      responseType: '',
      loadedRequestId: request.id,
      loadedCollectionId: collectionId,
      preRequestScript: request.preRequestScript || '',
      postRequestScript: request.postRequestScript || ''
    });
  };

  const handleOpenDeleteRequestModal = (event, request, collectionId) => {
    event.stopPropagation();
    setRequestToDelete({ request, collectionId });
    setIsDeleteRequestModalOpen(true);
  };

  const handleCloseDeleteRequestModal = () => {
    setIsDeleteRequestModalOpen(false);
    setRequestToDelete(null);
  };

  const handleDeleteRequest = () => {
    if (requestToDelete) {
      setCollections(collections.map(col => {
        if (col.id === requestToDelete.collectionId) {
          return {
            ...col,
            requests: col.requests.filter(req => req.id !== requestToDelete.request.id)
          };
        }
        return col;
      }));
      
      // If the deleted request is currently loaded, clear the loaded request tracking
      if (currentTabData.loadedRequestId === requestToDelete.request.id) {
        updateTabData({
          loadedRequestId: null,
          loadedCollectionId: null
        });
      }
    }
    handleCloseDeleteRequestModal();
  };

  const handleOpenDeleteCollectionModal = () => {
    // Store the collection to delete
    const collection = collections.find(col => col.id === editingCollectionId);
    setCollectionToDelete(collection);
    // Close rename modal and open delete confirmation
    setIsRenameModalOpen(false);
    setIsDeleteCollectionModalOpen(true);
  };

  const handleCloseDeleteCollectionModal = () => {
    setIsDeleteCollectionModalOpen(false);
    setCollectionToDelete(null);
  };

  const handleDeleteCollection = () => {
    if (collectionToDelete) {
      setCollections(collections.filter(col => col.id !== collectionToDelete.id));
      
      // If any tab has a request from this collection loaded, clear the tracking
      const newTabs = tabs.map(tab => {
        if (tab.loadedCollectionId === collectionToDelete.id) {
          return {
            ...tab,
            loadedRequestId: null,
            loadedCollectionId: null
          };
        }
        return tab;
      });
      setTabs(newTabs);
    }
    handleCloseDeleteCollectionModal();
  };

  const handleOpenExportModal = () => {
    setIsExportModalOpen(true);
  };

  const handleCloseExportModal = () => {
    setIsExportModalOpen(false);
  };

  const handleCopyToClipboard = () => {
    const jsonString = JSON.stringify(collections, null, 2);
    navigator.clipboard.writeText(jsonString).then(() => {
      // Could add a snackbar notification here
      console.log('Copied to clipboard!');
    }).catch(err => {
      console.error('Failed to copy:', err);
    });
  };

  const handleOpenImportModal = () => {
    setImportJson('');
    setIsImportModalOpen(true);
  };

  const handleCloseImportModal = () => {
    setIsImportModalOpen(false);
    setImportJson('');
  };

  const handleOpenHistoryModal = () => {
    setIsHistoryModalOpen(true);
  };

  const handleCloseHistoryModal = () => {
    setIsHistoryModalOpen(false);
  };

  const handleLoadHistoryItem = (historyItem) => {
    updateTabData({
      url: historyItem.url,
      method: historyItem.method,
      headers: historyItem.headers,
      requestBody: historyItem.body,
      response: null,
      responseHeaders: null,
      statusCode: null,
      responseType: '',
      loadedRequestId: null,
      loadedCollectionId: null
    });
    handleCloseHistoryModal();
  };

  const handleImportCollections = () => {
    try {
      const importedCollections = JSON.parse(importJson);
      
      // Validate that it's an array
      if (!Array.isArray(importedCollections)) {
        alert('Invalid format: Expected an array of collections');
        return;
      }
      
      // Overwrite existing collections
      setCollections(importedCollections);
      
      // Clear all tabs' loaded request tracking since collections changed
      const newTabs = tabs.map(tab => ({
        ...tab,
        loadedRequestId: null,
        loadedCollectionId: null
      }));
      setTabs(newTabs);
      
      handleCloseImportModal();
    } catch (error) {
      alert('Invalid JSON format: ' + error.message);
    }
  };

  const handleOpenRunCollectionModal = (event, collection) => {
    event.stopPropagation();
    setRunningCollection(collection);
    setRunResults([]);
    setIsRunCollectionModalOpen(true);
  };

  const handleCloseRunCollectionModal = () => {
    setIsRunCollectionModalOpen(false);
    setRunningCollection(null);
    setRunResults([]);
    setIsRunning(false);
  };

  const handleOpenVariablesModal = (collection) => {
    setSelectedVariablesCollection(collection);
    setIsVariablesModalOpen(true);
  };

  const handleCloseVariablesModal = () => {
    setIsVariablesModalOpen(false);
    setSelectedVariablesCollection(null);
  };

  const handleSaveVariables = (variables) => {
    if (selectedVariablesCollection) {
      setCollections(collections.map(col =>
        col.id === selectedVariablesCollection.id
          ? { ...col, variables }
          : col
      ));
    }
  };

  const handleDragStart = (e, request, collectionId) => {
    setDraggedRequest(request);
    setDraggedCollectionId(collectionId);
    e.dataTransfer.effectAllowed = 'move';
  };

  const handleDragOver = (e) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  };

  const handleDrop = (e, targetRequest, targetCollectionId) => {
    e.preventDefault();
    e.stopPropagation();

    if (!draggedRequest || draggedCollectionId !== targetCollectionId || draggedRequest.id === targetRequest.id) {
      return;
    }

    setCollections(collections.map(col => {
      if (col.id === targetCollectionId) {
        const requests = [...col.requests];
        const draggedIndex = requests.findIndex(r => r.id === draggedRequest.id);
        const targetIndex = requests.findIndex(r => r.id === targetRequest.id);

        if (draggedIndex !== -1 && targetIndex !== -1) {
          // Remove dragged item
          const [removed] = requests.splice(draggedIndex, 1);
          // Insert at target position
          requests.splice(targetIndex, 0, removed);
        }

        return { ...col, requests };
      }
      return col;
    }));

    setDraggedRequest(null);
    setDraggedCollectionId(null);
  };

  const handleDragEnd = () => {
    setDraggedRequest(null);
    setDraggedCollectionId(null);
  };

  const handleRunCollection = async () => {
    if (!runningCollection || runningCollection.requests.length === 0) {
      return;
    }

    setIsRunning(true);
    const results = [];
    
    // Get or initialize collection-specific context and variables
    const collectionId = runningCollection.id;
    const context = getCollectionContext(collectionId);
    const variables = runningCollection.variables || {};

    for (let i = 0; i < runningCollection.requests.length; i++) {
      const request = runningCollection.requests[i];
      
      try {
        // Interpolate variables in URL, headers, and body
        let interpolatedUrl = request.url;
        let interpolatedHeaders = request.headers || [];
        let interpolatedBody = request.body;

        try {
          interpolatedUrl = interpolateVariables(request.url, variables);
          
          interpolatedHeaders = (request.headers || []).map(header => ({
            name: header.name ? interpolateVariables(header.name, variables) : '',
            value: header.value ? interpolateVariables(header.value, variables) : ''
          }));

          if (interpolatedBody) {
            interpolatedBody = interpolateVariables(interpolatedBody, variables);
          }
        } catch (varError) {
          results.push({
            name: request.name,
            status: 'Variable Error',
            success: false,
            response: varError.message
          });
          setRunResults([...results]);
          break;
        }

        // Execute pre-request script if exists
        let requestData = {
          url: interpolatedUrl,
          headers: interpolatedHeaders,
          body: interpolatedBody
        };

        if (request.preRequestScript && request.preRequestScript.trim()) {
          const scriptContext = {
            url: interpolatedUrl,
            method: request.method,
            headers: [...interpolatedHeaders],
            body: interpolatedBody,
            variables: { ...variables },
            context: { ...context },
            setContext: (key, value) => {
              updateCollectionContext(collectionId, key, value);
              context[key] = value; // Update local copy for this run
            },
            getContext: (key) => {
              return context[key];
            },
            getVariable: (key) => {
              return variables[key];
            },
            setHeader: (name, value) => {
              const existingIndex = scriptContext.headers.findIndex(h => h.name === name);
              if (existingIndex >= 0) {
                scriptContext.headers[existingIndex].value = value;
              } else {
                scriptContext.headers.push({ name, value });
              }
            },
            setUrl: (newUrl) => {
              scriptContext.url = newUrl;
            },
            setBody: (newBody) => {
              scriptContext.body = newBody;
            }
          };

          try {
            const func = new Function('ctx', `with(ctx) { ${request.preRequestScript} }`);
            func(scriptContext);
            requestData = {
              url: scriptContext.url,
              headers: scriptContext.headers,
              body: scriptContext.body
            };
          } catch (scriptError) {
            results.push({
              name: request.name,
              status: 'Script Error',
              success: false,
              response: scriptError.message
            });
            setRunResults([...results]);
            break;
          }
        }

        // Prepare headers
        const headers = {};
        requestData.headers.forEach(header => {
          if (header.name && header.value) {
            headers[header.name] = header.value;
          }
        });

        const options = {
          method: request.method,
          headers: headers,
        };

        if (requestData.body && (request.method === 'POST' || request.method === 'PUT' || request.method === 'PATCH')) {
          options.body = requestData.body;
        }

        const response = await fetch(requestData.url, options);
        const responseText = await response.text();
        
        // Extract response headers
        const resHeaders = {};
        response.headers.forEach((value, key) => {
          resHeaders[key] = value;
        });

        // Execute post-request script if exists
        if (request.postRequestScript && request.postRequestScript.trim()) {
          try {
            let parsedResponse = responseText;
            try {
              parsedResponse = JSON.parse(responseText);
            } catch (e) {
              // Not JSON, use as is
            }

            const postScriptContext = {
              response: parsedResponse,
              responseText: responseText,
              responseHeaders: resHeaders,
              statusCode: response.status,
              variables: { ...variables },
              context: { ...context },
              setContext: (key, value) => {
                updateCollectionContext(collectionId, key, value);
                context[key] = value; // Update local copy for this run
              },
              getContext: (key) => {
                return context[key];
              },
              getVariable: (key) => {
                return variables[key];
              },
              getResponseValue: (path) => {
                const keys = path.split('.');
                let value = parsedResponse;
                for (const key of keys) {
                  if (value && typeof value === 'object') {
                    value = value[key];
                  } else {
                    return undefined;
                  }
                }
                return value;
              },
              getResponseHeader: (name) => {
                return resHeaders[name.toLowerCase()];
              }
            };

            const postFunc = new Function('ctx', `with(ctx) { ${request.postRequestScript} }`);
            postFunc(postScriptContext);
          } catch (postScriptError) {
            console.error('Post-request script error:', postScriptError);
          }
        }
        
        const result = {
          name: request.name,
          status: response.status,
          success: response.ok,
          response: responseText.substring(0, 200) // Truncate for display
        };
        
        results.push(result);
        setRunResults([...results]);

        // Stop if error response
        if (!response.ok) {
          break;
        }
      } catch (error) {
        results.push({
          name: request.name,
          status: 'Error',
          success: false,
          response: error.message
        });
        setRunResults([...results]);
        break;
      }
    }

    setIsRunning(false);
  };

  useEffect(() => {
    if (isExportModalOpen) {
      // Highlight the JSON after modal opens
      setTimeout(() => {
        document.querySelectorAll('.export-json code').forEach((block) => {
          hljs.highlightElement(block);
        });
      }, 100);
    }
  }, [isExportModalOpen]);

  const closeTab = (event, indexToClose) => {
    event.stopPropagation();
    
    // Don't close if it's the only tab
    if (tabs.length === 1) {
      return;
    }

    const newTabs = tabs.filter((_, index) => index !== indexToClose);
    setTabs(newTabs);

    // Adjust current tab if necessary
    if (currentTab >= newTabs.length) {
      setCurrentTab(newTabs.length - 1);
    } else if (currentTab === indexToClose && currentTab > 0) {
      setCurrentTab(currentTab - 1);
    }
  };

  const addHeader = () => {
    const newHeaders = [...currentTabData.headers, { name: '', value: '' }];
    updateTabData({ headers: newHeaders });
  };

  const updateHeader = (index, field, value) => {
    const newHeaders = [...currentTabData.headers];
    newHeaders[index][field] = value;
    updateTabData({ headers: newHeaders });
  };

  const removeHeader = (index) => {
    const newHeaders = currentTabData.headers.filter((_, i) => i !== index);
    updateTabData({ headers: newHeaders.length > 0 ? newHeaders : [{ name: '', value: '' }] });
  };

  const addToHistory = (requestData) => {
    const historyItem = {
      id: Date.now(),
      timestamp: new Date().toISOString(),
      url: requestData.url,
      method: requestData.method,
      headers: requestData.headers,
      body: requestData.requestBody,
      statusCode: requestData.statusCode,
      success: requestData.statusCode >= 200 && requestData.statusCode < 300
    };
    setRequestHistory(prev => [...prev, historyItem].slice(-50)); // Keep last 50 requests
  };

  const getCollectionContext = (collectionId) => {
    if (!collectionId) return {};
    return collectionContexts[collectionId] || {};
  };

  const updateCollectionContext = (collectionId, key, value) => {
    if (!collectionId) return;
    setCollectionContexts(prev => ({
      ...prev,
      [collectionId]: {
        ...(prev[collectionId] || {}),
        [key]: value
      }
    }));
  };

  const executePostRequestScript = (script, collectionId, response, responseHeaders, statusCode) => {
    if (!script || !script.trim()) {
      return;
    }

    try {
      // Parse response if it's JSON
      let parsedResponse = response;
      try {
        parsedResponse = JSON.parse(response);
      } catch (e) {
        // Not JSON, use as is
      }

      const context = getCollectionContext(collectionId);

      const scriptContext = {
        response: parsedResponse,
        responseText: response,
        responseHeaders: responseHeaders,
        statusCode: statusCode,
        context: { ...context },
        setContext: (key, value) => {
          updateCollectionContext(collectionId, key, value);
        },
        getContext: (key) => {
          return context[key];
        },
        // Helper to get value from JSON response
        getResponseValue: (path) => {
          const keys = path.split('.');
          let value = parsedResponse;
          for (const key of keys) {
            if (value && typeof value === 'object') {
              value = value[key];
            } else {
              return undefined;
            }
          }
          return value;
        },
        // Helper to get header value
        getResponseHeader: (name) => {
          return responseHeaders[name.toLowerCase()];
        }
      };

      const func = new Function('ctx', `with(ctx) { ${script} }`);
      func(scriptContext);
    } catch (error) {
      console.error('Post-request script error:', error);
      alert(`Post-request script error: ${error.message}`);
    }
  };

  const executePreRequestScript = (script, collectionId, interpolatedUrl, interpolatedHeaders, interpolatedBody) => {
    if (!script || !script.trim()) {
      return { url: interpolatedUrl, headers: interpolatedHeaders, body: interpolatedBody };
    }

    try {
      const context = getCollectionContext(collectionId);
      const variables = getCollectionVariables(collectionId);

      // Create a safe execution context
      const scriptContext = {
        url: interpolatedUrl,
        method: currentTabData.method,
        headers: [...interpolatedHeaders],
        body: interpolatedBody,
        variables: { ...variables },
        context: { ...context },
        setContext: (key, value) => {
          updateCollectionContext(collectionId, key, value);
        },
        getContext: (key) => {
          return context[key];
        },
        getVariable: (key) => {
          return variables[key];
        },
        setHeader: (name, value) => {
          const existingIndex = scriptContext.headers.findIndex(h => h.name === name);
          if (existingIndex >= 0) {
            scriptContext.headers[existingIndex].value = value;
          } else {
            scriptContext.headers.push({ name, value });
          }
        },
        setUrl: (newUrl) => {
          scriptContext.url = newUrl;
        },
        setBody: (newBody) => {
          scriptContext.body = newBody;
        }
      };

      // Execute the script
      const func = new Function('ctx', `with(ctx) { ${script} }`);
      func(scriptContext);

      return {
        url: scriptContext.url,
        headers: scriptContext.headers,
        body: scriptContext.body
      };
    } catch (error) {
      console.error('Pre-request script error:', error);
      alert(`Pre-request script error: ${error.message}`);
      return null;
    }
  };

  const getCollectionVariables = (collectionId) => {
    if (!collectionId) return {};
    const collection = collections.find(col => col.id === collectionId);
    return collection?.variables || {};
  };

  const makeRequest = async () => {
    if (!currentTabData.url) {
      alert('Please enter a URL');
      return;
    }

    // Get collection variables
    const variables = getCollectionVariables(currentTabData.loadedCollectionId);

    // Interpolate variables in URL, headers, and body
    let interpolatedUrl = currentTabData.url;
    let interpolatedHeaders = [...currentTabData.headers];
    let interpolatedBody = currentTabData.requestBody;

    try {
      interpolatedUrl = interpolateVariables(currentTabData.url, variables);
      
      interpolatedHeaders = currentTabData.headers.map(header => ({
        name: header.name ? interpolateVariables(header.name, variables) : '',
        value: header.value ? interpolateVariables(header.value, variables) : ''
      }));

      if (interpolatedBody) {
        interpolatedBody = interpolateVariables(interpolatedBody, variables);
      }
    } catch (error) {
      alert(`Variable interpolation error: ${error.message}`);
      return;
    }

    // Execute pre-request script with collection-specific context and interpolated values
    const scriptResult = executePreRequestScript(
      currentTabData.preRequestScript, 
      currentTabData.loadedCollectionId,
      interpolatedUrl,
      interpolatedHeaders,
      interpolatedBody
    );
    if (!scriptResult) {
      return; // Script failed
    }

    updateTabData({ loading: true, response: null });
    try {
      // Build headers object from the headers array (use script-modified headers)
      const requestHeaders = {};
      scriptResult.headers.forEach(header => {
        if (header.name && header.value) {
          requestHeaders[header.name] = header.value;
        }
      });

      const fetchOptions = {
        method: currentTabData.method,
        headers: requestHeaders
      };

      // Add body to request if it exists and method supports it (use script-modified body)
      if (scriptResult.body && (currentTabData.method === 'POST' || currentTabData.method === 'PUT' || currentTabData.method === 'PATCH')) {
        fetchOptions.body = scriptResult.body;
      }

      const res = await fetch(scriptResult.url, fetchOptions);

      // Extract response headers
      const resHeaders = {};
      res.headers.forEach((value, key) => {
        resHeaders[key] = value;
      });

      let data;
      let type = 'text';

      if (res.headers.get('content-type') && res.headers.get('content-type').includes('application/json')) {
        data = await res.json();
        data = JSON.stringify(data, null, 2);
        type = 'json';
      } else if (res.headers.get('content-type') && res.headers.get('content-type').includes('text/html')) {
        data = await res.text();
        type = 'html';
      } else if (res.headers.get('content-type') && res.headers.get('content-type').includes('text/xml')) {
        data = await res.text();
        type = 'xml';
      } else {
        data = await res.text();
        type = 'text';
      }

      updateTabData({
        response: data,
        responseType: type,
        statusCode: res.status,
        responseHeaders: resHeaders,
        loading: false
      });

      // Execute post-request script with collection-specific context
      executePostRequestScript(
        currentTabData.postRequestScript,
        currentTabData.loadedCollectionId,
        data,
        resHeaders,
        res.status
      );

      // Add to history
      addToHistory({
        url: currentTabData.url,
        method: currentTabData.method,
        headers: currentTabData.headers,
        requestBody: currentTabData.requestBody,
        statusCode: res.status
      });
    } catch (error) {
      updateTabData({
        response: `Error: ${error.message}`,
        responseType: 'text',
        responseHeaders: null,
        statusCode: null,
        loading: false
      });
    }
  };

  useEffect(() => {
    if (currentTabData.response) {
      document.querySelectorAll('pre code').forEach((block) => {
        hljs.highlightElement(block);
      });
    }
  }, [currentTabData.response]);

  useEffect(() => {
    let newTitle = 'New Request';
    if (currentTabData.url) {
      try {
        const urlObj = new URL(currentTabData.url);
        const pathname = urlObj.pathname || '/';
        newTitle = `${currentTabData.method} ${pathname}`;
      } catch (error) {
        // If URL is invalid, keep 'New Request'
      }
    }
    
    // Only update if title has changed
    if (newTitle !== currentTabData.title) {
      updateTabData({ title: newTitle });
    }
  }, [currentTabData.url, currentTabData.method, currentTabData.title]);

  return (
    <div className="AppWrapper">
      <Sidebar
        collections={collections}
        onAddCollection={handleOpenAddCollectionModal}
        onRenameCollection={handleOpenRenameModal}
        onRunCollection={handleOpenRunCollectionModal}
        onLoadRequest={handleLoadRequest}
        onDeleteRequest={handleOpenDeleteRequestModal}
        onDragStart={handleDragStart}
        onDragOver={handleDragOver}
        onDrop={handleDrop}
        onDragEnd={handleDragEnd}
        onOpenHistory={handleOpenHistoryModal}
        onOpenExport={handleOpenExportModal}
        onOpenImport={handleOpenImportModal}
        onOpenVariables={handleOpenVariablesModal}
      />
      <div className="App">
      <div className="TabsContainer">
        <Tabs value={currentTab} onChange={(e, newValue) => setCurrentTab(newValue)} aria-label="request tabs">
          {tabs.map((tab, index) => (
            <Tab 
              key={tab.id} 
              label={
                <div className="TabLabel">
                  <span>{tab.title}</span>
                  {tabs.length > 1 && (
                    <IconButton
                      size="small"
                      className="TabCloseButton"
                      onClick={(e) => closeTab(e, index)}
                      aria-label="close tab"
                    >
                      <CloseIcon fontSize="small" />
                    </IconButton>
                  )}
                </div>
              }
            />
          ))}
        </Tabs>
        <IconButton color="primary" aria-label="add new tab" className="AddTabButton" onClick={addNewTab}>
          <AddIcon />
        </IconButton>
      </div>
      <RequestPanel
        tabData={currentTabData}
        onUpdateTabData={updateTabData}
        onMakeRequest={makeRequest}
        onSaveRequest={handleOpenSaveRequestModal}
        onAddHeader={addHeader}
        onUpdateHeader={updateHeader}
        onRemoveHeader={removeHeader}
        getStatusText={getStatusText}
      />
    </div>
    
    {/* Modals */}
    <RenameCollectionModal
      open={isRenameModalOpen}
      onClose={handleCloseRenameModal}
      collectionName={newCollectionName}
      onNameChange={setNewCollectionName}
      onRename={handleSaveCollectionName}
      onDelete={handleOpenDeleteCollectionModal}
    />
    
    <AddCollectionModal
      open={isAddCollectionModalOpen}
      onClose={handleCloseAddCollectionModal}
      collectionName={newCollectionName}
      onNameChange={setNewCollectionName}
      onAdd={handleAddCollection}
    />
    
    <SaveRequestModal
      open={isSaveRequestModalOpen}
      onClose={handleCloseSaveRequestModal}
      requestName={requestName}
      onNameChange={setRequestName}
      collections={collections}
      selectedCollectionId={selectedCollectionId}
      onCollectionChange={setSelectedCollectionId}
      onSave={handleSaveRequest}
      isOverwriting={isOverwriting}
    />
    
    <DeleteRequestModal
      open={isDeleteRequestModalOpen}
      onClose={handleCloseDeleteRequestModal}
      requestName={requestToDelete?.request.name}
      onDelete={handleDeleteRequest}
    />
    
    <DeleteCollectionModal
      open={isDeleteCollectionModalOpen}
      onClose={handleCloseDeleteCollectionModal}
      collection={collectionToDelete}
      onDelete={handleDeleteCollection}
    />
    
    <ExportModal
      open={isExportModalOpen}
      onClose={handleCloseExportModal}
      collections={collections}
      onCopyToClipboard={handleCopyToClipboard}
    />
    
    <ImportModal
      open={isImportModalOpen}
      onClose={handleCloseImportModal}
      importJson={importJson}
      onJsonChange={setImportJson}
      onImport={handleImportCollections}
    />
    
    <RunCollectionModal
      open={isRunCollectionModalOpen}
      onClose={handleCloseRunCollectionModal}
      collection={runningCollection}
      runResults={runResults}
      isRunning={isRunning}
      onRun={handleRunCollection}
    />
    
    <HistoryModal
      open={isHistoryModalOpen}
      onClose={handleCloseHistoryModal}
      requestHistory={requestHistory}
      onLoadHistoryItem={handleLoadHistoryItem}
    />
    
    <CollectionVariablesModal
      open={isVariablesModalOpen}
      onClose={handleCloseVariablesModal}
      collection={selectedVariablesCollection}
      onSave={handleSaveVariables}
    />
    </div>
  );
}

export default App;
