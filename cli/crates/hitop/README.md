# hitop

HTTP API client CLI — send requests, run collections, and script with JavaScript from the terminal.

Same feature set as the [hitop desktop app](https://github.com/bohdaq/hitop), but from your shell.

## Install

```bash
cargo install hitop
```

Or download a pre-built binary from [GitHub Releases](https://github.com/bohdaq/hitop/releases).

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
| `hitop requests auth <collection> <request>` | Set (`--bearer`, `--basic`, `--api-key`) or clear (`--none`) a request's auth |
| `hitop variables list/set/delete` | Manage collection variables |
| `hitop env list/add/delete/show/set/unset/use/off/import/export` | Manage environments |
| `hitop history` | View request history |
| `hitop import <file>` | Import collections (HITOP or Postman format) |
| `hitop export <collection>` | Export a collection |
| `hitop completions <shell>` | Generate shell completions |

## Environments, auth and query parameters

```bash
# Variable sets that override collection variables (Postman environments import too)
hitop env add staging
hitop env set staging baseUrl https://staging.api.example.com
hitop env set staging token stg-123
hitop env use staging             # or pass -e staging per command; hitop env off to stop

# Auth presets and query parameters on a one-off request
hitop send '${baseUrl}/users' -q page=2 -q per_page=50 --bearer '${token}'
hitop send https://api.example.com/me --basic ada:s3cret
hitop send https://api.example.com/search --api-key api_key=k-1 --api-key-in query

# Saved requests keep their auth
hitop requests add MyAPI --name "Me" --url '${baseUrl}/me' -H "Accept: application/json" --bearer '${token}'
```

## Checks in CI

Post-request scripts can assert. `hitop run` exits with status 1 when a request fails or a check doesn't pass, so it can gate a pipeline; each step is printed even when output isn't a terminal.

```bash
hitop send https://api.example.com/health --post-script "assert.status(200); assert.responseTime(500)"
hitop run MyAPI -e staging
```

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

Collections, environments, history, and runtime context are stored in:
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

// Checks, reported with the response and by `hitop run`
assert.status(200)
assert.bodyContains('id')
assert.header('content-type', 'json')   // or assert.header(name) for presence
assert.responseTime(500)                // milliseconds
assert.equal(getResponseValue('user.id'), 42, 'user id')
assert.ok(statusCode < 300, 'success')
```
