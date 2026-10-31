import React, { useState, useEffect } from 'react';
import Dialog from '@mui/material/Dialog';
import DialogTitle from '@mui/material/DialogTitle';
import DialogContent from '@mui/material/DialogContent';
import DialogActions from '@mui/material/DialogActions';
import Button from '@mui/material/Button';
import TextField from '@mui/material/TextField';
import IconButton from '@mui/material/IconButton';
import Chip from '@mui/material/Chip';
import DeleteIcon from '@mui/icons-material/Delete';
import AddIcon from '@mui/icons-material/Add';

/**
 * Environments: named variable sets (Dev, Staging, Prod) whose values
 * override collection variables. Shared with the CLI's `hitop env`.
 */
const EnvironmentsModal = ({ open, onClose, environments, activeEnvironmentId, onChange, onActivate }) => {
  const [selectedId, setSelectedId] = useState(null);
  const [newName, setNewName] = useState('');
  const [rows, setRows] = useState([]);

  const selected = environments.find(e => e.id === selectedId) || null;

  useEffect(() => {
    if (open) setSelectedId(activeEnvironmentId ?? environments[0]?.id ?? null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  useEffect(() => {
    const vars = Object.entries(selected?.variables || {}).map(([key, value]) => ({ key, value }));
    setRows(vars.length > 0 ? vars : [{ key: '', value: '' }]);
  }, [selectedId, open]); // eslint-disable-line react-hooks/exhaustive-deps

  const saveRows = (next) => {
    setRows(next);
    if (!selected) return;
    const variables = {};
    next.forEach(({ key, value }) => {
      if (key.trim()) variables[key.trim()] = value;
    });
    onChange(environments.map(e => (e.id === selected.id ? { ...e, variables } : e)));
  };

  const addEnvironment = () => {
    const name = newName.trim();
    if (!name || environments.some(e => e.name === name)) return;
    const env = { id: Date.now().toString(16), name, variables: {} };
    onChange([...environments, env]);
    if (!activeEnvironmentId) onActivate(env.id);
    setSelectedId(env.id);
    setNewName('');
  };

  const deleteEnvironment = () => {
    if (!selected) return;
    onChange(environments.filter(e => e.id !== selected.id));
    if (activeEnvironmentId === selected.id) onActivate(null);
    setSelectedId(null);
  };

  return (
    <Dialog open={open} onClose={onClose} maxWidth="md" fullWidth>
      <DialogTitle>Environments</DialogTitle>
      <DialogContent>
        <p style={{ marginTop: 0 }}>
          The active environment's variables override collection variables with the same name.
          The CLI reads the same environments (<code>hitop env</code>).
        </p>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginBottom: 16 }}>
          {environments.map(env => (
            <Chip
              key={env.id}
              label={env.id === activeEnvironmentId ? `${env.name} (active)` : env.name}
              color={env.id === selectedId ? 'primary' : 'default'}
              onClick={() => setSelectedId(env.id)}
            />
          ))}
        </div>
        <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
          <TextField
            size="small"
            label="New environment"
            placeholder="Staging"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && addEnvironment()}
          />
          <Button variant="outlined" onClick={addEnvironment} disabled={!newName.trim()}>Add</Button>
        </div>
        {selected && (
          <>
            <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
              {selected.id === activeEnvironmentId
                ? <Button variant="outlined" onClick={() => onActivate(null)}>Deactivate</Button>
                : <Button variant="contained" onClick={() => onActivate(selected.id)}>Use {selected.name}</Button>}
              <Button color="error" onClick={deleteEnvironment}>Delete {selected.name}</Button>
            </div>
            {rows.map((row, index) => (
              <div key={index} style={{ display: 'flex', gap: 8, marginBottom: 8 }}>
                <TextField size="small" label="Name" value={row.key}
                  onChange={(e) => saveRows(rows.map((r, i) => (i === index ? { ...r, key: e.target.value } : r)))} />
                <TextField size="small" label="Value" value={row.value} fullWidth
                  onChange={(e) => saveRows(rows.map((r, i) => (i === index ? { ...r, value: e.target.value } : r)))} />
                <IconButton aria-label="Remove variable" onClick={() => saveRows(rows.length > 1 ? rows.filter((_, i) => i !== index) : [{ key: '', value: '' }])}>
                  <DeleteIcon />
                </IconButton>
              </div>
            ))}
            <Button startIcon={<AddIcon />} onClick={() => setRows([...rows, { key: '', value: '' }])}>Add variable</Button>
          </>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Close</Button>
      </DialogActions>
    </Dialog>
  );
};

export default EnvironmentsModal;
