# HITOP - HTTP API Testing Tool

A powerful, modern HTTP API testing tool built with React. Test APIs, manage collections, automate workflows with custom scripts, and use variables for dynamic requests.

**Available as:**
- 🌐 Web Application
- 🦊 Firefox Extension ([Installation Guide](./EXTENSION_GUIDE.md))

## Features

### 🚀 Core Features
- **Multiple HTTP Methods**: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS
- **Request Management**: Headers, body, and URL configuration
- **Response Viewer**: Syntax-highlighted JSON, XML, HTML, and text responses
- **Status Code Display**: Clear status codes with descriptions
- **Response Headers**: View all response headers

### 📁 Collections
- **Organize Requests**: Group related requests into collections
- **Save & Load**: Save requests to collections for reuse
- **Run Collections**: Execute all requests in a collection sequentially
- **Drag & Drop**: Reorder requests within collections
- **Import/Export**: Share collections as JSON

### 🔧 Variables
- **Collection Variables**: Define reusable variables per collection
- **Dynamic Interpolation**: Use `${variableName}` syntax in URLs, headers, and body
- **Variable Management**: Easy-to-use modal for managing variables
- See [VARIABLES_GUIDE.md](./VARIABLES_GUIDE.md) for detailed documentation

### 📝 Custom Scripting
- **Pre-Request Scripts**: Modify requests before they're sent
- **Post-Request Scripts**: Extract data from responses
- **Context Sharing**: Pass data between requests
- **Variable Access**: Use collection variables in scripts
- See [SCRIPTING_GUIDE.md](./SCRIPTING_GUIDE.md) for detailed documentation

### 📊 Additional Features
- **Multiple Tabs**: Work on multiple requests simultaneously
- **Request History**: Track and reload recent requests
- **Syntax Highlighting**: Beautiful code highlighting for responses
- **Auto-Save**: Collections saved to browser localStorage

## Getting Started

### Web Application

#### Prerequisites
- Node.js (v14 or higher)
- npm or yarn

#### Installation

1. Clone the repository:
```bash
git clone https://github.com/yourusername/hitop.git
cd hitop
```

2. Install dependencies:
```bash
cd frontend
npm install
```

3. Start the development server:
```bash
npm start
```

4. Open [http://localhost:3000](http://localhost:3000) in your browser

#### Building for Production

```bash
npm run build
```

### Firefox Extension

1. Build the extension:
```bash
./build-extension.sh
```

2. Install in Firefox:
   - Open `about:debugging`
   - Click "This Firefox"
   - Click "Load Temporary Add-on"
   - Select `extension/manifest.json`

See [EXTENSION_GUIDE.md](./EXTENSION_GUIDE.md) for detailed instructions.

## Quick Start Guide

### 1. Make Your First Request

1. Enter a URL in the URL field (e.g., `https://jsonplaceholder.typicode.com/todos/1`)
2. Select HTTP method (GET, POST, etc.)
3. Click "Make Request"
4. View the response below

### 2. Add Headers

1. Click "Add Header" in the Headers section
2. Enter header name and value
3. Headers are automatically included in the request

### 3. Save to Collection

1. Click the save icon next to "Make Request"
2. Enter a request name
3. Select or create a collection
4. Click "Save"

### 4. Use Variables

1. Click "Variables" under a collection name
2. Add variable key-value pairs (e.g., `apiUrl = https://api.example.com`)
3. Use in requests: `${apiUrl}/users`
4. See [VARIABLES_GUIDE.md](./VARIABLES_GUIDE.md) for more

### 5. Add Scripts

1. Scroll to "Pre-Request Script" or "Post-Request Script" sections
2. Write JavaScript code to modify requests or extract data
3. See [SCRIPTING_GUIDE.md](./SCRIPTING_GUIDE.md) for examples

## Documentation

- **[Variables Guide](./VARIABLES_GUIDE.md)** - Complete guide to using collection variables
- **[Scripting Guide](./SCRIPTING_GUIDE.md)** - Custom scripting documentation with examples
- **[Components README](./frontend/src/components/README.md)** - Component architecture
- **[Services README](./frontend/src/services/README.md)** - Service layer documentation

## Project Structure

```
hitop/
├── frontend/
│   ├── public/
│   └── src/
│       ├── components/          # React components
│       │   ├── Sidebar.js
│       │   ├── RequestPanel.js
│       │   ├── *Modal.js        # Various modal components
│       │   └── README.md
│       ├── services/            # Business logic services
│       │   ├── variableInterpolation.js
│       │   └── README.md
│       ├── App.js               # Main application component
│       ├── App.css              # Styles
│       └── index.js             # Entry point
├── VARIABLES_GUIDE.md           # Variables documentation
├── SCRIPTING_GUIDE.md           # Scripting documentation
└── README.md                    # This file
```

## Key Concepts

### Collections
Collections are groups of related API requests. Each collection can have:
- Multiple requests
- Collection-specific variables
- Shared context across requests

### Variables
Variables are key-value pairs defined at the collection level. They can be used in:
- URLs: `${apiUrl}/users`
- Headers: `Authorization: ${authToken}`
- Request body: `{"user": "${username}"}`
- Scripts: `getVariable('apiUrl')`

### Context
Context is runtime data shared between requests in a collection:
- Set in post-request scripts: `setContext('userId', 123)`
- Use in pre-request scripts: `getContext('userId')`
- Persists across requests in the same session

### Scripts
Custom JavaScript code that runs before or after requests:
- **Pre-Request**: Modify URL, headers, body before sending
- **Post-Request**: Extract data from response, store in context

## Use Cases

### 1. API Development & Testing
Test your APIs during development with different methods, headers, and payloads.

### 2. Authentication Workflows
```javascript
// Login request (post-request script)
const token = getResponseValue('token');
setContext('authToken', token);

// Authenticated request (pre-request script)
const token = getContext('authToken');
setHeader('Authorization', `Bearer ${token}`);
```

### 3. Multi-Environment Testing
Create collections for different environments:
- Dev: `apiUrl = https://dev-api.example.com`
- Staging: `apiUrl = https://staging-api.example.com`
- Production: `apiUrl = https://api.example.com`

### 4. Data Extraction & Chaining
```javascript
// Request 1: Create user (post-request)
const userId = getResponseValue('id');
setContext('userId', userId);

// Request 2: Get user (pre-request)
const userId = getContext('userId');
setUrl(`${getVariable('apiUrl')}/users/${userId}`);
```

## Browser Support

- Chrome (recommended)
- Firefox
- Safari
- Edge

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License.

## Acknowledgments

- Built with [React](https://reactjs.org/)
- UI components from [Material-UI](https://mui.com/)
- Syntax highlighting by [highlight.js](https://highlightjs.org/)

## Support

For issues, questions, or suggestions, please open an issue on GitHub.
