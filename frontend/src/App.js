import './App.css';
import { useState, useEffect } from 'react';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import Select from '@mui/material/Select';
import MenuItem from '@mui/material/MenuItem';
import FormControl from '@mui/material/FormControl';
import InputLabel from '@mui/material/InputLabel';
import Tabs from '@mui/material/Tabs';
import Tab from '@mui/material/Tab';
import IconButton from '@mui/material/IconButton';
import AddIcon from '@mui/icons-material/Add';
import CloseIcon from '@mui/icons-material/Close';
import EditIcon from '@mui/icons-material/Edit';
import SaveIcon from '@mui/icons-material/Save';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import FileDownloadIcon from '@mui/icons-material/FileDownload';
import FileUploadIcon from '@mui/icons-material/FileUpload';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import Drawer from '@mui/material/Drawer';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import MenuList from '@mui/material/MenuList';
import ListItemText from '@mui/material/ListItemText';
import ListItemIcon from '@mui/material/ListItemIcon';
import FolderIcon from '@mui/icons-material/Folder';
import HttpIcon from '@mui/icons-material/Http';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import hljs from 'highlight.js';
import 'highlight.js/styles/github-dark.css';
import styles from './App.css'

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
                      body: currentTabData.requestBody
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
          body: currentTabData.requestBody
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
      loadedCollectionId: collectionId
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

  const handleRunCollection = async () => {
    if (!runningCollection || runningCollection.requests.length === 0) {
      return;
    }

    setIsRunning(true);
    const results = [];

    for (let i = 0; i < runningCollection.requests.length; i++) {
      const request = runningCollection.requests[i];
      
      try {
        // Prepare headers
        const headers = {};
        request.headers.forEach(header => {
          if (header.name && header.value) {
            headers[header.name] = header.value;
          }
        });

        const options = {
          method: request.method,
          headers: headers,
        };

        if (request.body && (request.method === 'POST' || request.method === 'PUT' || request.method === 'PATCH')) {
          options.body = request.body;
        }

        const response = await fetch(request.url, options);
        const responseText = await response.text();
        
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

  const handleMakeRequest = async () => {
    if (!currentTabData.url) {
      alert('Please enter a URL');
      return;
    }

    updateTabData({ loading: true, response: null });
    try {
      // Build headers object from the headers array
      const requestHeaders = {};
      currentTabData.headers.forEach(header => {
        if (header.name && header.value) {
          requestHeaders[header.name] = header.value;
        }
      });

      const fetchOptions = {
        method: currentTabData.method,
        headers: requestHeaders
      };

      // Add body to request if it exists and method supports it
      if (currentTabData.requestBody && (currentTabData.method === 'POST' || currentTabData.method === 'PUT' || currentTabData.method === 'PATCH')) {
        fetchOptions.body = currentTabData.requestBody;
      }

      const res = await fetch(currentTabData.url, fetchOptions);

      // Extract response headers
      const resHeaders = {};
      res.headers.forEach((value, key) => {
        resHeaders[key] = value;
      });

      const contentType = res.headers.get('content-type');
      let data;
      let type = 'text';

      if (contentType && contentType.includes('application/json')) {
        data = await res.json();
        data = JSON.stringify(data, null, 2);
        type = 'json';
      } else if (contentType && contentType.includes('text/html')) {
        data = await res.text();
        type = 'html';
      } else if (contentType && contentType.includes('text/xml')) {
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
      console.log('Response:', data);
    } catch (error) {
      console.error('Error:', error);
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
      <Drawer
        variant="permanent"
        className="Sidebar"
        classes={{
          paper: 'SidebarPaper',
        }}
      >
        <MenuList sx={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
          <MenuItem className="CollectionsMenuItem">
            <ListItemText>Collections</ListItemText>
            <IconButton
              size="small"
              className="AddCollectionButton"
              onClick={handleOpenAddCollectionModal}
              aria-label="add collection"
            >
              <AddIcon fontSize="small" />
            </IconButton>
          </MenuItem>
          {collections.map((collection) => (
            <div key={collection.id}>
              <MenuItem 
                className="SubMenuItem"
                onClick={(e) => {
                  if (collection.requests.length > 0) {
                    handleOpenRunCollectionModal(e, collection);
                  }
                }}
              >
                <ListItemIcon sx={{ minWidth: '32px' }}>
                  <FolderIcon fontSize="small" />
                </ListItemIcon>
                <ListItemText>{collection.name}</ListItemText>
                <IconButton
                  size="small"
                  className="EditCollectionButton"
                  onClick={(e) => handleOpenRenameModal(e, collection.id, collection.name)}
                  aria-label="edit collection"
                >
                  <EditIcon fontSize="small" />
                </IconButton>
              </MenuItem>
              {collection.requests.map((request) => (
                <MenuItem 
                  key={request.id} 
                  className="RequestMenuItem"
                  onClick={() => handleLoadRequest(request, collection.id)}
                >
                  <ListItemIcon sx={{ minWidth: '32px' }}>
                    <HttpIcon fontSize="small" />
                  </ListItemIcon>
                  <ListItemText>{request.name}</ListItemText>
                  <IconButton
                    size="small"
                    className="DeleteRequestButton"
                    onClick={(e) => handleOpenDeleteRequestModal(e, request, collection.id)}
                    aria-label="delete request"
                  >
                    <CloseIcon fontSize="small" />
                  </IconButton>
                </MenuItem>
              ))}
            </div>
          ))}
          <MenuItem onClick={handleOpenExportModal} sx={{ marginTop: 'auto', borderTop: '1px solid #ddd' }}>
            <ListItemIcon>
              <FileDownloadIcon fontSize="small" />
            </ListItemIcon>
            <ListItemText>Export</ListItemText>
          </MenuItem>
          <MenuItem onClick={handleOpenImportModal}>
            <ListItemIcon>
              <FileUploadIcon fontSize="small" />
            </ListItemIcon>
            <ListItemText>Import</ListItemText>
          </MenuItem>
        </MenuList>
      </Drawer>
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
      <div className="AppContainer">
        <div className="ControlsContainer">
          <FormControl className='MethodSelect'>
            <InputLabel id="method-select-label">Method</InputLabel>
            <Select
              labelId="method-select-label"
              id="method-select"
              value={currentTabData.method}
              label="Method"
              onChange={(e) => updateTabData({ method: e.target.value })}
            >
              <MenuItem value="GET">GET</MenuItem>
              <MenuItem value="POST">POST</MenuItem>
              <MenuItem value="PUT">PUT</MenuItem>
              <MenuItem value="PATCH">PATCH</MenuItem>
              <MenuItem value="DELETE">DELETE</MenuItem>
              <MenuItem value="HEAD">HEAD</MenuItem>
              <MenuItem value="OPTIONS">OPTIONS</MenuItem>
            </Select>
          </FormControl>
          <div className='UrlContainer'>
            <TextField
              fullWidth
              id="outlined-basic"
              label="URL"
              variant="outlined"
              value={currentTabData.url}
              onChange={(e) => updateTabData({ url: e.target.value })}
            />
          </div>
          <div className='ButtonContainer'>
            <Button
              size='large'
              variant="contained"
              onClick={handleMakeRequest}
              disabled={currentTabData.loading}
            >
              {currentTabData.loading ? 'Loading...' : 'Make Request'}
            </Button>
            <IconButton
              color="primary"
              onClick={handleOpenSaveRequestModal}
              aria-label="save request"
              disabled={!currentTabData.url}
            >
              <SaveIcon />
            </IconButton>
          </div>
        </div>
        <div className="HeadersSection">
          <h3>Headers</h3>
          {currentTabData.headers.map((header, index) => (
            <div key={index} className="HeaderRow">
              <TextField
                label="Header Name"
                variant="outlined"
                size="small"
                value={header.name}
                onChange={(e) => updateHeader(index, 'name', e.target.value)}
                className="HeaderInput"
              />
              <TextField
                label="Header Value"
                variant="outlined"
                size="small"
                value={header.value}
                onChange={(e) => updateHeader(index, 'value', e.target.value)}
                className="HeaderInput"
              />
              {currentTabData.headers.length > 1 && (
                <Button
                  variant="outlined"
                  color="error"
                  size="small"
                  onClick={() => removeHeader(index)}
                >
                  Remove
                </Button>
              )}
            </div>
          ))}
          <Button
            variant="outlined"
            onClick={addHeader}
            className="AddHeaderButton"
          >
            Add Header
          </Button>
        </div>
        {(currentTabData.method === 'POST' || currentTabData.method === 'PUT' || currentTabData.method === 'PATCH') && (
          <div className="BodySection">
            <h3>Body</h3>
            <TextField
              fullWidth
              multiline
              rows={8}
              variant="outlined"
              placeholder="Enter request body here..."
              value={currentTabData.requestBody}
              onChange={(e) => updateTabData({ requestBody: e.target.value })}
            />
          </div>
        )}
        {currentTabData.response && (
          <div className="ResponseViewer">
            <h3>Response:</h3>
            {currentTabData.statusCode && (
              <div className="StatusCodeSection">
                <h4>Status Code</h4>
                <div className="StatusCodeValue">
                  <span className={`StatusCode status-${Math.floor(currentTabData.statusCode / 100)}xx`}>
                    {currentTabData.statusCode}
                  </span>
                  <span className="StatusCodeText">
                    {getStatusText(currentTabData.statusCode)}
                  </span>
                </div>
              </div>
            )}
            {currentTabData.responseHeaders && (
              <div className="ResponseHeadersSection">
                <h4>Headers</h4>
                <div className="ResponseHeadersList">
                  {Object.entries(currentTabData.responseHeaders).map(([key, value]) => (
                    <div key={key} className="ResponseHeaderItem">
                      <span className="ResponseHeaderKey">{key}:</span>
                      <span className="ResponseHeaderValue">{value}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
            <pre>
              <code className={`language-${currentTabData.responseType}`}>
                {currentTabData.response}
              </code>
            </pre>
          </div>
        )}
      </div>
    </div>
    <Dialog open={isRenameModalOpen} onClose={handleCloseRenameModal}>
      <DialogTitle>Rename Collection</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="Collection Name"
          type="text"
          fullWidth
          variant="outlined"
          value={newCollectionName}
          onChange={(e) => setNewCollectionName(e.target.value)}
          onKeyPress={(e) => {
            if (e.key === 'Enter') {
              handleSaveCollectionName();
            }
          }}
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={handleOpenDeleteCollectionModal} color="error" sx={{ marginRight: 'auto' }}>
          Delete Collection
        </Button>
        <Button onClick={handleCloseRenameModal}>Cancel</Button>
        <Button onClick={handleSaveCollectionName} variant="contained">Save</Button>
      </DialogActions>
    </Dialog>
    <Dialog open={isAddCollectionModalOpen} onClose={handleCloseAddCollectionModal}>
      <DialogTitle>Add New Collection</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="Collection Name"
          type="text"
          fullWidth
          variant="outlined"
          value={newCollectionName}
          onChange={(e) => setNewCollectionName(e.target.value)}
          onKeyPress={(e) => {
            if (e.key === 'Enter') {
              handleAddCollection();
            }
          }}
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={handleCloseAddCollectionModal}>Cancel</Button>
        <Button onClick={handleAddCollection} variant="contained">Add</Button>
      </DialogActions>
    </Dialog>
    <Dialog open={isSaveRequestModalOpen} onClose={handleCloseSaveRequestModal}>
      <DialogTitle>{isOverwriting ? 'Update Request' : 'Save Request'}</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="Request Name"
          type="text"
          fullWidth
          variant="outlined"
          value={requestName}
          onChange={(e) => setRequestName(e.target.value)}
          onKeyPress={(e) => {
            if (e.key === 'Enter') {
              handleSaveRequest();
            }
          }}
          sx={{ marginBottom: isOverwriting ? 0 : 2 }}
        />
        {!isOverwriting && (
          <FormControl fullWidth>
            <InputLabel id="collection-select-label">Collection</InputLabel>
            <Select
              labelId="collection-select-label"
              value={selectedCollectionId || ''}
              label="Collection"
              onChange={(e) => setSelectedCollectionId(e.target.value)}
            >
              {collections.map((collection) => (
                <MenuItem key={collection.id} value={collection.id}>
                  {collection.name}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={handleCloseSaveRequestModal}>Cancel</Button>
        <Button onClick={handleSaveRequest} variant="contained">
          {isOverwriting ? 'Update' : 'Save'}
        </Button>
      </DialogActions>
    </Dialog>
    <Dialog open={isDeleteRequestModalOpen} onClose={handleCloseDeleteRequestModal}>
      <DialogTitle>Delete Request</DialogTitle>
      <DialogContent>
        Are you sure you want to delete "{requestToDelete?.request.name}"? This action cannot be undone.
      </DialogContent>
      <DialogActions>
        <Button onClick={handleCloseDeleteRequestModal}>Cancel</Button>
        <Button onClick={handleDeleteRequest} variant="contained" color="error">
          Delete
        </Button>
      </DialogActions>
    </Dialog>
    <Dialog open={isDeleteCollectionModalOpen} onClose={handleCloseDeleteCollectionModal}>
      <DialogTitle>Confirm Collection Deletion</DialogTitle>
      <DialogContent>
        Are you sure you want to delete the collection "{collectionToDelete?.name}"? 
        {collectionToDelete?.requests.length > 0 && (
          <span> This will also delete {collectionToDelete.requests.length} saved request(s).</span>
        )}
        {' '}This action cannot be undone.
      </DialogContent>
      <DialogActions>
        <Button onClick={handleCloseDeleteCollectionModal}>Cancel</Button>
        <Button onClick={handleDeleteCollection} variant="contained" color="error">
          Delete Collection
        </Button>
      </DialogActions>
    </Dialog>
    <Dialog open={isExportModalOpen} onClose={handleCloseExportModal} maxWidth="md" fullWidth>
      <DialogTitle>Export Collections</DialogTitle>
      <DialogContent>
        <div className="export-json">
          <pre>
            <code className="language-json">
              {JSON.stringify(collections, null, 2)}
            </code>
          </pre>
        </div>
      </DialogContent>
      <DialogActions>
        <Button onClick={handleCopyToClipboard} startIcon={<ContentCopyIcon />}>
          Copy to Clipboard
        </Button>
        <Button onClick={handleCloseExportModal}>Close</Button>
      </DialogActions>
    </Dialog>
    <Dialog open={isImportModalOpen} onClose={handleCloseImportModal} maxWidth="md" fullWidth>
      <DialogTitle>Import Collections</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="Paste JSON here"
          multiline
          rows={15}
          fullWidth
          variant="outlined"
          value={importJson}
          onChange={(e) => setImportJson(e.target.value)}
          placeholder='Paste exported collections JSON here...'
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={handleCloseImportModal}>Cancel</Button>
        <Button onClick={handleImportCollections} variant="contained" color="warning">
          Import (Overwrite All)
        </Button>
      </DialogActions>
    </Dialog>
    <Dialog open={isRunCollectionModalOpen} onClose={handleCloseRunCollectionModal} maxWidth="md" fullWidth>
      <DialogTitle>
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5em' }}>
          <PlayArrowIcon />
          Run Collection: {runningCollection?.name}
        </div>
      </DialogTitle>
      <DialogContent>
        <List>
          {runningCollection?.requests.map((request, index) => {
            const result = runResults[index];
            return (
              <ListItem 
                key={request.id}
                sx={{
                  borderLeft: result ? (result.success ? '4px solid #4caf50' : '4px solid #f44336') : '4px solid #ddd',
                  marginBottom: '0.5em',
                  backgroundColor: result ? (result.success ? '#f1f8f4' : '#fef1f1') : '#fafafa',
                  borderRadius: '4px'
                }}
              >
                <ListItemIcon>
                  <HttpIcon fontSize="small" />
                </ListItemIcon>
                <ListItemText
                  primary={request.name}
                  secondary={
                    result 
                      ? `Status: ${result.status} - ${result.response.substring(0, 50)}${result.response.length > 50 ? '...' : ''}`
                      : `${request.method} ${request.url}`
                  }
                />
              </ListItem>
            );
          })}
        </List>
      </DialogContent>
      <DialogActions>
        <Button onClick={handleCloseRunCollectionModal}>Close</Button>
        <Button 
          onClick={handleRunCollection} 
          variant="contained" 
          startIcon={<PlayArrowIcon />}
          disabled={isRunning || !runningCollection?.requests.length}
        >
          {isRunning ? 'Running...' : 'Run'}
        </Button>
      </DialogActions>
    </Dialog>
    </div>
  );
}

export default App;
