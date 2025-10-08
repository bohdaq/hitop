import React from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';

const ImportModal = ({ open, onClose, importJson, onJsonChange, onImport }) => {
  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
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
          onChange={(e) => onJsonChange(e.target.value)}
          placeholder='Paste exported collections JSON here...'
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
