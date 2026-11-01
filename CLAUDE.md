# HITOP — Claude Project Context

## What this project is

HITOP is an HTTP API testing tool available in four forms:
- **CLI** (`cli/`) — Rust binary published to crates.io as `hitop` + `hitop-core`
- **Desktop** (`desktop/`) — Tauri 2.0 native app (macOS, Windows, Linux)
- **Firefox Extension** (`extension/`)
- **Chrome Extension** (`chrome-extension/`, `chrome-store-package/`)

The React frontend (`frontend/`) is shared between the web/extension builds.

---

## Repo structure

```
hitop/
├── cli/                        # Rust workspace
│   └── crates/
│       ├── hitop-core/         # Library crate (published to crates.io)
│       │   └── src/
│       │       ├── model.rs        # Collection, Request, Header, HistoryItem structs
│       │       ├── storage.rs      # Load/save collections, history, contexts
│       │       ├── http.rs         # reqwest blocking client (rustls, no OpenSSL); execute_opts/send_with: timeout + cookie jar
│       │       ├── cookies.rs      # cookie jar per workspace (<config>/hitop/cookies/<ws>.json), web app rules and JSON shape
│       │       ├── form.rs         # urlencoded and multipart bodies; saved form requests (bodyMode) as sent
│       │       ├── snippet.rs      # code snippets (curl, fetch, python, go, java, rust): secrets/missing vars as {{NAME}} (no native deps)
│       │       ├── websocket.rs    # kind websocket: target()/message() fill in variables (no native deps); connect() tokio-tungstenite + rustls (feature `websocket`)
│       │       ├── grpc.rs         # kind grpc: target() fills in variables, headers+auth → metadata (no native deps); connect/schema/services/describe/template/call: tonic + prost-reflect + protox (feature `grpc`)
│       │       ├── graphql.rs      # GraphQL bodies: bodyType graphql + `graphql` {variables, operationName}, request_body() (no native deps)
│       │       ├── telemetry.rs    # opt-in weekly usage ping, feature `telemetry` (telemetry.json per surface)
│       │       ├── interpolation.rs # ${variable} substitution
│       │       ├── scripting.rs    # Pre/post request JS via boa_engine; loads the pm.* shim (PM_SHIM) for Postman scripts
│       │       ├── runner.rs       # Sequential collection runner (RunOptions: bail, timeout, variable overrides)
│       │       ├── report.rs       # run reports: JSON (spec/run-report.md, reportVersion 1), JUnit XML and to_html (one page, no script, CSP loads nothing), no deps
│       │       ├── export.rs       # HITOP and Postman v2.1 import/export
│       │       ├── postman.rs      # Postman import: collections (folders, auth, scripts), environments, globals, ImportReport
│       │       ├── openapi/        # OpenAPI 3.0/3.1 import (refs.rs: parse + local $ref, example.rs: schema → example) and sync.rs (three-way per field)
│       │       ├── secrets.rs      # secret variables: resolve order, keychain (keyring), Masker (••••)
│       │       ├── secret_refs.rs  # ${secret:vault|aws|azure|op:...} references: find, parse, expand (no deps, also in hitop-wasm)
│       │       ├── secret_managers.rs # feature `secret-managers`: Resolver for Vault/OpenBao KV, AWS (SigV4 on ring), Azure Key Vault, `op read`
│       │       └── format/         # collection folders (YAML, spec/collection-format.md) ⇄ the JSON model; files.rs: in memory (path → text)
│       │   ├── pm-shim.js          # byte-for-byte copy of shared/pm-shim.js (packaged; a test checks they match)
│       │   ├── tests/grpc.rs       # gRPC client against tests/support/grpc_server.rs (tonic, reflection v1/v1alpha/none; fixtures/grpc/*.proto)
│       │   ├── tests/format.rs     # golden files (tests/golden/, UPDATE_GOLDEN=1 regenerates) + proptest round trip
│       │   └── tests/report_html.rs # golden report.html from tests/golden/report/report.json (UPDATE_GOLDEN=1), escaping of every field
│       ├── hitop-wasm/         # format + model for the web app (wasm-bindgen =0.2.126), own workspace, not published
│       ├── hitop-git/          # desktop Git panel: git2 0.21 + vendored libgit2 (feature `net`: https/ssh, vendored OpenSSL), not published;
│       │                       # json.rs = the one entry point (desktop `git` command, examples/git-json for the e2e mock); tests on temp repos
│       └── hitop/              # Binary crate (published to crates.io)
│           ├── tests/run.rs    # hitop run: JUnit shape (roxmltree), JSON and HTML reports, exit codes, flags, masking
│           ├── tests/schedule.rs # hitop schedule: exact workflow text, actionlint (HITOP_ACTIONLINT or PATH; required when CI is set), crontab line, exit codes
│           ├── tests/postman.rs # hitop import --to of the fixtures/postman collections, run against httpmock
│           ├── tests/openapi.rs # hitop import openapi (file, URL via httpmock), run, openapi sync keeping edits, --prune
│           ├── tests/cookies_forms.rs # cookie jar across send/run, -F uploads, saved form bodies
│           ├── tests/snippet.rs # hitop snippet: curl, fetch, Python, Java, Go output run against a capture server = what requests send sends
│           ├── tests/ws.rs     # hitop ws against a local tokio-tungstenite echo server: flags, stdin, --count/--wait/--json, saved requests, masking
│           ├── tests/grpc.rs   # hitop grpc against hitop-core's test server (included by #[path]): calls, streaming, status, list/describe, --proto, saved requests
│           ├── tests/graphql.rs # send --graphql, saved graphql requests via run/requests send, Postman graphql import (httpmock)
│           ├── tests/globals.rs # globals below collection/env, secret globals, pm.globals, import, convert
│           ├── tests/mcp.rs    # hitop mcp: JSON-RPC over stdio for every tool (httpmock), no secret value anywhere on stdout
│           ├── tests/secret_managers.rs # ${secret:...}: stub op/az on PATH + Key Vault mock; Vault dev and LocalStack (HITOP_TEST_VAULT_ADDR, HITOP_TEST_LOCALSTACK_URL; required on CI)
│           ├── tests/telemetry.rs # nothing sent while off, at most once a week (HITOP_TELEMETRY_URL → httpmock)
│           ├── tests/launch_gate.rs # T11 scenario: Postman import, two environments, login token via pm.environment.set, JUnit
│           └── src/
│               ├── main.rs         # clap CLI entry point
│               ├── commands/source.rs # a collection argument: global name or folder path
│               ├── commands/folder.rs # `hitop init`, `hitop convert`
│               ├── commands/secret.rs # `hitop secret set|get|rm|ls|check`, the `--secret` and `--no-secret-managers` flags, add_refs/fill_refs
│               ├── commands/run.rs # `hitop run`: reporters (cli, junit, json, html), iterations, --data, exit codes 0/1/2
│               ├── commands/schedule.rs # `hitop schedule <folder> --github | --cron`: workflow file or crontab line; cron validation
│               ├── commands/data.rs # `--data` CSV (RFC 4180, hand written) and JSON rows
│               ├── commands/cookies.rs # `hitop cookies ls|clear`, `--no-cookies`
│               ├── commands/globals.rs # `hitop globals show|set|unset|import|export [-c folder]`
│               ├── commands/openapi.rs # `hitop import openapi <file|url>`, `hitop openapi sync <file|url> <folder> [--dry-run] [--prune]`
│               ├── commands/ws.rs  # `hitop ws <url|request> [-c]`: stdin or --send/--message, --count, --wait, --timeout, --json
│               ├── commands/grpc.rs # `hitop grpc <target> <Service/Method> | list [service] | describe <symbol>`, saved requests with -c
│               ├── commands/mcp.rs # `hitop mcp [folder]`: MCP server over stdio (rmcp), the nine tools, masking
│               ├── commands/telemetry.rs # `hitop telemetry on|off|status` and the background ping (feature `telemetry`)
│               └── output.rs       # Colored output, syntect highlighting
│   ├── fixtures/sample/        # collection folder run by tests/folder.rs against httpmock
│   ├── fixtures/postman/       # Conduit (MIT), newman sample (Apache-2.0), Adyen BIN lookup (MIT), vendored with licenses
│   ├── fixtures/openapi/       # swagger-petstore 3.0 (Apache-2.0), Adyen Checkout v71 3.1 (MIT), Kubernetes apps/v1 3.0 (Apache-2.0); README with sources
│   └── fixtures/launch-gate/   # Shop API Postman collection + Staging/Local envs, mock_server.py, check_junit.py (CI job launch-gate)
├── desktop/                    # Tauri 2.0 desktop app
│   ├── src/                    # React frontend (JSX, Vite 6)
│   │   ├── App.jsx             # Main component, async loads data from Rust on mount
│   │   ├── components/         # UI components (all .jsx)
│   │   └── services/           # Service layer — all invoke() Tauri commands
│   │       ├── storageService.jsx
│   │       ├── httpService.jsx
│   │       └── scriptExecutionService.jsx
│   └── src-tauri/
│       ├── src/
│       │   ├── commands.rs     # All Tauri commands wrapping hitop-core
│       │   ├── git.rs          # git {cmd,args} over hitop-git, git_token_status|set|delete (keychain git/<host>/token)
│       │   └── lib.rs          # Command registration
│       ├── Cargo.toml          # depends on hitop-core via path
│       ├── tauri.conf.json     # Bundle ID: tech.rws8.hitop, category: Developer Tools; React frontend (../dist)
│       ├── tauri.webapp.conf.json # the switch: frontend = webapp/dist-tauri (npm run build:webapp / dev:webapp)
│       ├── Entitlements.mas.plist          # Mac App Store entitlements
│       └── Entitlements.mas.inherit.plist  # MAS child process entitlements
├── webapp/                     # hitop.rws8.tech/app/: Lit + lit-material, reuses frontend/src/services
│   ├── src/store.js            # all app logic (port of frontend/src/App.jsx), same localStorage keys; folders, zip
│   ├── src/platform/           # adapters: web.js (fetch/proxy, localStorage, File System Access), tauri.js (invoke), history.js;
│   │                           # tauri-settings.js: desktop-only Settings page + first-run usage ping question
│   │                           # tauri-git.js: desktop-only Git page for folder collections (status, diff, commit, pull/push, branches, conflicts)
│   ├── scripts/check-no-telemetry.mjs # CI: dist/ and web.js have no usage ping (control: dist-tauri has the page)
│   ├── src/pm.js               # Postman scripts: loads shared/pm-shim.js, runScript, import report helpers
│   ├── src/views/openapi-dialogs.js # OpenAPI import (URL or paste), its report, Sync with OpenAPI (diff preview, apply)
│   ├── src/wasm/format.js      # hitop-wasm wrapper; src/wasm/pkg/ is built by scripts/build-wasm.mjs (gitignored)
│   ├── src/diff.js             # response diff: JSON by path (key order ignored), lines (LCS), headers; history response snapshots
│   ├── src/views/tools-dialogs.js # Code dialog (hitop-wasm snippet) and Compare responses dialog
│   ├── src/graphql-schema.js   # graphql-js: introspection query, schema from the answer (lazy)
│   ├── src/views/grpc-view.js # gRPC tab: server, Schema (reflection/.proto), Metadata, Message (pickers, template), call log (store.grpcTabs)
│   ├── src/views/websocket-view.js # WebSocket tab: connect, saved/typed messages, timestamped log (store.sockets, memory only)
│   ├── src/views/code-editor-graphql.js # cm6-graphql: GraphQL language, completion, schema lint (own lazy chunk)
│   ├── src/zip.js              # collection folder ⇄ .zip (fflate)
│   ├── tests/harness.cjs       # e2e in web or Tauri mode (HITOP_MODE=tauri); tests/tauri-mock.cjs mocks invoke in Node
│   └── tests/e2e.cjs           # Playwright e2e, seeded with React-app storage (legacy-storage.json)
├── frontend/                   # React app, now only for the browser extensions
│   ├── index.html              # Vite entry point (root of frontend/)
│   ├── vite.config.js          # Vite 6, outDir: build, multi-entry (main + sandbox)
│   └── src/                    # All files are .jsx (not .js)
├── extension/                  # Firefox extension source
├── chrome-extension/           # Chrome extension source
├── site/                       # hitop.rws8.tech: lit-material + Lit SSR, rws8.tech design system
│   ├── src/pages/              # home, pricing.html, privacy.html, 404, terms/ (guides)
│   ├── content/guides/*.md     # the /terms/*.html guide articles (Markdown + front matter); hitop-vs-*.md comparisons carry a checked date and sources
│   ├── demo/demo.tape          # vhs tape for the terminal GIF (vhs site/demo/demo.tape from the repo root)
│   ├── public-assets/          # favicons, social preview; favicon.svg is also the desktop icon source
│   └── scripts/prerender.js    # npm run build → dist/
├── server/                     # hitop-server: rws app serving the site + POST /api/proxy
│   └── src/guard.rs            # address/port policy for the proxy (SSRF guard)
├── deploy/                     # build.sh, install.sh, systemd unit, front rws routes, README
├── sync-server/                # hitop sync (AGPL-3.0-or-later, own LICENSE/deny.toml): rws app, SQLite or Postgres
│   ├── src/                    # api.rs (routing, CORS), auth.rs (magic link, passkeys), workspaces.rs (members, wraps, rotation), blobs.rs (CAS + version vectors), db.rs
│   ├── crates/hitop-sync-crypto/ # MIT client crypto: HPKE key wrap + XChaCha20-Poly1305 blobs; vectors/v1.json
│   ├── tests/                  # auth, sync, plaintext (server never sees plaintext), no_crypto_in_server, postgres
│   ├── deploy/                 # hitop-sync.service, install.sh, rws-routes.toml (sync.hitop.rws8.tech)
│   └── THREAT_MODEL.md, README.md (self-host guide), Dockerfile
├── ios/
│   ├── HiTop/                  # the live App Store iOS app ("HiTop HTTP Client", editor, own JSON storage); frozen, untouched
│   ├── HitopKit/               # Swift package (MIT): format v1 reader (Yams 6.2.2), variables, secrets, scripts, URLSession runner, report.json
│   └── HitopCompanion/         # SwiftUI companion (T26): XcodeGen project.yml, app + App Intents, WidgetKit widget, Shared/ (app group)
├── spec/collection-format.md   # collection folder format v1 (CC BY 4.0)
├── shared/pm-shim.js           # the pm.* shim for Postman scripts, run by hitop-core (copy in the crate), the web app and HitopKit (copy in Resources/)
├── spec/run-report.md          # `hitop run --reporter json` report, reportVersion 1, and the JUnit shape (CC BY 4.0)
├── ROADMAP.md                  # agent work order to v1.x: decisions, rules, tranches T0–T26, status
├── launch/                     # launch text drafts for the owner (Show HN, Reddit, TWiR, Product Hunt); never posted by agents
├── docs/CNAME                  # only file left: keeps GitHub Pages 301ing bohdaq.github.io/hitop/* → hitop.rws8.tech
└── CLAUDE.md                   # This file
```

