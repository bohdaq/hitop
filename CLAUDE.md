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
│       │       ├── runner.rs       # Sequential collection runner
│       │       └── export.rs       # HITOP and Postman v2.1 import/export
│       └── hitop/              # Binary crate (published to crates.io)
│           └── src/
│               ├── main.rs         # clap CLI entry point
│               └── output.rs       # Colored output, syntect highlighting
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
├── frontend/                   # React web app (used by browser extensions)
│   ├── index.html              # Vite entry point (root of frontend/)
│   ├── vite.config.js          # Vite 6, outDir: build, multi-entry (main + sandbox)
│   └── src/                    # All files are .jsx (not .js)
├── extension/                  # Firefox extension source
├── chrome-extension/           # Chrome extension source
├── docs/                       # GitHub Pages site (bohdaq.github.io/hitop)
│   └── privacy.md              # Privacy policy — URL used for App Store submission
└── CLAUDE.md                   # This file
```

---

## Key technical facts

### Rust CLI (`cli/`)
- Workspace version: `0.1.2` (both crates in sync)
- Rust toolchain: `stable` via `cli/rust-toolchain.toml` — always use `~/.cargo/bin/cargo`, NOT Homebrew rustc
- Run tests: `cd cli && ~/.cargo/bin/cargo test`
- Publish: `cd cli && cargo publish -p hitop-core` then `cargo publish -p hitop`
- HTTP client: `reqwest 0.12` blocking + rustls (no OpenSSL)
- JS engine: `boa_engine 0.19` (pure Rust, used for pre/post request scripts)
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
- Icons generated from `docs/favicon.svg` via `rsvg-convert` + `npx tauri icon`

### Frontend (`frontend/`)
- Build tool: Vite 6 (migrated from CRA — zero vulnerabilities)
- All source files use `.jsx` extension (not `.js`)
- Build output: `build/` (compatible with extension build pipeline)
- Dev server: `npm start` (port 3000)
- Build: `npm run build`
- Extension build: `npm run build:extension` (copies manifest_extension.json)

### Data model
- `Collection` — has `id`, `name`, `requests[]`, `variables` (HashMap, `#[serde(default)]`)
- `Request` — has `id`, `name`, `url`, `method`, `headers[]`, `body`, `preRequestScript`, `postRequestScript`
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

- Apple Developer account: active (renewed June 2025)
- Bundle ID `com.bohdaq.hitop` — needs to be registered at developer.apple.com
- Entitlements and icon set ready
- Privacy policy: `https://bohdaq.github.io/hitop/privacy`
- Next step: register App ID → create certificates → sign and build → upload via Transporter

---

## Crates.io

- `hitop-core` and `hitop` both at version `0.1.2`
- To publish a new version: bump `version` in `cli/Cargo.toml` (workspace), then publish core first, then hitop
