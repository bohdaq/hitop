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
│       │       ├── openapi/        # OpenAPI 3.0/3.1 import (refs.rs: parse + local $ref, example.rs: schema → example) and sync.rs (three-way per field; headers/query params row by row)
│       │       ├── secrets.rs      # secret variables: resolve order, keychain (keyring), Masker (••••)
│       │       ├── sending.rs      # what every surface adds when sending (spec/sending.md): raw-body Content-Type, URL scheme (no deps, also in hitop-wasm, mirrored by HitopKit)
│       │       ├── secret_refs.rs  # ${secret:vault|aws|azure|op:...} references: find, parse, expand (no deps, also in hitop-wasm)
│       │       ├── sync.rs         # hitop sync, client side (no deps, also in hitop-wasm): plan() three-way pull/push/conflicts per file, key-by-key merge of hitop.yaml/env/globals, path allowlist; sync/tests.rs
│       │       ├── secret_managers.rs # feature `secret-managers`: Resolver for Vault/OpenBao KV, AWS (SigV4 on ring), Azure Key Vault, `op read`;
│       │       │                      # Settings (secret-managers.json + keychain `secret-managers/<NAME>`), layered under the env (GROUPS)
│       │       └── format/         # collection folders (YAML, spec/collection-format.md) ⇄ the JSON model; files.rs: in memory (path → text)
│       │   ├── pm-shim.js          # byte-for-byte copy of shared/pm-shim.js (packaged; a test checks they match)
│       │   ├── tests/grpc.rs       # gRPC client against tests/support/grpc_server.rs (tonic, reflection v1/v1alpha/none; fixtures/grpc/*.proto)
│       │   ├── tests/format.rs     # golden files (tests/golden/, UPDATE_GOLDEN=1 regenerates) + proptest round trip
│       │   ├── tests/send_vectors.rs # spec/vectors/send.json (also run by webapp unit.mjs and HitopKit SendRulesTests)
│       │   ├── tests/compat.rs     # format v1 compatibility suite (spec/compat/v1/): read, in-memory read, round trip, canonical write; v1 report shape
│       │   └── tests/report_html.rs # golden report.html from tests/golden/report/report.json (UPDATE_GOLDEN=1), escaping of every field
│       ├── hitop-wasm/         # format + model for the web app (wasm-bindgen =0.2.126), own workspace, not published
│       ├── hitop-sync-crypto/  # MIT client crypto for hitop sync (0.4.0; recovery.rs: BIP-39 RecoveryPhrase (wordlist/bip39-english.txt), seal_recovery / seal_key_bundle;
│       │                       # password.rs: derive_password_keys(password, PasswordKdf) = Argon2id → HKDF authKey (server) + bundleKey (seals the bundle), check() floor m>=64MiB t>=3, vectors/password.json): format 2 key wraps = HPKE Auth mode with the sender's pinned X25519 key
│       │                       # (seal_wrap/open_wrap; format 1 base mode wrap_key/unwrap_key = legacy), keyring.rs accept_wraps (the one key
│       │                       # acceptance policy: pinned senders, invite fingerprints, strict, key commitments, no rollback; WorkspaceMemory,
│       │                       # feature `serde`), XChaCha20-Poly1305 blobs; vectors/v1.json (frozen) + v2.json (published before hitop)
│       ├── hitop-git/          # desktop Git panel: git2 0.21 + vendored libgit2 (feature `net`: https/ssh, vendored OpenSSL), not published;
│       │                       # json.rs = the one entry point (desktop `git` command, examples/git-json for the e2e mock); tests on temp repos
│       └── hitop/              # Binary crate (published to crates.io)
│           ├── tests/run.rs    # hitop run: JUnit shape (roxmltree), JSON and HTML reports, exit codes, flags, masking
│           ├── tests/schedule.rs # hitop schedule: exact workflow text, actionlint (HITOP_ACTIONLINT or PATH; required when CI is set), crontab line, exit codes
│           ├── tests/postman.rs # hitop import --to of the fixtures/postman collections, run against httpmock
│           ├── tests/openapi.rs # hitop import openapi (file, URL via httpmock), run, openapi sync keeping edits, --prune
│           ├── tests/cookies_forms.rs # cookie jar across send/run, -F uploads, saved form bodies
│           ├── tests/sending.rs # Content-Type and scheme added by send, run, requests send, snippet (httpmock)
│           ├── tests/snippet.rs # hitop snippet: curl, fetch, Python, Java, Go output run against a capture server = what requests send sends
│           ├── tests/ws.rs     # hitop ws against a local tokio-tungstenite echo server (tests/support/ws_echo.rs, shared with mcp.rs): flags, stdin, --count/--wait/--json, saved requests, masking
│           ├── tests/grpc.rs   # hitop grpc against hitop-core's test server (included by #[path]): calls, streaming, status, list/describe, --proto, saved requests
│           ├── tests/graphql.rs # send --graphql, saved graphql requests via run/requests send, Postman graphql import (httpmock)
│           ├── tests/globals.rs # globals below collection/env, secret globals, pm.globals, import, convert
│           ├── tests/mcp.rs    # hitop mcp: JSON-RPC over stdio for every tool (httpmock, ws echo server), path confinement, no secret value anywhere on stdout
│           ├── tests/secret_managers.rs # ${secret:...}: stub op/az on PATH + Key Vault mock; the settings file and env precedence; Vault dev and LocalStack (HITOP_TEST_VAULT_ADDR, HITOP_TEST_LOCALSTACK_URL; required on CI)
│           ├── tests/sync.rs   # two hitop clients against a real sync server (HITOP_SYNC_SERVER_BIN; required when CI is set): create, edit, conflict, resolve, delete, invite/trust/add, viewer, runner, remove, audit; a malicious server (rusqlite edits its DB: forged/unauthenticated/made-up-device wraps, swapped key, hidden rotation, replay, fake epoch); a hitop 1.1 workspace upgraded; login --sso via Keycloak (HITOP_SYNC_TEST_KEYCLOAK)
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
│               ├── commands/sync/ # `hitop sync ...`: mod.rs (commands, TOFU pins), client.rs (API + hitop-sync-crypto), store.rs (account, keychain/HITOP_SYNC_KEY_FILE, links, state, runner credential, audit-heads.json, audit-keys.json = pinned server audit keys), sso.rs (login --sso/--google/--github/--apple: loopback listener or paste), password.rs (login by password, signup, password set|forgot|reset|unlock, adopt = become the bundle's device), audit.rs (audit, local chain check; C5 org export --export/--file/--anchor: Ed25519 via ring, binary crate only)
│               ├── commands/mcp.rs # `hitop mcp [folder]`: MCP server over stdio (rmcp), the twelve tools, masking, path confinement
│               ├── commands/telemetry.rs # `hitop telemetry on|off|status` and the background ping (feature `telemetry`)
│               └── output.rs       # Colored output, syntect highlighting
│   ├── fuzz/                   # cargo fuzz (nightly, own workspace): collection_yaml, postman, openapi; seed.sh <target> <secs> (corpora from fixtures, not in git); workflow fuzz.yml
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
│   │                           # tauri-settings.js: desktop-only sections of the Settings page (secret managers; usage ping where built) + first-run usage ping question
│   │                           # response-store.js: large response bodies in IndexedDB (both adapters), storage usage for Settings
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
│   ├── src/sync.js             # hitop sync client (web + desktop): SyncClient (HTTP, wasm crypto/plan), withSyncFolders ("hitop-sync:<ws>" folder refs in app storage)
│   ├── src/library.js          # HITOP Cloud C8 personal library: signed in, collections/environments/globals pulled before the store reads storage and pushed after saves (baseVersion conflicts keep a "(this browser)" copy); localStorage hitop_library; e2e tests/library.cjs
│   ├── src/views/sign-in-dialog.js # <hitop-sign-in>: the Sign in dialog (dialog kind `sign-in`, `app.openSignIn(then)`; `app.go("sync")` signed out opens it): Continue with GitHub / Google (only what `/v1/auth/methods` has; none → straight to the second step), fine print, "Company SSO or your own server…" (Work email = org SSO by domain, Server = `hitop_settings.syncServer` or HITOP Cloud's, errors inline on the field); web: pop-up → `public/sync-callback.{html,js}` (`returnTo`, code over BroadcastChannel `hitop-sign-in` + opener), blocked → "Continue in this tab" (`#sso=`, `?sso=` on loopback origins, + sessionStorage `hitop_sync_signin`, `app.resumeSignIn()`); desktop or a refused `returnTo`: paste the code; `app.signedIn(r)` closes it, toast "Signed in as …", runs `then`. No password/mailed-code UI (sync.js keeps those calls; e2e signs in through `harness.cjs` `signInByCode`/`inApp`)
│   ├── src/views/sync-view.js  # <hitop-sync>: the Account page for signed-in people (kind "sync", not in the sidebar: avatar menu → Account & devices; signed out it is a Sign in button), minimal page: address + save state (`renderLibrary`), `.account-rows` (Shared with you → Open here when waiting, GitHub row from `<hitop-github>`, Plan & billing), `<details class="account-security">` Password & recovery (password status + Unlock with password for accounts that have one, recovery phrase, This device fingerprint; e2e `security(h)` opens it); with `.join` = the Join dialog (Accept invite); with `.collection` = the Share dialog's People tab (Share with people, members/invites/trust, Advanced: run history, runner, audit)
│   ├── tests/harness.cjs       # e2e in web or Tauri mode (HITOP_MODE=tauri); tests/tauri-mock.cjs mocks invoke in Node
│   ├── tests/sync.cjs          # two people against a real sync server (HITOP_SYNC_SERVER_BIN), web and Tauri modes; invite code with fingerprint; a malicious server via node:sqlite; audit log; SSO via Keycloak (HITOP_SYNC_TEST_KEYCLOAK)
│   ├── src/shortcuts.js        # keyboard shortcuts (Mod+Enter, Mod+S, Mod+K = command palette (src/commands.js), Mod+L = URL field, Mod+P, Alt+Shift+N/W/[/], ?): one capture listener in hitop-app.js
│   ├── src/settings.js, split.js, quick-open.js, collection-tree.js # 1.2: hitop_settings, the pane split, Mod+P fuzzy finder, import merge + folder tree
│   ├── src/views/settings-view.js, history-view.js # the Settings page (both builds) and the History page
│   ├── src/platform/local-storage.js # localStorage writes under the React keys that return false when refused (full storage → toast)
│   ├── tests/ux.cjs            # UI/UX review fixes (reviews/2026-11-01-webapp-ux): Content-Type, scheme, response tools, quota, dirty tabs, import, shortcuts, top bar
│   ├── tests/ux12.cjs          # 1.2 deferred items: IndexedDB bodies + fallbacks, Cancel/time limit, Settings, additive import, split panes, Mod+P, folder groups, request menu, tab ×, History page
│   └── tests/e2e.cjs           # Playwright e2e, seeded with React-app storage (legacy-storage.json)
├── frontend/                   # React app, now only for the browser extensions
│   ├── index.html              # Vite entry point (root of frontend/)
│   ├── vite.config.js          # Vite 6, outDir: build, multi-entry (main + sandbox)
│   └── src/                    # All files are .jsx (not .js)
├── extension/                  # Firefox extension source
├── chrome-extension/           # Chrome extension source
├── site/                       # hitop.rws8.tech: lit-material + Lit SSR, rws8.tech design system
│   ├── src/pages/              # home, pricing.html, privacy.html, 404, guides/
│   ├── content/guides/*.md     # the /guides/<name>/ guide articles (/terms/*.html 301s there: server/src/legacy.rs) (Markdown + front matter); hitop-vs-*.md comparisons carry a checked date and sources
│   ├── demo/demo.tape          # vhs tape for the terminal GIF (vhs site/demo/demo.tape from the repo root)
│   ├── public-assets/          # favicons (renders of brand/logo.svg; favicon.svg = the master), social preview
│   └── scripts/prerender.js    # npm run build → dist/
├── server/                     # hitop-server: rws app serving the site + POST /api/proxy
│   └── src/guard.rs            # address/port policy for the proxy (SSRF guard)
├── deploy/                     # build.sh, install.sh, systemd unit, front rws routes, README
├── sync-server/                # hitop sync (AGPL-3.0-or-later, own LICENSE/deny.toml): rws app, SQLite or Postgres
│   ├── src/                    # api.rs (routing, CORS), auth.rs (magic link, passkeys), workspaces.rs (members, wraps, rotation), teams.rs (invites, roles, runner accounts), blobs.rs (CAS + version vectors), oidc.rs (SSO + Continue with Google: OIDC code + PKCE, JWKS cache, `Sso` per provider kind), github.rs (Continue with GitHub, OAuth 2), apple.rs (Continue with Apple: oidc.rs `Sso::apple` with an ES256 client secret signed from the .p8 (client_secret_post, no PKCE), form_post callback `POST /v1/auth/apple/callback`, notifications `POST /v1/auth/apple/notifications` unlink on consent-revoked/account-delete; env `HITOP_SYNC_APPLE_{CLIENT_ID,TEAM_ID,KEY_ID,PRIVATE_KEY_FILE,APP_IDS}`; tests/apple.rs), password.rs (passwords, backoff), audit.rs (audit log, hash chain, purge), db.rs (migrations 1-9), billing.rs (Stripe Managed Payments, trials), dbtools.rs (db-stats, migrate-db, dump), ratelimit.rs, recovery.rs (key bundles),
│   │                           # orgs.rs (organizations, personal orgs, members), limits.rs (plans, entitlements, usage, the one place limits are enforced, grandfather), tokens.rs (personal access tokens);
│   │                           # C5: hostkey.rs (host key, AES-256-GCM for the server's own secrets), orgsso.rs (SSO per org, domain claims by DNS TXT over DoH), anchors.rs (signed audit export, daily anchors), admin.rs (org admin console)
│   ├── tests/                  # auth, sync, roles (every role × operation, removed member), teams (invites, runners), plaintext, no_crypto_in_server, postgres, sso (mock IdP, common/idp.rs), keycloak (fixtures/keycloak realm), audit, wraps_v2 (format 2 wraps, upgrade of format 1),
│   │                           # orgs (org role × operation, other org, tokens), limits (every limit at its boundary per plan), migration (3 → 4 of common/t25.rs, also in postgres), migrate_db, abuse, recovery; C5: domains (common/dns.rs fake DoH), audit_export, admin, sso/keycloak per org (common/orgsso.rs, fixtures/keycloak/hitop-b-realm.json)
│   ├── deploy/                 # hitop-sync.service, install.sh, rws-routes.toml (sync.hitop.rws8.tech); C2: postgres-setup.sh, cutover-postgres.sh, rollback-postgres.sh,
│   │                           # backup.sh + s3.sh + hitop-sync-backup.{service,timer}, restore-drill.sh, backup.env.example
│   └── THREAT_MODEL.md, README.md (self-host guide), Dockerfile
├── ios/
│   ├── HiTop/                  # the live App Store iOS app (listed as "HiTop HTTP Client", shown as HITOP; editor, own JSON storage); frozen but for names
│   ├── HitopKit/               # Swift package (MIT): format v1 reader (Yams 6.2.2), variables, secrets, scripts, URLSession runner, report.json
│   └── HitopCompanion/         # SwiftUI companion (T26): XcodeGen project.yml, app + App Intents, WidgetKit widget, Shared/ (app group)
├── spec/collection-format.md   # collection folder format v1, stable since 1.0 (CC BY 4.0); section 7 = compatibility promise
├── spec/compat/v1/             # compatibility suite: <case>/folder + expected.json (never changed, only added), run-report/report.json
├── shared/pm-shim.js           # the pm.* shim for Postman scripts, run by hitop-core (copy in the crate), the web app and HitopKit (copy in Resources/)
├── spec/sending.md             # what hitop adds when sending (behaviour, not format) + spec/vectors/send.json (cases only added)
├── spec/run-report.md          # `hitop run --reporter json` report, reportVersion 1, and the JUnit shape (CC BY 4.0)
├── spec/STATE_MACHINES.md      # every lifecycle (send, socket, call, sync link, billing...): the rule (one status, guarded transitions with an identity, wire strings fixed, `data-state` for automation) and what is still implicit
├── brand/                      # logo.svg (master), logo-mono.svg, png/, BRAND.md (name, tagline, colors, asset map), export.sh, check.sh (CI job brand)
├── ROADMAP.md                  # agent work order to v1.x: decisions, rules, tranches T0–T26, status
├── cloud/                      # HITOP Cloud (AGPL, spec/CLOUD_SERVICE.md): portal/ (Lit + lit-material console: app shell, real paths /orgs/<id>/plan served by hitop-server's spa fallback, sign-in/pricing vendored in vendor/ until npm; served at cloud.hitop.rws8.tech from www/cloud/, npm test = e2e against the sync server + axe + CSP),
│                               # compat/released-cli.sh (a released hitop against this tree's server, CI job compat), policies/ (SOC 2), evidence/access-reviews/, ops-log.md,
│                               # RUNBOOK.md, ops/ (systemd drop-ins, disk-alert.sh, install.sh <part>, test/: ops-e2e.sh, cutover-sim.sh, postgres-setup-docker.sh = CI job ops), status/ (static page + check.sh, off-VM)
├── launch/                     # launch text drafts for the owner (Show HN, Reddit, TWiR, Product Hunt); never posted by agents
├── docs/CNAME                  # only file left: keeps GitHub Pages 301ing bohdaq.github.io/hitop/* → hitop.rws8.tech
└── CLAUDE.md                   # This file
```

---

## Key technical facts

### Brand
- The product and every app are called **HITOP** (window titles, display names, manifests, `<title>`, store names); `hitop` is
  the command and every identifier (crates, bundle ids, extension ids, paths), which never change. iOS companion: "HITOP Runs"
- Tagline (all description fields): "The API client your terminal, your CI and your AI agent can all drive."
- Icons are renders of `brand/logo.svg` (gradient #667eea → #764ba2): edit the master, run `brand/export.sh`, commit. CI job `brand`
  (`brand/check.sh`) fails when an icon differs from the render or an old app name (list in check.sh) comes back.
  Details and the asset map: `brand/BRAND.md`

### Rust CLI (`cli/`)
- Workspace version: `1.2.0` (hitop, hitop-core, hitop-git in sync; also `[workspace.dependencies] hitop-core` version; the locks of
  hitop-wasm, cli/fuzz and desktop carry hitop-core's version too)
- Format v1 is stable: additive changes only (spec section 7). A new request/collection key needs a new case in
  `spec/compat/v1/` (the test writes its expected.json once; review, commit); existing cases never change
- Fuzzing: `cd cli && fuzz/seed.sh <collection_yaml|postman|openapi> <seconds>` (nightly + cargo-fuzz; FUZZ_TOOLCHAIN);
  CI `fuzz.yml` runs each 60 s on nightly-2026-10-24. Audits: `licenses.yml` runs `cargo deny check licenses advisories`
  per workspace (ignores with reasons in deny.toml) and `npm audit --omit=dev` for desktop, webapp, site
- Rust toolchain: `stable` via `cli/rust-toolchain.toml` — always use `~/.cargo/bin/cargo`, NOT Homebrew rustc
- Run tests: `cd cli && ~/.cargo/bin/cargo test`; CI also runs `cargo clippy --all-targets -- -D warnings`
- Postman: `hitop import <files>... [--to <dir>] [--force]` (collections, environments, globals detected by content;
  secret values to the keychain; prints the import report). Edit `shared/pm-shim.js`, then copy it to
  `cli/crates/hitop-core/pm-shim.js` (test `shared_shim_is_the_packaged_copy`)
- OpenAPI: `hitop import openapi <file|url> [--to <dir>] [--force] [--flat]` (also `hitop import <spec>` by content; Swagger 2.0 refused);
  `hitop openapi sync <file|url> <folder> [--dry-run] [--prune]`. Core `openapi::import`/`sync` (pure, in hitop-wasm): tags → folders,
  first server → `baseUrl` variable, path params `${name}`, security → auth (bearer, basic, apiKey header/query; OAuth 2 reported),
  response examples `source: openapi`. Requests and `hitop.yaml` carry an `openapi` record (extra key, spec section "OpenAPI source"):
  operation, operationId, per-field FNV hashes as imported (`synced`) and per-row hashes of headers and params (`rows`, since 1.1);
  sync updates a field only where it still has that value, and merges header/param rows one by one (`OpChange.rows`: RowChange
  {field, name, change, outcome updated|kept|yours}; a record without `rows` keeps an edited field whole once). Report
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
  Settings (since 1.1): `secret_managers::Settings` in `<config>/hitop/secret-managers.json` (vaultAddr, vaultNamespace, awsRegion,
  awsProfile, azureTenantId, azureClientId, opPath/azPath/awsPath) + keychain `secret-managers/<NAME>` for VAULT_TOKEN,
  AZURE_CLIENT_SECRET, AWS keys; `Resolver::configured(store)` layers them under the env (env wins per `GROUPS`); every call site
  (CLI add_refs/check, desktop) uses it; never written into the process env. Desktop page = webapp `platform/tauri-settings.js`
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
  `snippet` and MCP `run_request` skip or refuse WebSocket requests; MCP `run_websocket` does one bounded exchange (100 msgs/256 KiB/60 s)
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
  `{{NAME}}` (the CLI never resolves secrets without `--show-secrets`); one value per header name, as hitop sends, with the
  Content-Type and scheme `sending` adds (spec/sending.md; every send path: `form::outgoing`, `hitop send`, `prepare_in`); golden files
  `hitop-core/tests/golden/snippets/<case>/<lang>.<ext>` (cases.json, `UPDATE_GOLDEN=1`; syntax-checked with bash/node/python3/javac/gofmt when installed)
- Cookies: `send`, `run`, `requests send` keep a jar per workspace (`global` without `-c`, shared with the desktop);
  `hitop cookies ls|clear [-c]`, `--no-cookies`. Forms: `send -F name=value -F file=@path[;type=mime]`, `--urlencoded`;
  `run`/`requests send` send saved `bodyMode` urlencoded/multipart bodies (`form::outgoing`)
- Usage ping: hitop-core feature `telemetry` (off by default; the CLI crate's default feature turns it on);
  `hitop telemetry on|off|status`; never sent unless on, at most weekly; `HITOP_TELEMETRY_URL` points tests at a mock
- Sync: `hitop sync login --email E [--password-stdin] | --code [CODE] | --sso|--google|--github [--no-browser]`, `signup --email [--code] [--password-stdin]`, `password set|forgot|reset --code|unlock`, `logout|whoami|ls|init|link|unlink|status|pull|push|resolve|members|invite|accept|add|remove|role|trust|rotate|runner create|audit [--json] [--verify [--file F] [--anchor F]...] [--mine] [--export --org O [--out F]]|recovery create|use [--phrase-file F|-]|status|remove`
  (folders only; `-c <dir>`, default cwd). State in `<config>/hitop/sync/` (account.json, pins.json, keys.json = WorkspaceMemory per workspace, links.json, state/<ws>-<hash>.json);
  session + device secret in the keychain (`sync/<host>`) or `HITOP_SYNC_KEY_FILE` (0600; tests use it with `HITOP_KEYCHAIN=off`);
  `HITOP_SYNC_SERVER` (default https://sync.hitop.rws8.tech; http only for localhost); `HITOP_SYNC_RUNNER=hitop-runner-1.<b64url JSON>` = read-only
  runner credential for `pull` (since 1.2 with `pins`: accepts keys only from those devices). Wraps only for pinned keys (`--trust <fp>` or a TTY prompt; changed key refused);
  since 1.2 every wrap is format 2 and keys are accepted only via `accept_wraps` (client.rs `refresh_keys`); invite codes `<token>.<fingerprint>`;
  owners' sync/members upgrade format 1 wraps of pinned non-runner devices (`upgrade_wraps`); `remove` unpins the removed devices. Exit 1 while a conflict is open,
  2 on errors. `hitop schedule --sync` adds a pull step (GitHub: docker image + `secrets.HITOP_SYNC_RUNNER`; needs an image >= 1.1.0) or a pull before
  the cron run. Core `hitop_core::sync` decides; `hitop-sync-crypto` (cli workspace, published before hitop by release-cli.yml when its version is new)
- MCP: `hitop mcp [folder]` (rmcp 3.8 + tokio, binary crate only): stdio server, workspace = folder or global storage;
  tools list_collections, list_requests, get_request, run_request, run_collection, create_request, update_request,
  list_environments, set_environment, and since 1.1 run_websocket (connect, send, collect until count/wait, close; URL mode
  gets no collection auth), import_openapi, sync_openapi (preview unless `apply`; `prune` deletes only with `apply`: the one
  way a tool deletes); spec/`to` paths confined to the served folder (cwd with global storage) (names and descriptions
  are a contract: change additively). Runs go through
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
  Both pushed by `release-cli.yml`; `latest`/`debug` only on non-rc tags. Newest release: `1.2.0`
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
- Frontend: the Lit web app (`webapp/dist-tauri`) is the default in tauri.conf.json since HITOP Next T5 (2026-11-09);
  GitHub builds used it already (tauri.github.conf.json). The old React UI `desktop/src` stays reachable with
  `--config src-tauri/tauri.react.conf.json` (`npm run dev:react`, `build:react`; CI checks it with
  `TAURI_CONFIG="$(cat tauri.react.conf.json)" cargo check`). The Mac App Store build (plain `tauri build` on the owner's Mac)
  now builds the web app first: it needs the wasm32 target and wasm-bindgen-cli 0.2.126, and `npm ci` in webapp/.
  WebSocket: `ws_connect {args: {url, headers, protocols, timeoutMs}, onEvent: Channel}` → `{id, status, protocol, headers}`, events
  `{type: message|closed|error}` (binary as base64) on the channel; `ws_send {id, data, binary?}`, `ws_close {id}` (state `Sockets`).
  gRPC: `grpc_list {args: {address, plaintext, metadata, protos, importPaths, baseDir, timeoutMs}}` → `{source, services}`,
  `grpc_describe {args, symbol}` → `{description, template}`, `grpc_call {args: {...server, service, method, message, maxTimeMs},
  onEvent: Channel}` → `{id}`, events `{type: headers|message|status}`; `grpc_cancel {id}` (state `GrpcCalls`).
  Web app commands: `send_request` (proxy envelope; `cookies: true` uses hitop-core's `global` jar and returns it; optional `timeoutMs`,
  and `id` for `send_cancel {id}`: the exchange then runs on its own thread, cancel answers at once and drops its response and cookies),
  `cookies_load|save`, `load_globals|save_globals`, `read_folder`, `write_folder` (optional `globals`), `secret_get|set|delete`, `secret_refs_resolve {refs}` (inner texts → values; errors one line per reference; Settings layered in),
  `secret_manager_settings` → `{settings, secrets: {NAME: saved?}, shell: [names], path, keychainError}`, `secret_manager_settings_set
  {settings, secrets: {NAME: text|null}}`, `write_file`, `telemetry_status|set`; dialog plugin.
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
- MAS entitlements: sandbox + network.client + user-selected files + bookmarks.app-scope. `src/bookmarks.rs` (macOS, objc2):
  inside the sandbox (`APP_SANDBOX_CONTAINER_ID`) read_folder/write_folder/git first resolve the folder's security-scoped
  bookmark (`<app data>/folder-bookmarks.json`) or save one; `folder_forget {path}` drops it. CI job `desktop-macos` compiles it
- Icons from `brand/logo.svg`: `brand/export.sh --tauri` (Tauri's `icon` generator on `brand/png/logo-1024.png`)

### Frontend (`frontend/`)
- Build tool: Vite 6 (migrated from CRA — zero vulnerabilities)
- All source files use `.jsx` extension (not `.js`)
- Build output: `build/` (compatible with extension build pipeline)
- Dev server: `npm start` (port 3000)
- Build: `npm run build`
- Extension build: `npm run build:extension` (copies manifest_extension.json). Since HITOP Next A8 the repo's `build-*extension*.sh`/`build-chrome-store.sh` ship the web app instead (`webapp`: `npm run build:extension` → `dist-extension/`, then `webapp/scripts/package-extension.mjs chrome|store|firefox <dir>`; scripts run in the manifest's sandboxed page in Chrome; Firefox has no sandboxed pages, so there they run in boa compiled to wasm in a worker: `cli/crates/hitop-scripts-wasm` (own workspace, hitop-core feature `scripting`), `build-wasm.mjs --scripts` → `src/wasm/pkg-scripts/`, `platform/wasm-scripts.js` + `src/scripts-worker.js`, 30 s limit, e2e `tests/firefox.cjs` (Playwright Firefox, `?wasm-scripts`)); `HITOP_EXTENSION_UI=react` builds this React app
- `services/` are shared with `webapp/` (plain JS, no React): change them with both apps in mind
- `services/transportService.jsx`: every request goes through `sendRequest()`; public hosts use
  the proxy when `VITE_HITOP_PROXY` is set (webapp build), otherwise fetch

### Web app (`webapp/`)
- Lit + lit-material in light DOM; theme from `site/src/theme.css`; dark only (owner 2026-11-09: no theme toggle or setting anywhere, `color-scheme: dark`, `src/theme.js` drops old `hitop-theme`/`hitop-theme-system` keys; the site and portal too); `npm run build` → `dist/` (base `/app/`)
- Storage must stay readable by the React app (same keys/shapes); `npm run test:e2e` with BASE= a running hitop-server
- Response bodies over 64 KB go to IndexedDB (`platform/response-store.js`, DB `hitop-responses`, by tab id, `{body, raw}`,
  `platform().responses`, both adapters); `hitop_tabs` keeps their first 16 KB with `responseCut` + `responseStored`
  (`storedTabInDb`), restored after a reload (`store.restoreResponses`). Without IndexedDB or after a refused write (one toast):
  1.1 rule, first 64 KB of a body over 256 KB (`storedTab`, `responseCut` only). Raw bodies of smaller responses (`store.rawBodies`),
  URL errors and dirty state are memory only.
- 1.2 UX (reviews/2026-11-01-webapp-ux "Status (1.2.0)", e2e `tests/ux12.cjs`, unit `tests/unit-ux.mjs`): Settings page in both builds
  (`views/settings-view.js`, element `hitop-app-settings`: never name it `hitop-settings*`, check-no-telemetry greps that; desktop
  sections via `platform().settingsSections` from tauri-settings.js); localStorage `hitop_settings` {timeoutMs} (`settings.js`),
  `hitop_layout` {columns %, rows px, responseCollapsed} (`split.js`; panes in request-view.js: columns ≥ 880 px view width, rows
  ≥ 600, stack); Cancel = `store.cancel(tabId)` (AbortController per tab in `store.inflight`, time limit → TimeoutError; web.js
  races proxied sends, tauri.js `send_cancel {id}`); Import add (default, `collection-tree.js` addCollections) or replace;
  quick open Mod+P (`quick-open.js`); History page (`views/history-view.js`); sidebar folder groups from `file`
  (`requestTree`, fold keys `<collectionId>/<path>` in `hitop_collapsed_collections`) and request context menu Raw-body Content-Type is decided in `dispatch` and URLs get a scheme (never a relative
  fetch) through hitop-wasm `rawContentType`/`withScheme` = hitop-core `sending` (`store.format` for the Headers note while rendering). Sidebar folding: localStorage `hitop_collapsed_collections`
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
- 2026-11-06 additions: quick open finds collections/environments/history; sample collection (`store.addSampleCollection`); Ask AI (`src/ai.js` own Anthropic key in localStorage `hitop_ai`, `views/ai-view.js`; the SDK chunk is the one exception in check-no-telemetry); auth presets oauth2 (client_credentials, password, authorization_code + PKCE: `store.oauthSignIn` pop-up + `public/oauth-callback.{html,js}`), awsv4 (`awsV4Headers`, signed in `dispatch`), digest (`digestAnswer`, 401 retry in `dispatch`) = hitop-core oauth2.rs, aws_sigv4.rs, digest_auth.rs (same test vectors); Share dialog tabs Mock/Monitor (`views/mock-view.js`, `monitor-view.js`), request menu Version history (`views/history-dialog.js`), team environments (`src/teamenvs.js`, env `team: <org>`), SSE live events (`store.events`), WebSocket/gRPC through the hitop-server bridges (web.js `webSocketFor`, `bridgeGrpc`). e2e: onboarding.cjs, sse.cjs, ai.cjs
- FV1 new requests land in a collection (2026-11-12, HITOP_NEXT_UI.md FV1, e2e `tests/first-visit.cjs`, unit in unit-ux.mjs): pure helpers in
  `src/new-request.js` (`requestAutoName` "GET /users" / host only / "Untitled", `isBlankRequest`, `MY_REQUESTS`/`isMyRequests` = "My requests" or an older
  "Default", `sampleRequests`, `isPristineSample`, `isEmptyMyRequests`). `store.newTab(collection, folder)` adds an "Untitled" request to the collection at
  once and opens it linked (tab `autoName: true`); every way in goes through `app.newRequest(collectionId?, folder?)` (tab bar +, Mod+N/Alt+Shift+N, palette,
  sidebar, collection page: expands the row, focuses the URL). `store.canTakeRequest(c)` is false for a viewer's shared collection (`store.workspaceRoles`,
  from `/v1/workspaces` in orgs.js) and for a cloud-kept collection whose org storage is full (`orgUsage`): the request goes to My requests
  (`defaultCollection()`: My requests, else Default, else makes My requests) with a toast. While `autoName`, the tab title and the sidebar row
  (`store.autoNameTab`) follow the URL, but the collection keeps the saved name until Mod+S (`store.savedName`), so typing never writes the collection
  (library, folders, sync); renaming (tab, sidebar menu, a Save dialog name) clears it. `closeTab` takes out a still blank "Untitled" (`dropIfUntouched`).
  Folder collections get the request (and its file, `<folder>/untitled` when added in a folder) at once; the folder write removes the file when it is taken
  out; its first save under a real name renames an `untitled` file. Deleting a request or collection closes its tabs (`closeTabsOf`; the last one becomes a
  new request); a history item opens as a new request in My requests. First visit (no stored collections and no tabs): the sample + an empty My requests,
  "List posts" open, URL focused (`store.firstVisit`); restored old draft tabs with content are filed into My requests (`fileDrafts`), empty ones dropped.
  library.js doesn't upload an empty My requests/Default or the untouched sample at the first link; `isFresh` ignores the sample's own requests
- UX2 (2026-11-12, HITOP_NEXT_UI.md UX2, e2e `tests/ux2.cjs`, unit `tests/unit-ux2.mjs`): `src/errors.js` `explainError(raw, {kind})` = headline + next step for
  engine/proxy errors (rendered by `views/error-card.js`, raw under Details); every URL field is request-view's `urlField()` (mirror drawn
  over a transparent input only under `[data-mirror]`); Esc goes through hitop-app `onEscape` (in-flight cancel via request-view
  `cancelInFlight()`); `store.callStarts` = when each tab's send started; method menu SSE = GET + Accept text/event-stream (`methodLabel`
  "SSE"); dispatch reads an answer with Content-Type text/event-stream as a live stream (`web.js streamFrom` for direct answers;
  proxied: request field `eventStreams`, hitop-server answers `eventStream: true` with the body unread, then `/api/proxy/stream`);
  `store.contextCollection()` skips the untouched sample; `openSignIn(then, reason)`; `src/curl.js` parseCurl (pasted into a URL field = `app.pasteIntoUrl` → `store.applyCurl`, or Import; unit tests read
  hitop-core's snippet goldens' curl.sh); `app.toast(message, {label, run})` = a toast with an action button; layout: rail Collections = `showWorkspace()`, Esc on a page = `leavePage`, side pane
  Mod+B = `toggleSidePane()` (`hitop_layout.sidePane`, `split.js loadSidePane`; `saveLayout` merges keys), response tools on `.response-tabbar`;
  `src/binary.js` (binary bodies: base64 on the tab, `responseType: "binary"`, `responseSize`; Diff by size + digest); the sidebar is an ARIA
  tree (`data-tree`/`data-key` rows, `treeKey`/`rovingTree` in hitop-app; e2e locate rows by role `treeitem`); smaller findings: tab labels `tabLabel()` (tag drawn from `data-tag`, e2e read it there), `confirmDiscard()`
  (Save and close), `commands.js rankSections`, `getting-started.js markImported`, `collection-tree.js folderTitle`, body `${var}` marks
  (`hitop-code-editor .variables`), `store.duplicateEnvironment`, `store.exportJson(format, only)`, `sync.authMethods` throws when unreachable
- DD1/DD2 tree and tabs (2026-11-13, HITOP_NEXT_UI.md DD1/DD2): sidebar drag and drop = `views/tree-drag.js` (pointer controller, rows carry `data-drag` request|folder|collection, `targetAt` → before/after/inside places, ghost + `.tree-drop-line` + `.drop-inside`) over pure `src/tree-moves.js` (moves, `treeOf` with stored folders, `neighbourPlace` for Alt+Up/Down); `store.moveNode` (Undo), `store.addFolder`/`deleteFolder`; collections keep folders in `folders` (ordered paths, empty ones too; hitop.yaml `folders`, spec compat case `folders`); run order stays the request order. Tabs: `views/tab-drag.js` (drop line by lit-material's rule, edge scroll, drop on the tree moves the tab's request), `store.togglePin` (pinned first, no ×), `store.closeTabsAt` (keeps pinned/unsaved). e2e `tests/tree-dnd.cjs`, `tests/tabs-dnd.cjs`, unit `tests/unit-tree.mjs`
- Secrets: environment `secrets` names, masked editor; `features.js` `exportableEnvironments()`/`maskSecrets()` for exports and run history
- Account and sharing (2026-11-06 simplification): top bar `accountButton()` (signed out: Sign in = the Sign in dialog, `openSignIn()`; signed in: avatar + badge = `needs()`: GitHub clashes, shared-collection conflicts, folder/library errors) and `#account-menu` (Account & devices, Join with invite code… = dialog kind `join`, Plan & billing ↗ web only, Sign out); shared-collection conflicts open the Share dialog's People tab; Organizations (2026-11-06, `src/orgs.js` loadOrgs on account change in main.js): top bar `orgSelect()` (`.org-select`, label Organization, "Personal · Free") replaced the environment picker, which is now `envSelect()` in request-view's `.request-line` next to Send. Separate spaces: `store.orgOf(c)` = `c.org` (library collections; server files `data.org` under that org if a member, library.rs) else personal; browser-only/folder collections personal; shared ones = their workspace's org (`store.workspaceOrgs`, share() passes `org`). Views use `store.visibleCollections`/`visibleEnvironments` (never filter `store.collections`: library/git/sync iterate it); new collections get `store.inOrg()`; environments carry `org` (none = every org; switch "Available in every organization" on the Environments page); Plan & billing = `app.planUrl()` → portal `/orgs/<current>/plan` in hitop-app.js; Share dialog (`openShare(c, tab)`, dialog kind `share`, tabs people|github (web)|file); the library switch is in Settings → Privacy (same aria-label). e2e helpers open the Account page via the account menu and the Share dialog via the collection page
- Sync (both modes): `src/sync.js` + the Account page (`views/sync-view.js`, added to `platform().pages` in main.js with `sidebar: false`, which also wraps
  `platform().folders` with `withSyncFolders`). A shared collection is a folder collection: on disk (desktop/FS Access) or in app storage under
  ref `hitop-sync:<ws>` (`hitop_sync_files:<ws>` = files map). localStorage `hitop_sync_account`, `hitop_sync` {pins, links, states, memory}; session +
  device secret through `platform().folderSecrets` (desktop keychain `hitop-sync/<host>`, web localStorage). HTTP: web `fetch` straight to the
  server (its CORS allows the web origin, never the proxy), desktop `platform().send` (Rust). Crypto and plan: hitop-wasm `sync*` (format.js `sync`);
  keys only through `syncAcceptWraps` (`workspaceKeys`, refusals = `KeyError` {kind, fingerprint}), wraps `syncSealWrap`, owners `upgradeWraps`
- Secret manager references: `features.js` `secretRefs()`/`withSecretRefs()`; `store.resolveRefs()` (in `prepare`, `wsConnect`, `grpcCall`) asks
  `platform().resolveSecretRefs` (tauri.js → `secret_refs_resolve`; web.js `null` → the request isn't sent, the note names them);
  values in `store.refValues` (memory, reset per send/run), merged by `variables()`, masked via `secretValuesFor()`; Code dialog keeps placeholders
- Lint: `npm run lint` (ESLint 10 flat config `eslint.config.js`: recommended + eslint-plugin-lit, browser globals for src/, Node for tests/scripts) in webapp/, site/ and cloud/portal/; CI job `lint`. Fix findings, don't disable rules
- No telemetry in the web build: telemetry UI lives only in `platform/tauri-settings.js` (imported by tauri.js);
  `node scripts/check-no-telemetry.mjs` after both builds checks it. Adapter hooks `pages`/`onStart` add desktop-only pages
- Builds need the wasm32 target and the wasm-bindgen CLI 0.2.126 (`cargo install wasm-bindgen-cli --version 0.2.126 --locked`):
  `npm run build` (web, `/app/`, proxy), `npm run build:tauri` (`dist-tauri/`, no proxy). `npm run test:e2e:tauri` runs every
  e2e suite against `dist-tauri` with a mocked invoke (no server needed); new e2e suites use `tests/harness.cjs` so they run in both modes
- hitop-core feature `native` (default): http, scripting, prepare, runner, report, storage; hitop-wasm builds without it,
  so a new core module using reqwest, boa, dirs or those modules starts with `#![cfg(feature = "native")]`

### iOS companion (`ios/HitopKit`, `ios/HitopCompanion`)
- HitopKit follows hitop-core: same reader rules and errors, ids (FNV-1a), interpolation (one pass), auth, GraphQL and
  form bodies, send rules (`Sending.swift`: raw-body Content-Type, URL scheme; `SendRulesTests` on spec/vectors/send.json), script preambles + `pm-shim.js`, masking, report.json shape. A change to any of these in the core needs the
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
- hitop-server Origins (`server/src/origins.rs`, `HITOP_ALLOWED_ORIGINS`): exact entries plus `moz-extension://*` (any per-install Firefox UUID); default = web app + Chrome Web Store id `mmbpagencfjfgeicmeobbmeocdpeokdk` + Firefox; the extensions (`webapp/.env.extension` `VITE_HITOP_PROXY=https://hitop.rws8.tech/api/proxy`) send public hosts through the proxy and bridges like the web app (owner 2026-11-10; targets Chrome Web Store + Firefox Add-ons only; listing texts `launch/store-listings.md`, limits also on the extension's Settings page)
- hitop-server bridges (2026-11-06): `POST /api/proxy/stream` (SSE/chunked passthrough, 8 at once), `GET /api/ws` (wsbridge.rs: first frame {url, headers, protocols}, 16 at once) and `POST /api/grpc/{list,describe,call}` (grpcbridge.rs, reflection only, NDJSON events); all check Origin and dial an address checked by `guard::checked_addr` (hitop-core `websocket::Target.connect_to`, `grpc::Target.connect_to`); a default User-Agent on proxied requests; `HITOP_BRIDGE_ALLOW_PRIVATE=1` = tests only
- Build all: `./deploy/build.sh`; server tests: `cd server && cargo test`
- Old URLs must keep working: `/privacy.html` (App Store privacy URL), `/pricing.html`, `/app.html`, `/terms/*.html` (301 to `/guides/<name>/` in hitop-server)
- App Store (listing names until the owner renames them to HITOP, ROADMAP section 6): macOS "HiTop API Client" id6785294166 ($4.99, macOS 12+), iOS "HiTop HTTP Client" id6785326628 ($2.99, iOS 17+)

### Sync server (`sync-server/`)
- T24: migration 2 (`users.runner_of`, `invites`); `PUT .../members/:user {role}`, `POST/GET/DELETE .../invites`, `POST /v1/invites/accept`,
  `POST .../runners`; runner accounts `<id>@runner.invalid` (viewer, one workspace, 1-year session, no sign-in/keys/invites/workspaces); `/invite` page
- 1.2 (no migration): wraps are format 1 (80 bytes) or format 2 (`0x02 || head || sender key id || 80 bytes`; sender must be the caller's key);
  a format 2 wrap replaces a format 1 one in `POST .../wraps` (`stored`), `GET /v1/workspaces/:ws` keys have `legacyEpochs`, invite accept ignores `.<fingerprint>`
- T25: migration 3 (`oidc_identities`, `audit_log`, `audit_chain`); SSO `GET /v1/auth/sso`, `POST .../sso/start|finish`, `GET .../sso/callback`
  (env `HITOP_SYNC_OIDC_*`, `HITOP_SYNC_SSO_REQUIRED`; jsonwebtoken 10 with the aws_lc_rs backend + ureq 3); audit `GET /v1/workspaces/:ws/audit[/verify]`, `GET /v1/me/audit`,
  `hitop-sync-server audit-verify`, hourly purge (`HITOP_SYNC_AUDIT_RETENTION_DAYS`, 365), IPs truncated (`HITOP_SYNC_AUDIT_IP`, `HITOP_SYNC_TRUSTED_PROXIES`)
- Own Cargo workspace: `hitop-sync-server` (AGPL); the MIT client crate `cli/crates/hitop-sync-crypto` is in the cli workspace and only a dev-dependency here
- Tests: `cd sync-server && cargo test --workspace`; Postgres: `HITOP_SYNC_TEST_POSTGRES=postgres://... cargo test --features postgres --test postgres`;
  Keycloak: `docker run ... quay.io/keycloak/keycloak:26.4.2 start-dev --import-realm` with `tests/fixtures/keycloak` (README), then `HITOP_SYNC_TEST_KEYCLOAK=http://127.0.0.1:18080`
- Licenses: `cargo deny --config sync-server/deny.toml --manifest-path sync-server/Cargo.toml --workspace check licenses advisories` (the MIT crypto crate is checked with the cli workspace)
- HITOP Cloud C1: migration 4 (`orgs`, `org_members`, `org_invites`, `workspaces.org_id`, `entitlements`, `usage_counters`, `api_tokens`; a personal org
  `personal-<user id>` per person, grandfathered limits); `/v1/orgs/*`, `/v1/me/tokens` (`hitop_pat_` tokens, read/write scopes); plan limits only with
  `HITOP_SYNC_LIMITS=plans` (default off = self-hosted, no limits): 402 `{error: limit, limit, plan, org, used, max, upgradeUrl}` on creates, never on reads;
  `HITOP_SYNC_PORTAL_URL`; `hitop-sync-server over-limits` (exit 0 = nobody over) and `set-plan <org> <plan> [seats]`. Plan numbers in `limits::plan_limits`
  (proposal until the owner sets prices). CLI `hitop sync init --org <name|id>`, `sync ls` shows the org
- HITOP Cloud C2 (applied 2026-11-03 except backups, restore drill and uptime; the test server runs on Postgres; spec/CLOUD_SERVICE.md C2, cloud/ops-log.md): migration 5 `recovery_blobs`
  (user_id, method; `PUT/GET/DELETE /v1/me/recovery[?method=phrase]`, device sessions only; `DELETE /v1/me/keys/:id` for an own key without wraps);
  `hitop-sync-server db-stats [--json] | migrate-db --from --to | dump <dir> | snapshot <file>` (migrate-db/dump/stats read one snapshot: `Db::read_tx`);
  rate limits `HITOP_SYNC_RATE_*` (429 `rate_limited`), `HITOP_SYNC_MAX_JSON_BYTES`; `/healthz` = {ok, version, database, schema}. Tests needing pg_dump:
  `HITOP_SYNC_TEST_PG_DUMP`/`_PG_RESTORE` (docker postgres:17-alpine commands). MinIO images aren't public any more: S3 tests use LocalStack (`SERVICES=s3`).
  The VM build must be `--features postgres` from C2 on
- HITOP Cloud C1b (built 2026-11-03, not deployed; spec/CLOUD_SERVICE.md C1b, THREAT_MODEL "Passwords"): migration 6 `password_credentials` (kdf JSON, server Argon2id
  PHC hash of the client's authKey, peppered flag), `password_tokens` (signup/reset/notice, SHA-256 of the mailed code), `server_secrets` (made-up params HMAC key);
  `/v1/auth/password/{params,signup,verify,login,forgot,reset}`, `POST /v1/me/password {currentAuthKey?, kdf, authKey, bundle?}`, `GET /v1/auth/methods`,
  `/v1/auth/{google,github}/{start,finish}` + `GET .../callback` (handoff code as SSO; GitHub: primary verified address; issuers `https://accounts.google.com`,
  `https://github.com` in oidc_identities). `/v1/me` adds `password`, `bundles` (`recovery` = phrase only, for 1.2). Env `HITOP_SYNC_PASSWORDS`, `HITOP_SYNC_PASSWORD_PEPPER[_FILE]`,
  `HITOP_SYNC_GOOGLE_CLIENT_ID/SECRET[_FILE]` (+ `_ISSUER` test hook), `HITOP_SYNC_GITHUB_CLIENT_ID/SECRET[_FILE]` (+ `_URL`, `_API_URL`). A password sign-in on a new device
  adopts the bundle's device (no new key, no trust step); only that device keeps `password` (bundleKey) in its secret and re-seals while the server's bundle
  still opens with it; a reset deletes the bundle (`bundleLost`), and a new device after a reset doesn't claim it. Tests: sync-server tests/password.rs, oauth.rs
  (common/github.rs mock GitHub, common/pw.rs helpers); argon2 needs `[profile.dev.package.argon2] opt-level = 3` (cli, hitop-wasm, sync-server)
- HITOP Cloud C3: `billing.rs` (Stripe Managed Payments, off unless `HITOP_SYNC_STRIPE_SECRET_KEY` + `_WEBHOOK_SECRET`): `POST /v1/billing/stripe`
  (Stripe-Signature t/v1 HMAC + 5 min tolerance before parsing, idempotent by event id, event `created` order per subscription, livemode vs key mode,
  org from the subscription id or server-signed metadata, bodies never stored), `/v1/orgs/:org/billing[/checkout|/portal|/seats|/trial]` (Checkout
  Session `managed_payments[enabled]`, never the params it refuses; prices by lookup key `<plan>_<monthly|yearly>`, `HITOP_SYNC_STRIPE_PRICE_*` overrides;
  pinned `Stripe-Version` `STRIPE_VERSION`); migration 7 columns `ls_*` hold Stripe ids (no rename); `paused`/`refunded`/`incomplete` = Free limits,
  `billing::expire` ends trials and cancelled periods; tests `tests/billing.rs` with `common/stripe.rs` (fake API, signed fixtures in `tests/fixtures/stripe/`);
  `billing-events`, `billing-link` commands.
  Site: `site/src/legal-status.js` DRAFT gates `/security.html` + `/legal/*.html` (content/legal/*.md); `site/scripts/check-pages.mjs` in CI
- HITOP Cloud C5 (built, not deployed): migration 9 (`org_sso`, `org_domains`, `org_audit_settings`, `audit_anchors`, `workspaces.rotation_requested_at|_by`).
  SSO per org: `/v1/orgs/:org/sso[/test]`, `/v1/orgs/:org/domains[/:d/verify]`; off (404 `org_sso_off`) without `HITOP_SYNC_HOST_KEY[_FILE]` (the test
  server keeps it unset, Q15); TXT `_hitop-verify.<d>` via `HITOP_SYNC_DNS_URL`; an org's provider signs in only its verified domains; flow in oidc.rs
  keyed by `Flow.org` (all SSO flows in `state.sso.flows`, one callback); `sso_required_for` checks `state.org_sso` (in memory, `orgsso::reload` after
  writes: never open a db tx in it). Audit: `GET /v1/orgs/:org/audit/export` (JSONL, answered outside the router in api.rs), daily anchors (hourly
  worker, mailed to owners, `HITOP_SYNC_AUDIT_ANCHORS`), `/v1/audit/key`, `/v1/audit-anchors/:org/:day` (public opt-in). Admin: `/v1/orgs/:org/admin`
  (+ owner transfer, rotation request, device removal from the org's workspaces only: migration 11 `workspace_excluded_keys`, honoured by exact_wraps/insert_wraps/key lists/shares). `limits::require_plan` gates features (402 `sso`/`auditExport`/`adminConsole`/`adminActions`).
  Business prices: lookup keys `business_yearly|_monthly`. The server links `ring` (own secrets only; `no_crypto_in_server` bans client crates)
- HITOP Cloud C8 personal library (deployed 2026-11-05): migration 12 `library_items`; `GET /v1/library[?since=]`, `PUT/DELETE /v1/library/:kind/:id` (kind collection | environments | globals, id `all` for the last two; `baseVersion` → 409 `conflict` with `current`; tombstones). **Readable by the server** (owner decision); secret values sealed under the host key (none stored without one; the hosted server has one since 2026-11-06, `/etc/hitop-sync/host.key` via drop-in `40-host-key.conf`); bytes count in the personal org's storage (`limits::stored_bytes`), not a workspace. Tests `tests/library.rs`
- HITOP Cloud gaps tranche (2026-11-06, TODO.md): schema 14 `mocks` (mocks.rs: `/mock/<id>/…` from saved examples, never HTML, 600/min, paid plans), `mcp.rs` (`POST /mcp`, 6 tools over the library, PAT scopes via `tokens::authenticate_token_for`), schema 15 `org_items` (teamenvs.rs: `/v1/orgs/:org/environments`, members, no secret values), schema 16 `monitors`/`monitor_runs` (monitors.rs: worker every 15 s runs `<bin> monitor-run` child under `ulimit -v`, 150 s kill, `monitor_guard.rs` = copy of server/src/guard.rs, hitop-core `http::install_client`/`install_url_check` + `scripting::set_limits`; mail on fail/recover; `HITOP_SYNC_MONITOR_BIN` (off), `HITOP_SYNC_DENY_IPS`, test hook `HITOP_SYNC_MONITOR_ALLOW_PRIVATE`), schema 17 `request_history` (library.rs `record_history` on every collection save: app/github/mcp, last 50 per request; `GET .../history?request=`). hitop-core is now with `native` (runner used only by the monitor child). library collections carry `data.org` (filed under that org if a member)
- HITOP Cloud C8 Git sync (deployed 2026-11-05, App hitop-cloud): `gitsync.rs`, migration 13 (`git_installations`, `git_links`); GitHub App settings `HITOP_SYNC_GITHUB_APP_{ID,SLUG,KEY_FILE,CLIENT_ID,CLIENT_SECRET[_FILE],WEBHOOK_SECRET[_FILE]}` (+ `HITOP_SYNC_GITHUB_URL/_API_URL`, `HITOP_SYNC_GIT_DEBOUNCE` 10 s), off = 404 `git_off`; links a library collection to `repo`+`branch`+`path` (only `hitop.yaml`/`requests/**`), push on every save via the worker (`run_due`, 2 s), push webhook, three-way merge (`merge3`, hitop.yaml via hitop-core `sync::merge_text`), conflicts answered mine/theirs/both. hitop-core is a path dependency without default features (the Dockerfile copies `cli/Cargo.toml` + `cli/crates/hitop-core`). Tests `tests/gitsync.rs` + fake `tests/common/ghapp.rs` (test App key in `tests/fixtures/github-app/`); web app: `src/git.js` (GitState = `store.git`, web build only: links, repos of every installation, `#git=` return + sessionStorage `hitop_git_pending` reopens the collection's dialog; `storageOf(store, c)` = where a collection is kept: github/cloud/shared/folder/browser) → sidebar icon (+ dot on conflict/error, aria-describedby), collection page chip, dialog `git` = `<hitop-git-panel>` (Connect → repository → folder/branch → file preview → Save to GitHub; linked: Sync now, Stop saving to GitHub, conflicts) in `views/github-view.js`, Sync page `<hitop-github>` (accounts, Manage repository access, list); e2e `tests/github.cjs` (web mode; fake GitHub records `visits`)
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

- `hitop-core` and `hitop` 1.0.0, published to crates.io 2026-11-01 (tag `cli-v1.0.0`); 0.3.0 and 0.2.0 before
- Desktop 1.0.0 (tauri.conf.json, Cargo.toml, package.json): GitHub Release `desktop-v1.0.0` (React UI until the T5 switch)
- To publish: bump `version` in `cli/Cargo.toml` (workspace), push, then push tag `cli-v<version>`; `.github/workflows/release-cli.yml` publishes core then hitop (needs the `CARGO_REGISTRY_TOKEN` secret)
