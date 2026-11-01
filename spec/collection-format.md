<!--
hitop collection format, version 1
Copyright 2026 Bohdan Tsap
This specification is licensed under the Creative Commons Attribution 4.0
International License (CC BY 4.0): https://creativecommons.org/licenses/by/4.0/
You may share and adapt it, including commercially, if you give credit.
The hitop code that implements it is MIT licensed.
-->

# hitop collection format, version 1

Status: **stable** since hitop 1.0 (2026-11-01). Version 1 changes only by
additions (section 7). Reference implementation:
`cli/crates/hitop-core/src/format/`. Compatibility suite: `spec/compat/v1/`.

A collection is a folder of YAML files, one file per request, so it can live
in git, be reviewed in pull requests and be run in CI with `hitop run ./my-api`.

```
my-api/
  hitop.yaml                 # format, name, variables, default auth, request order
  requests/
    get-user.yaml            # one request per file
    orders/                  # subfolders are folders
      create-order.yaml
  environments/
    dev.yaml                 # variables; secret ones have no value in the file
    prod.yaml
  globals.yaml               # optional: variables below everything else (section 5a)
```

## 1. General rules

- Files are YAML 1.2, UTF-8. Readers must use the YAML 1.2 core schema:
  `on`, `yes` and `no` are strings, not booleans.
- `.yaml` and `.yml` are both read. A writer keeps an existing file's
  extension and uses `.yaml` for new files. Two files for the same request
  (`a.yaml` and `a.yml`) are an error, and so are request files whose paths
  differ only in case (`Get.yaml` and `get.yaml`): they are one file on
  macOS and Windows.
- Hidden files and folders (name starting with `.`) and symlinks are ignored.
- Keys are written in the order this document lists them, so diffs stay
  small. Keys a reader doesn't know are kept and written back after the
  known ones (sorted by name). This holds for `hitop.yaml`, request files,
  environment files and header, disabled parameter and form field rows.
  Unknown keys inside `body` and `scripts`, or on an enabled query
  parameter (enabled parameters live in the URL), are an error rather than
  silently dropped. A key that means something in the JSON model (section 6)
  can't be used as an unknown key.
- Multi-line strings (bodies, scripts) are written as block scalars (`|`).
- Fields with their default value are left out.
- Values written as numbers or booleans where a string is expected
  (`port: 8080`) are read as strings (`"8080"`).
- Paths inside files (`order` entries) are relative, use `/`, and may not
  contain empty, `.`, `..` or hidden segments, `\` or `:`. Folders come from
  untrusted repositories; nothing may point outside the folder.

## 2. `hitop.yaml`

```yaml
format: 1
name: Shop API
id: 1793470569736000
workspace: 5f0c2a9e7d1b4c3a8e6f0b2d4a6c8e01
variables:
  base: https://api.example.com
auth:
  type: bearer
  token: ${token}
order:
- get-user
- orders/create-order
```

| Key | Required | Meaning |
| --- | --- | --- |
| `format` | yes | Format version, `1`. A reader refuses a higher version. |
| `name` | no | Collection name. Default: the folder's name. |
| `id` | no | Numeric id used by hitop's JSON storage. Default: derived from `name` (section 6). |
| `workspace` | no | Workspace id: the key of this collection's secrets in the OS keychain (section 5). `hitop init` writes a random one (32 hex digits). Default: the collection's `id` in decimal. |
| `variables` | no | Map of variable name to value, used as `${name}` in requests. |
| `auth` | no | Default auth (section 4) for requests that have no `auth` of their own. |
| `order` | no | Request order: paths under `requests/` without the extension. Requests not listed follow in path order; listed paths that don't exist are ignored. Left out when it equals path order. |

## 3. Request files: `requests/**/*.yaml`

```yaml
name: Create order
method: POST
url: ${base}/orders
params:
- name: dry_run
  value: '1'
  enabled: false
headers:
- name: Content-Type
  value: application/json
auth:
  type: basic
  username: shop
  password: ${password}
body:
  type: json
  content: |
    {"product": 42}
scripts:
  pre: setHeader('Idempotency-Key', String(Date.now()));
  post: |
    assert.status(201);
    setContext('orderId', getResponseValue('id'));
