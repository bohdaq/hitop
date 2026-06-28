import React from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';

const RenameRequestModal = ({ 
  open, 
  onClose, 
  requestName, 
  onNameChange, 
  onRename 
}) => {
  return (
    <Dialog open={open} onClose={onClose}>
      <DialogTitle>Rename Request</DialogTitle>
      <DialogContent>
        <TextField
          autoFocus
          margin="dense"
          label="Request Name"
          fullWidth
          variant="outlined"
          value={requestName}
          onChange={(e) => onNameChange(e.target.value)}
          onKeyPress={(e) => {
            if (e.key === 'Enter') {
              onRename();
            }
          }}
        />
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button onClick={onRename} variant="contained">
          Rename
        </Button>
      </DialogActions>
    </Dialog>
  );
};

export default RenameRequestModal;
