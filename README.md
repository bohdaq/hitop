# HITOP - HTTP API Testing Tool

A powerful, modern HTTP API testing tool. Test APIs, manage collections, automate workflows with custom scripts, and use variables for dynamic requests.

**Available as:**
- 💻 CLI ([Install from crates.io](https://crates.io/crates/hitop) | [CLI Guide](#cli))
- 🦊 Firefox Extension ([Install from Mozilla Add-ons](https://addons.mozilla.org/addon/hitop/) | [Installation Guide](./EXTENSION_GUIDE.md))
- 🔵 Chrome Extension ([Install from Chrome Web Store](https://chromewebstore.google.com/detail/hitop-http-api-testing-to/mmbpagencfjfgeicmeobbmeocdpeokdk))

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
- **Import/Export**: Share collections as JSON (HITOP and Postman v2.1 formats)

### 🔧 Variables
- **Collection Variables**: Define reusable variables per collection
- **Dynamic Interpolation**: Use `${variableName}` syntax in URLs, headers, and body
- **Variable Management**: Easy-to-use interface for managing variables
- See [VARIABLES_GUIDE.md](./VARIABLES_GUIDE.md) for detailed documentation

### 📝 Custom Scripting
- **Pre-Request Scripts**: Modify requests before they're sent
- **Post-Request Scripts**: Extract data from responses
- **Context Sharing**: Pass data between requests in a collection run
- **Variable Access**: Use collection variables in scripts
- See [SCRIPTING_GUIDE.md](./SCRIPTING_GUIDE.md) for detailed documentation

### 📊 Additional Features
- **Request History**: Track and reload recent requests
- **Syntax Highlighting**: Code highlighting for responses
- **Auto-Save**: Collections persisted to disk (CLI) or browser localStorage (extensions)

---

## CLI

Install the CLI with Cargo:

```bash
cargo install hitop
```

Or download a pre-built binary from [GitHub Releases](https://github.com/bohdaq/hitop/releases).

### Quick start

```bash
# Send a one-off request
hitop send https://jsonplaceholder.typicode.com/todos/1

# With method, headers, and body
hitop send https://api.example.com/users \
  -X POST \
  -H "Content-Type: application/json" \
  -b '{"name":"Alice"}'

# Create a collection and set variables
hitop collections add MyAPI
hitop variables set MyAPI hostname https://api.example.com

# Add a request and send it
hitop requests add MyAPI --name "Get user" --url '${hostname}/users/1'
hitop requests send MyAPI "Get user"

# Run the whole collection (pre/post scripts + context passing)
hitop run MyAPI

# View history
hitop history --limit 10

# Import from Postman
hitop import my-collection.json --format postman

# Export to HITOP format (compatible with the browser extensions)
hitop export MyAPI -o my-collection.json
```

### Commands

| Command | Description |
|---|---|
| `hitop send <url>` | Send a one-off request |
| `hitop run <collection>` | Run all requests in a collection sequentially |
| `hitop run --all` | Run every collection |
| `hitop collections list/add/delete/rename/duplicate` | Manage collections |
| `hitop requests list/add/show/delete/rename/send` | Manage requests |
| `hitop variables list/set/delete` | Manage collection variables |
| `hitop history` | View request history |
| `hitop import <file>` | Import collections (HITOP or Postman format) |
| `hitop export <collection>` | Export a collection |
| `hitop completions <shell>` | Generate shell completions |

### Shell completions

```bash
hitop completions bash >> ~/.bash_completion
hitop completions zsh > ~/.zfunc/_hitop
hitop completions fish > ~/.config/fish/completions/hitop.fish
```

### Storage

Collections, history, and runtime context are stored in:
- **Linux**: `~/.config/hitop/`
- **macOS**: `~/Library/Application Support/hitop/`
- **Windows**: `%APPDATA%\hitop\`

The JSON format is identical to the browser extensions — collections exported from the extension can be imported into the CLI and vice versa.

---

## Browser Extensions

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

### Chrome Extension

Install directly from the [Chrome Web Store](https://chromewebstore.google.com/detail/hitop-http-api-testing-to/mmbpagencfjfgeicmeobbmeocdpeokdk).

---

## Building from source

### Frontend (React)

```bash
cd frontend
npm install
npm start          # development server at http://localhost:3000
npm run build      # production build
```

### CLI (Rust)

```bash
cd cli
cargo build --release
./target/release/hitop --help
```

---

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
- Read in pre-request scripts: `getContext('userId')`
- Persists across requests in the same run

### Scripts
Custom JavaScript code that runs before or after requests:
- **Pre-Request**: Modify URL, headers, body before sending (`setUrl`, `setHeader`, `setBody`)
- **Post-Request**: Extract data from response, store in context (`response`, `getResponseValue`, `setContext`)

## Use Cases

### Authentication workflows
```javascript
// Login request — post-request script
const token = getResponseValue('token');
setContext('authToken', token);

// Authenticated request — pre-request script
const token = getContext('authToken');
setHeader('Authorization', `Bearer ${token}`);
```

### Multi-environment testing
Create collections for different environments:
- Dev: `hostname = https://dev-api.example.com`
- Staging: `hostname = https://staging-api.example.com`
- Production: `hostname = https://api.example.com`

### Request chaining
```javascript
// Request 1: Create user — post-request script
const userId = getResponseValue('id');
setContext('userId', userId);

// Request 2: Get user — pre-request script
const id = getContext('userId');
setUrl(`${getVariable('hostname')}/users/${id}`);
```

## Project Structure

```
hitop/
├── cli/                         # Rust CLI
│   ├── crates/
│   │   ├── hitop-core/          # Core library (published to crates.io)
│   │   └── hitop/               # CLI binary (published to crates.io)
│   └── Cargo.toml
├── frontend/                    # React web app (used by extensions)
│   └── src/
├── extension/                   # Firefox extension
├── chrome-extension/            # Chrome extension
├── VARIABLES_GUIDE.md
├── SCRIPTING_GUIDE.md
└── README.md
```

## Documentation

- **[Variables Guide](./VARIABLES_GUIDE.md)** — Complete guide to collection variables
- **[Scripting Guide](./SCRIPTING_GUIDE.md)** — Custom scripting with examples
- **[Extension Guide](./EXTENSION_GUIDE.md)** — Browser extension installation

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

MIT — see the [LICENSE](LICENSE) file for details.