examples:
- name: created
  status: 201
  body: '{"id": 42}'
```

| Key | Default | Meaning |
| --- | --- | --- |
| `name` | the file name | Display name. |
| `kind` | `http` | `websocket` for a WebSocket request, `grpc` for a gRPC call (below). Left out for HTTP. |
| `method` | `GET` | HTTP method, as written. |
| `url` | empty | URL; may contain `${variables}`. |
| `params` | none | Query parameters: rows of `name`, `value`, `enabled`. Enabled rows are appended to `url` in order; disabled rows are kept but not sent. |
| `headers` | none | Rows of `name`, `value`, `enabled` (default `true`). Disabled rows are not sent. |
| `auth` | none | Auth preset (section 4). `{type: none}` turns off the collection's default auth. |
| `body` | none | `type` and `content` or `fields`, see below. |
| `websocket` | none | A WebSocket request's `subprotocols` and saved `messages` (below). |
| `grpc` | none | A gRPC request's `service`, `method`, `message` and schema source (below). |
| `scripts.pre` / `scripts.post` | empty | Pre-request and post-request JavaScript (hitop's script API: `setHeader`, `setUrl`, `setBody`, `getVariable`, `getContext`, `setContext`, `getResponseValue`, `assert.*`). |
| `examples` | none | Saved responses, free-form maps. |
| `id` | derived from the path | Numeric id used by hitop's JSON storage (section 6). |

The request's path under `requests/` (without extension) is its identity.
Subfolders are folders. A request written by hitop without a chosen path
gets its name in lower case, with every run of characters other than letters
and digits turned into `-` (`Get user` → `get-user.yaml`); clashes are
numbered `get-user-2.yaml`, `get-user-3.yaml`.

When a URL's query can be rebuilt exactly from its parts, writers move it
into `params` and leave `url` without it. Otherwise (`?` with nothing after
it, empty parts like `a=1&&b`, a `?` inside the fragment, unencoded spaces) the
URL is written exactly as typed.

### Body

| `type` | Content | Sent as |
| --- | --- | --- |
| `none` | | no body |
| `json`, `text`, `xml` | `content`: the body text | the text, after `${variable}` substitution |
| `graphql` | `content`: the query; `variables`: JSON text of an object (optional); `operationName` (optional) | `application/json`: `{"query", "variables", "operationName"}` (below) |
| `form` | `fields`: rows | `application/x-www-form-urlencoded` |
| `multipart` | `fields`: rows; a row with `type: file` is a file | `multipart/form-data` |

Form rows have `name`, `value`, `type` (`text`, the default, or `file`) and
`enabled`. A form body may also carry `content` (text kept from before the
body type was switched), and a text body may carry `fields`; neither is sent.

Without a `body` key the request has no body. For text bodies the `type` is
a label: hitop infers it from the `Content-Type` header (`json`, `xml`, other
types are `text`) or, without one, from the content (valid JSON object or
array → `json`, starts with `<` → `xml`, else `text`). `graphql` is never
inferred: it is always written.

### GraphQL

```yaml
body:
  type: graphql
  content: |
    query User($id: ID!) {
      user(id: $id) { name }
    }
  variables: |
    {"id": "${userId}"}
  operationName: User
```

A `graphql` body is sent the GraphQL-over-HTTP way: a JSON object with
`query` (the `content`), `variables` and `operationName`, in that order, with
`Content-Type: application/json` unless a header of the request sets a
`Content-Type`. `${variables}` are substituted in the query, the variables
text and the operation name first; the query is then encoded as a JSON
string and the variables text, which must then be a JSON object (or `null`,
or empty), goes in as written. Empty `variables` and `operationName` are left
out of the body. Use the method the server expects (normally `POST`).

`variables` is text, not a YAML map, so it can hold `${variables}` where
JSON allows no string (`{"limit": ${limit}}`) and comes back exactly as
typed. `variables` and `operationName` may also stay on a body of another
type (kept from before the body type was switched); they are then not sent.
Both keys were added to version 1 after its first release: readers from
before them (hitop 0.3.0 and older) refuse a file that has them (unknown
keys inside `body` are an error), and read every file without them.

### WebSocket

```yaml
name: Live prices
kind: websocket
url: wss://stream.example.com/prices
params:
- name: market
  value: eu
