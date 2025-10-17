import React from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemIcon from '@mui/material/ListItemIcon';
import ListItemText from '@mui/material/ListItemText';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import HttpIcon from '@mui/icons-material/Http';

const RunCollectionModal = ({
  open,
  onClose,
  collection,
  runResults,
  isRunning,
  onRun
}) => {
  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5em' }}>
          <PlayArrowIcon />
          Run Collection: {collection?.name}
        </div>
      </DialogTitle>
      <DialogContent sx={{ paddingTop: '20px', paddingBottom: '20px' }}>
        <List sx={{ padding: '40px' }}>
          {collection?.requests.map((request, index) => {
            const result = runResults[index];
            return (
              <ListItem
                key={request.id}
                className={
                  result
                    ? (result.success ? 'request-success' : 'request-failed')
                    : 'request-pending'
                }
                sx={{
                  borderLeft: result ? (result.success ? '4px solid #4caf50' : '4px solid #f44336') : '4px solid #ddd',
                  marginBottom: '0.5em',
                  marginTop: '0.5em',
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
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Close</Button>
        <Button
          onClick={onRun}
          variant="contained"
          startIcon={<PlayArrowIcon />}
          disabled={isRunning || !collection?.requests.length}
        >
          {isRunning ? 'Running...' : 'Run'}
        </Button>
      </DialogActions>
    </Dialog>
  );
};

export default RunCollectionModal;
