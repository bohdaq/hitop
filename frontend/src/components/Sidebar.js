import React from 'react';
import Drawer from '@mui/material/Drawer';
import MenuList from '@mui/material/MenuList';
import MenuItem from '@mui/material/MenuItem';
import ListItemText from '@mui/material/ListItemText';
import ListItemIcon from '@mui/material/ListItemIcon';
import IconButton from '@mui/material/IconButton';
import AddIcon from '@mui/icons-material/Add';
import EditIcon from '@mui/icons-material/Edit';
import CloseIcon from '@mui/icons-material/Close';
import FolderIcon from '@mui/icons-material/Folder';
import HttpIcon from '@mui/icons-material/Http';
import HistoryIcon from '@mui/icons-material/History';
import FileDownloadIcon from '@mui/icons-material/FileDownload';
import FileUploadIcon from '@mui/icons-material/FileUpload';
import SettingsIcon from '@mui/icons-material/Settings';

const Sidebar = ({
  collections,
  onAddCollection,
  onRenameCollection,
  onRunCollection,
  onLoadRequest,
  onDeleteRequest,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
  onOpenHistory,
  onOpenExport,
  onOpenImport,
  onOpenVariables
}) => {
  return (
    <Drawer
      variant="permanent"
      className="Sidebar"
      classes={{
        paper: 'SidebarPaper',
      }}
    >
      <MenuList sx={{ height: '100%', display: 'flex', flexDirection: 'column', padding: 0 }}>
        <MenuItem className="SidebarHeader" disabled>
          <img 
            src={`${process.env.PUBLIC_URL}/logo192.png`}
            alt="HITOP Logo" 
            className="SidebarLogo"
          />
          <span className="SidebarTitle">
            HITOP
          </span>
        </MenuItem>
        <MenuItem className="CollectionsMenuItem">
          <ListItemText>Collections</ListItemText>
          <IconButton
            size="small"
            className="AddCollectionButton"
            onClick={onAddCollection}
            aria-label="add collection"
          >
            <AddIcon fontSize="small" />
          </IconButton>
        </MenuItem>
        {collections.map((collection) => (
          <div key={collection.id}>
            <MenuItem 
              className="SubMenuItem"
              onClick={(e) => {
                if (collection.requests && collection.requests.length > 0) {
                  onRunCollection(e, collection);
                }
              }}
            >
              <ListItemIcon sx={{ minWidth: '32px' }}>
                <FolderIcon fontSize="small" />
              </ListItemIcon>
              <ListItemText>{collection.name}</ListItemText>
              <IconButton
                size="small"
                className="EditCollectionButton"
                onClick={(e) => onRenameCollection(e, collection.id, collection.name)}
                aria-label="edit collection"
              >
                <EditIcon fontSize="small" />
              </IconButton>
            </MenuItem>
            <MenuItem 
              className="RequestMenuItem VariablesMenuItem"
              onClick={(e) => {
                e.stopPropagation();
                onOpenVariables(collection);
              }}
              sx={{ 
                paddingLeft: '48px',
                cursor: 'pointer !important'
              }}
            >
              <ListItemIcon sx={{ minWidth: '32px' }}>
                <SettingsIcon fontSize="small" />
              </ListItemIcon>
              <ListItemText>Variables</ListItemText>
            </MenuItem>
            {(collection.requests || []).map((request) => (
              <MenuItem 
                key={request.id} 
                className="RequestMenuItem"
                onClick={() => onLoadRequest(request, collection.id)}
                draggable
                onDragStart={(e) => onDragStart(e, request, collection.id)}
                onDragOver={onDragOver}
                onDrop={(e) => onDrop(e, request, collection.id)}
                onDragEnd={onDragEnd}
                sx={{
                  cursor: 'grab',
                  '&:active': {
                    cursor: 'grabbing'
                  }
                }}
              >
                <ListItemIcon sx={{ minWidth: '32px' }}>
                  <HttpIcon fontSize="small" />
                </ListItemIcon>
                <ListItemText>{request.name}</ListItemText>
                <IconButton
                  size="small"
                  className="DeleteRequestButton"
                  onClick={(e) => onDeleteRequest(e, request, collection.id)}
                  aria-label="delete request"
                >
                  <CloseIcon fontSize="small" />
                </IconButton>
              </MenuItem>
            ))}
          </div>
        ))}
        <MenuItem onClick={onOpenHistory} sx={{ marginTop: 'auto', borderTop: '1px solid #ddd' }}>
          <ListItemIcon>
            <HistoryIcon fontSize="small" />
          </ListItemIcon>
          <ListItemText>History</ListItemText>
        </MenuItem>
        <MenuItem onClick={onOpenExport}>
          <ListItemIcon>
            <FileDownloadIcon fontSize="small" />
          </ListItemIcon>
          <ListItemText>Export</ListItemText>
        </MenuItem>
        <MenuItem onClick={onOpenImport}>
          <ListItemIcon>
            <FileUploadIcon fontSize="small" />
          </ListItemIcon>
          <ListItemText>Import</ListItemText>
        </MenuItem>
      </MenuList>
    </Drawer>
  );
};

export default Sidebar;