headers:
- name: X-Client
  value: hitop
auth:
  type: bearer
  token: ${token}
websocket:
  subprotocols:
  - prices.v2
  - prices.v1
  messages:
  - name: subscribe
    content: '{"op": "subscribe", "symbol": "${symbol}"}'
  - name: ping
    content: ping
```

A request with `kind: websocket` opens a WebSocket connection instead of
sending an HTTP request. `url` is a `ws://` or `wss://` URL (`http://` and
`https://` are read as `ws://` and `wss://`); `params`, `headers` and `auth`
work as for HTTP and are sent with the opening handshake, so an API key in
the query, a bearer token or a custom header all reach the server.
`${variables}` are substituted in the URL, headers, auth, subprotocols and
messages, at connect and send time. `method`, `body` and `scripts` are kept
but not used.

| Key | Default | Meaning |
| --- | --- | --- |
| `websocket.subprotocols` | none | Offered in `Sec-WebSocket-Protocol`, in order; the server picks one. |
| `websocket.messages` | none | Saved messages: rows of `name` and `content` (text). Rows keep unknown keys like header rows. |

Unknown keys directly under `websocket` are an error. Collection runs
(`hitop run`) skip WebSocket requests; tools connect them on demand (the
CLI's `hitop ws`). Browsers can't set handshake headers, so a web client
sends only the URL (with an API key in the query) and the subprotocols.

`kind` and `websocket` were added to version 1 after its first release.
They are top-level keys, so readers from before them (hitop 0.3.0 and older)
keep them as unknown keys and write them back, but treat the request as an
HTTP `GET`.

### OpenAPI source (`openapi`)

```yaml
# requests/pets/get-a-pet.yaml
name: Get a pet
method: GET
url: ${baseUrl}/pets/${petId}
examples:
- name: 200 The pet
  status: 200
  body: '{"id": 7}'
  source: openapi
openapi:
  operation: GET /pets/{petId}
  operationId: getPet
  rows:
    headers:
      x-trace: f84ca3ca03237
  synced:
    url: 221e9cd838898
    headers: 1c5a416c2e8c43
```

A request imported from an OpenAPI spec records the operation it came from
under `openapi`, and the collection records the spec in `hitop.yaml`:

```yaml
openapi:
  source: https://api.example.com/openapi.yaml
  version: 1.4.0
  synced:
    baseUrl: 1bcca0eb6e5e4
```

| Key | Meaning |
| --- | --- |
| `openapi.operation` (request) | `METHOD /path` as in the spec. |
| `openapi.operationId` (request) | The operation's `operationId`, when it has one. |
| `openapi.synced` (request) | A hash of each spec-owned field as last imported: `method`, `url`, `params`, `headers`, `body`, `auth`, `examples`. |
| `openapi.rows` (request, since hitop 1.1) | A hash of each header row (`headers`, keyed by the name in lower case) and each query parameter row of `params` (keyed by the key as written) as last imported; the second row of one name is `name#2`. Rows hash `[value, enabled]` (headers) or `value` (parameters). |
| `openapi.source` (`hitop.yaml`) | The spec's URL, when it was imported from one. |
| `openapi.version` (`hitop.yaml`) | The spec's `info.version`. |
| `openapi.flat` (`hitop.yaml`) | `true` when requests were imported without a folder per tag. |
| `openapi.synced` (`hitop.yaml`) | Hashes of `baseUrl` (the collection variable holding the server URL) and of the collection's `auth`. |
| `source: openapi` (saved example) | The example came from the spec; examples without it are the user's. |

A sync with a newer spec matches requests by `operationId`, then by
`operation`. A field whose spec value changed is updated when the request
still has the value the hash was taken from, and kept otherwise (the user
edited it). Names, files, scripts and the user's examples are never the
spec's. The hash is FNV-1a 64 (low 53 bits, lower-case hex) of the field's
JSON in hitop's model, with blank header rows and keys other than name,
value and enabled left out of rows; a tool that can't compute it may leave
`synced` out, and a sync then keeps every field as edited.

Headers and query parameters merge row by row where `openapi.rows` is
present (or the field had no rows): a row the user added, edited or removed
stays as the user has it, and each row the spec added, changed or removed is
applied when the user's row still has the hashed value (a new row goes after
the spec's row before it). A record without `rows` (written before hitop
1.1) keeps an edited `headers` or `params` field whole, as above; the sync
writes `rows`, so the next one merges by row.

