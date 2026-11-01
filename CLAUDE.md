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
│       │       ├── http.rs         # reqwest blocking client (rustls, no OpenSSL)
│       │       ├── interpolation.rs # ${variable} substitution
│       │       ├── scripting.rs    # Pre/post request JS via boa_engine
│       │       ├── runner.rs       # Sequential collection runner (RunOptions: bail, timeout, variable overrides)
│       │       ├── report.rs       # run reports: JSON (spec/run-report.md, reportVersion 1) and JUnit XML, no deps
│       │       ├── export.rs       # HITOP and Postman v2.1 import/export
│       │       ├── secrets.rs      # secret variables: resolve order, keychain (keyring), Masker (••••)
│       │       └── format/         # collection folders (YAML, spec/collection-format.md) ⇄ the JSON model
│       │   └── tests/format.rs     # golden files (tests/golden/, UPDATE_GOLDEN=1 regenerates) + proptest round trip
│       └── hitop/              # Binary crate (published to crates.io)
│           ├── tests/run.rs    # hitop run: JUnit shape (roxmltree), JSON report, exit codes, flags, masking
│           └── src/
│               ├── main.rs         # clap CLI entry point
│               ├── commands/source.rs # a collection argument: global name or folder path
│               ├── commands/folder.rs # `hitop init`, `hitop convert`
│               ├── commands/secret.rs # `hitop secret set|get|rm|ls`, the `--secret` flag
│               ├── commands/run.rs # `hitop run`: reporters, iterations, --data, exit codes 0/1/2
│               ├── commands/data.rs # `--data` CSV (RFC 4180, hand written) and JSON rows
│               └── output.rs       # Colored output, syntect highlighting
│   └── fixtures/sample/        # collection folder run by tests/folder.rs against httpmock
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
│       │   └── lib.rs          # Command registration
│       ├── Cargo.toml          # depends on hitop-core via path
│       ├── tauri.conf.json     # Bundle ID: com.bohdaq.hitop, category: Developer Tools
│       ├── Entitlements.mas.plist          # Mac App Store entitlements
│       └── Entitlements.mas.inherit.plist  # MAS child process entitlements
├── webapp/                     # hitop.rws8.tech/app/: Lit + lit-material, reuses frontend/src/services
│   ├── src/store.js            # all app logic (port of frontend/src/App.jsx), same localStorage keys
│   └── tests/e2e.cjs           # Playwright e2e, seeded with React-app storage (legacy-storage.json)
├── frontend/                   # React app, now only for the browser extensions
│   ├── index.html              # Vite entry point (root of frontend/)
│   ├── vite.config.js          # Vite 6, outDir: build, multi-entry (main + sandbox)
│   └── src/                    # All files are .jsx (not .js)
├── extension/                  # Firefox extension source
├── chrome-extension/           # Chrome extension source
├── site/                       # hitop.rws8.tech: lit-material + Lit SSR, rws8.tech design system
│   ├── src/pages/              # home, pricing.html, privacy.html, 404, terms/ (guides)
│   ├── content/guides/*.md     # the /terms/*.html guide articles (Markdown + front matter)
│   ├── public-assets/          # favicons, social preview; favicon.svg is also the desktop icon source
│   └── scripts/prerender.js    # npm run build → dist/
├── server/                     # hitop-server: rws app serving the site + POST /api/proxy
│   └── src/guard.rs            # address/port policy for the proxy (SSRF guard)
├── deploy/                     # build.sh, install.sh, systemd unit, front rws routes, README
├── spec/collection-format.md   # collection folder format v1 (CC BY 4.0)
├── spec/run-report.md          # `hitop run --reporter json` report, reportVersion 1, and the JUnit shape (CC BY 4.0)
├── ROADMAP.md                  # agent work order to v1.x: decisions, rules, tranches T0–T26, status
├── docs/CNAME                  # only file left: keeps GitHub Pages 301ing bohdaq.github.io/hitop/* → hitop.rws8.tech
└── CLAUDE.md                   # This file
```

---

## Key technical facts

### Rust CLI (`cli/`)
- Workspace version: `0.2.0` (both crates in sync)
- Rust toolchain: `stable` via `cli/rust-toolchain.toml` — always use `~/.cargo/bin/cargo`, NOT Homebrew rustc
- Run tests: `cd cli && ~/.cargo/bin/cargo test`; CI also runs `cargo clippy --all-targets -- -D warnings`
- Collection folders: `hitop init [dir]`, `hitop convert <json> <dir>` / `<dir> <json>` (`--environments`, `-c`, `--force`);
  `run`, `requests`, `variables`, `export`, `send -c` also take a folder path (`hitop run ./my-api --env local`); a folder uses only its own `environments/`
- Secrets: `hitop secret set|get|rm|ls --env <env> [-c <folder>]`; `--secret NAME=value` on `send`, `run`, `requests send`;
  resolve order `HITOP_SECRET_<ENV>_<NAME>`, `--secret`, OS keychain (`keyring` 4, service `hitop`, account
  `<workspace>/<env>/<name>`), prompt on a TTY. `HITOP_KEYCHAIN=off` skips the keychain (CLI tests set it). Values are
  masked as `••••` in output, history and runner steps (`runner::run_collection_with`); hitop-core feature `keychain` (default)
- CI runs: `hitop run <folder|name> --env <e> | --env-file <yaml>`, `--var k=v`, `--bail`, `--timeout <ms>`,
  `--iterations <n>` or `--data <csv|json>`, `--reporter cli,junit,json --out <dir>` (`junit.xml`, `report.json`);
  exit 0 passed, 1 a request or assertion failed, 2 usage or config error (`main.rs` maps `run`'s Err to 2).
  Default is to run every request; `--no-stop-on-fail` is a hidden no-op. Variable order: collection, environment,
  data row, `--var`, secrets. A new hitop-core runtime dep also changes `desktop/src-tauri/Cargo.lock` (CI checks it `--locked`)
- YAML crate: `yaml_serde` (serde_yaml continued by the YAML org); HTTP tests use `httpmock` 0.8, never the public internet
- Publish: `cd cli && cargo publish -p hitop-core` then `cargo publish -p hitop`
- HTTP client: `reqwest 0.12` blocking + rustls (no OpenSSL)
- JS engine: `boa_engine 0.22` (pure Rust, used for pre/post request scripts)
- Storage paths: `~/Library/Application Support/hitop/` (macOS), `~/.config/hitop/` (Linux)
- `MAX_HISTORY = 50` in `storage.rs`

### Desktop (`desktop/`)
- Framework: Tauri 2.0, frontend: React 19 + MUI v7, build: Vite 6
- Bundle ID: `com.bohdaq.hitop`
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
- Secrets: environment `secrets` names, masked editor; `features.js` `exportableEnvironments()`/`maskSecrets()` for exports and run history

### Website and proxy
- Hosting: VM bohdan-prod-donotdelete (168.119.17.57) behind the front rws; see `deploy/README.md`
- Build all: `./deploy/build.sh`; server tests: `cd server && cargo test`
- Old URLs must keep working: `/privacy.html` (App Store privacy URL), `/pricing.html`, `/app.html`, `/terms/*.html`
- App Store: macOS "HiTop API Client" id6785294166 ($4.99, macOS 12+), iOS "HiTop HTTP Client" id6785326628 ($2.99, iOS 17+)

### Data model
- `Collection` — has `id`, `name`, `requests[]`, `variables` (HashMap, `#[serde(default)]`), optional default `auth`, optional `workspace` (keychain key)
- `Environment` — `secrets` lists secret variable names: CLI values in the keychain, web app values in `variables` (localStorage), never in files or exports
- `Request` — has `id`, `name`, `url`, `method`, `headers[]` (`enabled`), `body`, `preRequestScript`, `postRequestScript`,
  `auth`, and the web app's `bodyMode`/`formFields`/`disabledParams`, plus `bodyType`, `examples`, `file` (folder path)
- Every model struct keeps unknown JSON keys (`#[serde(flatten)] extra`); new fields are skipped when empty, so old JSON reads and writes unchanged
- ids are `u64`: web app storage with string ids (`"col-legacy"`) doesn't load in Rust yet (open item for T5)
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
- Bundle ID `com.bohdaq.hitop`; MAS entitlements and icon set in `desktop/src-tauri/`
- Privacy policy: `https://hitop.rws8.tech/privacy.html` (old Pages URL redirects there)
- A desktop update needs a signed MAS build from the owner's Mac (upload via Transporter)

---

## Crates.io

- `hitop-core` and `hitop` 0.2.0, published to crates.io 2026-10-31 (tag `cli-v0.2.0`)
- To publish: bump `version` in `cli/Cargo.toml` (workspace), push, then push tag `cli-v<version>`; `.github/workflows/release-cli.yml` publishes core then hitop (needs the `CARGO_REGISTRY_TOKEN` secret)
