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

# Form fields and file uploads (multipart/form-data; --urlencoded for a plain form)
hitop send https://api.example.com/upload -F title=Q3 -F file=@report.csv -F 'logo=@logo.bin;type=image/png'

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

# Import from Postman: a collection, its environment and globals, into a folder
hitop import shop.postman_collection.json dev.postman_environment.json --to ./shop

# Export to HITOP format (compatible with the web app)
hitop export MyAPI -o my-collection.json
```

## Commands

| Command | Description |
|---|---|
| `hitop send <url>` | Send a one-off request |
| `hitop init [dir]` | Create a collection folder (`hitop.yaml`, `requests/`, `environments/`) |
| `hitop convert <json> <dir>` / `hitop convert <dir> <json>` | Convert a HITOP JSON export to a collection folder, or back |
| `hitop run <collection>` | Run all requests in a collection sequentially (reports, data files and exit codes: see "Checks in CI") |
| `hitop run --all` | Run every collection |
| `hitop collections list/add/delete/rename/duplicate` | Manage collections |
| `hitop requests list/add/show/delete/rename/send` | Manage requests |
| `hitop requests auth <collection> <request>` | Set (`--bearer`, `--basic`, `--api-key`) or clear (`--none`) a request's auth |
| `hitop variables list/set/delete` | Manage collection variables |
| `hitop env list/add/delete/show/set/unset/use/off/import/export` | Manage environments |
| `hitop secret set/get/rm/ls --env <env>` | Secret variables in the OS keychain (`-c <folder>` for a folder's environment) |
| `hitop cookies ls/clear [-c <collection>]` | The cookies hitop keeps between requests (`--no-cookies` on `send`, `run`, `requests send` turns the jar off) |
| `hitop history` | View request history |
| `hitop import <files>... [--to <dir>]` | Import HITOP or Postman collections, Postman environments and globals; prints what didn't come over (see "From Postman") |
| `hitop export <collection>` | Export a collection |
| `hitop telemetry on/off/status` | The anonymous weekly usage ping: off unless you turn it on; `status` shows exactly what would be sent |
| `hitop completions <shell>` | Generate shell completions |

`run`, `requests`, `variables`, `export` and `send -c` take a path to a collection folder as well as a collection name.

## From Postman

`hitop import` reads Postman collections (v2.0 and v2.1), environments and globals, detected
from the content. With `--to <dir>` it writes a collection folder; without it, global storage.

- Folders become subfolders of `requests/`; collection auth is the folder's default auth;
  collection and folder scripts run before each request's own, as in Postman. `{{name}}`
  becomes `${name}`.
- Environment values of Postman's `secret` type become secret variables: their values go to
  the OS keychain, never into a file (without a keychain hitop names the `HITOP_SECRET_*`
  variable to set instead). Globals become defaults in every environment imported with them,
  or an environment of their own.
- Scripts keep working through hitop's `pm.*` shim: `pm.environment`, `pm.collectionVariables`,
  `pm.globals` and `pm.variables` (one set of variables in hitop; values set by a script last
  for the run), `pm.response`, `pm.request`, `pm.test`, a chai style `pm.expect`, and the old
  `tests[...]`, `responseBody`, `responseCode` and `postman.*Variable` API.
- What can't come over is listed after the import: every script line with a `pm.*` call the
  shim doesn't have (`pm.sendRequest`, `pm.cookies`, `postman.setNextRequest`, `require`, ...),
  OAuth and other auth types, file fields and Postman's `{{$dynamic}}` variables.

```bash
$ hitop import api.postman_collection.json --to ./api
✓ Imported 'API' into ./api (12 requests)
! 'API': 2 things to check (1 script line with unsupported calls)
  Users / Create (post-request script) line 4: pm.sendRequest in `pm.sendRequest(url, cb);`
  collection auth: oauth2 auth