`openapi` is a top-level key added to version 1 after its first release.
Readers from before it (hitop 0.3.0 and older) keep it as an unknown key and
write it back; nothing about sending the request depends on it.

### gRPC

```yaml
name: Say hello
kind: grpc
url: ${grpcHost}:50051
headers:
- name: x-tenant
  value: ${tenant}
auth:
  type: bearer
  token: ${token}
grpc:
  service: hitop.test.Greeter
  method: SayHello
  message: |
    {
      "name": "${user}",
      "times": 2
    }
  protos:
  - protos/greeter.proto
  importPaths:
  - protos/include
  plaintext: true
```

A request with `kind: grpc` makes a gRPC call. `url` is the server address:
`host:port` (TLS unless `plaintext` is set), `http://host:port` (always
plaintext) or `https://host:port` (always TLS). The request's enabled
`headers` are sent as gRPC metadata (names are lowercased by HTTP/2; a name
ending in `-bin` takes a base64 value) and `auth` applies as for HTTP (a
bearer token becomes `authorization` metadata; an API key `in: query` is not
sent, gRPC has no query). `${variables}` are substituted in the address,
metadata, auth and message. `method`, `params`, `body` and `scripts` are
kept but not used.

| Key | Default | Meaning |
| --- | --- | --- |
| `grpc.service` | empty | The service's full name, `package.Service`. |
| `grpc.method` | empty | The method's name in the service. Unary and server-streaming methods are called; client-streaming and bidirectional ones are refused. |
| `grpc.message` | empty (`{}`) | The request message as JSON text in the proto3 JSON mapping (a string, usually a block scalar, so `${variables}` work anywhere in it). |
| `grpc.protos` | none | `.proto` files the schema comes from, relative to the collection folder (absolute paths work too). None means the schema comes from the server's reflection service (`grpc.reflection.v1`, then `v1alpha`). |
| `grpc.importPaths` | none | Directories `import`s are looked up in, relative to the collection folder. Each listed file's own directory is always searched first; the well-known `google/protobuf/*.proto` files are built in. |
| `grpc.plaintext` | `false` | HTTP/2 without TLS for a `host:port` address. |

Unknown keys directly under `grpc` are an error. Collection runs
(`hitop run`) skip gRPC requests; tools call them on demand (the CLI's
`hitop grpc`, the desktop app). Browsers can't make gRPC calls (they can't
use HTTP/2 trailers), so the web app keeps gRPC requests but doesn't send them.

`kind: grpc` and `grpc` were added to version 1 after the WebSocket keys.
They are top-level keys, so older readers keep them as unknown keys and
write them back, but treat the request as an HTTP `GET`.

## 4. Auth

The same objects as hitop's JSON storage. Values may contain `${variables}`.

```yaml
auth: {type: none}
auth: {type: bearer, token: ${token}}
auth: {type: basic, username: shop, password: ${password}}
auth: {type: apikey, key: X-API-Key, value: ${apiKey}, in: header}   # or in: query
```

## 5. Environment files: `environments/*.yaml`

```yaml
name: Production
variables:
  base: https://api.example.com
  token:
    secret: true
```

| Key | Default | Meaning |
| --- | --- | --- |
| `name` | the file name | Environment name, as given to `--env`. |
| `id` | the file name | Id used by hitop's JSON storage. |
| `variables` | none | Map of name to value. Environment values override collection variables. |

A variable written as `{secret: true}` has no value in the file, ever.
Its value is resolved at run time, in this order: the
`HITOP_SECRET_<ENV>_<NAME>` environment variable, `--secret NAME=value`, the
OS keychain (service `hitop`, account `<workspace-id>/<env>/<name>`), then an
interactive prompt (TTY only). Writers never put a secret's value in a file.

- `<ENV>` and `<NAME>` in the environment variable are the environment's
  name and the variable's name, upper-cased, with every character other than
  an ASCII letter or digit replaced by `_`: environment `my-env`, variable
  `api.key` → `HITOP_SECRET_MY_ENV_API_KEY`.
