# Collection Variables Guide

## Overview

Collection Variables allow you to define reusable values that can be used across all requests within a collection. Variables are interpolated using the `${variableName}` syntax.

## Features

- **Collection-Specific**: Each collection has its own set of variables
- **Automatic Interpolation**: Variables are automatically replaced in URLs, headers, and request bodies
- **Script Access**: Variables are accessible in both pre-request and post-request scripts
- **Error Handling**: Clear error messages if a variable is not found

## How to Use

### 1. Define Variables

1. In the sidebar, expand a collection
2. Click on the **"Variables"** menu item (first item under collection name)
3. In the modal, add variable key-value pairs:
   - **Variable Name**: The name you'll use to reference the variable (e.g., `apiUrl`, `authToken`)
   - **Value**: The actual value (e.g., `https://api.example.com`, `Bearer abc123`)
4. Click **"Save Variables"**

### 2. Use Variables in Requests

Variables can be used in:
- **URLs**
- **Header names and values**
- **Request body**
- **Pre-request scripts**
- **Post-request scripts**

#### Syntax

Use `${variableName}` to reference a variable:

```
${apiUrl}/users/${userId}
```

### 3. Examples

#### Example 1: API Base URL

**Variables:**
```
apiUrl = https://api.example.com
version = v1
```

**URL:**
```
${apiUrl}/${version}/users
```

**Result:**
```
https://api.example.com/v1/users
```

#### Example 2: Authentication

**Variables:**
```
authToken = Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Header:**
```
Name: Authorization
Value: ${authToken}
```

**Result:**
```
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

#### Example 3: Dynamic Request Body

**Variables:**
```
username = john_doe
email = john@example.com
```

**Request Body:**
```json
{
  "username": "${username}",
  "email": "${email}",
  "role": "user"
}
```

**Result:**
```json
{
  "username": "john_doe",
  "email": "john@example.com",
  "role": "user"
}
```

#### Example 4: Multiple Variables in URL

**Variables:**
```
apiUrl = https://api.example.com
userId = 123
postId = 456
```

**URL:**
```
${apiUrl}/users/${userId}/posts/${postId}
```

**Result:**
```
https://api.example.com/users/123/posts/456
```

## Using Variables in Scripts

Variables are accessible in both pre-request and post-request scripts.

### Pre-Request Script

Variables are available through the `variables` object and `getVariable()` function:

```javascript
// Access variables
const baseUrl = getVariable('apiUrl');
const token = getVariable('authToken');

// Use in script logic
if (token) {
  setHeader('Authorization', token);
}

// Modify URL using variable
const userId = getContext('userId');
setUrl(`${baseUrl}/users/${userId}`);
```

### Post-Request Script

Variables are also available in post-request scripts:

```javascript
// Access variable
const expectedStatus = getVariable('expectedStatusCode');

// Validate response
if (statusCode !== parseInt(expectedStatus)) {
  console.error('Unexpected status code');
}
```

## Variable Interpolation Order

Variables are processed in the following order:

1. **Variable Interpolation**: Collection variables are replaced in URL, headers, and body
2. **Pre-Request Script**: Script executes with interpolated values and can further modify them
3. **Request Execution**: Final values are sent in the HTTP request
4. **Post-Request Script**: Script executes with response data and has access to variables

## Variables in Collection Runs

When you run an entire collection (click on collection name to run all requests), variables are automatically available to all requests:

- **Automatic Interpolation**: Variables are interpolated in each request's URL, headers, and body
- **Script Access**: Both pre-request and post-request scripts have access to variables via `getVariable()`
- **Shared Context**: Context is shared across all requests in the run, allowing data flow between requests
- **Error Handling**: If a variable is missing, the collection run stops and shows which request failed

### Example Collection Run

**Collection Variables:**
```
apiUrl = https://api.example.com
```

**Request 1: Login**
- URL: `${apiUrl}/auth/login`
- Post-Request Script: `setContext('token', getResponseValue('token'))`

**Request 2: Get Profile**
- URL: `${apiUrl}/users/me`
- Pre-Request Script: `setHeader('Authorization', 'Bearer ' + getContext('token'))`

**Request 3: Update Profile**
- URL: `${apiUrl}/users/me`
- Pre-Request Script: Uses both variable and context
  ```javascript
  const token = getContext('token');
  const baseUrl = getVariable('apiUrl');
  setHeader('Authorization', 'Bearer ' + token);
  setUrl(baseUrl + '/users/me');
  ```

All three requests automatically use the `apiUrl` variable, and context flows from one request to the next!

## Error Handling

If a variable is referenced but not defined, you'll see an error:

```
Variable interpolation error: Variable 'apiUrl' not found in variables object
```

**To fix:**
1. Open the Variables modal for the collection
2. Add the missing variable
3. Save and retry the request

## Best Practices

### 1. Naming Conventions

Use clear, descriptive names:
- ✅ `apiUrl`, `authToken`, `userId`
- ❌ `a`, `x`, `temp`

### 2. Environment-Specific Variables

Create different collections for different environments:
- **Development Collection**: `apiUrl = https://dev-api.example.com`
- **Production Collection**: `apiUrl = https://api.example.com`

### 3. Sensitive Data

Be cautious with sensitive data in variables:
- Variables are stored in browser localStorage
- Don't commit exported collections with sensitive tokens to version control
- Consider using context (runtime) for truly sensitive data

### 4. Combine with Context

Use variables for static configuration and context for dynamic runtime data:

**Variables** (static):
```
apiUrl = https://api.example.com
```

**Context** (dynamic, set by scripts):
```javascript
// In post-request script of login
const token = getResponseValue('token');
setContext('authToken', token);

// In pre-request script of other requests
const token = getContext('authToken');
setHeader('Authorization', `Bearer ${token}`);
```

## Common Use Cases

### 1. Multi-Environment Testing

Define different base URLs for dev, staging, and production:

```
apiUrl = https://dev-api.example.com
apiKey = dev_key_123
```

### 2. API Versioning

```
apiUrl = https://api.example.com
apiVersion = v2
```

URL: `${apiUrl}/${apiVersion}/endpoint`

### 3. Shared Authentication

```
authToken = Bearer abc123
apiKey = xyz789
```

### 4. User IDs and Resource IDs

```
userId = 12345
organizationId = org_67890
```

### 5. Pagination Parameters

```
pageSize = 20
sortOrder = desc
```

## Troubleshooting

### Variable Not Replaced

**Problem**: `${apiUrl}` appears literally in the request

**Solutions**:
1. Check that the variable is defined in the collection
2. Verify the variable name matches exactly (case-sensitive)
3. Ensure the request is loaded from the collection (check if collection name appears in tab)

### Error: Variable Not Found

**Problem**: Error message about missing variable

**Solutions**:
1. Open Variables modal and add the missing variable
2. Check for typos in variable name
3. Verify you're using the correct collection

### Variables Not Working in Scripts

**Problem**: `getVariable()` returns undefined

**Solutions**:
1. Use `getVariable('name')` not `variables.name`
2. Ensure the request is associated with a collection
3. Check that variables are saved in the collection

## Technical Details

- Variables are stored per collection in the collections array
- Interpolation uses the `interpolateVariables` service
- Variables are processed before pre-request scripts
- Scripts receive both interpolated values and original variables object
- Variable syntax: `${variableName}` (standard JavaScript template literal syntax)
