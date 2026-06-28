import React from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';

const DeleteCollectionModal = ({ open, onClose, collection, onDelete }) => {
  return (
    <Dialog open={open} onClose={onClose}>
      <DialogTitle>Delete Collection</DialogTitle>
      <DialogContent>
        Are you sure you want to delete the collection "{collection?.name}"? 
        {collection?.requests.length > 0 && (
          <span> This will also delete {collection.requests.length} saved request(s).</span>
        )}
        {' '}This action cannot be undone.
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancel</Button>
        <Button onClick={onDelete} variant="contained" color="error">
          Delete Collection
        </Button>
      </DialogActions>
    </Dialog>
  );
};

export default DeleteCollectionModal;
