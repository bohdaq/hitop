import React from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import FormControl from '@mui/material/FormControl';
import InputLabel from '@mui/material/InputLabel';
import Select from '@mui/material/Select';
import MenuItem from '@mui/material/MenuItem';

const SaveRequestModal = ({ 
  open, 
  onClose, 
  requestName, 
  onNameChange, 
  collections, 
  selectedCollectionId, 
  onCollectionChange, 
  onSave,
  isOverwriting 
}) => {
  return (
    <Dialog open={open} onClose={onClose}>
      <DialogTitle>{isOverwriting ? 'Update Request' : 'Save Request'}</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="Request Name"
          fullWidth
          variant="outlined"
          value={requestName}
          onChange={(e) => onNameChange(e.target.value)}
        />
        <FormControl fullWidth margin="dense">
          <InputLabel>Collection</InputLabel>
          <Select
            value={selectedCollectionId || ''}
            onChange={(e) => onCollectionChange(e.target.value)}
            label="Collection"
          >
            {collections.map((collection) => (
              <MenuItem key={collection.id} value={collection.id}>
                {collection.name}
              </MenuItem>
            ))}
          </Select>
        </FormControl>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button onClick={onSave} variant="contained">
          {isOverwriting ? 'Update' : 'Save'}
        </Button>
      </DialogActions>
    </Dialog>
  );
};

export default SaveRequestModal;