---

## Key technical facts

### Rust CLI (`cli/`)
- Workspace version: `0.3.0` (both crates in sync; also `[workspace.dependencies] hitop-core` version)
- Rust toolchain: `stable` via `cli/rust-toolchain.toml` — always use `~/.cargo/bin/cargo`, NOT Homebrew rustc
- Run tests: `cd cli && ~/.cargo/bin/cargo test`; CI also runs `cargo clippy --all-targets -- -D warnings`
- Postman: `hitop import <files>... [--to <dir>] [--force]` (collections, environments, globals detected by content;
  secret values to the keychain; prints the import report). Edit `shared/pm-shim.js`, then copy it to
  `cli/crates/hitop-core/pm-shim.js` (test `shared_shim_is_the_packaged_copy`)
- OpenAPI: `hitop import openapi <file|url> [--to <dir>] [--force] [--flat]` (also `hitop import <spec>` by content; Swagger 2.0 refused);
  `hitop openapi sync <file|url> <folder> [--dry-run] [--prune]`. Core `openapi::import`/`sync` (pure, in hitop-wasm): tags → folders,
  first server → `baseUrl` variable, path params `${name}`, security → auth (bearer, basic, apiKey header/query; OAuth 2 reported),
  response examples `source: openapi`. Requests and `hitop.yaml` carry an `openapi` record (extra key, spec section "OpenAPI source"):
  operation, operationId, per-field FNV hashes as imported; sync updates a field only where it still has that value. Report
  `OpenApiReport {mapped, skipped, warnings}`. JSON maps are sorted (no serde_json preserve_order), so requests come in path order
