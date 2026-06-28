import React from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import Button from '@mui/material/Button';
import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import List from '@mui/material/List';
import ListItem from '@mui/material/ListItem';
import ListItemText from '@mui/material/ListItemText';
import ListItemIcon from '@mui/material/ListItemIcon';
import IconButton from '@mui/material/IconButton';
import EditIcon from '@mui/icons-material/Edit';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import HttpIcon from '@mui/icons-material/Http';
import DeleteIcon from '@mui/icons-material/Delete';
import FolderIcon from '@mui/icons-material/Folder';
import SettingsIcon from '@mui/icons-material/Settings';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import CheckCircleIcon from '@mui/icons-material/CheckCircle';
import ErrorIcon from '@mui/icons-material/Error';
import CircularProgress from '@mui/material/CircularProgress';
import Chip from '@mui/material/Chip';

const CollectionDetailView = ({ 
  collection, 
  onRenameCollection, 
  onRunCollection,
  onLoadRequest,
  onDeleteRequest,
  onDeleteCollection,
  onDuplicateCollection,
  onOpenVariables,
  runResults,
  isRunning
}) => {
  if (!collection) {
    return (
      <Box sx={{ padding: 3, textAlign: 'center' }}>
        <Typography variant="h6" color="text.secondary">
          No collection selected
        </Typography>
      </Box>
    );
  }

  return (
    <Box sx={{ padding: 3 }}>
      <Box sx={{ marginBottom: 3 }}>
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, marginBottom: 1.5 }}>
          <FolderIcon sx={{ fontSize: 40, color: 'primary.main' }} />
          <Typography variant="h4">
            {collection.name}
          </Typography>
        </Box>
        <Box sx={{ display: 'flex', gap: 1, flexWrap: 'wrap' }}>
          <Button
            variant="outlined"
            startIcon={<EditIcon />}
            onClick={(e) => onRenameCollection(e, collection.id, collection.name)}
          >
            Rename
          </Button>
          <Button
            variant="outlined"
            startIcon={<SettingsIcon />}
            onClick={onOpenVariables}
          >
            Variables
          </Button>
          <Button
            variant="outlined"
            startIcon={<ContentCopyIcon />}
            onClick={(e) => onDuplicateCollection(e, collection)}
          >
            Duplicate
          </Button>
          <Button
            variant="outlined"
            color="error"
            startIcon={<DeleteIcon />}
            onClick={(e) => onDeleteCollection(e, collection)}
          >
            Delete
          </Button>
          <Button
            variant="contained"
            startIcon={isRunning ? <CircularProgress size={20} color="inherit" /> : <PlayArrowIcon />}
            onClick={(e) => onRunCollection(e, collection)}
            disabled={!collection.requests || collection.requests.length === 0 || isRunning}
          >
            {isRunning ? 'Running...' : 'Run Collection'}
          </Button>
        </Box>
      </Box>

      {runResults && runResults.length > 0 && (
        <Card sx={{ marginBottom: 3 }}>
          <CardContent>
            <Typography variant="h6" gutterBottom>
              Run Results
            </Typography>
            <List>
              {runResults.map((result, index) => (
                <ListItem
                  key={index}
                  sx={{
                    border: '1px solid',
                    borderColor: result.success ? 'success.main' : 'error.main',
                    borderRadius: 1,
                    marginBottom: 1,
                    backgroundColor: result.success ? 'success.light' : 'error.light',
                    opacity: 0.9
                  }}
                >
                  <ListItemIcon>
                    {result.success ? (
                      <CheckCircleIcon color="success" />
                    ) : (
                      <ErrorIcon color="error" />
                    )}
                  </ListItemIcon>
                  <ListItemText
                    primary={result.requestName}
                    secondary={
                      <Box>
                        <Typography variant="body2" component="span">
                          {result.method} {result.url}
                        </Typography>
                        <br />
                        <Chip
                          label={result.statusCode || 'Error'}
                          size="small"
                          color={result.success ? 'success' : 'error'}
                          sx={{ marginTop: 0.5 }}
                        />
                        {result.duration && (
                          <Chip
                            label={`${result.duration}ms`}
                            size="small"
                            sx={{ marginLeft: 1, marginTop: 0.5 }}
                          />
                        )}
                      </Box>
                    }
                  />
                </ListItem>
              ))}
            </List>
          </CardContent>
        </Card>
      )}

      <Typography variant="h6" gutterBottom>
        Requests
      </Typography>
      
      {collection.requests && collection.requests.length > 0 ? (
        <List>
          {collection.requests.map((request) => (
            <ListItem
              key={request.id}
              sx={{
                border: '1px solid',
                borderColor: 'divider',
                borderRadius: 1,
                marginBottom: 1,
                cursor: 'pointer',
                '&:hover': {
                  backgroundColor: 'action.hover'
                }
              }}
              onClick={() => onLoadRequest(request, collection.id)}
              secondaryAction={
                <IconButton
                  edge="end"
                  aria-label="delete"
                  onClick={(e) => onDeleteRequest(e, request, collection.id)}
                >
                  <DeleteIcon />
                </IconButton>
              }
            >
              <ListItemIcon>
                <HttpIcon />
              </ListItemIcon>
              <ListItemText
                primary={request.name}
                secondary={`${request.method} ${request.url || '(no URL)'}`}
              />
            </ListItem>
          ))}
        </List>
      ) : (
        <Box sx={{ 
          textAlign: 'center', 
          padding: 4,
          border: '1px dashed',
          borderColor: 'divider',
          borderRadius: 1,
          color: 'text.secondary'
        }}>
          <Typography variant="body1">
            No requests in this collection
          </Typography>
          <Typography variant="body2" sx={{ marginTop: 1 }}>
            Create a new request and save it to this collection
          </Typography>
        </Box>
      )}
    </Box>
  );
};

export default CollectionDetailView;
