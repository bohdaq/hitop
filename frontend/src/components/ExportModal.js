import React, { useEffect } from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import hljs from 'highlight.js';

const ExportModal = ({ open, onClose, collections, onCopyToClipboard }) => {
  useEffect(() => {
    if (open) {
      setTimeout(() => {
        document.querySelectorAll('.export-json code').forEach((block) => {
          hljs.highlightElement(block);
        });
      }, 100);
    }
  }, [open]);

  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>Export Collections</DialogTitle>
      <DialogContent>
        <div className="export-json">
          <pre>
            <code className="language-json">
              {JSON.stringify(collections, null, 2)}
            </code>
          </pre>
        </div>
      </DialogContent>
      <DialogActions>
        <Button onClick={onCopyToClipboard} startIcon={<ContentCopyIcon />}>
          Copy to Clipboard
        </Button>
        <Button onClick={onClose}>Close</Button>
      </DialogActions>
    </Dialog>
  );
};

export default ExportModal;
