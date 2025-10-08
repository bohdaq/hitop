# Services

This directory contains utility services and business logic functions used throughout the HITOP application.

## Service Overview

### collectionService.js
Manages collection operations including creation, modification, import/export, and request management.

### contextService.js
Manages collection-specific runtime contexts for sharing data between requests.

### historyService.js
Handles request history tracking, creation, and filtering.

### httpService.js
Handles HTTP request execution, response processing, and status code utilities.

### scriptExecutionService.js
Executes pre-request and post-request scripts in a sandboxed environment.

### storageService.js
Manages localStorage operations for persisting collections, tabs, history, and contexts.

### tabService.js
Manages tab creation, updates, and lifecycle operations.

### variableInterpolation.js
Provides variable interpolation functionality for dynamic request values.

---

## Variable Interpolation Service

**File**: `variableInterpolation.js`

Provides functionality to interpolate variables in strings using `${variable}` syntax.

### Functions

#### `interpolateVariables(template, variables)`

Interpolates variables in a string template. **Throws an error** if any variable is not found.

**Parameters:**
- `template` (string): String containing `${variable}` placeholders
- `variables` (object): Object containing variable values

**Returns:** Interpolated string

**Throws:** Error if variable not found

**Example:**
```javascript
import { interpolateVariables } from './services/variableInterpolation';

const result = interpolateVariables(
  'Hello ${name}, you are ${age} years old',
  { name: 'John', age: 30 }
);
// Returns: 'Hello John, you are 30 years old'

// Throws error for missing variable
interpolateVariables('Hello ${name}!', {});
// Error: Variable 'name' not found in variables object
```

#### `safeInterpolateVariables(template, variables, defaultValue)`

Safely interpolates variables, using a default value for missing variables instead of throwing an error.

**Parameters:**
- `template` (string): String containing `${variable}` placeholders
- `variables` (object): Object containing variable values
- `defaultValue` (string, optional): Default value for missing variables (default: '')

**Returns:** Interpolated string

**Example:**
```javascript
import { safeInterpolateVariables } from './services/variableInterpolation';

const result = safeInterpolateVariables(
  'Hello ${name}!',
  {},
  'Unknown'
);
// Returns: 'Hello Unknown!'
```

#### `extractVariableNames(template)`

Extracts all unique variable names from a template string.

**Parameters:**
- `template` (string): String containing `${variable}` placeholders

**Returns:** Array of unique variable names

**Example:**
```javascript
import { extractVariableNames } from './services/variableInterpolation';

const vars = extractVariableNames('Hello ${name}, you are ${age} years old');
// Returns: ['name', 'age']
```

#### `validateVariables(template, variables)`

Validates if all required variables are present in the variables object.

**Parameters:**
- `template` (string): String containing `${variable}` placeholders
- `variables` (object): Object containing variable values

**Returns:** Object with `{ valid: boolean, missing: string[] }`

**Example:**
```javascript
import { validateVariables } from './services/variableInterpolation';

const result = validateVariables('${greeting} ${name}', { greeting: 'Hi' });
// Returns: { valid: false, missing: ['name'] }
```

### Use Cases

1. **Dynamic URL Generation**
   ```javascript
   const url = interpolateVariables(
     'https://api.example.com/users/${userId}/posts/${postId}',
     { userId: 123, postId: 456 }
   );
   // 'https://api.example.com/users/123/posts/456'
   ```

2. **Request Body Templates**
   ```javascript
   const body = interpolateVariables(
     '{"username": "${username}", "token": "${authToken}"}',
     { username: 'john', authToken: 'abc123' }
   );
   ```

3. **Header Value Templates**
   ```javascript
   const authHeader = interpolateVariables(
     'Bearer ${token}',
     { token: getContext('authToken') }
   );
   ```

4. **Pre-Request Script Variables**
   ```javascript
   // In pre-request script
   const userId = getContext('userId');
   const newUrl = interpolateVariables(
     'https://api.example.com/users/${id}',
     { id: userId }
   );
   setUrl(newUrl);
   ```

### Testing

Run tests with:
```bash
npm test variableInterpolation.test.js
```

### Error Handling

The service provides clear error messages:
- Single missing variable: `Variable 'name' not found in variables object`
- Multiple missing variables: `Variables 'name', 'age' not found in variables object`
- Invalid template type: `Template must be a string`
- Invalid variables type: `Variables must be an object`