```

## Collections in git

A collection folder is YAML, one file per request, so it can be reviewed and run from the repository
([format spec](https://github.com/bohdaq/hitop/blob/main/spec/collection-format.md)):

```bash
hitop init my-api                      # hitop.yaml, requests/get-example.yaml, environments/local.yaml
hitop convert export.json my-api       # or from the web app's JSON export (--environments envs.json too)
hitop requests add ./my-api --name "List orders" --url '${base}/orders'
hitop run ./my-api --env local         # a folder uses its own environments/
hitop convert ./my-api export.json     # back to JSON
```

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

## Secrets

A secret variable has no value in collection files, JSON storage, history or exports; it is shown as `••••`.
Its value comes from, in this order: `HITOP_SECRET_<ENV>_<NAME>`, `--secret NAME=value` (on `send`, `run`
and `requests send`), the OS keychain (macOS Keychain, Windows Credential Manager, Secret Service on Linux),
then a prompt when a terminal is attached.

```bash
hitop secret set token --env staging          # prompts for the value, stores it in the keychain, marks token secret
hitop secret set token --env prod -c ./my-api # a folder's environment: environments/prod.yaml gets token: {secret: true}
hitop secret ls --env prod -c ./my-api        # names and where each value comes from, never the values
HITOP_SECRET_PROD_TOKEN=$TOKEN hitop run ./my-api --env prod   # CI: no keychain needed
```

Without a keychain (headless Linux, or `HITOP_KEYCHAIN=off`) hitop says so and uses the env vars and `--secret`.

## Checks in CI

Post-request scripts can assert. `hitop run` prints every request with its status, time and each assertion, and
exits with a code a pipeline can use:

| Exit code | Meaning |
|---|---|
| `0` | every request passed (sent, status below 400, every assertion true) |
| `1` | a request or an assertion failed |
| `2` | usage or configuration error (unknown collection or environment, bad flag, unreadable `--env-file` or `--data`, missing secret, reports not written) |

```bash
hitop send https://api.example.com/health --post-script "assert.status(200); assert.responseTime(500)"
hitop run ./my-api --env ci --reporter cli,junit,json --out reports   # reports/junit.xml, reports/report.json
hitop run ./my-api --env-file ci.yaml --var base=https://preview.example.com --timeout 5000 --bail
hitop run ./my-api --env ci --data users.csv     # one iteration per row (CSV with a header row, or a JSON array of objects)
hitop run ./my-api --env ci --iterations 3
```

- `--reporter`: `cli` (default), `junit`, `json`, comma separated; files go to `--out` (default `hitop-reports`).
  The JSON report is versioned (`"reportVersion": 1`) and documented in
  [spec/run-report.md](https://github.com/bohdaq/hitop/blob/main/spec/run-report.md). Secret values are `••••` in every report.
- Variables, later ones win: collection, environment (`--env` or `--env-file`), the `--data` row, `--var`, then secrets.
  `--var` values are not masked; use `--secret` or `HITOP_SECRET_<ENV>_<NAME>` for secrets.
- `--bail` stops at the first failed request; without it every request runs. (Before 0.3 stopping was the default
  and `--no-stop-on-fail` turned it off; that flag is still accepted.)
- `--timeout <ms>` limits each request; a slower one fails the run with a timeout.

## Cookies and form bodies

Cookies a response sets are sent with later requests to the same site, the way a browser does
(domain, path, `Secure`, `Expires`/`Max-Age`; the same rules as the web app). Each workspace has its
own jar, kept between runs: collections in global storage and `hitop send` without `-c` share the
global one (also the desktop app's), each collection folder has its own. `hitop cookies ls -c ./my-api` lists them, `hitop cookies clear`
empties a jar, and `--no-cookies` sends none and keeps none. A request's own `Cookie` header wins.
Only the final response's `Set-Cookie` lines count: cookies set on a redirect hitop follows are not kept.

`-F name=value` and `-F name=@path[;type=mime]` send `multipart/form-data` (POST unless `-X` says
otherwise); add `--urlencoded` for `application/x-www-form-urlencoded`. Saved requests with a form or
multipart body (`body.type: form` / `multipart` in a folder, or from the web app or a Postman import) go
out as one in `hitop run` and `requests send`; a file row has no file in a collection, so it goes out as an
empty part, as in the web app after a reload.

## Shell completions

```bash
# bash
hitop completions bash >> ~/.bash_completion

# zsh
hitop completions zsh > ~/.zfunc/_hitop

# fish
hitop completions fish > ~/.config/fish/completions/hitop.fish
```

## Usage ping

Off by default; nothing is sent until you run `hitop telemetry on`. Then, at most once a week, the next
hitop command sends `POST https://hitop.rws8.tech/api/ping` with a random install id made on your machine,
`"surface":"cli"`, the version and the OS family, and nothing else. `hitop telemetry status` prints that body,
`hitop telemetry off` stops it and deletes the id. Builds without the default `telemetry` feature
(`cargo install hitop --no-default-features`) don't contain the code at all. Details:
[privacy policy](https://hitop.rws8.tech/privacy.html).

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
