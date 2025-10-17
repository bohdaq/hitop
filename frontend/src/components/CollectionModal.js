import React, { useState, useEffect } from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import IconButton from '@mui/material/IconButton';
import DeleteIcon from '@mui/icons-material/Delete';
import AddIcon from '@mui/icons-material/Add';
import FileDownloadIcon from '@mui/icons-material/FileDownload';
import Snackbar from '@mui/material/Snackbar';
import Alert from '@mui/material/Alert';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import HttpIcon from '@mui/icons-material/Http';
import Tabs from '@mui/material/Tabs';
import Tab from '@mui/material/Tab';
import Box from '@mui/material/Box';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemIcon from '@mui/material/ListItemIcon';
import ListItemText from '@mui/material/ListItemText';
import * as collectionService from '../services/collectionService';

const CollectionModal = ({ open, onClose, collection, onSave, runResults, isRunning, onRun, onImport }) => {
  const [variables, setVariables] = useState([]);
  const [snackbarOpen, setSnackbarOpen] = useState(false);
  const [snackbarMessage, setSnackbarMessage] = useState('');
  const [currentTab, setCurrentTab] = useState(0);
  const [importJson, setImportJson] = useState('');

  useEffect(() => {
    if (collection && collection.variables) {
      // Convert object to array of key-value pairs
      const varsArray = Object.entries(collection.variables).map(([key, value]) => ({
        key,
        value
      }));
      setVariables(varsArray.length > 0 ? varsArray : [{ key: '', value: '' }]);
    } else {
      setVariables([{ key: '', value: '' }]);
    }
  }, [collection, open]);

  const handleAddVariable = () => {
    setVariables([...variables, { key: '', value: '' }]);
  };

  const handleRemoveVariable = (index) => {
    if (variables.length > 1) {
      setVariables(variables.filter((_, i) => i !== index));
    }
  };

  const handleVariableChange = (index, field, value) => {
    const newVariables = [...variables];
    newVariables[index][field] = value;
    setVariables(newVariables);
  };

  const handleSave = () => {
    // Convert array back to object, filtering out empty keys
    const variablesObject = {};
    variables.forEach(({ key, value }) => {
      if (key.trim()) {
        variablesObject[key.trim()] = value;
      }
    });
    onSave(variablesObject);
    onClose();
  };

  const handleExportFormat = (format) => {
    if (!collection) return;

    const exportedJson = collectionService.exportCollections([collection], format);

    // Copy to clipboard
    navigator.clipboard.writeText(exportedJson).then(() => {
      const formatName = format === 'postman' ? 'Postman v2.1' : format === 'bruno' ? 'Bruno' : 'HITOP';
      setSnackbarMessage(`Collection exported as ${formatName} format and copied to clipboard!`);
      setSnackbarOpen(true);
    }).catch(err => {
      console.error('Failed to copy:', err);
      setSnackbarMessage('Failed to copy to clipboard');
      setSnackbarOpen(true);
    });
  };

  const handleSnackbarClose = () => {
    setSnackbarOpen(false);
  };

  const handleTabChange = (event, newValue) => {
    setCurrentTab(newValue);
  };

  const handleImport = () => {
    if (!importJson.trim()) {
      setSnackbarMessage('Please paste JSON to import');
      setSnackbarOpen(true);
      return;
    }

    try {
      const importedCollections = collectionService.importCollections(importJson);
      if (onImport) {
        onImport(importedCollections);
        setSnackbarMessage(`Successfully imported ${importedCollections.length} collection(s)!`);
        setSnackbarOpen(true);
        setImportJson('');
      }
    } catch (error) {
      setSnackbarMessage(`Import failed: ${error.message}`);
      setSnackbarOpen(true);
    }
  };

  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>
        Collection: {collection?.name}
      </DialogTitle>
      <DialogContent>
        <Tabs value={currentTab} onChange={handleTabChange} sx={{ borderBottom: 1, borderColor: 'divider', marginBottom: 2 }}>
          <Tab label="Settings" />
          <Tab label="Runner" />
          <Tab label="Export" />
          <Tab label="Import" />
        </Tabs>
        {currentTab === 0 && (
          <>
            <div style={{ marginBottom: '1em', color: '#666', fontSize: '0.9em' }}>
              Define variables that can be used in URLs, headers, body, and scripts using ${'{'}variableName{'}'} syntax.
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5em' }}>
              {variables.map((variable, index) => (
                <div key={index} style={{ display: 'flex', gap: '0.5em', alignItems: 'center' }}>
                  <TextField
                    label="Variable Name"
                    variant="outlined"
                    size="small"
                    value={variable.key}
                    onChange={(e) => handleVariableChange(index, 'key', e.target.value)}
                    placeholder="e.g., apiUrl, authToken"
                    style={{ flex: 1 }}
                  />
                  <TextField
                    label="Value"
                    variant="outlined"
                    size="small"
                    value={variable.value}
                    onChange={(e) => handleVariableChange(index, 'value', e.target.value)}
                    placeholder="e.g., https://api.example.com"
                    style={{ flex: 2 }}
                  />
                  <IconButton
                    size="small"
                    color="error"
                    onClick={() => handleRemoveVariable(index)}
                    disabled={variables.length === 1}
                    aria-label="delete variable"
                  >
                    <DeleteIcon fontSize="small" />
                  </IconButton>
                </div>
              ))}
            </div>
            <Button
              variant="outlined"
              startIcon={<AddIcon />}
              onClick={handleAddVariable}
              style={{ marginTop: '1em' }}
            >
              Add Variable
            </Button>
          </>
        )}
        {currentTab === 1 && (
          <Box>
            <List>
              {(collection?.requests || []).map((request, index) => {
                const result = runResults?.[index];
                return (
                  <ListItem
                    key={request.id}
                    sx={{
                      borderLeft: result ? (result.success ? '4px solid #4caf50' : '4px solid #f44336') : '4px solid #ddd',
                      marginBottom: '0.5em',
                      backgroundColor: result ? (result.success ? '#f1f8f4' : '#fef1f1') : '#fafafa',
                      borderRadius: '4px',
                      transition: 'all 0.3s ease'
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
          </Box>
        )}
        {currentTab === 2 && (
          <Box>
            <div style={{ marginBottom: '1.5em', color: '#666' }}>
              Export this collection in HITOP format. The JSON will be copied to your clipboard.
            </div>
            <Button
              variant="contained"
              startIcon={<FileDownloadIcon />}
              onClick={() => handleExportFormat('hitop')}
              size="large"
              fullWidth
              sx={{ padding: '1.5em' }}
            >
              <div style={{ textAlign: 'center', flex: 1 }}>
                <div style={{ fontWeight: 'bold', fontSize: '1.1em' }}>Export Collection</div>
                <div style={{ fontSize: '0.9em', marginTop: '0.5em', opacity: 0.9 }}>HITOP native format</div>
              </div>
            </Button>
          </Box>
        )}
        {currentTab === 3 && (
          <Box>
            <div style={{ marginBottom: '1.5em', color: '#666' }}>
              Paste HITOP collection JSON below to import. The imported collections will be added to your workspace.
            </div>
            <TextField
              multiline
              rows={15}
              fullWidth
              variant="outlined"
              value={importJson}
              onChange={(e) => setImportJson(e.target.value)}
              placeholder='Paste HITOP collection JSON here...'
              sx={{ marginBottom: '1em' }}
            />
            <Button
              variant="contained"
              onClick={handleImport}
              disabled={!importJson.trim()}
              fullWidth
            >
              Import Collection(s)
            </Button>
          </Box>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Close</Button>
        {currentTab === 0 && (
          <Button onClick={handleSave} variant="contained">
            Save Variables
          </Button>
        )}
        {currentTab === 1 && (
          <Button
            onClick={onRun}
            variant="contained"
            startIcon={<PlayArrowIcon />}
            disabled={isRunning || !collection?.requests?.length}
          >
            {isRunning ? 'Running...' : 'Run Collection'}
          </Button>
        )}
      </DialogActions>
      <Snackbar
        open={snackbarOpen}
        autoHideDuration={3000}
        onClose={handleSnackbarClose}
        anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
      >
        <Alert onClose={handleSnackbarClose} severity="success" sx={{ width: '100%' }}>
          {snackbarMessage}
        </Alert>
      </Snackbar>
    </Dialog>
  );
};

export default CollectionModal;
