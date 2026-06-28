# hitop

HTTP API client CLI — send requests, run collections, and script with JavaScript from the terminal.

Same feature set as the [hitop web app](https://github.com/bohdantsap/hitop), but from your shell.

## Install

```bash
cargo install hitop
```

Or download a pre-built binary from [GitHub Releases](https://github.com/bohdantsap/hitop/releases).

## Quick start

```bash
# Send a one-off request
hitop send https://jsonplaceholder.typicode.com/todos/1

# With method, headers, and body
hitop send https://api.example.com/users \
  -X POST \
  -H "Content-Type: application/json" \
  -b '{"name":"Alice"}'

# Create a collection and save variables
hitop collections add MyAPI
hitop variables set MyAPI hostname https://api.example.com

# Add and send a request using collection variables
hitop requests add MyAPI --name "Get user" --url '${hostname}/users/1'
hitop requests send MyAPI "Get user"

# Run the whole collection (with pre/post scripts and context passing)
hitop run MyAPI

# View history
hitop history --limit 10

# Import from Postman
hitop import my-collection.json --format postman

# Export to HITOP format (compatible with the web app)
hitop export MyAPI -o my-collection.json
```

## Commands

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

## Shell completions

```bash
# bash
hitop completions bash >> ~/.bash_completion

# zsh
hitop completions zsh > ~/.zfunc/_hitop

# fish
hitop completions fish > ~/.config/fish/completions/hitop.fish
```

## Storage

Collections, history, and runtime context are stored in:
- **Linux**: `~/.config/hitop/`
- **macOS**: `~/Library/Application Support/hitop/`
- **Windows**: `%APPDATA%\hitop\`

The JSON format is identical to the hitop web app — collections can be exported from the app and imported into the CLI (and vice versa).

## Scripting

Pre-request and post-request scripts are JavaScript (executed via [boa_engine](https://crates.io/crates/boa_engine)).

**Pre-request script API:**
```js
setUrl(url)           // override the request URL
setBody(body)         // override the request body
setHeader(name, val)  // add or update a header
setContext(key, val)  // store a value for subsequent requests
getContext(key)       // read a stored value
getVariable(key)      // read a collection variable
```

**Post-request script API:**
```js
response              // parsed JSON (or null if not JSON)
responseText          // raw response body string
responseHeaders       // object of response headers
statusCode            // HTTP status code integer
setContext(key, val)
getContext(key)
getVariable(key)
getResponseValue(path) // dot-notation path into response JSON
getResponseHeader(name)
```
