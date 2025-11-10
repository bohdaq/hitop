import React, { useState } from 'react';
import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import Button from '@mui/material/Button';
import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import IconButton from '@mui/material/IconButton';
import DeleteIcon from '@mui/icons-material/Delete';
import AddIcon from '@mui/icons-material/Add';
import TextField from '@mui/material/TextField';
import Table from '@mui/material/Table';
import TableBody from '@mui/material/TableBody';
import TableCell from '@mui/material/TableCell';
import TableContainer from '@mui/material/TableContainer';
import TableHead from '@mui/material/TableHead';
import TableRow from '@mui/material/TableRow';
import SettingsIcon from '@mui/icons-material/Settings';

const VariablesView = ({ 
  collection, 
  onSaveVariables
}) => {
  const [variables, setVariables] = useState(collection.variables || {});
  const [newVarKey, setNewVarKey] = useState('');
  const [newVarValue, setNewVarValue] = useState('');

  const handleAddVariable = () => {
    if (newVarKey.trim()) {
      const updatedVariables = { ...variables, [newVarKey.trim()]: newVarValue };
      setVariables(updatedVariables);
      onSaveVariables(updatedVariables);
      setNewVarKey('');
      setNewVarValue('');
    }
  };

  const handleUpdateVariable = (key, value) => {
    const updatedVariables = { ...variables, [key]: value };
    setVariables(updatedVariables);
    onSaveVariables(updatedVariables);
  };

  const handleDeleteVariable = (key) => {
    const updatedVariables = { ...variables };
    delete updatedVariables[key];
    setVariables(updatedVariables);
    onSaveVariables(updatedVariables);
  };

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
      <Box sx={{ display: 'flex', alignItems: 'center', marginBottom: 3 }}>
        <SettingsIcon sx={{ fontSize: 40, color: 'primary.main', marginRight: 2 }} />
        <Box>
          <Typography variant="h4">
            Variables
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {collection.name}
          </Typography>
        </Box>
      </Box>

      <Card>
        <CardContent>
          {Object.keys(variables).length > 0 ? (
            <TableContainer>
              <Table>
                <TableHead>
                  <TableRow>
                    <TableCell><strong>Key</strong></TableCell>
                    <TableCell><strong>Value</strong></TableCell>
                    <TableCell width={60}></TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {Object.entries(variables).map(([key, value]) => (
                    <TableRow key={key}>
                      <TableCell>{key}</TableCell>
                      <TableCell>
                        <TextField
                          fullWidth
                          size="small"
                          value={value}
                          onChange={(e) => handleUpdateVariable(key, e.target.value)}
                        />
                      </TableCell>
                      <TableCell>
                        <IconButton
                          size="small"
                          onClick={() => handleDeleteVariable(key)}
                          aria-label="delete variable"
                        >
                          <DeleteIcon fontSize="small" />
                        </IconButton>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          ) : (
            <Typography variant="body2" color="text.secondary" sx={{ marginBottom: 2 }}>
              No variables defined
            </Typography>
          )}
          
          <Box sx={{ display: 'flex', gap: 1, marginTop: 3 }}>
            <TextField
              size="small"
              label="Key"
              value={newVarKey}
              onChange={(e) => setNewVarKey(e.target.value)}
              onKeyPress={(e) => {
                if (e.key === 'Enter') {
                  handleAddVariable();
                }
              }}
              sx={{ width: '200px' }}
            />
            <TextField
              size="small"
              label="Value"
              value={newVarValue}
              onChange={(e) => setNewVarValue(e.target.value)}
              onKeyPress={(e) => {
                if (e.key === 'Enter') {
                  handleAddVariable();
                }
              }}
              sx={{ flexGrow: 1 }}
            />
            <Button
              variant="contained"
              startIcon={<AddIcon />}
              onClick={handleAddVariable}
              disabled={!newVarKey.trim()}
            >
              Add Variable
            </Button>
          </Box>
        </CardContent>
      </Card>
    </Box>
  );
};

export default VariablesView;