- `<env>` in the keychain account is the environment's name as written.
- `<workspace-id>` is the `workspace` key of `hitop.yaml`, or the
  collection's `id` in decimal when there is none. A team that shares the
  folder in git shares the workspace id, so each member keeps their own
  values under the same accounts. Collections in a tool's own storage rather
  than a folder (hitop's `~/.config/hitop`) use the fixed id `global`.
- Without a reachable keychain (headless Linux without a Secret Service, or
  `HITOP_KEYCHAIN=off`) the keychain step is skipped with a note; a secret
  with no value from any step is an error that names each way to give it.
- Tools show a resolved secret as `••••` wherever its value would appear in
  output, history, logs or reports, and never write it to an export. HITOP
  JSON exports keep only the names, as `secrets`.

## 5a. Globals: `globals.yaml`

```yaml
variables:
  region: eu
  sharedKey:
    secret: true
```

An optional file at the folder's root, with the `variables` of an
environment file (section 5) and nothing else (no `name` or `id`). Globals
are the lowest scope. Variables resolve in this order, later ones win:
globals, collection variables (`hitop.yaml`), the environment, a data row,
values given for the run (`--var`), values a script sets, then secrets
(section 5). A secret global's value sits at the globals level, so an
environment's value of the same name still wins over it.

- Secret globals resolve like environment secrets with the environment name
  `global`: environment variable `HITOP_SECRET_GLOBAL_<NAME>`, then
  `--secret NAME=value`, then the OS keychain (account
  `<workspace-id>/global/<name>`), then a prompt. An environment named
  `global` shares these names.
- A folder's globals apply to that folder only. hitop's own storage keeps
  one set of globals (`globals.json`, the same shape as an environment in
  the JSON model) for every collection stored there.
- Postman globals import into the globals. In Postman scripts,
  `pm.globals` reads and writes this scope; a global a script sets lasts for
  the run and shows through `pm.variables` unless another scope has the name.
- Writers that don't handle globals leave an existing `globals.yaml` alone.

## 5b. Secret manager references

A string that is filled in with variables (URL, header names and values, body, auth fields,
GraphQL query, variables and operation name, form fields, WebSocket and gRPC fields) and the value
of any collection, environment or global variable may hold a reference to a secret in a secret
manager. A reference is never resolved inside a script or a saved example.

| Reference | Reads |
| --- | --- |
| `${secret:vault:<mount>/<path>[#<field>]}` | HashiCorp Vault or OpenBao, KV v2 or v1. On KV v2, `data/` after the mount may be left out. |
| `${secret:aws:<name or ARN>[#<jsonKey>]}` | AWS Secrets Manager `SecretString`; `#jsonKey` picks a key of a JSON secret. |
| `${secret:azure:<vault>/<name>[/<version>]}` | Azure Key Vault. A vault name with a dot is a host name (`myvault.vault.azure.cn`). |
| `${secret:op:<vault>/<item>/[<section>/]<field>}` | 1Password, as `op read op://<vault>/<item>/...` reads it; `${secret:op://...}` works too. |

- The text between `${` and `}` (`secret:vault:kv/app#token`) is the reference's name. A tool
  resolves each reference at run time, before it fills in variables, and from then on treats the
  name as a secret variable with that value: it fills `${secret:...}` wherever it appears, also
  inside other variables' values (`token: ${secret:vault:kv/app#token}` in an environment), and
  shows the value as `••••` wherever a secret would be masked (section 5). The value is never
  written to a file, history, report or export; files keep the reference.
- Without `#field`, a Vault secret with one field gives that field; with several it is an error
  that lists the field names. A non-string JSON value is given as JSON text.
- A reference that can't be resolved is an error that names the reference and the reason, never
  a value. A tool may refuse to resolve references (hitop: `--no-secret-managers`,
  `HITOP_SECRET_MANAGERS=off`); a request that uses one is then an error.
- How a tool reaches each manager is the tool's concern. hitop uses `VAULT_ADDR`, `VAULT_TOKEN` (or
  `~/.vault-token`), `VAULT_NAMESPACE`; the AWS credential chain (env vars, `~/.aws` profiles,
  `credential_process`, the AWS CLI for SSO) and region; `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`,
  `AZURE_CLIENT_SECRET` or the `az` CLI; and the `op` CLI. A web app in a browser can't, and says so.

