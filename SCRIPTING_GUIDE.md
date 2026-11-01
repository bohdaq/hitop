# Custom Scripting Guide

## Overview

HITOP supports custom JavaScript code execution before and after HTTP requests through **Pre-Request Scripts** and **Post-Request Scripts**. These scripts enable dynamic request modification, data extraction, and workflow automation.

## Pre-Request Scripts

Pre-request scripts run **before** the HTTP request is sent. They can modify the request URL, headers, and body dynamically.

### Available Variables

- `url` - Current request URL (after variable interpolation)
- `method` - HTTP method (GET, POST, etc.)
- `headers` - Array of header objects `[{ name, value }]`
- `body` - Request body string
- `variables` - Collection variables object
- `context` - Runtime context object (shared across requests)

### Available Functions

#### Request Modification
- `setUrl(url)` - Change the request URL
- `setHeader(name, value)` - Add or update a header
- `setBody(body)` - Change the request body

#### Data Access
- `getVariable(key)` - Get a collection variable value
- `getContext(key)` - Get a runtime context value
- `setContext(key, value)` - Store data for use in subsequent requests

### Examples

#### Example 1: Use Collection Variables
```javascript
// Get base URL from collection variables
const baseUrl = getVariable('apiUrl');
const version = getVariable('apiVersion');

// Build dynamic URL
setUrl(`${baseUrl}/${version}/users`);
```

#### Example 2: Add Dynamic Authentication
```javascript
// Get token from previous request's context
const token = getContext('authToken');

if (token) {
  setHeader('Authorization', `Bearer ${token}`);
} else {
  console.error('No auth token found!');
}
```

#### Example 3: Dynamic URL with Context
```javascript
// Use stored user ID from previous request
const userId = getContext('userId');
const baseUrl = getVariable('apiUrl');

if (userId) {
  setUrl(`${baseUrl}/users/${userId}/profile`);
} else {
  // Fallback to default user
  const defaultUserId = getVariable('defaultUserId');
  setUrl(`${baseUrl}/users/${defaultUserId}/profile`);
}
```

#### Example 4: Add Timestamp Header
```javascript
// Add current timestamp to request
const timestamp = Date.now().toString();
setHeader('X-Request-Timestamp', timestamp);
setHeader('X-Request-ID', `req_${timestamp}`);
```

#### Example 5: Modify Request Body
```javascript
// Build dynamic request body
const username = getVariable('username');
const timestamp = Date.now();

const requestBody = JSON.stringify({
  username: username,
  timestamp: timestamp,
  client: 'hitop'
});

setBody(requestBody);
```

#### Example 6: Conditional Logic
```javascript
// Different behavior based on environment
const env = getVariable('environment');
const baseUrl = getVariable('apiUrl');

if (env === 'production') {
  setUrl(`${baseUrl}/api/v2/users`);
  setHeader('X-Environment', 'production');
} else {
  setUrl(`${baseUrl}/api/v1/users`);
  setHeader('X-Environment', 'development');
}
```

## Post-Request Scripts

Post-request scripts run **after** the HTTP response is received. They can extract data from responses and store it for use in subsequent requests.

### Available Variables

- `response` - Parsed JSON response (or `responseText` if not JSON)
- `responseText` - Raw response text
- `responseHeaders` - Response headers object
- `statusCode` - HTTP status code
- `variables` - Collection variables object
- `context` - Runtime context object

### Available Functions

#### Data Storage
- `setContext(key, value)` - Store data for use in subsequent requests
- `getContext(key)` - Get a runtime context value
- `getVariable(key)` - Get a collection variable value

#### Response Parsing
- `getResponseValue('path.to.value')` - Extract value from JSON response using dot notation
- `getResponseHeader('header-name')` - Get specific response header value

### Examples

#### Example 1: Extract and Store Auth Token
```javascript
// Extract token from response
const token = getResponseValue('data.token');

if (token) {
  setContext('authToken', token);
  console.log('Auth token stored successfully');
} else {
  console.error('No token found in response');
}
```

#### Example 2: Store User ID
```javascript
// Extract user ID from response
const userId = response.user.id;
const username = response.user.username;

// Store for use in next request
setContext('userId', userId);
setContext('username', username);
```

#### Example 3: Extract Nested Data
```javascript
// Extract from deeply nested response
const accessToken = getResponseValue('auth.tokens.access');
const refreshToken = getResponseValue('auth.tokens.refresh');
const expiresIn = getResponseValue('auth.tokens.expiresIn');

// Store all tokens
setContext('accessToken', accessToken);
setContext('refreshToken', refreshToken);
setContext('tokenExpiry', Date.now() + (expiresIn * 1000));
```

