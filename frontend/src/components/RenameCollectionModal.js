import React from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';

const RenameCollectionModal = ({ 
  open, 
  onClose, 
  collectionName, 
  onNameChange, 
  onRename, 
  onDelete 
}) => {
  return (
    <Dialog open={open} onClose={onClose}>
      <DialogTitle>Rename Collection</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="Collection Name"
          fullWidth
          variant="outlined"
          value={collectionName}
          onChange={(e) => onNameChange(e.target.value)}
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={onDelete} color="error">
          Delete Collection
        </Button>
        <div style={{ flex: 1 }} />
        <Button onClick={onClose}>Cancel</Button>
        <Button onClick={onRename} variant="contained">
          Rename
        </Button>
      </DialogActions>
    </Dialog>
  );
};

export default RenameCollectionModal;
