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
import * as historyService from './services/historyService';
import * as collectionService from './services/collectionService';
import * as contextService from './services/contextService';
import * as scriptExecutionService from './services/scriptExecutionService';
import * as tabService from './services/tabService';
import * as storageService from './services/storageService';
import * as httpService from './services/httpService';

function App() {
  const [tabs, setTabs] = useState([tabService.createNewTab()]);
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
    const newTabs = tabService.updateTab(tabs, currentTab, updates);
    setTabs(newTabs);
  };

  const addNewTab = () => {
    const { tabs: newTabs, newTabIndex } = tabService.addNewTab(tabs);
    setTabs(newTabs);
    setCurrentTab(newTabIndex);
  };

  const handleOpenRenameModal = (event, collectionId, currentName) => {
    event.stopPropagation();
    setEditingCollectionId(collectionId);
    setNewCollectionName(currentName);
    setIsRenameModalOpen(true);
  };

  const handleCloseRenameModal = () => {
    setIsRenameModalOpen(false);
    setEditingCollectionId(null);
    setNewCollectionName('');
  };

  const handleSaveCollectionName = () => {
    setCollections(collectionService.renameCollection(collections, editingCollectionId, newCollectionName));
    handleCloseRenameModal();
  };

  const handleOpenAddCollectionModal = () => {
    setIsAddCollectionModalOpen(true);
  };

  const handleCloseAddCollectionModal = () => {
    setIsAddCollectionModalOpen(false);
    setNewCollectionName('');
  };

  const handleAddCollection = () => {
    const newCollection = collectionService.createCollection(newCollectionName);
    setCollections([...collections, newCollection]);
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
        const updatedRequest = {
          name: requestName.trim(),
          url: currentTabData.url,
          method: currentTabData.method,
          headers: currentTabData.headers,
          body: currentTabData.requestBody,
          preRequestScript: currentTabData.preRequestScript,
          postRequestScript: currentTabData.postRequestScript
        };
        
        setCollections(collectionService.updateRequestInCollection(
          collections,
          selectedCollectionId,
          currentTabData.loadedRequestId,
          updatedRequest
        ));
        
        // Update tab to reflect new name if changed
        updateTabData({
          loadedRequestId: currentTabData.loadedRequestId,
          loadedCollectionId: selectedCollectionId,
          title: requestName.trim()
        });
      } else {
        // Create new request
        const newRequest = collectionService.createRequest(requestName, {
          url: currentTabData.url,
          method: currentTabData.method,
          headers: currentTabData.headers,
          body: currentTabData.requestBody,
          preRequestScript: currentTabData.preRequestScript,
          postRequestScript: currentTabData.postRequestScript
        });
        
        setCollections(collectionService.addRequestToCollection(
          collections,
          selectedCollectionId,
          newRequest
        ));
        
        // Update tab to track this as a loaded request
        updateTabData({
          loadedRequestId: newRequest.id,
          loadedCollectionId: selectedCollectionId,
          title: requestName.trim()
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
      postRequestScript: request.postRequestScript || '',
      title: request.name // Set tab title to request name
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
      setCollections(collectionService.deleteRequestFromCollection(
        collections,
        requestToDelete.collectionId,
        requestToDelete.request.id
      ));
      
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
      setCollections(collectionService.deleteCollection(collections, collectionToDelete.id));
      
      // If any tab has a request from this collection, clear the loaded request tracking
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
      loadedCollectionId: null,
      preRequestScript: historyItem.preRequestScript || '',
      postRequestScript: historyItem.postRequestScript || '',
      title: historyItem.isCollectionRun 
        ? `${historyItem.collectionName} → ${historyItem.requestName}`
        : `${historyItem.method} ${historyItem.url}`
    });
    handleCloseHistoryModal();
  };

  const handleImportCollections = () => {
    try {
      const importedCollections = collectionService.importCollections(importJson);
      
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
      setCollections(collectionService.updateCollectionVariables(
        collections,
        selectedVariablesCollection.id,
        variables
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

    setCollections(collectionService.reorderRequest(
      collections,
      draggedCollectionId,
      targetCollectionId,
      draggedRequest,
      targetRequest.id
    ));

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

        // Add to history
        const historyItem = historyService.createCollectionRunHistoryItem(
          request,
          requestData,
          response,
          runningCollection.name,
          i
        );
        setRequestHistory(prev => historyService.addToHistory(prev, historyItem));

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

  const closeTab = (indexToClose) => {
    const { tabs: newTabs, newCurrentTab } = tabService.closeTab(tabs, indexToClose, currentTab);
    setTabs(newTabs);
    if (newCurrentTab !== currentTab) {
      setCurrentTab(newCurrentTab);
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
    const historyItem = historyService.createHistoryItem(requestData);
    setRequestHistory(prev => historyService.addToHistory(prev, historyItem));
  };

  const getCollectionContext = (collectionId) => {
    return contextService.getCollectionContext(collectionContexts, collectionId);
  };

  const updateCollectionContext = (collectionId, key, value) => {
    const newContexts = contextService.updateCollectionContext(collectionContexts, collectionId, key, value);
    setCollectionContexts(newContexts);
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
    return collectionService.getCollectionVariables(collections, collectionId);
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
        statusCode: res.status,
        preRequestScript: currentTabData.preRequestScript,
        postRequestScript: currentTabData.postRequestScript
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
    // Don't auto-generate title if request was loaded from collection (has loadedRequestId)
    if (currentTabData.loadedRequestId) {
      return;
    }

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
  }, [currentTabData.url, currentTabData.method, currentTabData.title, currentTabData.loadedRequestId]);

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
        getStatusText={httpService.getStatusText}
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
