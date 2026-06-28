import React from 'react';
import Drawer from '@mui/material/Drawer';
import MenuList from '@mui/material/MenuList';
import MenuItem from '@mui/material/MenuItem';
import ListItemText from '@mui/material/ListItemText';
import ListItemIcon from '@mui/material/ListItemIcon';
import FolderIcon from '@mui/icons-material/Folder';
import HttpIcon from '@mui/icons-material/Http';
import HistoryIcon from '@mui/icons-material/History';
import AddIcon from '@mui/icons-material/Add';
import FileDownloadIcon from '@mui/icons-material/FileDownload';
import FileUploadIcon from '@mui/icons-material/FileUpload';
import SettingsIcon from '@mui/icons-material/Settings';

const Sidebar = ({
  collections,
  onRunCollection,
  onLoadRequest,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
  onOpenHistory,
  onOpenExport,
  onOpenImport,
  onOpenVariables,
  onShowCollections,
  onShowCollectionDetail,
  onCreateNewRequest
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
        <MenuItem 
          className="NewRequestMenuItem"
          onClick={onCreateNewRequest}
          sx={{ 
            cursor: 'pointer',
            backgroundColor: 'action.hover',
            '&:hover': {
              backgroundColor: 'action.selected'
            }
          }}
        >
          <ListItemIcon sx={{ minWidth: '32px' }}>
            <AddIcon fontSize="small" />
          </ListItemIcon>
          <ListItemText>New Request</ListItemText>
        </MenuItem>
        <MenuItem 
          className="CollectionsMenuItem"
          onClick={onShowCollections}
          sx={{ cursor: 'pointer' }}
        >
          <ListItemText>Collections</ListItemText>
        </MenuItem>
        {collections.map((collection) => (
          <div key={collection.id}>
            <MenuItem 
              className="SubMenuItem"
              onClick={(e) => {
                e.stopPropagation();
                onShowCollectionDetail(collection);
              }}
              sx={{ 
                paddingLeft: '32px !important'
              }}
            >
              <ListItemIcon sx={{ minWidth: '32px' }}>
                <FolderIcon fontSize="small" />
              </ListItemIcon>
              <ListItemText>{collection.name}</ListItemText>
            </MenuItem>
            <MenuItem 
              className="RequestMenuItem VariablesMenuItem"
              onClick={(e) => {
                e.stopPropagation();
                onOpenVariables(collection);
              }}
              sx={{ 
                paddingLeft: '64px !important',
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
                  paddingLeft: '64px !important',
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
