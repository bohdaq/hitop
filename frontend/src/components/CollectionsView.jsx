import React from 'react';
import Card from '@mui/material/Card';
import CardContent from '@mui/material/CardContent';
import CardActions from '@mui/material/CardActions';
import Typography from '@mui/material/Typography';
import Button from '@mui/material/Button';
import IconButton from '@mui/material/IconButton';
import EditIcon from '@mui/icons-material/Edit';
import VisibilityIcon from '@mui/icons-material/Visibility';
import FolderIcon from '@mui/icons-material/Folder';
import AddIcon from '@mui/icons-material/Add';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import CircularProgress from '@mui/material/CircularProgress';
import LinearProgress from '@mui/material/LinearProgress';
import Alert from '@mui/material/Alert';
import AlertTitle from '@mui/material/AlertTitle';
import CheckCircleIcon from '@mui/icons-material/CheckCircle';
import ErrorIcon from '@mui/icons-material/Error';
import Box from '@mui/material/Box';

const CollectionsView = ({
    collections,
    onRenameCollection,
    onViewCollection,
    onAddCollection,
    onRunAll,
    isRunningAll,
    runningCollectionId,
    completedCollections,
    runAllResults,
    onCollectionDragStart,
    onCollectionDragOver,
    onCollectionDrop,
    onCollectionDragEnd
}) => {
    return (
        <Box sx={{ padding: 3 }}>
            <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 3 }}>
                <Typography variant="h4">
                    Collections
                </Typography>
                <Box sx={{ display: 'flex', gap: 2 }}>
                    <Button
                        variant="outlined"
                        startIcon={isRunningAll ? <CircularProgress size={20} /> : <PlayArrowIcon />}
                        onClick={onRunAll}
                        disabled={collections.length === 0 || isRunningAll}
                    >
                        {isRunningAll ? 'Running...' : 'Run All'}
                    </Button>
                    <Button
                        variant="contained"
                        startIcon={<AddIcon />}
                        onClick={onAddCollection}
                    >
                        Add New Collection
                    </Button>
                </Box>
            </Box>
            {isRunningAll && (
                <Box sx={{ marginBottom: 3 }}>
                    <Box sx={{ display: 'flex', justifyContent: 'space-between', marginBottom: 1 }}>
                        <Typography variant="body2" color="text.secondary">
                            Running collections: {completedCollections} / {collections.filter(c => c.requests?.length > 0).length}
                        </Typography>
                    </Box>
                    <LinearProgress 
                        variant="determinate" 
                        value={(completedCollections / collections.filter(c => c.requests?.length > 0).length) * 100}
                    />
                </Box>
            )}
            {runAllResults && (
                <Alert 
                    severity={runAllResults.failedCount === 0 ? "success" : "warning"}
                    sx={{ marginBottom: 3 }}
                    icon={runAllResults.failedCount === 0 ? <CheckCircleIcon /> : <ErrorIcon />}
                >
                    <AlertTitle>
                        {runAllResults.failedCount === 0 ? 'All Requests Completed Successfully' : 'Run Completed with Errors'}
                    </AlertTitle>
                    <Box sx={{ display: 'flex', gap: 3, marginTop: 1 }}>
                        <Typography variant="body2">
                            <strong>Total Collections:</strong> {runAllResults.totalCollections}
                        </Typography>
                        <Typography variant="body2">
                            <strong>Total Requests:</strong> {runAllResults.totalRequests}
                        </Typography>
                        <Typography variant="body2" color="success.main">
                            <strong>Successful:</strong> {runAllResults.successCount}
                        </Typography>
                        {runAllResults.failedCount > 0 && (
                            <Typography variant="body2" color="error.main">
                                <strong>Failed:</strong> {runAllResults.failedCount}
                            </Typography>
                        )}
                        <Typography variant="body2">
                            <strong>Duration:</strong> {runAllResults.duration}ms
                        </Typography>
                    </Box>
                </Alert>
            )}
            <Box sx={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))',
                gap: 3
            }}>
                {collections.map((collection, index) => {
                    const isRunning = runningCollectionId === collection.id;
                    return (
                    <Card
                        key={collection.id}
                        draggable
                        onDragStart={(e) => onCollectionDragStart(e, collection, index)}
                        onDragOver={onCollectionDragOver}
                        onDrop={(e) => onCollectionDrop(e, index)}
                        onDragEnd={onCollectionDragEnd}
                        sx={{
                            display: 'flex',
                            flexDirection: 'column',
                            transition: 'transform 0.2s, box-shadow 0.2s',
                            border: isRunning ? '2px solid' : '1px solid',
                            borderColor: isRunning ? 'primary.main' : 'divider',
                            backgroundColor: isRunning ? 'action.selected' : 'background.paper',
                            cursor: 'grab',
                            '&:active': {
                                cursor: 'grabbing'
                            },
                            '&:hover': {
                                transform: 'translateY(-4px)',
                                boxShadow: 4
                            }
                        }}
                    >
                        <CardContent sx={{ flexGrow: 1 }}>
                            <Box sx={{ display: 'flex', alignItems: 'center', marginBottom: 2 }}>
                                {isRunning && <CircularProgress size={20} sx={{ marginRight: 1 }} />}
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
                                startIcon={<VisibilityIcon />}
                                onClick={() => onViewCollection(collection)}
                            >
                                View
                            </Button>
                        </CardActions>
                    </Card>
                    );
                })}
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
