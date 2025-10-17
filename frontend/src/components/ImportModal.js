import React, { useState } from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import ToggleButton from '@mui/material/ToggleButton';
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup';

const ImportModal = ({ open, onClose, importJson, onJsonChange, onImport }) => {
  const [formatHint, setFormatHint] = useState('auto');

  const handleFormatHintChange = (event, newFormat) => {
    if (newFormat !== null) {
      setFormatHint(newFormat);
    }
  };

  const getPlaceholder = () => {
    if (formatHint === 'postman') {
      return 'Paste Postman collection JSON here (v2.1 format)...';
    } else if (formatHint === 'bruno') {
      return 'Paste Bruno collection JSON here...';
    } else if (formatHint === 'hitop') {
      return 'Paste HITOP collection JSON here...';
    }
    return 'Paste Postman, Bruno, or HITOP collection JSON here (auto-detected)...';
  };

  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>Import Collections</DialogTitle>
      <DialogContent>
        <div style={{ marginBottom: '15px' }}>
          <ToggleButtonGroup
            value={formatHint}
            exclusive
            onChange={handleFormatHintChange}
            aria-label="import format"
            size="small"
          >
            <ToggleButton value="auto" aria-label="auto detect">
              Auto-Detect
            </ToggleButton>
            <ToggleButton value="postman" aria-label="postman format">
              Postman
            </ToggleButton>
            <ToggleButton value="bruno" aria-label="bruno format">
              Bruno
            </ToggleButton>
            <ToggleButton value="hitop" aria-label="hitop format">
              HITOP
            </ToggleButton>
          </ToggleButtonGroup>
          <p style={{ marginTop: '10px', color: '#666', fontSize: '0.9em' }}>
            {formatHint === 'auto' 
              ? 'Format will be automatically detected from the JSON structure'
              : formatHint === 'postman'
              ? 'Expecting Postman v2.1 collection format'
              : formatHint === 'bruno'
              ? 'Expecting Bruno collection format'
              : 'Expecting HITOP native collection format'}
          </p>
        </div>
        <TextField
          autoFocus
          margin="dense"
          label="Paste Postman or HITOP collection JSON"
          multiline
          rows={15}
          fullWidth
          variant="outlined"
          value={importJson}
          onChange={(e) => onJsonChange(e.target.value)}
          placeholder={getPlaceholder()}
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button onClick={onImport} variant="contained" color="warning">
          Import (Overwrite All)
        </Button>
      </DialogActions>
    </Dialog>
  );
};

export default ImportModal;
