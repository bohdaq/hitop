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
import HistoryIcon from '@mui/icons-material/History';
import HttpIcon from '@mui/icons-material/Http';

const HistoryModal = ({ open, onClose, requestHistory, onLoadHistoryItem }) => {
  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5em' }}>
          <HistoryIcon />
          Request History
        </div>
      </DialogTitle>
      <DialogContent>
        {requestHistory.length === 0 ? (
          <div style={{ textAlign: 'center', padding: '2em', color: '#666' }}>
            No requests in history yet. Make a request to see it here.
          </div>
        ) : (
          <List>
            {requestHistory.map((item) => {
              const date = new Date(item.timestamp);
              const timeStr = date.toLocaleTimeString();
              return (
                <ListItem 
                  key={item.id}
                  onClick={() => onLoadHistoryItem(item)}
                  sx={{
                    borderLeft: item.success ? '4px solid #4caf50' : '4px solid #f44336',
                    marginBottom: '0.5em',
                    backgroundColor: item.success ? '#f1f8f4' : '#fef1f1',
                    borderRadius: '4px',
                    cursor: 'pointer',
                    '&:hover': {
                      backgroundColor: item.success ? '#e8f5e9' : '#ffebee',
                      transform: 'translateX(4px)',
                      transition: 'all 0.2s'
                    }
                  }}
                >
                  <ListItemIcon>
                    <HttpIcon fontSize="small" />
                  </ListItemIcon>
                  <ListItemText
                    primary={`${item.method} ${item.url}`}
                    secondary={`${timeStr} - Status: ${item.statusCode || 'Error'}`}
                  />
                </ListItem>
              );
            })}
          </List>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Close</Button>
      </DialogActions>
    </Dialog>
  );
};

export default HistoryModal;
