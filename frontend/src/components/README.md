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
  - `onOpenVariables`: function
- **Features**:
  - Branded header with HITOP logo and title
  - Each collection has a "Variables" menu item to manage collection-specific variables
  - Gradient background header for visual appeal

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
- **Features**:
  - Shows individual requests and collection runs
  - Collection runs display with folder icon and format: "Collection Name → Request Name"
  - Color-coded by success/failure status
  - Displays timestamp, method, URL, and status code

### CollectionVariablesModal
- **Purpose**: Define and manage collection-level variables
- **Props**:
  - `open`: boolean
  - `onClose`: function
  - `collection`: object
  - `onSave`: function
- **Features**:
  - Add/remove variable key-value pairs
  - Variables can be used in URLs, headers, body, and scripts using `${variableName}` syntax

## Benefits of Component Extraction

1. **Maintainability**: Each component is in its own file, making it easier to find and update
2. **Reusability**: Components can be reused across different parts of the application
3. **Testability**: Each component can be tested independently
4. **Readability**: Main App.js is cleaner and more focused on business logic
5. **Performance**: Potential for better code splitting and lazy loading
6. **Separation of Concerns**: UI components separated from business logic
7. **Easier Collaboration**: Multiple developers can work on different components simultaneously