- Collection folders: `hitop init [dir]`, `hitop convert <json> <dir>` / `<dir> <json>` (`--environments`, `-c`, `--force`);
  `run`, `requests`, `variables`, `export`, `send -c` also take a folder path (`hitop run ./my-api --env local`); a folder uses only its own `environments/`
- Secrets: `hitop secret set|get|rm|ls --env <env> [-c <folder>]`; `--secret NAME=value` on `send`, `run`, `requests send`;
  resolve order `HITOP_SECRET_<ENV>_<NAME>`, `--secret`, OS keychain (`keyring` 4, service `hitop`, account
  `<workspace>/<env>/<name>`), prompt on a TTY. `HITOP_KEYCHAIN=off` skips the keychain (CLI tests set it). Values are
  masked as `••••` in output, history and runner steps (`runner::run_collection_with`); hitop-core feature `keychain` (default)
- Secret managers (spec §5b): `${secret:vault:mount/path#field}`, `${secret:aws:name|ARN#jsonKey}`, `${secret:azure:vault/name[/ver]}`,
  `${secret:op:vault/item/field}` (or `op://`). The inner text is a variable name: `secret::add_refs` resolves the refs a command uses
  (core `secret_refs::find_in_*`, scripts and examples skipped) into `Secrets` (Origin::SecretManager), `fill_refs`/`Secrets::expand_refs`
  fill them, also inside variable values (the runner does it itself). Core `secret_managers::Resolver` (feature `secret-managers`, default;
  `ring` for SigV4, no AWS SDK): per-run cache, 15 s HTTP / 60 s `op`/`az`/`aws` (`HITOP_SECRET_MANAGER_TIMEOUT`), errors name the ref.
  Env: VAULT_ADDR/TOKEN/NAMESPACE (BAO_*, ~/.vault-token), AWS_* + ~/.aws (credential_process, `aws configure export-credentials`),
  AZURE_TENANT_ID/CLIENT_ID/CLIENT_SECRET or `az`; `HITOP_AZURE_VAULT_URL` (`{vault}` template) and `HITOP_{OP,AZ,AWS}_BIN` are test hooks.
  `--no-secret-managers` on every SecretFlags command and `hitop mcp`, `HITOP_SECRET_MANAGERS=off`. `hitop secret check [-c] [--env] [--list]`.
  MCP resolves only refs in the workspace files (not a call's `variables`); snippets keep them as `{{secret:...}}` without `--show-secrets`
- CI runs: `hitop run <folder|name> --env <e> | --env-file <yaml>`, `--var k=v`, `--bail`, `--timeout <ms>`,
  `--iterations <n>` or `--data <csv|json>`, `--reporter cli,junit,json,html --out <dir>` (`junit.xml`, `report.json`, `report.html`);
  exit 0 passed, 1 a request or assertion failed, 2 usage or config error (`main.rs` maps `run`'s Err to 2).
  Default is to run every request; `--no-stop-on-fail` is a hidden no-op. Variable order: collection, environment,
  data row, `--var`, secrets. A new hitop-core runtime dep also changes `desktop/src-tauri/Cargo.lock` (CI checks it `--locked`)
  and `cli/crates/hitop-wasm/Cargo.lock` (its own workspace; `npm run build` builds it `--locked`, so a stale lock breaks every web build)
- Scheduled runs: `hitop schedule <folder> --github [--cron E] [--env E] [--secrets a,b] [--name N] [--force] [--hitop-version V]`
  writes `<git root>/.github/workflows/hitop-<slug>.yml` (`bohdaq/hitop-action@v1`, `schedule:` + `workflow_dispatch:`, secrets as
  `HITOP_SECRET_<ENV>_<NAME>: ${{ secrets.<same> }}`, default = the env's + globals.yaml's secret names; html artifact only for
  versions > 0.3.0); `--cron` alone prints a crontab line (`current_exe`, `%` escaped, `--log-dir`). Errors exit 2. CI (cli job)
  installs actionlint 1.7.12 (sha256-pinned) as `HITOP_ACTIONLINT` and lints generated workflows
- Globals: lowest scope (globals < collection < environment < data row < `--var` < script-set < secrets);
  global storage `globals.json`, a folder's `globals.yaml` (spec 5a); `hitop globals ...`, `hitop secret ... --globals`
  (`HITOP_SECRET_GLOBAL_<NAME>`, keychain `<ws>/global/<name>`); Postman globals import into them;
  core `prepare::Scopes` + `prepare_in`, `RunOptions.globals`; scripts get `globals` and return `globals` updates
- GraphQL: `bodyType: graphql` (no bodyMode) = query in `body`, `graphql: {variables, operationName}` (folder: `body.variables`,
  `body.operationName`, spec §3). `prepare::Draft.graphql` (`Draft::of(request, collection_auth)`) builds the JSON body after
  `${var}` substitution, adds `Content-Type: application/json` unless set. `hitop send --graphql <file|-> [--graphql-vars JSON|@file]
  [--operation-name N]`. Postman graphql bodies import/export as GraphQL
- WebSocket: a request with `kind: websocket` + `websocket: {subprotocols, messages: [{name, content}]}` (spec §3 "WebSocket";
  URL, params, headers, auth are the request's own). Core `websocket::target(request, collection_auth, vars)` / `message()`, then
  `websocket::connect(&target, timeout)` (async, needs a tokio runtime) → `Connection {opened, sender, events}`; feature `websocket`
  (default; tokio-tungstenite 0.30, rustls with ring + webpki-roots; no aws-lc). `hitop ws <url>` or `hitop ws <request> -c <col>`:
  `-H`, `--protocol`, auth flags, `-m/--message <saved>` then `-s/--send <text>`, else stdin lines; `-n/--count`, `--wait <ms>`
  (default 1000, 30000 with --count), `--timeout`, `--json` (JSON lines); exit 1 on failure or count not reached. `hitop run`,
  `snippet` and MCP `run_request` skip or refuse WebSocket requests; MCP has no WebSocket tools (open)
- gRPC: `kind: grpc` + `grpc: {service, method, message (JSON text), protos, importPaths, plaintext}` (spec §3 "gRPC"; the URL is the
  address `host:port` (TLS unless plaintext) / `http://` / `https://`; headers and auth are the metadata). Core `grpc::target(request,
  collection_auth, vars, base_dir)` (no native deps), then feature `grpc` (default; tonic 0.14.6 pinned in the locks, tls-ring +
  webpki roots; prost-reflect 0.16 dynamic messages; protox 0.10 parses .proto at run time, no protoc): `connect`, `schema` (protos
  or reflection v1 → v1alpha, missing deps fetched by file name), `services`, `describe`, `template`, `call` (unary/server streaming
  via one code path → `Event::{Headers, Message, Status}`; client/bidi streaming refused). Not in hitop-wasm. `hitop grpc <target>
  <Service/Method> [-d json|@file|-] [-H] [--proto f]… [-I dir]… [--plaintext] [--max-time] [--json] [-v]`, `list [service]`,
  `describe <symbol>`, saved with `-c` (protos relative to the folder); exit 1 on non-OK. `run` skips, `requests send`/`snippet`/MCP
  `run_request` refuse; MCP `get_request` shows `grpc`. Test server `hitop-core/tests/support/grpc_server.rs` (dynamic Greeter service)
- Snippets: `hitop snippet <collection> <request> --lang curl|fetch|python|go|java|rust [-e env] [--show-secrets]`; core
  `snippet::generate(lang, request, collection_auth, variables, secrets)`; secret names and variables with no value become
  `{{NAME}}` (the CLI never resolves secrets without `--show-secrets`); one value per header name, as hitop sends; golden files
  `hitop-core/tests/golden/snippets/<case>/<lang>.<ext>` (cases.json, `UPDATE_GOLDEN=1`; syntax-checked with bash/node/python3/javac/gofmt when installed)
- Cookies: `send`, `run`, `requests send` keep a jar per workspace (`global` without `-c`, shared with the desktop);
  `hitop cookies ls|clear [-c]`, `--no-cookies`. Forms: `send -F name=value -F file=@path[;type=mime]`, `--urlencoded`;
  `run`/`requests send` send saved `bodyMode` urlencoded/multipart bodies (`form::outgoing`)
- Usage ping: hitop-core feature `telemetry` (off by default; the CLI crate's default feature turns it on);
  `hitop telemetry on|off|status`; never sent unless on, at most weekly; `HITOP_TELEMETRY_URL` points tests at a mock
- MCP: `hitop mcp [folder]` (rmcp 3.8 + tokio, binary crate only): stdio server, workspace = folder or global storage;
  tools list_collections, list_requests, get_request, run_request, run_collection, create_request, update_request,
  list_environments, set_environment (names and descriptions are a contract: change additively). Runs go through
  `runner::run_collection_jar` (precedence and cookie jar as `hitop run`), tool bodies run in `spawn_blocking` behind one
  mutex; every secret value resolved in the session is masked in all output; create/update refuse secret values.
  stdout is the protocol: nothing under `commands/mcp.rs` may print to it. Guide: `site/content/guides/use-hitop-with-claude-code-and-cursor.md`
- YAML crate: `yaml_serde` (serde_yaml continued by the YAML org); HTTP tests use `httpmock` 0.8, never the public internet
- Publish: `cd cli && cargo publish -p hitop-core` then `cargo publish -p hitop`
- HTTP client: `reqwest 0.12` blocking + rustls (no OpenSSL)
- JS engine: `boa_engine 0.22` (pure Rust, used for pre/post request scripts)
- Storage paths: `~/Library/Application Support/hitop/` (macOS), `~/.config/hitop/` (Linux)
- `MAX_HISTORY = 50` in `storage.rs`

### GitHub Action and images
- `ghcr.io/bohdaq/hitop:<version>` is distroless static, nonroot, entrypoint `hitop`, workdir `/work`;
  `<version>-debug` (`cli/docker/Dockerfile.debug`, busybox shell, root) is for GitLab CI (`entrypoint: [""]`).
  Both pushed by `release-cli.yml`; `latest`/`debug` only on non-rc tags. Newest release: `0.3.0`
- `bohdaq/hitop-action` (separate repo, MIT, tags `v1` → `v1.0.1`, `v1.0.0`): composite action, `run.sh` (docker run
  `--network host --user $(id -u)`; a tag not on ghcr.io but already on the runner is used as is, since v1.0.1), `summary.js` (job summary from report.json v1, sets the outcome from the
  exit code), self-test with a local Python mock server. Move `v1` to each compatible release
- Guides: `site/content/guides/run-hitop-in-github-actions.md`, `run-hitop-in-gitlab-ci.md`
- CI job `launch-gate` (ci.yml): musl build → `ghcr.io/bohdaq/hitop:launch-gate` on the runner → `hitop import` in the
  container → `bohdaq/hitop-action@v1` per environment against `cli/fixtures/launch-gate/mock_server.py` → `check_junit.py`
- Reports: JUnit `<testcase file=...>` and report.json `file` (folder as given + `requests/<file>.yaml`, null for global storage)

### Desktop (`desktop/`)
- Framework: Tauri 2.0, frontend: React 19 + MUI v7, build: Vite 6
- Bundle ID: `tech.rws8.hitop` (tauri.conf.json `identifier`)
- Two frontends until the owner's macOS check (ROADMAP T5): React `desktop/src` (default config, what
  `release-desktop.yml` ships) and the Lit web app via `--config src-tauri/tauri.webapp.conf.json`
  (`npm run build:webapp`, `dev:webapp`; CI checks it with `TAURI_CONFIG="$(cat tauri.webapp.conf.json)" cargo check`).
  WebSocket: `ws_connect {args: {url, headers, protocols, timeoutMs}, onEvent: Channel}` → `{id, status, protocol, headers}`, events
  `{type: message|closed|error}` (binary as base64) on the channel; `ws_send {id, data, binary?}`, `ws_close {id}` (state `Sockets`).
  gRPC: `grpc_list {args: {address, plaintext, metadata, protos, importPaths, baseDir, timeoutMs}}` → `{source, services}`,
  `grpc_describe {args, symbol}` → `{description, template}`, `grpc_call {args: {...server, service, method, message, maxTimeMs},
  onEvent: Channel}` → `{id}`, events `{type: headers|message|status}`; `grpc_cancel {id}` (state `GrpcCalls`).
  Web app commands: `send_request` (proxy envelope; `cookies: true` uses hitop-core's `global` jar and returns it),
  `cookies_load|save`, `load_globals|save_globals`, `read_folder`, `write_folder` (optional `globals`), `secret_get|set|delete`, `secret_refs_resolve {refs}` (inner texts → values; errors one line per reference), `write_file`, `telemetry_status|set`; dialog plugin.
  Script commands return script-set `variables`; `run_post_script` takes the sent `request` (pm.request)
  Git (`src/git.rs`): `git {cmd, args: {path, ...}}` → `hitop_git::json::call` (status, diff, stage, unstage, discard, commit,
  setIdentity, branches, createBranch, switchBranch, deleteBranch, mergeBranch, conflict, resolve, abortMerge, remoteHost,
  fetch, pull, push); fetch/pull/push read the token from the keychain (`git/<host>/token`); `git_token_status|set|delete {path}`.
  Pull = fetch + fast-forward or merge, never rebase. MAS sandbox: no SSH agent, credential helpers or ~/.gitconfig (HTTPS token, identity in the panel)
- Cargo feature `telemetry` (not default): the usage ping. `release-desktop.yml` builds with `--features telemetry`;
  the MAS build (plain `tauri build`) must not; CI builds both and greps the binary for the endpoint
- Dev: `cd desktop && npx tauri dev`
- Build: `cd desktop && PATH="$HOME/.cargo/bin:$PATH" RUSTC="$HOME/.cargo/bin/rustc" npx tauri build`
- All Tauri commands are in `src-tauri/src/commands.rs`
- The frontend calls Rust via `invoke()` from `@tauri-apps/api/core`
- MAS entitlements: sandbox + network.client + user-selected files
- Icons generated from `site/public-assets/favicon.svg` via `rsvg-convert` + `npx tauri icon`

### Frontend (`frontend/`)
- Build tool: Vite 6 (migrated from CRA — zero vulnerabilities)
- All source files use `.jsx` extension (not `.js`)
- Build output: `build/` (compatible with extension build pipeline)
- Dev server: `npm start` (port 3000)
- Build: `npm run build`
- Extension build: `npm run build:extension` (copies manifest_extension.json)
- `services/` are shared with `webapp/` (plain JS, no React): change them with both apps in mind
- `services/transportService.jsx`: every request goes through `sendRequest()`; public hosts use
  the proxy when `VITE_HITOP_PROXY` is set (webapp build), otherwise fetch

### Web app (`webapp/`)
- Lit + lit-material in light DOM; theme from `site/src/theme.css`; `npm run build` → `dist/` (base `/app/`)
- Storage must stay readable by the React app (same keys/shapes); `npm run test:e2e` with BASE= a running hitop-server
- GraphQL body: tab `bodyMode: "graphql"` ⇄ saved `bodyType: graphql` + `graphql` (store.js `EXTRA_FIELDS`/`SAVED_FIELDS`); the body
  is built by hitop-wasm `graphqlBody` (same bytes as the CLI); `store.fetchSchema()` introspects through `dispatch` (proxy/Rust),
  schemas live in memory per tab (`store.schemas`). Editor deps: cm6-graphql, graphql, @codemirror/autocomplete (MIT)
- WebSocket: method select "WebSocket" sets tab `kind: "websocket"` (tab `subprotocols`, `wsMessages`, `wsDraft`; saved as `kind` +
  `websocket` via `EXTRA_FIELDS`/`SAVED_FIELDS`); `store.wsConnect/wsSend/wsDisconnect`, log in `store.sockets` (never saved).
  Adapter `websocket(url, {headers, protocols}, handlers)`: web.js = browser WebSocket (no handshake headers; the page says so),
  tauri.js = Rust `ws_connect` over a Tauri channel (harness `transformCallback`/`runCallback`, mock `mock.deliver`). Runs skip them.
  e2e `tests/websocket.cjs` with a hand-written Node echo server
- gRPC: method select "gRPC" sets tab `kind: "grpc"` (tab `grpcService`, `grpcMethod`, `grpcMessage`, `grpcProtos`, `grpcImportPaths`,
  `grpcPlaintext`; saved as `kind` + `grpc`); `store.grpcLoadSchema/grpcTemplate/grpcCall/grpcCancel`, state in `store.grpcTabs`
  (never saved). Adapter `grpc`: web.js = null (the page says "gRPC is available in the desktop app and CLI", Send off), tauri.js =
  Rust `grpc_*` over a channel. Tab base dir for .proto paths = the collection's desktop folder path. Runs skip them. e2e
  `tests/grpc.cjs` (Tauri mode: canned Greeter in tauri-mock.cjs)
- Response diff and Code: history items keep `response` {status, headers, body ≤ 100 kB, truncated?, durationMs} (secrets masked;
  older items have none and aren't offered); "Save as example" appends {name, status, headers, body} to the saved request's
  `examples`; Compare picks current / examples / history (`store.compareSources()`); Code uses hitop-wasm `snippet` (`store.snippetInput()`)
- Postman collections import through hitop-wasm `importPostman` (same importer and report as the CLI); scripts that
  mention pm/postman/tests/... run with the shim; script-set variables live in `store.scriptVariables` until reload,
  script-set globals in `store.scriptGlobals`; the Globals section is on the Environments page (`store.globals`)
- OpenAPI: the Import dialog takes a pasted spec (hitop-wasm `isOpenapi`) and "Load an OpenAPI spec from a URL" (`store.fetchSpec`
  through `platform().send`: proxy on the web, Rust on the desktop); it adds a collection (never replaces), `inlineAuth: true`
  (no collection auth in the web app). "Sync with OpenAPI" on a collection with an `openapi` record: `previewOpenapiSync`
  (request ids go to Rust as positions and come back), then `applyOpenapiSync`; folder collections save through live sync
- Git (desktop only): `platform().git` (null on the web) and the "Git" page from `platform/tauri-git.js`; it flushes folder
  edits before each action (`store.flushFolder`) and re-reads the folder after pull/switch/merge/resolve (`store.reloadFolder`).
  e2e `tests/git.cjs`: the mock's `git` runs `cli/target/debug/examples/git-json` (build: `cargo build -p hitop-git --example
  git-json --no-default-features` in `cli/`, or `HITOP_GIT_JSON`) on temp repos with a bare remote
- Secrets: environment `secrets` names, masked editor; `features.js` `exportableEnvironments()`/`maskSecrets()` for exports and run history
- Secret manager references: `features.js` `secretRefs()`/`withSecretRefs()`; `store.resolveRefs()` (in `prepare`, `wsConnect`, `grpcCall`) asks
  `platform().resolveSecretRefs` (tauri.js → `secret_refs_resolve`; web.js `null` → the request isn't sent, the note names them);
  values in `store.refValues` (memory, reset per send/run), merged by `variables()`, masked via `secretValuesFor()`; Code dialog keeps placeholders
- No telemetry in the web build: telemetry UI lives only in `platform/tauri-settings.js` (imported by tauri.js);
  `node scripts/check-no-telemetry.mjs` after both builds checks it. Adapter hooks `pages`/`onStart` add desktop-only pages
- Builds need the wasm32 target and the wasm-bindgen CLI 0.2.126 (`cargo install wasm-bindgen-cli --version 0.2.126 --locked`):
  `npm run build` (web, `/app/`, proxy), `npm run build:tauri` (`dist-tauri/`, no proxy). `npm run test:e2e:tauri` runs every
  e2e suite against `dist-tauri` with a mocked invoke (no server needed); new e2e suites use `tests/harness.cjs` so they run in both modes
- hitop-core feature `native` (default): http, scripting, prepare, runner, report, storage; hitop-wasm builds without it,
  so a new core module using reqwest, boa, dirs or those modules starts with `#![cfg(feature = "native")]`

### iOS companion (`ios/HitopKit`, `ios/HitopCompanion`)
- HitopKit follows hitop-core: same reader rules and errors, ids (FNV-1a), interpolation (one pass), auth, GraphQL and
  form bodies, script preambles + `pm-shim.js`, masking, report.json shape. A change to any of these in the core needs the
  same change in HitopKit; `CLICompatibilityTests` runs one folder through both and diffs the wire requests and reports
- Tests on Linux: `. ~/.local/share/swiftly/env.sh && cd ios/HitopKit && HITOP_BIN=../../cli/target/debug/hitop swift test`
  (needs python3 for `mock_server.py` and node as the script engine; without HITOP_BIN the CLI test is skipped). CI job
  `hitopkit` runs it in `swift:6.4.2-noble` pinned by digest and fails on any skipped test
- YAML: Yams with `Resolver.basic`, plain scalars resolved by the YAML 1.2 core schema in `YAML.swift` (`on`/`yes` stay strings)
- Scripts: JavaScriptCore on Apple platforms (`defaultScriptEngine`); none on Linux (scripts reported as not run, `notes`)
- Secrets: env var, given values, then `SecretStore` (`KeychainSecretStore` service `hitop`, `MemorySecretStore` in tests);
  `${secret:...}` references fail the request (no secret managers on iOS); WebSocket/gRPC left out of runs
- The app (no editing) is generated with `xcodegen` on a Mac; bundle ids `hitop.rws8.tech.HitopCompanion` (+ `.Widget`),
  app group `group.hitop.rws8.tech.companion`; Swift 5 language mode for the app targets, Swift 6 for HitopKit

### Website and proxy
- Hosting: VM bohdan-prod-donotdelete (168.119.17.57) behind the front rws; see `deploy/README.md`
- Build all: `./deploy/build.sh`; server tests: `cd server && cargo test`
- Old URLs must keep working: `/privacy.html` (App Store privacy URL), `/pricing.html`, `/app.html`, `/terms/*.html`
- App Store: macOS "HiTop API Client" id6785294166 ($4.99, macOS 12+), iOS "HiTop HTTP Client" id6785326628 ($2.99, iOS 17+)

### Sync server (`sync-server/`)
- Own Cargo workspace: `hitop-sync-server` (AGPL) + `crates/hitop-sync-crypto` (MIT, dev-dependency of the server only)
- Tests: `cd sync-server && cargo test --workspace`; Postgres: `HITOP_SYNC_TEST_POSTGRES=postgres://... cargo test --features postgres --test postgres`
- Licenses: `cargo deny --config sync-server/deny.toml --manifest-path sync-server/Cargo.toml --workspace check licenses advisories`, and the root `deny.toml` with `--exclude hitop-sync-server` for the MIT crate
- Live at https://sync.hitop.rws8.tech (VM, `hitop-sync.service`, 127.0.0.1:7881, env `/etc/hitop-sync/env`, DB `/var/lib/private/hitop-sync/sync.db`); deploy: copy the release binary + `deploy/` and run `deploy/install.sh`
- `rws --check` on the VM needs `/etc/rws/admin.env` loaded (admin token), or it fails on any config

### Data model
- `Collection` — has `id`, `name`, `requests[]`, `variables` (HashMap, `#[serde(default)]`), optional default `auth`, optional `workspace` (keychain key)
- `Environment` — `secrets` lists secret variable names: CLI values in the keychain, web app values in `variables` (localStorage), never in files or exports
- Globals — an `Environment` with id/name `globals` (`model::GLOBALS_ID`); web app `hitop_globals` `{variables, secrets}`; `format::Folder.globals` (None leaves globals.yaml alone)
- `Request` — has `id`, `name`, `url`, `method`, `headers[]` (`enabled`), `body`, `preRequestScript`, `postRequestScript`,
  `auth`, and the web app's `bodyMode`/`formFields`/`disabledParams`, plus `bodyType`, `graphql`, `kind`/`websocket`/`grpc`, `examples`, `file` (folder path)
- Every model struct keeps unknown JSON keys (`#[serde(flatten)] extra`); new fields are skipped when empty, so old JSON reads and writes unchanged
- ids are `u64`; old web app string ids (`"col-legacy"`) read as a number derived from them (`model.rs` `de_id`)
- `HistoryItem` (history.json, CLI and desktop) keeps extra keys; the desktop web app maps its items onto it (`platform/history.js`)
- Variable interpolation: `${varName}` syntax in URLs, headers, body
- Export formats: HITOP native JSON (array of collections) and Postman v2.1

---

## Toolchain gotcha

The system has two Rust installations:
- Homebrew: `rustc 1.83` — **do not use**, too old for clap 4.6+
- rustup: `rustc 1.96` at `~/.cargo/bin/` — **always use this one**

`~/.zshrc` has `export PATH="$HOME/.cargo/bin:$PATH"` to fix this permanently.
When running cargo from a script or shell that might not source `.zshrc`, prefix with:
```bash
PATH="$HOME/.cargo/bin:$PATH" RUSTC="$HOME/.cargo/bin/rustc" ~/.cargo/bin/cargo ...
```

---

## App Store status

- Both apps are live: macOS "HiTop API Client" and iOS "HiTop HTTP Client" (ids under "Website and proxy")
- Bundle ID `tech.rws8.hitop`; MAS entitlements and icon set in `desktop/src-tauri/`
- Privacy policy: `https://hitop.rws8.tech/privacy.html` (old Pages URL redirects there)
- A desktop update needs a signed MAS build from the owner's Mac (upload via Transporter)

---

## Crates.io

- `hitop-core` and `hitop` 0.3.0, published to crates.io 2026-11-01 (tag `cli-v0.3.0`); 0.2.0 on 2026-10-31
- Desktop 0.2.0 (tauri.conf.json, Cargo.toml, package.json): GitHub Release `desktop-v0.2.0`
- To publish: bump `version` in `cli/Cargo.toml` (workspace), push, then push tag `cli-v<version>`; `.github/workflows/release-cli.yml` publishes core then hitop (needs the `CARGO_REGISTRY_TOKEN` secret)