#### Example 4: Validate Response
```javascript
// Validate response status
const expectedStatus = getVariable('expectedStatusCode');

if (statusCode !== parseInt(expectedStatus)) {
  console.error(`Expected ${expectedStatus}, got ${statusCode}`);
}

// Validate response structure
if (!response.data || !response.data.id) {
  console.error('Invalid response structure');
}
```

#### Example 5: Extract from Response Headers
```javascript
// Get pagination info from headers
const totalPages = getResponseHeader('x-total-pages');
const currentPage = getResponseHeader('x-current-page');

if (totalPages && currentPage) {
  setContext('totalPages', totalPages);
  setContext('currentPage', currentPage);
  
  // Calculate next page
  const nextPage = parseInt(currentPage) + 1;
  if (nextPage <= parseInt(totalPages)) {
    setContext('nextPage', nextPage);
  }
}
```

#### Example 6: Process Array Response
```javascript
// Extract IDs from array response
if (Array.isArray(response.data)) {
  const ids = response.data.map(item => item.id);
  
  // Store first ID for next request
  if (ids.length > 0) {
    setContext('firstItemId', ids[0]);
    setContext('itemCount', ids.length);
  }
}
```

## Variables in Collection Runs

When running an entire collection, variables are automatically available to all requests:

- Variables are interpolated in URLs, headers, and body for each request
- Scripts have access to variables via `getVariable()` and `variables` object
- Context is shared across all requests in the run
- If a variable is missing, the run stops with a clear error message

**Example:**
```javascript
// Collection Variables: apiUrl = https://api.example.com

// Request 1 Pre-Request Script
const baseUrl = getVariable('apiUrl');
setUrl(baseUrl + '/auth/login');

// Request 2 Pre-Request Script
const baseUrl = getVariable('apiUrl');
const token = getContext('authToken'); // From Request 1
setUrl(baseUrl + '/users/me');
setHeader('Authorization', 'Bearer ' + token);
```

## Complete Workflow Example

### Request 1: Login (POST /auth/login)

**Pre-Request Script:**
```javascript
// Use credentials from collection variables
const username = getVariable('username');
const password = getVariable('password');

const loginBody = JSON.stringify({
  username: username,
  password: password
});

setBody(loginBody);
```

**Post-Request Script:**
```javascript
// Extract and store auth token
const token = getResponseValue('token');
const userId = getResponseValue('user.id');

if (token) {
  setContext('authToken', token);
  setContext('userId', userId);
  console.log('Login successful, token stored');
} else {
  console.error('Login failed: No token in response');
}
```

### Request 2: Get User Profile (GET /users/{id})

**Pre-Request Script:**
```javascript
// Use stored token and user ID
const token = getContext('authToken');
const userId = getContext('userId');
const baseUrl = getVariable('apiUrl');

if (!token) {
  console.error('No auth token! Please login first.');
}

// Set auth header
setHeader('Authorization', `Bearer ${token}`);

// Build URL with user ID
setUrl(`${baseUrl}/users/${userId}`);
```

**Post-Request Script:**
```javascript
// Store user profile data
const email = getResponseValue('email');
const name = getResponseValue('name');

setContext('userEmail', email);
setContext('userName', name);

console.log(`Profile loaded for ${name}`);
```

### Request 3: Update User (PUT /users/{id})

**Pre-Request Script:**
```javascript
// Use stored data
const token = getContext('authToken');
const userId = getContext('userId');
const baseUrl = getVariable('apiUrl');

// Set auth header
setHeader('Authorization', `Bearer ${token}`);

// Build URL
setUrl(`${baseUrl}/users/${userId}`);

// Build update body
const updateData = JSON.stringify({
  name: 'Updated Name',
  updatedAt: new Date().toISOString()
});

setBody(updateData);
```

## Best Practices

### 1. Error Handling
```javascript
// Always check if data exists
const token = getContext('authToken');
if (!token) {
  console.error('Missing auth token');
  // Could set a default or skip request
}
```

### 2. Use Variables for Configuration
```javascript
// Store configuration in collection variables
const apiUrl = getVariable('apiUrl');
const timeout = getVariable('timeout');
const retryCount = getVariable('retryCount');
```

### 3. Clear Context When Needed
```javascript
// Clear old data in post-request script
if (statusCode === 401) {
  // Clear auth data on unauthorized
  setContext('authToken', null);
  setContext('userId', null);
}
```

### 4. Log Important Information
```javascript
// Use console.log for debugging
console.log('Request URL:', url);
console.log('Auth token:', getContext('authToken') ? 'Present' : 'Missing');
console.log('Response status:', statusCode);
```

