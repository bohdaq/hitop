import React, { useState, useEffect } from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import hljs from 'highlight.js';
import * as collectionService from '../services/collectionService';

const ExportModal = ({ open, onClose, collections, onCopyToClipboard }) => {
  const [exportedJson, setExportedJson] = useState('');

  useEffect(() => {
    if (open) {
      const json = collectionService.exportCollections(collections, 'hitop');
      setExportedJson(json);
      setTimeout(() => {
        document.querySelectorAll('.export-json code').forEach((block) => {
          hljs.highlightElement(block);
        });
      }, 100);
    }
  }, [open, collections]);

  const handleCopyToClipboard = () => {
    onCopyToClipboard(exportedJson);
  };

  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>
        Export Collections
      </DialogTitle>
      <DialogContent>
        <p style={{ marginBottom: '15px', color: '#666' }}>
          Collections exported in HITOP native format.
        </p>
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
