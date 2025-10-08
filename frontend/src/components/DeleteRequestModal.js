import React from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';

const DeleteRequestModal = ({ open, onClose, requestName, onDelete }) => {
  return (
    <Dialog open={open} onClose={onClose}>
      <DialogTitle>Delete Request</DialogTitle>
      <DialogContent>
        Are you sure you want to delete the request "{requestName}"? This action cannot be undone.
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button onClick={onDelete} variant="contained" color="error">
          Delete
        </Button>
      </DialogActions>
    </Dialog>
  );
};

export default DeleteRequestModal;
