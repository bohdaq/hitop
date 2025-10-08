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

const CollectionVariablesModal = ({ open, onClose, collection, onSave }) => {
  const [variables, setVariables] = useState([]);

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

  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>
        Collection Variables: {collection?.name}
      </DialogTitle>
      <DialogContent>
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
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button onClick={handleSave} variant="contained">
          Save Variables
        </Button>
      </DialogActions>
    </Dialog>
  );
};

export default CollectionVariablesModal;