## 6. Ids and the JSON model

hitop's JSON storage (`collections.json`, the web app's localStorage, HITOP
JSON exports) and a collection folder convert into each other without loss:
reading the folder written from a collection gives the same collection back,
and writing the collection read from a folder gives the same files. Fields
that only exist in one place are carried in JSON as:

| JSON key | Folder |
| --- | --- |
| `bodyMode` (`multipart`, `urlencoded`; `raw` is the default) and `formFields` | `body.type` and `body.fields` |
| `disabledParams` | `params` rows with `enabled: false` |
| `bodyType` | `body.type` of a text body, kept only when it differs from the inferred type |
| `graphql` (`{variables, operationName}`; the query is `body`, `bodyType` is `graphql`) | `body.variables` and `body.operationName` |
| `kind` (`websocket`, `grpc`; `http` and none mean HTTP) and `websocket` (`{subprotocols, messages: [{name, content}]}`) | `kind` and `websocket` |
| `grpc` (`{service, method, message, protos, importPaths, plaintext}`) | `grpc` |
| `file` | the request's path, kept only when it differs from the default for its name |
| `secrets` (environment) | variables written as `{secret: true}` |
| the globals (`globals.json`, or `globals` beside `collection` and `environments`) | `globals.yaml` (section 5a) |
| any other key | the same key in the file |

Files without `id` get one derived from their path, so the same folder always
gives the same ids: FNV-1a 64 of `requests/<path>` for a request and of
`collection/<name>` for the collection, keeping the low 53 bits. Writers leave
`id` out when it equals the derived one.

Two normalizations are part of the format: blank header rows (empty name and
value, the editor's placeholder) are not written, and `bodyMode: "raw"` reads
as no body mode. Environments are read in file name order.

Ids in JSON are whole numbers. Older web app data may hold string ids
(`"col-legacy"`); readers take a string of digits as that number and any
other string as FNV-1a 64 of `legacy-id/<string>` (low 53 bits), and write
the number back.

## 7. Stability and compatibility

Version 1 is stable. What a v1 folder means today it means in every later
hitop, and a v1 reader keeps working on folders written by later 1.x tools
as far as the rules below allow.

- **Additive only.** Within version 1 a key, value or row field is never
  removed or renamed, never gets another meaning, type or default, and an
  optional key never becomes required. New things are optional keys (or new
  values of an existing key, such as another `kind`), and a file that doesn't
  use them reads exactly as before.
- **Unknown keys.** A reader keeps keys it doesn't know where section 1 says
  so (`hitop.yaml`, request files, environment files, header, disabled
  parameter, form field and WebSocket message rows) and writes them back
  after the known ones, sorted by name. Inside `body`, `scripts`,
  `websocket` and `grpc`, and on enabled query parameters, an unknown key is
  an error: a reader refuses the file rather than send a request it only
  half understood. So a key added at one of those places (as `body.variables`
  was) makes older readers refuse the files that use it, never misread them.
  An `auth` object with a `type` the reader doesn't know is an error too;
  other unknown keys inside `auth` are ignored and not written back, so new
  auth settings come as a new `type`. Additions go at the top level of a
  file where they can.
- **Breaking changes** need `format: 2`. A reader refuses a `format` higher
  than it knows (`format 2 needs a newer hitop`), and a reader of a later
  version keeps reading version 1.
- **Compatibility suite.** `spec/compat/v1/` holds v1 folders (HTTP with
  every body and auth type, GraphQL, WebSocket, gRPC, an OpenAPI import,
  environments, globals, secret variables and secret manager references,
  saved examples, unknown keys, and input no writer produces: `.yml`, flow
  style, numbers and booleans as strings, hidden and non-YAML files) with
  the JSON model each reads as (`expected.json`, section 6). Every reader
  must read every case to exactly that model. Cases are only added, never
  changed. hitop runs them in `cli/crates/hitop-core/tests/compat.rs`, which
  also writes each case back and reads it again, and checks that the writer
  reproduces the files of every case that isn't marked `"canonical": false`
  in its `case.json`.