### 5. Combine Variables and Context
```javascript
// Variables for static config, context for dynamic data
const baseUrl = getVariable('apiUrl');  // Static
const userId = getContext('userId');    // Dynamic from previous request

setUrl(`${baseUrl}/users/${userId}`);
```

## Common Patterns

### Pattern 1: Token Refresh Flow
```javascript
// Pre-request: Check token expiry
const tokenExpiry = getContext('tokenExpiry');
const now = Date.now();

if (!tokenExpiry || now >= tokenExpiry) {
  // Token expired, use refresh token
  const refreshToken = getContext('refreshToken');
  // Trigger refresh logic
}
```

### Pattern 2: Pagination
```javascript
// Post-request: Store pagination info
const nextPage = getResponseValue('pagination.nextPage');
const hasMore = getResponseValue('pagination.hasMore');

if (hasMore) {
  setContext('nextPage', nextPage);
} else {
  setContext('nextPage', null);
}
```

### Pattern 3: Retry Logic
```javascript
// Post-request: Track retries
if (statusCode >= 500) {
  const retries = getContext('retryCount') || 0;
  const maxRetries = getVariable('maxRetries') || 3;
  
  if (retries < maxRetries) {
    setContext('retryCount', retries + 1);
    console.log(`Retry ${retries + 1}/${maxRetries}`);
  }
}
```

## Limitations

- Scripts run in a sandboxed environment
- No access to file system or external modules
- No async/await support (scripts are synchronous)
- Limited to JavaScript ES5/ES6 features
- Cannot make additional HTTP requests from scripts

## Troubleshooting

### Script Not Running
- Check for syntax errors in the script
- Ensure the request is associated with a collection
- Check browser console for error messages

### Variables Not Available
- Ensure variables are defined in collection
- Verify request is loaded from collection
- Use `getVariable('name')` not `variables.name`

### Context Not Persisting
- Context is collection-specific
- Context is stored in memory (cleared on page refresh)
- Use `setContext()` to store, `getContext()` to retrieve

## Postman scripts (`pm.*`)

Scripts from imported Postman collections run as they are: CLI, desktop and web app load
the same shim (`shared/pm-shim.js`) whenever a script mentions `pm`, `postman`, `tests` or
another Postman name. Other scripts run exactly as before.

| Postman | In hitop |
| --- | --- |
| `pm.environment`, `pm.collectionVariables`, `pm.variables`: `get`, `set`, `has`, `unset`, `toObject`, `replaceIn` | One set of variables (collection, environment, run). A value set by a script is used by later requests in the run, and by the same request when a pre-request script sets it. The CLI never writes it back to files; the web app keeps it until the page reloads. |
| `pm.globals`: the same methods | The globals scope, below collection variables and the environment (`hitop globals`, a folder's `globals.yaml`, the Globals section in the app). `pm.globals.get` reads only globals; a global a script sets lasts for the run like other script values, and `${name}` and `pm.variables` see it unless another scope has that name. |
| `pm.response`: `code`, `status`, `responseTime`, `headers.get/has/toObject`, `json()`, `text()`, `to.have.status/header/body/jsonBody`, `to.be.ok/success/error/clientError/serverError/notFound/json` | Same |
| `pm.request`: `url`, `method`, `headers.get/has/add/upsert/remove`, `body.raw/update` | Pre-request scripts can change them |
| `pm.test(name, fn)`, `pm.expect(value)` | A test result per `pm.test`; failures carry chai's message. `expect` supports `to/be/been/is/that/which/and/has/have/with`, `not`, `deep`, `equal/eql`, `above/below/least/most/within/closeTo`, `a/an`, `include/contain`, `property`, `lengthOf`, `match`, `oneOf`, `keys`, `members`, `satisfy`, `ok/true/false/null/undefined/exist/empty/NaN`. An `expect` outside `pm.test` is a test result of its own. |
| `tests['name'] = bool`, `responseBody`, `responseCode`, `responseTime`, `environment`, `globals`, `postman.set/get/clearEnvironmentVariable`, `postman.set/get/clearGlobalVariable` | Postman's old sandbox API, same behaviour (the `*GlobalVariable` calls and `globals` use the globals scope; `globals` and `environment` are the values as the script started) |

Not there (listed by `hitop import` and the web app's import dialog): `pm.sendRequest`,
`pm.cookies`, `pm.info`, `pm.iterationData`, `pm.execution`, `postman.setNextRequest`,
`pm.visualizer`, `require(...)` and the sandbox libraries (lodash, CryptoJS, cheerio).

## Security Considerations

- Scripts have access to all request/response data
- Be cautious with sensitive data in scripts
- Scripts are stored in browser localStorage
- Don't log sensitive information to console
