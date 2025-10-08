# Components

This directory contains all reusable components used in the HITOP application.

## Main Components

### Sidebar
- **Purpose**: Navigation sidebar with collections, requests, and actions
- **Props**:
  - `collections`: array
  - `onAddCollection`: function
  - `onRenameCollection`: function
  - `onRunCollection`: function
  - `onLoadRequest`: function
  - `onDeleteRequest`: function
  - `onDragStart`: function
  - `onDragOver`: function
  - `onDrop`: function
  - `onDragEnd`: function
  - `onOpenHistory`: function
  - `onOpenExport`: function
  - `onOpenImport`: function

### RequestPanel
- **Purpose**: Main request form and response viewer
- **Props**:
  - `tabData`: object (current tab data)
  - `onUpdateTabData`: function
  - `onMakeRequest`: function
  - `onSaveRequest`: function
  - `onAddHeader`: function
  - `onUpdateHeader`: function
  - `onRemoveHeader`: function
  - `getStatusText`: function

## Modal Components

### AddCollectionModal
- **Purpose**: Add a new collection
- **Props**: 
  - `open`: boolean
  - `onClose`: function
  - `collectionName`: string
  - `onNameChange`: function
  - `onAdd`: function

### RenameCollectionModal
- **Purpose**: Rename or delete an existing collection
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `collectionName`: string
  - `onNameChange`: function
  - `onRename`: function
  - `onDelete`: function

### SaveRequestModal
- **Purpose**: Save or update a request to a collection
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `requestName`: string
  - `onNameChange`: function
  - `collections`: array
  - `selectedCollectionId`: number
  - `onCollectionChange`: function
  - `onSave`: function
  - `isOverwriting`: boolean

### DeleteRequestModal
- **Purpose**: Confirm deletion of a request
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `requestName`: string
  - `onDelete`: function

### DeleteCollectionModal
- **Purpose**: Confirm deletion of a collection
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `collection`: object
  - `onDelete`: function

### ExportModal
- **Purpose**: Export collections as JSON
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `collections`: array
  - `onCopyToClipboard`: function

### ImportModal
- **Purpose**: Import collections from JSON
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `importJson`: string
  - `onJsonChange`: function
  - `onImport`: function

### RunCollectionModal
- **Purpose**: Run all requests in a collection sequentially
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `collection`: object
  - `runResults`: array
  - `isRunning`: boolean
  - `onRun`: function

### HistoryModal
- **Purpose**: View and reload request history
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `requestHistory`: array
  - `onLoadHistoryItem`: function

## Benefits of Component Extraction

1. **Maintainability**: Each component is in its own file, making it easier to find and update
2. **Reusability**: Components can be reused across different parts of the application
3. **Testability**: Each component can be tested independently
4. **Readability**: Main App.js is cleaner and more focused on business logic
5. **Performance**: Potential for better code splitting and lazy loading
6. **Separation of Concerns**: UI components separated from business logic
7. **Easier Collaboration**: Multiple developers can work on different components simultaneously
