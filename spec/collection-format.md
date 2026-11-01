<!--
hitop collection format, version 1
Copyright 2026 Bohdan Tsap
This specification is licensed under the Creative Commons Attribution 4.0
International License (CC BY 4.0): https://creativecommons.org/licenses/by/4.0/
You may share and adapt it, including commercially, if you give credit.
The hitop code that implements it is MIT licensed.
-->

# hitop collection format, version 1

Status: draft (frozen as stable in T19, v1.0). Reference implementation:
`cli/crates/hitop-core/src/format/`.

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
  (`a.yaml` and `a.yml`) are an error.
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
| `method` | `GET` | HTTP method, as written. |
| `url` | empty | URL; may contain `${variables}`. |
| `params` | none | Query parameters: rows of `name`, `value`, `enabled`. Enabled rows are appended to `url` in order; disabled rows are kept but not sent. |
| `headers` | none | Rows of `name`, `value`, `enabled` (default `true`). Disabled rows are not sent. |
| `auth` | none | Auth preset (section 4). `{type: none}` turns off the collection's default auth. |
| `body` | none | `type` and `content` or `fields`, see below. |
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
| `json`, `text`, `xml`, `graphql` | `content`: the body text | the text, after `${variable}` substitution |
| `form` | `fields`: rows | `application/x-www-form-urlencoded` |
| `multipart` | `fields`: rows; a row with `type: file` is a file | `multipart/form-data` |

Form rows have `name`, `value`, `type` (`text`, the default, or `file`) and
`enabled`. A form body may also carry `content` (text kept from before the
body type was switched), and a text body may carry `fields`; neither is sent.

Without a `body` key the request has no body. For text bodies the `type` is
a label: hitop infers it from the `Content-Type` header (`json`, `xml`, other
types are `text`) or, without one, from the content (valid JSON object or
array → `json`, starts with `<` → `xml`, else `text`). GraphQL bodies are sent
as text until GraphQL support lands (T13).

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
