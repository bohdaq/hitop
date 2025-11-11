import './App.css';
import { useState, useEffect } from 'react';
import { ThemeProvider, createTheme } from '@mui/material/styles';
import CssBaseline from '@mui/material/CssBaseline';
import useMediaQuery from '@mui/material/useMediaQuery';
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
import CollectionsView from './components/CollectionsView';
import CollectionDetailView from './components/CollectionDetailView';
import VariablesView from './components/VariablesView';

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
import TabContextMenu from './components/TabContextMenu';
import RenameRequestModal from './components/RenameRequestModal';

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
  const prefersDarkMode = useMediaQuery('(prefers-color-scheme: dark)');
  
  // Initialize with a default request in the Default collection
  const initializeDefaultState = () => {
    const defaultCollectionId = 1;
    const defaultRequest = collectionService.createRequest('New Request', {
      url: '',
      method: 'GET',
      headers: [{ name: '', value: '' }],
      body: '',
      preRequestScript: '',
      postRequestScript: ''
    });
    
    const defaultCollection = {
      id: defaultCollectionId,
      name: 'Default',
      requests: [defaultRequest]
    };
    
    const initialTab = {
      ...tabService.createNewTab(),
      loadedRequestId: defaultRequest.id,
      loadedCollectionId: defaultCollectionId
    };
    
    return {
      collections: [defaultCollection],
      tabs: [initialTab]
    };
  };
  
  const initialState = initializeDefaultState();
  const [tabs, setTabs] = useState(initialState.tabs);
  const [currentTab, setCurrentTab] = useState(0);
  const [collections, setCollections] = useState(initialState.collections);

  const theme = createTheme({
    palette: {
      mode: prefersDarkMode ? 'dark' : 'light',
      primary: {
        main: prefersDarkMode ? '#8b9dff' : '#667eea',
      },
      secondary: {
        main: prefersDarkMode ? '#9d6ec5' : '#764ba2',
      },
      background: {
        default: prefersDarkMode ? '#0d1117' : '#f5f5f5',
        paper: prefersDarkMode ? '#161b22' : '#ffffff',
      },
    },
  });

  useEffect(() => {
    document.body.className = prefersDarkMode ? 'dark-theme' : 'light-theme';
  }, [prefersDarkMode]);

  // Load collections from localStorage on mount
  useEffect(() => {
    const savedCollections = storageService.loadCollections();
    if (savedCollections && savedCollections.length > 0) {
      setCollections(savedCollections);
    }
  }, []);

  // Save collections to localStorage whenever they change
  useEffect(() => {
    storageService.saveCollections(collections);
  }, [collections]);

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
  const [requestName, setRequestName] = useState(''); const [isOverwriting, setIsOverwriting] = useState(false);
  const [requestToDelete, setRequestToDelete] = useState(null);
  const [collectionToDelete, setCollectionToDelete] = useState(null);
  const [importJson, setImportJson] = useState('');
  const [runningCollection, setRunningCollection] = useState(null);
  const [runResults, setRunResults] = useState([]);
  const [isRunning, setIsRunning] = useState(false);
  const [completedCollectionsCount, setCompletedCollectionsCount] = useState(0);
  const [runAllResults, setRunAllResults] = useState(null);
  const [draggedRequest, setDraggedRequest] = useState(null);
  const [draggedCollectionId, setDraggedCollectionId] = useState(null);
  const [draggedCollection, setDraggedCollection] = useState(null);
  const [requestHistory, setRequestHistory] = useState([]);
  const [collectionContexts, setCollectionContexts] = useState({}); // { collectionId: context }
  const [isVariablesModalOpen, setIsVariablesModalOpen] = useState(false);
  const [selectedVariablesCollection, setSelectedVariablesCollection] = useState(null);
  const [showCollectionsView, setShowCollectionsView] = useState(false);
  const [selectedCollectionView, setSelectedCollectionView] = useState(null);
  const [showVariablesView, setShowVariablesView] = useState(false);
  const [tabContextMenu, setTabContextMenu] = useState(null);
  const [contextMenuTabIndex, setContextMenuTabIndex] = useState(null);
  const [isRenameRequestModalOpen, setIsRenameRequestModalOpen] = useState(false);
  const [renameRequestName, setRenameRequestName] = useState('');

  const currentTabData = tabs[currentTab];

  const updateTabData = (updates) => {
    const newTabs = tabService.updateTab(tabs, currentTab, updates);
    setTabs(newTabs);
  };

  const addNewTab = () => {
    setShowCollectionsView(false);
    
    // Find or create Default collection
    let defaultCollection = collections.find(col => col.name === 'Default');
    let updatedCollections = collections;
    
    if (!defaultCollection) {
      defaultCollection = collectionService.createCollection('Default');
      updatedCollections = [...collections, defaultCollection];
      setCollections(updatedCollections);
    }
    
    // Create a new request in the Default collection
    const newRequest = collectionService.createRequest('New Request', {
      url: '',
      method: 'GET',
      headers: [{ name: '', value: '' }],
      body: '',
      preRequestScript: '',
      postRequestScript: ''
    });
    
    // Add request to Default collection
    updatedCollections = collectionService.addRequestToCollection(
      updatedCollections,
      defaultCollection.id,
      newRequest
    );
    setCollections(updatedCollections);
    
    // Create new tab linked to the request
    const newTab = {
      ...tabService.createNewTab(),
      loadedRequestId: newRequest.id,
      loadedCollectionId: defaultCollection.id
    };
    
    const newTabs = [...tabs, newTab];
    setTabs(newTabs);
    setCurrentTab(newTabs.length - 1);
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
        setSelectedCollectionId(null);
        setIsOverwriting(false);
      }
    } else {
      setRequestName('');
      setSelectedCollectionId(null);
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
    if (requestName.trim()) {
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
        
        // Check if collection has changed
        if (selectedCollectionId !== currentTabData.loadedCollectionId) {
          // Move request to different collection
          // 1. Remove from old collection
          let updatedCollections = collectionService.deleteRequestFromCollection(
            collections,
            currentTabData.loadedCollectionId,
            currentTabData.loadedRequestId
          );
          
          // 2. Add to new collection with updated data
          const requestWithId = {
            id: currentTabData.loadedRequestId,
            ...updatedRequest
          };
          updatedCollections = collectionService.addRequestToCollection(
            updatedCollections,
            selectedCollectionId,
            requestWithId
          );
          
          setCollections(updatedCollections);
        } else {
          // Update in same collection
          setCollections(collectionService.updateRequestInCollection(
            collections,
            selectedCollectionId,
            currentTabData.loadedRequestId,
            updatedRequest
          ));
        }
        
        // Update tab to reflect new name and collection if changed
        updateTabData({
          loadedRequestId: currentTabData.loadedRequestId,
          loadedCollectionId: selectedCollectionId,
          title: requestName.trim()
        });
      } else {
        // Create new request - automatically add to Default collection
        const defaultCollection = collections.find(col => col.name === 'Default');
        const targetCollectionId = defaultCollection ? defaultCollection.id : (collections.length > 0 ? collections[0].id : null);
        
        if (targetCollectionId) {
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
            targetCollectionId,
            newRequest
          ));
          
          // Update tab to track this as a loaded request
          updateTabData({
            loadedRequestId: newRequest.id,
            loadedCollectionId: targetCollectionId,
            title: requestName.trim()
          });
        }
      }
    }
    handleCloseSaveRequestModal();
  };

  const handleShowCollections = () => {
    setShowCollectionsView(true);
    setSelectedCollectionView(null);
    // Clear running state when navigating back to collections view
    if (!isRunning) {
      setRunningCollection(null);
      setRunResults([]);
    }
  };

  const handleShowCollectionDetail = (collection) => {
    setShowCollectionsView(false);
    setShowVariablesView(false);
    setSelectedCollectionView(collection);
  };

  const handleShowVariables = () => {
    setShowVariablesView(true);
  };

  const handleBackFromVariables = () => {
    setShowVariablesView(false);
  };

  const handleLoadRequest = (request, collectionId) => {
    setShowCollectionsView(false);
    setSelectedCollectionView(null);
    setShowVariablesView(false);
    
    // Check if this request is already open in a tab
    const existingTabIndex = tabs.findIndex(
      tab => tab.loadedRequestId === request.id && tab.loadedCollectionId === collectionId
    );
    
    if (existingTabIndex !== -1) {
      // Request is already open, just focus that tab
      setCurrentTab(existingTabIndex);
    } else {
      // Request is not open, create a new tab for it
      const newTab = {
        ...tabService.createNewTab(),
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
        title: request.name
      };
      
      const newTabs = [...tabs, newTab];
      setTabs(newTabs);
      setCurrentTab(newTabs.length - 1);
    }
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

  const handleOpenDeleteCollectionModalFromDetail = (event, collection) => {
    event.stopPropagation();
    setCollectionToDelete(collection);
    setIsDeleteCollectionModalOpen(true);
  };

  const handleCloseDeleteCollectionModal = () => {
    setIsDeleteCollectionModalOpen(false);
    setCollectionToDelete(null);
  };

  const handleDeleteCollection = () => {
    if (collectionToDelete) {
      setCollections(collectionService.deleteCollection(collections, collectionToDelete.id));

      // If this collection is currently being viewed, clear it
      if (selectedCollectionView?.id === collectionToDelete.id) {
        setSelectedCollectionView(null);
        setShowVariablesView(false);
      }

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

  const handleDuplicateCollection = (event, collection) => {
    event.stopPropagation();
    const updatedCollections = collectionService.duplicateCollection(collections, collection.id);
    setCollections(updatedCollections);
  };

  const handleOpenExportModal = () => {
    setIsExportModalOpen(true);
  };

  const handleCloseExportModal = () => {
    setIsExportModalOpen(false);
  };

  const handleCopyToClipboard = (jsonString) => {
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

      // Close all existing tabs and create a new tab with the first request
      if (importedCollections.length > 0 && importedCollections[0].requests.length > 0) {
        const firstCollection = importedCollections[0];
        const firstRequest = firstCollection.requests[0];
        
        // Create a single new tab with the first request
        const newTab = {
          ...tabService.createNewTab(),
          url: firstRequest.url,
          method: firstRequest.method,
          headers: firstRequest.headers,
          requestBody: firstRequest.body,
          response: null,
          responseHeaders: null,
          statusCode: null,
          responseType: '',
          loadedRequestId: firstRequest.id,
          loadedCollectionId: firstCollection.id,
          preRequestScript: firstRequest.preRequestScript || '',
          postRequestScript: firstRequest.postRequestScript || '',
          title: firstRequest.name
        };
        
        setTabs([newTab]);
        setCurrentTab(0);
      } else {
        // No requests in imported collections, create an empty tab
        setTabs([tabService.createNewTab()]);
        setCurrentTab(0);
      }

      handleCloseImportModal();
    } catch (error) {
      alert('Invalid JSON format: ' + error.message);
    }
  };

  const handleOpenRunCollectionModal = async (event, collection) => {
    event.stopPropagation();
    setRunningCollection(collection);
    setRunResults([]);
    
    // Run the collection directly without opening modal
    await runCollection(collection);
  };

  const handleRunAllCollections = async () => {
    if (collections.length === 0) return;

    setIsRunning(true);
    setCompletedCollectionsCount(0);
    setRunAllResults(null);

    const startTime = Date.now();
    let totalRequests = 0;
    let successCount = 0;
    let failedCount = 0;
    let collectionsRun = 0;

    // Run each collection sequentially
    let completed = 0;
    for (const collection of collections) {
      if (collection.requests && collection.requests.length > 0) {
        setRunningCollection(collection);
        setRunResults([]);
        
        collectionsRun++;
        
        // Run the collection and collect results
        const currentResults = await runCollection(collection);
        
        // Count results
        if (currentResults && currentResults.length > 0) {
          totalRequests += currentResults.length;
          successCount += currentResults.filter(r => r.success).length;
          failedCount += currentResults.filter(r => !r.success).length;
        }
        
        // Update completed count
        completed++;
        setCompletedCollectionsCount(completed);
        
        // Add a small delay between collections for better UX
        await new Promise(resolve => setTimeout(resolve, 500));
      }
    }

    const endTime = Date.now();
    const duration = endTime - startTime;

    // Set final results summary
    setRunAllResults({
      totalCollections: collectionsRun,
      totalRequests,
      successCount,
      failedCount,
      duration
    });

    setIsRunning(false);
    setRunningCollection(null);
    setCompletedCollectionsCount(0);
  };

  const handleCloseRunCollectionModal = () => {
    setIsRunCollectionModalOpen(false);
    setRunningCollection(null);
    setRunResults([]);
    setIsRunning(false);
  };

  const handleOpenVariablesModal = (collection) => {
    // Show collection detail view with variables view
    setShowCollectionsView(false);
    setSelectedCollectionView(collection);
    setShowVariablesView(true);
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

  const handleSaveCollectionVariables = (variables) => {
    if (selectedCollectionView) {
      setCollections(collectionService.updateCollectionVariables(
        collections,
        selectedCollectionView.id,
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

  // Collection drag and drop handlers
  const handleCollectionDragStart = (e, collection, index) => {
    setDraggedCollection({ collection, index });
    e.dataTransfer.effectAllowed = 'move';
  };

  const handleCollectionDragOver = (e) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
  };

  const handleCollectionDrop = (e, targetIndex) => {
    e.preventDefault();
    e.stopPropagation();

    if (!draggedCollection || draggedCollection.index === targetIndex) {
      return;
    }

    const reorderedCollections = collectionService.reorderCollections(
      collections,
      draggedCollection.index,
      targetIndex
    );
    setCollections(reorderedCollections);
    setDraggedCollection(null);
  };

  const handleCollectionDragEnd = () => {
    setDraggedCollection(null);
  };

  const runCollection = async (collection) => {
    if (!collection || collection.requests.length === 0) {
      return;
    }

    setIsRunning(true);
    const results = [];

    // Get or initialize collection-specific context and variables
    const collectionId = collection.id;
    const context = getCollectionContext(collectionId);
    const variables = collection.variables || {};

    for (let i = 0; i < collection.requests.length; i++) {
      const request = collection.requests[i];

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
            requestName: request.name,
            method: request.method,
            url: request.url,
            statusCode: 'Variable Error',
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
              requestName: request.name,
              method: request.method,
              url: request.url,
              statusCode: 'Script Error',
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

        const startTime = Date.now();
        const response = await fetch(requestData.url, options);
        const endTime = Date.now();
        const duration = endTime - startTime;
        
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
          requestName: request.name,
          method: request.method,
          url: requestData.url,
          statusCode: response.status,
          success: response.ok,
          response: responseText.substring(0, 200), // Truncate for display
          duration: duration
        };

        results.push(result);
        setRunResults([...results]);

        // Add to history
        const historyItem = historyService.createCollectionRunHistoryItem(
          request,
          requestData,
          response,
          collection.name,
          i
        );
        setRequestHistory(prev => historyService.addToHistory(prev, historyItem));

        // Stop if error response
        if (!response.ok) {
          break;
        }
      } catch (error) {
        results.push({
          requestName: request.name,
          method: request.method,
          url: request.url,
          statusCode: 'Error',
          success: false,
          response: error.message
        });
        setRunResults([...results]);
        break;
      }
    }

    setIsRunning(false);
    return results;
  };

  const handleRunCollection = async () => {
    if (runningCollection) {
      await runCollection(runningCollection);
    }
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

  // Update selectedCollectionView when collections change (e.g., after rename)
  useEffect(() => {
    if (selectedCollectionView) {
      const updatedCollection = collections.find(col => col.id === selectedCollectionView.id);
      if (updatedCollection) {
        setSelectedCollectionView(updatedCollection);
      }
    }
  }, [collections]);

  const closeTab = (indexToClose) => {
    const { tabs: newTabs, newCurrentTab } = tabService.closeTab(tabs, indexToClose, currentTab);
    setTabs(newTabs);
    if (newCurrentTab !== currentTab) {
      setCurrentTab(newCurrentTab);
    }
  };

  const handleTabContextMenu = (event, index) => {
    event.preventDefault();
    setTabContextMenu({
      mouseX: event.clientX - 2,
      mouseY: event.clientY - 4,
    });
    setContextMenuTabIndex(index);
  };

  const handleCloseTabContextMenu = () => {
    setTabContextMenu(null);
    // Don't reset contextMenuTabIndex here - it's needed for rename modal
    // It will be reset after the rename operation completes
  };

  const handleOpenRenameRequestModal = () => {
    if (contextMenuTabIndex !== null) {
      const tab = tabs[contextMenuTabIndex];
      setRenameRequestName(tab.title || 'New Request');
      setIsRenameRequestModalOpen(true);
      // Only close the context menu UI, keep the index for rename
      setTabContextMenu(null);
    }
  };

  const handleCloseRenameRequestModal = () => {
    setIsRenameRequestModalOpen(false);
    setRenameRequestName('');
    setContextMenuTabIndex(null);
  };

  const handleRenameRequest = () => {
    console.log('=== RENAME REQUEST START ===');
    console.log('contextMenuTabIndex:', contextMenuTabIndex);
    console.log('renameRequestName:', renameRequestName);
    
    if (contextMenuTabIndex !== null && renameRequestName.trim()) {
      const tab = tabs[contextMenuTabIndex];
      console.log('Tab:', tab);
      console.log('loadedRequestId:', tab.loadedRequestId);
      console.log('loadedCollectionId:', tab.loadedCollectionId);
      
      // If this tab is linked to a request in a collection, update the request name first
      if (tab.loadedRequestId && tab.loadedCollectionId) {
        // Find the collection and request
        const collection = collections.find(col => col.id === tab.loadedCollectionId);
        console.log('Found collection:', collection);
        
        if (collection) {
          const request = collection.requests.find(req => req.id === tab.loadedRequestId);
          console.log('Found request:', request);
          
          if (request) {
            // Update the request with all fields plus new name
            const updatedRequest = {
              ...request,
              name: renameRequestName.trim()
            };
            console.log('Updated request:', updatedRequest);
            
            const updatedCollections = collectionService.updateRequestInCollection(
              collections,
              tab.loadedCollectionId,
              tab.loadedRequestId,
              updatedRequest
            );
            console.log('Updated collections:', updatedCollections);
            setCollections(updatedCollections);
          } else {
            console.log('ERROR: Request not found!');
          }
        } else {
          console.log('ERROR: Collection not found!');
        }
      } else {
        console.log('Tab not linked to collection');
      }
      
      // Update tab title
      const newTabs = tabService.updateTab(tabs, contextMenuTabIndex, {
        title: renameRequestName.trim()
      });
      console.log('Updated tabs:', newTabs);
      setTabs(newTabs);
    }
    handleCloseRenameRequestModal();
    console.log('=== RENAME REQUEST END ===');
  };

  const handleDuplicateTab = () => {
    if (contextMenuTabIndex !== null) {
      const tabToDuplicate = tabs[contextMenuTabIndex];
      
      // Create a new request in the same collection or Default
      let targetCollectionId = tabToDuplicate.loadedCollectionId;
      
      if (!targetCollectionId) {
        // If tab is not linked to a collection, use Default
        let defaultCollection = collections.find(col => col.name === 'Default');
        if (!defaultCollection) {
          defaultCollection = collectionService.createCollection('Default');
          setCollections([...collections, defaultCollection]);
        }
        targetCollectionId = defaultCollection.id;
      }
      
      // Create new request with duplicated data
      const newRequest = collectionService.createRequest(
        `${tabToDuplicate.title} (Copy)`,
        {
          url: tabToDuplicate.url,
          method: tabToDuplicate.method,
          headers: tabToDuplicate.headers,
          body: tabToDuplicate.requestBody,
          preRequestScript: tabToDuplicate.preRequestScript,
          postRequestScript: tabToDuplicate.postRequestScript
        }
      );
      
      // Add to collection
      const updatedCollections = collectionService.addRequestToCollection(
        collections,
        targetCollectionId,
        newRequest
      );
      setCollections(updatedCollections);
      
      // Create new tab
      const newTab = {
        ...tabService.createNewTab(),
        title: `${tabToDuplicate.title} (Copy)`,
        url: tabToDuplicate.url,
        method: tabToDuplicate.method,
        headers: [...tabToDuplicate.headers],
        requestBody: tabToDuplicate.requestBody,
        preRequestScript: tabToDuplicate.preRequestScript,
        postRequestScript: tabToDuplicate.postRequestScript,
        loadedRequestId: newRequest.id,
        loadedCollectionId: targetCollectionId
      };
      
      const newTabs = [...tabs, newTab];
      setTabs(newTabs);
      setCurrentTab(newTabs.length - 1);
    }
    handleCloseTabContextMenu();
    setContextMenuTabIndex(null);
  };

  const handleDeleteTabRequest = () => {
    if (contextMenuTabIndex !== null) {
      const tab = tabs[contextMenuTabIndex];
      
      // If tab is linked to a request, delete it from collection
      if (tab.loadedRequestId && tab.loadedCollectionId) {
        setCollections(collectionService.deleteRequestFromCollection(
          collections,
          tab.loadedCollectionId,
          tab.loadedRequestId
        ));
      }
      
      // Close the tab
      closeTab(contextMenuTabIndex);
    }
    handleCloseTabContextMenu();
    setContextMenuTabIndex(null);
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
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <div className="AppWrapper">
        <Sidebar
          key={JSON.stringify(collections.map(c => ({ id: c.id, requests: c.requests.map(r => ({ id: r.id, name: r.name })) })))}
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
          onShowCollections={handleShowCollections}
          onShowCollectionDetail={handleShowCollectionDetail}
        />
        <div className={`App ${showCollectionsView || selectedCollectionView || showVariablesView ? 'no-tabs' : ''}`}>
          {!showCollectionsView && !selectedCollectionView && !showVariablesView && (
            <div className="TabsContainer">
              <Tabs value={currentTab} onChange={(e, newValue) => {
                setShowCollectionsView(false);
                setCurrentTab(newValue);
              }} aria-label="request tabs">
                {tabs.map((tab, index) => (
                  <Tab
                    key={tab.id}
                    onContextMenu={(e) => handleTabContextMenu(e, index)}
                    label={
                      <div className="TabLabel">
                        <span>{tab.title}</span>
                        {tabs.length > 1 && (
                          <IconButton
                            size="small"
                            className="TabCloseButton"
                            onClick={(e) => {
                              e.stopPropagation();
                              closeTab(index);
                            }}
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
          )}
          {showCollectionsView ? (
            <CollectionsView
              collections={collections}
              onRenameCollection={handleOpenRenameModal}
              onViewCollection={handleShowCollectionDetail}
              onAddCollection={handleOpenAddCollectionModal}
              onRunAll={handleRunAllCollections}
              isRunningAll={isRunning}
              runningCollectionId={runningCollection?.id}
              completedCollections={completedCollectionsCount}
              runAllResults={runAllResults}
              onCollectionDragStart={handleCollectionDragStart}
              onCollectionDragOver={handleCollectionDragOver}
              onCollectionDrop={handleCollectionDrop}
              onCollectionDragEnd={handleCollectionDragEnd}
            />
          ) : showVariablesView && selectedCollectionView ? (
            <VariablesView
              collection={selectedCollectionView}
              onSaveVariables={handleSaveCollectionVariables}
            />
          ) : selectedCollectionView ? (
            <CollectionDetailView
              collection={selectedCollectionView}
              onRenameCollection={handleOpenRenameModal}
              onRunCollection={handleOpenRunCollectionModal}
              onLoadRequest={handleLoadRequest}
              onDeleteRequest={handleOpenDeleteRequestModal}
              onDeleteCollection={handleOpenDeleteCollectionModalFromDetail}
              onDuplicateCollection={handleDuplicateCollection}
              onOpenVariables={handleShowVariables}
              runResults={runningCollection?.id === selectedCollectionView.id ? runResults : []}
              isRunning={isRunning && runningCollection?.id === selectedCollectionView.id}
            />
          ) : (
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
          )}
        </div>

        {/* Modals */}
        <RenameCollectionModal
          open={isRenameModalOpen}
          onClose={handleCloseRenameModal}
          collectionName={newCollectionName}
          onNameChange={setNewCollectionName}
          onRename={handleSaveCollectionName}
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

        <TabContextMenu
          anchorPosition={tabContextMenu}
          open={Boolean(tabContextMenu)}
          onClose={handleCloseTabContextMenu}
          onRename={handleOpenRenameRequestModal}
          onDuplicate={handleDuplicateTab}
          onDelete={handleDeleteTabRequest}
        />

        <RenameRequestModal
          open={isRenameRequestModalOpen}
          onClose={handleCloseRenameRequestModal}
          requestName={renameRequestName}
          onNameChange={setRenameRequestName}
          onRename={handleRenameRequest}
        />
      </div>
    </ThemeProvider>
  );
}

export default App;
