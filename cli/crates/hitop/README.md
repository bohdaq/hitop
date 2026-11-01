# hitop

The API client your terminal, your CI and your AI agent can all drive. One small Rust binary,
collections as files in your repo, nothing in anyone's cloud.

Send requests, keep them in collection folders (YAML, one file per request), script them in
JavaScript, import from Postman, run them in CI with JUnit and JSON reports, and let MCP clients such as
Claude Code and Cursor use them through `hitop mcp`. The same
collections open in the [web app](https://hitop.rws8.tech/app/) and the
[desktop app](https://github.com/bohdaq/hitop/releases). No account, MIT licensed.

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

# GraphQL: the query from a file (or - for stdin), sent as a JSON POST of {query, variables, operationName}
hitop send https://api.example.com/graphql --graphql user.graphql --graphql-vars '{"id": "${userId}"}' -c ./my-api

# WebSocket: type messages on stdin, or script it (send two, exit after two replies, JSON lines)
hitop ws wss://echo.example.com/socket
hitop ws wss://echo.example.com/socket --send hello --send '{"op": "ping"}' --count 2 --json

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

# Import an OpenAPI 3.0 or 3.1 spec (file or URL) into a folder; later, sync it with a newer spec
hitop import openapi https://petstore3.swagger.io/api/v3/openapi.json --to ./petstore
hitop openapi sync https://petstore3.swagger.io/api/v3/openapi.json ./petstore --dry-run

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
| `hitop schedule <folder> --github` / `--cron "<expr>"` | Run a collection folder on a schedule: write a GitHub Actions workflow, or print a crontab line (see "Scheduled runs") |
| `hitop collections list/add/delete/rename/duplicate` | Manage collections |
| `hitop requests list/add/show/delete/rename/send` | Manage requests |
| `hitop ws <url>` / `hitop ws <request> -c <collection>` | Open a WebSocket connection: send typed lines or `--send`/`--message` messages, print what arrives (see "WebSocket") |
| `hitop snippet <collection> <request> --lang <lang>` | Print a saved request as code: `curl`, `fetch`, `python`, `go`, `java`, `rust` (see "Code snippets") |
| `hitop requests auth <collection> <request>` | Set (`--bearer`, `--basic`, `--api-key`) or clear (`--none`) a request's auth |
| `hitop variables list/set/delete` | Manage collection variables |
| `hitop env list/add/delete/show/set/unset/use/off/import/export` | Manage environments |
| `hitop secret set/get/rm/ls --env <env>` | Secret variables in the OS keychain (`-c <folder>` for a folder's environment; `--globals` for a secret global) |
| `hitop globals show/set/unset/import/export [-c <folder>]` | The globals: variables below every collection's, in global storage or a folder's `globals.yaml` |
| `hitop cookies ls/clear [-c <collection>]` | The cookies hitop keeps between requests (`--no-cookies` on `send`, `run`, `requests send` turns the jar off) |
| `hitop history` | View request history |
| `hitop import <files>... [--to <dir>]` | Import HITOP or Postman collections, Postman environments and globals; prints what didn't come over (see "From Postman") |
| `hitop import openapi <file\|url> [--to <dir>] [--flat]` | Import an OpenAPI 3.0 or 3.1 spec (YAML or JSON): a folder per tag, a request per operation; prints what was mapped and skipped (see "From OpenAPI") |
| `hitop openapi sync <file\|url> <dir> [--dry-run] [--prune]` | Update a folder imported from a spec: adds new operations, updates changed ones where you didn't edit them, lists removed ones (deleted only with `--prune`) |
| `hitop export <collection>` | Export a collection |
| `hitop telemetry on/off/status` | The anonymous weekly usage ping: off unless you turn it on; `status` shows exactly what would be sent |
| `hitop mcp [folder]` | MCP server over stdio for Claude Code, Cursor and other MCP clients: list, read, run, create and update requests, switch environments; secrets never returned ([guide](https://hitop.rws8.tech/terms/use-hitop-with-claude-code-and-cursor.html)) |
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
  variable to set instead). Globals go to hitop's globals: the folder's `globals.yaml` with
  `--to`, else global storage (`hitop globals show`).
- Scripts keep working through hitop's `pm.*` shim: `pm.environment`, `pm.collectionVariables`
  and `pm.variables` (one set of variables in hitop), `pm.globals` (the globals scope; values set
  by a script last for the run), `pm.response`, `pm.request`, `pm.test`, a chai style `pm.expect`, and the old
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

## From OpenAPI

`hitop import openapi <file|url>` reads an OpenAPI 3.0 or 3.1 spec, YAML or JSON (Swagger 2.0
is refused: convert it first). `hitop import spec.yaml --to dir` finds a spec by its content too.

- Each tag is a folder (`--flat` puts every request at the top), each operation a request named
  after its summary (or operationId). The first server is the `baseUrl` collection variable;
  path parameters are `${name}` variables; required query parameters and headers are on,
  optional ones kept off. JSON bodies come from the spec's example or one built from the
  schema; form and multipart bodies become form fields. Local `$ref`s are followed (recursive
  schemas stop where they loop); refs to other files are reported.
- Security schemes become auth: HTTP bearer and basic, API keys in a header or query. OAuth 2,
  OpenID Connect and cookie keys are reported. The variables they use (`${api_key}`) are yours
  to set, as secrets in an environment.
- Response examples become saved examples (marked `source: openapi`).
- Each request records its operation under `openapi` (operationId, `METHOD /path` and a hash
  of each field as imported), and `hitop.yaml` records the spec.

`hitop openapi sync <file|url> <dir>` brings the folder up to a newer spec. Operations are matched
by operationId, then by method and path. New ones are added; a field the spec changed (URL,
parameters, headers, body, auth, its examples) is updated when you left it as imported, and kept
with a note when you edited it. Names, files, scripts and your own examples are never touched.
Operations gone from the spec are listed and kept; `--prune` deletes them. `--dry-run` shows the
diff and writes nothing.

```bash
$ hitop openapi sync ./openapi-v2.yaml ./shop --dry-run
Sync 'Shop' with ./openapi-v2.yaml (dry run: nothing is written)
  + GET /pets/{id} (getPet)  Get a pet → requests/pets/get-a-pet.yaml
  ~ GET /pets (listPets)  All my pets: updated url
  ~ POST /pets (addPet)  Add a pet: kept your body (the spec changed it too)
  - DELETE /pets/{id} (deletePet)  Delete a pet: removed from the spec: kept (--prune deletes it)
  2 added, 2 changed, 1 removed, 0 unchanged
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

## WebSocket

A request saved with `kind: websocket` (the web app and desktop: method "WebSocket") opens a connection
instead of sending an HTTP request. `hitop ws` connects to a URL, or to a saved request with `-c`: its URL,
query parameters, headers, auth and subprotocols go into the handshake, with `${variables}` and secrets
filled in as for `hitop send`. `-H`, `--protocol` and the auth flags add to or replace what is saved.

```bash
# Interactive: each line you type is sent; Ctrl-D closes after --wait (1 s)
hitop ws 'ws://localhost:8080/chat' -H 'X-Client: cli' --protocol chat.v2

# A saved request: its saved messages by name, then a text; exit after 3 replies
hitop ws "Live prices" -c ./my-api --env prod -m subscribe -m ping -s '${symbol}' --count 3

# For scripts: one JSON object per event on stdout (open, sent, received, closed, error)
hitop ws "Live prices" -c ./my-api -m subscribe -n 1 --json | jq -r 'select(.event == "received") | .data'
```

What the server sends goes to stdout (binary messages as hex, base64 in `--json`); sent messages and
connection events go to stderr. Secret values are shown as `••••`. Without `--send` or `--message`, lines
from stdin are sent (empty lines are skipped). After the last message (or the end of stdin) hitop keeps
listening for `--wait` ms (default 1000, or 30000 with `--count`), then closes. `--count N` exits once N
messages arrived; exit code 1 when the connection fails or fewer than N arrived. `--timeout` limits the
handshake (default 30000 ms). `hitop run` skips WebSocket requests.

## Globals

Globals are the lowest scope, as in Postman: globals < collection variables < environment < `--data` row <
`--var` < values a script sets < secrets. Global storage has one set (`hitop globals set region eu`), a folder
its own `globals.yaml`. A secret global (`hitop secret set apiKey --globals`) takes its value from
`HITOP_SECRET_GLOBAL_<NAME>`, `--secret`, the keychain or a prompt, and still yields to an environment's
value of the same name. `hitop globals export` writes `{variables, secrets}` without secret values.

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
hitop run ./my-api --env ci --reporter html     # hitop-reports/report.html, one page to open in a browser
hitop run ./my-api --env-file ci.yaml --var base=https://preview.example.com --timeout 5000 --bail
hitop run ./my-api --env ci --data users.csv     # one iteration per row (CSV with a header row, or a JSON array of objects)
hitop run ./my-api --env ci --iterations 3
```

- `--reporter`: `cli` (default), `junit`, `json`, `html`, comma separated; files go to `--out` (default `hitop-reports`).
  `html` writes `report.html`: one self-contained page (inline CSS, no script, loads nothing, light and dark)
  with the summary, every request, its timings, failure and assertions. The JSON report is versioned (`"reportVersion": 1`) and documented in
  [spec/run-report.md](https://github.com/bohdaq/hitop/blob/main/spec/run-report.md). Secret values are `••••` in every report.
  Run from a folder, each JUnit `<testcase>` and each request in `report.json` carries its file
  (`file="my-api/requests/get-user.yaml"`: the folder as given, then `requests/<file>.yaml`), so CI
  annotations land on the request file.
- Variables, later ones win: globals, collection, environment (`--env` or `--env-file`), the `--data` row, `--var`, then secrets.
  `--var` values are not masked; use `--secret` or `HITOP_SECRET_<ENV>_<NAME>` for secrets.
- `--bail` stops at the first failed request; without it every request runs. (Before 0.3 stopping was the default
  and `--no-stop-on-fail` turned it off; that flag is still accepted.)
- `--timeout <ms>` limits each request; a slower one fails the run with a timeout.

## Scheduled runs

```bash
hitop schedule ./my-api --github --env prod                        # .github/workflows/hitop-my-api-prod.yml, daily 06:00 UTC
hitop schedule ./my-api --github --env prod --cron "0 */6 * * *" --force
hitop schedule ./my-api --cron "*/30 * * * *" --env local          # prints a crontab line; add it with crontab -e
```

- `--github` writes a workflow at the root of the git repository the folder is in. It runs
  `bohdaq/hitop-action@v1` on `schedule:` and `workflow_dispatch:` (the "Run workflow" button) with this
  hitop's version (`--hitop-version` to change it), uploads the JUnit file and, for hitop releases after 0.3.0, the HTML report.
  Secrets are passed as `HITOP_SECRET_<ENV>_<NAME>: ${{ secrets.HITOP_SECRET_<ENV>_<NAME> }}`: by default every
  secret variable of the environment and of the folder's `globals.yaml`, or the names in `--secrets a,b`. The
  command prints the `gh secret set` lines to run; no value is ever written. An existing workflow is kept unless
  you pass `--force`. GitHub runs schedules in UTC, at most every 5 minutes.
- `--cron` alone prints one crontab line and installs nothing: absolute paths, `%` escaped for cron, every run's
  `junit.xml`, `report.json` and `report.html` in `<log dir>/<time>/`, output appended to `<log dir>/hitop.log`
  (`--log-dir`, default `~/.local/state/hitop/runs/<name>`). Secrets come from the keychain or
  `HITOP_SECRET_*` variables set in the crontab.
- Cron expressions have five fields (minute, hour, day of month, month, day of week) with `*`, numbers, ranges,
  steps and lists, or a single month or day name; hitop checks them before writing anything.

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

## Code snippets

`hitop snippet ./my-api "Create order" --lang python --env dev` prints the request as code for curl,
JavaScript `fetch` (Node, top-level `await`), Python `requests`, Go `net/http`, Java `HttpClient` or
Rust `reqwest` (blocking). The request is addressed as in `hitop requests send`, variables come from
the globals, the collection and the environment (`--env`, else the active one), and auth, headers,
query parameters and raw, form and multipart bodies are written the way hitop sends them (one value
per header name; no `Content-Type` that hitop wouldn't send). Scripts are not run.

Secret variables are never resolved by default: they are written as a `{{NAME}}` placeholder, as are
variables with no value, and stderr lists them. `--show-secrets` writes the values instead, resolved
like `requests send` does (`HITOP_SECRET_<ENV>_<NAME>`, `--secret`, keychain, prompt); treat that
output as a secret. The web app's Code panel uses the same generator (hitop-core through hitop-wasm).

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
