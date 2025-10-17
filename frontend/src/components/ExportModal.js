import React, { useState, useEffect } from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import ToggleButton from '@mui/material/ToggleButton';
import ToggleButtonGroup from '@mui/material/ToggleButtonGroup';
import hljs from 'highlight.js';
import * as collectionService from '../services/collectionService';

const ExportModal = ({ open, onClose, collections, onCopyToClipboard }) => {
  const [format, setFormat] = useState('postman');
  const [exportedJson, setExportedJson] = useState('');

  useEffect(() => {
    if (open) {
      const json = collectionService.exportCollections(collections, format);
      setExportedJson(json);
      setTimeout(() => {
        document.querySelectorAll('.export-json code').forEach((block) => {
          hljs.highlightElement(block);
        });
      }, 100);
    }
  }, [open, collections, format]);

  const handleFormatChange = (event, newFormat) => {
    if (newFormat !== null) {
      setFormat(newFormat);
    }
  };

  const handleCopyToClipboard = () => {
    onCopyToClipboard(exportedJson);
  };

  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>
        Export Collections
      </DialogTitle>
      <DialogContent>
        <div style={{ marginBottom: '20px' }}>
          <ToggleButtonGroup
            value={format}
            exclusive
            onChange={handleFormatChange}
            aria-label="export format"
            size="small"
          >
            <ToggleButton value="postman" aria-label="postman format">
              Postman v2.1
            </ToggleButton>
            <ToggleButton value="bruno" aria-label="bruno format">
              Bruno
            </ToggleButton>
            <ToggleButton value="hitop" aria-label="hitop format">
              HITOP Native
            </ToggleButton>
          </ToggleButtonGroup>
          <p style={{ marginTop: '10px', color: '#666', fontSize: '0.9em' }}>
            {format === 'postman' 
              ? 'Postman v2.1 format - Compatible with Postman and other tools'
              : format === 'bruno'
              ? 'Bruno format - Compatible with Bruno API client'
              : 'HITOP native format - Optimized for HITOP import'}
          </p>
        </div>
        <div className="export-json">
          <pre>
            <code className="language-json">
              {exportedJson}
            </code>
          </pre>
        </div>
      </DialogContent>
      <DialogActions>
        <Button onClick={handleCopyToClipboard} startIcon={<ContentCopyIcon />}>
          Copy to Clipboard
        </Button>
        <Button onClick={onClose}>Close</Button>
      </DialogActions>
    </Dialog>
  );
};

export default ExportModal;
