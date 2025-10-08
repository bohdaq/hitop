# Modal Components

This directory contains all modal dialog components used in the HITOP application.

## Components

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

1. **Maintainability**: Each modal is in its own file, making it easier to find and update
2. **Reusability**: Components can be reused across different parts of the application
3. **Testability**: Each component can be tested independently
4. **Readability**: Main App.js is cleaner and more focused on business logic
5. **Performance**: Potential for better code splitting and lazy loading
