import React from 'react';
import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import CardActions from '@mui/material/CardActions';
import Typography from '@mui/material/Typography';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import EditIcon from '@mui/icons-material/Edit';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import FolderIcon from '@mui/icons-material/Folder';
import Box from '@mui/material/Box';

const CollectionsView = ({
    collections,
    onRenameCollection,
    onRunCollection
}) => {
    return (
        <Box sx={{ padding: 3 }}>
            <Typography variant="h4" gutterBottom sx={{ marginBottom: 3 }}>
                Collections
            </Typography>
            <Box sx={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))',
                gap: 3
            }}>
                {collections.map((collection) => (
                    <Card
                        key={collection.id}
                        sx={{
                            display: 'flex',
                            flexDirection: 'column',
                            transition: 'transform 0.2s, box-shadow 0.2s',
                            '&:hover': {
                                transform: 'translateY(-4px)',
                                boxShadow: 4
                            }
                        }}
                    >
                        <CardContent sx={{ flexGrow: 1 }}>
                            <Box sx={{ display: 'flex', alignItems: 'center', marginBottom: 2 }}>
                                <FolderIcon sx={{ marginRight: 1, color: 'primary.main' }} />
                                <Typography variant="h6" component="div">
                                    {collection.name}
                                </Typography>
                            </Box>
                            <Typography variant="body2" color="text.secondary">
                                {collection.requests?.length || 0} request{collection.requests?.length !== 1 ? 's' : ''}
                            </Typography>
                            {collection.variables && Object.keys(collection.variables).length > 0 && (
                                <Typography variant="body2" color="text.secondary" sx={{ marginTop: 1 }}>
                                    {Object.keys(collection.variables).length} variable{Object.keys(collection.variables).length !== 1 ? 's' : ''}
                                </Typography>
                            )}
                        </CardContent>
                        <CardActions sx={{ justifyContent: 'space-between', padding: 2 }}>
                            <Button
                                size="small"
                                startIcon={<EditIcon />}
                                onClick={(e) => onRenameCollection(e, collection.id, collection.name)}
                            >
                                Rename
                            </Button>
                            <Button
                                size="small"
                                variant="contained"
                                startIcon={<PlayArrowIcon />}
                                onClick={(e) => onRunCollection(e, collection)}
                                disabled={!collection.requests || collection.requests.length === 0}
                            >
                                Run
                            </Button>
                        </CardActions>
                    </Card>
                ))}
            </Box>
            {collections.length === 0 && (
                <Box sx={{
                    textAlign: 'center',
                    padding: 4,
                    color: 'text.secondary'
                }}>
                    <Typography variant="h6">
                        No collections yet
                    </Typography>
                    <Typography variant="body2" sx={{ marginTop: 1 }}>
                        Click the + button in the sidebar to create your first collection
                    </Typography>
                </Box>
            )}
        </Box>
    );
};

export default CollectionsView;
