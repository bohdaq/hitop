# hitop roadmap: agent execution plan

This file is the work order for an autonomous agent building hitop from today's state to v1.x.
Strategy and reasoning live in the strategy doc
(https://claude.ai/artifact/8jLRKjNstT44vXhbHSaXQW). This file only says what to build, in what
order, and how to prove each step is done. Where the strategy doc and this file differ, this file wins:
section 2 is the source of truth.

Written 2026-11-01. Owner: Bohdan Tsap. All decisions below were made by the owner before the
work started. Don't re-ask them.

---

## 1. How the agent works through this file

1. Read this file, `CLAUDE.md`, and the status table (section 5).
2. Take the first tranche whose status is not `done` and whose `Needs` are met. Tranches marked
   `handoff` still count as done for ordering once their own work is finished. Never wait on the
   owner when a later tranche doesn't depend on the handoff.
3. Do the work. Run every command in the tranche's **Done when** list. Every one must pass.
4. Commit to `main` (small commits, one subject per change), push, and wait for CI to go green.
5. Update this file in the same push: status row (done, commits, release, date), plus any open
   items or deviations under the tranche. Record anything the owner must do in section 6
   (Handoffs).
6. Clean build output (`cargo clean` in touched workspaces, `rm -rf */dist */node_modules/.vite`)
   when free disk drops below 20 GB.
7. Continue with the next tranche, through T26 with no pause between tranches or phases (owner
   decision 2026-11-01). Stop only at a real blocker: a missing credential, a failing
   test you can't fix, or a decision this file doesn't cover. When blocked, write the question in
   section 6 and move on to any tranche that doesn't depend on it.

## 2. Decisions (fixed)

| Topic | Decision |
| --- | --- |
| Surfaces | CLI, desktop (Tauri) and web app. Browser extensions and the current iOS app are frozen. |
| Desktop UI | Tauri loads the Lit web app (`webapp/`) with a Tauri adapter. `desktop/src` (React + MUI) is retired once at parity. |
| Scope | Phases 1, 2 and 3 (through v1.x), including the iOS companion code. |
| Collection format | YAML folder, one file per request (section 4). |
| Web app and files | localStorage stays the web app's store. Import and export of collection folders as `.zip`, plus "Open folder" where the File System Access API exists. |
| Secrets | OS keychain first (macOS Keychain, Secret Service, Windows Credential Manager) through the `keyring` crate. Env vars cover CI and headless runs. |
| MCP | `hitop mcp` over stdio: list, run, create and update requests, and read results. |
| AI | Dropped by the owner (2026-11-01): no AI features. MCP (T12) stays, so users can drive hitop from their own AI tools. |
| Distribution | GitHub Releases (CLI binaries, free unsigned desktop builds), crates.io, a `ghcr.io/bohdaq/hitop` Docker image, and a GitHub Action in `bohdaq/hitop-action`. No Homebrew, winget, Scoop or AUR. |
| Store builds | Mac $4.99 and iOS $2.99 stay until v1.0. The owner changes App Store prices. |
| License | hitop-core, the CLI, desktop and web app stay MIT. Only the sync server (`sync-server/`) is AGPL-3.0-or-later. The format spec is CC BY 4.0. Contributions under the DCO (`Signed-off-by`), no CLA. (Owner, 2026-11-01: replaces the earlier GPL plus CLA plan.) |
| Telemetry | Opt-in anonymous weekly ping, off by default, in the CLI and the free unsigned desktop builds only. The web app and App Store builds (Mac, iOS) never contain the telemetry code: compile it out (Cargo feature `telemetry`, off in MAS builds), so the App Store "Data Not Collected" labels stay true. |
| hitop Cloud | Build the self-hostable AGPL sync server and client sync, deployed on the VM for testing. No billing, accounts for sale, or public launch: those wait for the owner. |
| iOS | After v1.0 the agent writes the SwiftUI companion code. The owner builds and submits it. |
| Release tags | Owner approved every `cli-v*` and `desktop-v*` tag through all 3 phases in advance (2026-11-01). The first release of each new pipeline still goes out as a `-rc` tag first. |
| Sync encryption | Industry standard construction, no custom crypto: HPKE (RFC 9180, X25519 + HKDF-SHA256 + ChaCha20-Poly1305) wraps workspace keys per member; XChaCha20-Poly1305 encrypts blobs with the workspace id, blob id and version as associated data; audited RustCrypto crates (`hpke`, `chacha20poly1305`) and `webauthn-rs` for passkeys. Threat model written down; external review before any public launch. |

## 3. Rules

**Allowed without asking:** commit and push to `main`; push `cli-v*` tags (publishes to crates.io); (owner pre-approved every tag, see section 2;
push `desktop-v*` tags (GitHub Releases); crates.io versions can only be yanked, so run the full **Done when** list before every tag); deploy the site and web app with `deploy/deploy.sh`
after green e2e; push to `bohdaq/hitop-action` and tag it.

**Never:**
- Touch `frontend/`, `extension/`, `chrome-extension/` or `chrome-store-package/` beyond fixes to
  services shared with `webapp/`.
- Commit files you didn't change. Stage explicit paths (`git add <paths>`, then
  `git commit -- <paths>`). Other sessions may have work in the tree.
- Use `git stash` or `git pull` without `--rebase --autostash`.
- Add Claude, Claude Code or Anthropic attribution to commits, tags, releases or PRs.
- Post anywhere public (Show HN, Reddit, MCP directories, Marketplace listing). Prepare the text
  and record it as a handoff.
- Change App Store Connect, prices or DNS.
- Change anything on the VM beyond what this file allows (next block).
- Store secrets in collection files, history, logs, reports or test fixtures.
- Add a feature to one surface only when it belongs to all three, unless the tranche says so.

**VM (bohdan-prod-donotdelete, 168.119.17.57), owner decision 2026-11-01:**
- Reach it through your own tunnel pod `claude-hitop-tunnel` in namespace `shalb-dev` on stage
  (`KUBECONFIG=~/.kube/stage_config`, image `alpine/socat:1.8.0.3`, cpu request 10m, args
  `tcp-listen:2222,fork,reuseaddr tcp:10.1.0.32:22`), `kubectl port-forward`, then
  `ssh -p <port> ubuntu@127.0.0.1`. Stop the port-forward by PID and delete the pod afterwards.
  Never touch other pods or the VM object itself.
- Allowed: `deploy/deploy.sh`; a hitop-server env file (T10 stats token); the sync-server unit,
  data directory and env (T23); a front rws route plus ACME name for `sync.hitop.rws8.tech` in
  `/etc/rws/rws.config.toml`. For every config change: back the file up (`.bak-hitop-<ts>`), run
  `rws --check=<file>` before `systemctl restart rws`, then verify rws8.tech, git.rws8.tech,
  hitop.rws8.tech and the mail certificate still answer (`curl --resolve <host>:443:127.0.0.1`);
  roll back at once on any failure. Nothing else on the VM (mail, Gitea, other sites) is yours.
- The VM can't reach its own public IP: probe with `curl --resolve`, and send mail through
  `127.0.0.1:587` (the existing `noreply` account, `SMTP_URL` in `/etc/hitop-sync/env`).

**Engineering standards:**
- Tests for every behaviour you add. HTTP tests run against a local mock server (`httpmock` in
  Rust, a local Node server in web tests), not the public internet. The existing e2e checks call
  example.com, httpbin.org and jsonplaceholder: when one fails, rerun once before treating it as
  a regression.
- Keep the JSON storage readable: existing users of the web app (localStorage keys), desktop and
  CLI (`~/.config/hitop/`, `~/Library/Application Support/hitop/`) must keep their data on
  upgrade. Add a migration test whenever a stored shape changes.
- `cargo clippy --all-targets -- -D warnings` clean in every Rust workspace you touch.
- Run `cargo audit` and `npm audit --omit=dev` before each release; fix highs and criticals.
- Update `CLAUDE.md` whenever a fact it states changes (stack, tree, versions, commands).

## 4. Collection format v1 (target of T1)

```
my-api/
  hitop.yaml                 # format: 1, name, variables, default auth, request order
  requests/
    get-user.yaml            # one request per file; subfolders = folders
    orders/
      create-order.yaml
  environments/
    dev.yaml                 # variables; keys marked secret have no value in the file
    prod.yaml
```

Request file fields: `name`, `method`, `url`, `params` (list with `enabled`), `headers` (list with
`enabled`), `auth` (same `{type, …}` object as today), `body` (`type`: none, json, text, xml,
form, multipart, graphql; `content`), `scripts.pre`, `scripts.post`, `examples` (saved
responses, optional). Multi-line bodies and scripts use YAML block scalars. Keys are written in a
fixed order so diffs stay small. Unknown keys are preserved on save.

Secret variables: `secret: true` in an environment file, with the value resolved in this order:
`HITOP_SECRET_<ENV>_<NAME>` env var, `--secret NAME=value`, OS keychain
(service `hitop`, account `<workspace-id>/<env>/<name>`), then an interactive prompt (TTY only).

---

## 5. Tranches and status

| ID | Tranche | Status | Commits / release | Date |
| --- | --- | --- | --- | --- |
| T0 | Bootstrap and housekeeping | done | pricing list, DEV_PLAN pointer, CLAUDE.md; deployed | 2026-11-01 |
| T1 | YAML collection format in hitop-core and CLI | done | 9697b6d, 8b1b6de, df70ec0, edf04e5 | 2026-11-01 |
| T2 | Secrets in the OS keychain | todo | | |
| T3 | CI runner: reports, exit codes, flags | todo | | |
| T4 | Postman migration: environments and pm.* | todo | | |
| T5 | Desktop = web app; web folder import/export | todo | | |
| T6 | Cookie jar and multipart in Rust and CLI | todo | | |
| T7 | Release infra: GitHub Releases, Docker image | todo | | |
| T8 | GitHub Action and GitLab template | todo | | |
| T9 | License notices, DCO, project policies | done | 51cd299; DCO verified on PR #1 | 2026-11-01 |
| T10 | Opt-in telemetry | todo | | |
| T11 | Launch readiness, release 0.3.0 / desktop 0.2.0 | todo | | |
| T12 | `hitop mcp` | todo | | |
| T13 | GraphQL | todo | | |
| T14 | WebSocket | todo | | |
| T15 | gRPC | todo | | |
| T16 | Response diff and code snippets | todo | | |
| T17 | OpenAPI import and sync | todo | | |
| T18 | Bring-your-own-key AI | dropped | owner decision | 2026-11-01 |
| T19 | v1.0 hardening and release | todo | | |
| T20 | Git UI in desktop | todo | | |
| T21 | Secret manager integrations | todo | | |
| T22 | HTML reports and scheduled runs | todo | | |
| T23 | Sync server (AGPL) | todo | | |
| T24 | Client sync, teams, cloud runs | todo | | |
| T25 | SSO and audit log | todo | | |
| T26 | iOS companion code | todo | | |

Common commands referenced below:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
(cd cli && cargo test --locked && cargo clippy --all-targets -- -D warnings)
(cd server && cargo test --locked)
(cd desktop/src-tauri && cargo check --locked)
./deploy/build.sh && (cd webapp && npm run test:unit)
# e2e: start deploy/out/hitop-server as in .github/workflows/ci.yml, then
(cd webapp && BASE=http://localhost:7879 npm run test:e2e)
```

### Phase 1: switchable (target CLI 0.3.0, desktop 0.2.0)

#### T0 Bootstrap and housekeeping
- Check the setup list in section 7: disk ≥ 40 GB free, `gh repo view bohdaq/hitop-action`
  works, the deploy path works (`deploy/README.md`), and the working tree has no uncommitted
  changes from the owner. If owner changes are still there, leave those files alone.
- Fold `DEV_PLAN.md` into this file: replace its body with a pointer to `ROADMAP.md`.
- Fix the pricing page (`site/src/pages/pricing.js`): "Included everywhere" must list environments,
  auth presets, query params and assertions. Remove the claim that every version has the full
  feature set (iOS doesn't).
- Add `ROADMAP.md` to the repo tree in `CLAUDE.md`, and fix its stale "App Store status" section
  (both apps are live; see "Website and proxy").
- **Done when:** site builds, e2e green, deployed, live `/pricing.html` shows the new list.

#### T1 YAML collection format in hitop-core and CLI
- New module `hitop-core/src/format/` that reads and writes section 4's folder layout. Pick a
  maintained serde YAML crate (`serde_yaml` is deprecated; check `serde_norway` or similar
  against RUSTSEC before adding it).
- Lossless conversion between the existing JSON model and the folder in both directions, including
  folders, params, auth, body types and scripts.
- CLI: `hitop init [dir]`, `hitop convert <json> <dir>` and `hitop convert <dir> <json>`. Every
  command that takes a collection name also accepts a path (`hitop run ./my-api`), so CI works
  from the repo, not from global storage.
- Write the spec to `spec/collection-format.md` (CC BY 4.0 header).
- **Done when:** golden-file tests for every field; round-trip property test (JSON → folder →
  JSON is equal); `hitop run ./fixtures/sample` passes against a mock server; CI green.
- **Notes (2026-11-01, done):**
  - YAML crate: `yaml_serde` 0.10.7 (serde_yaml continued by the YAML organization, released
    2026-08, no RustSec advisory, nor for its `libyaml-rs`). Rejected: `serde_yml` (RUSTSEC-2025-0068,
    unsound, unpatched; its `libyml` is RUSTSEC-2025-0067), `serde_yaml`/`unsafe-libyaml`
    (deprecated; RUSTSEC-2024-0449 unmaintained), `serde_norway` (last release 2024-12),
    `serde-saphyr` (pulls figment, garde, validator, miette).
  - Format additions beyond section 4, needed for a lossless round trip (all in the spec): `id` in
    `hitop.yaml` and request files (left out when it equals the id derived from the path); the
    request's path is its identity, kept in JSON as `file` only when it isn't the default slug;
    `order` in `hitop.yaml`; `body.fields` for form and multipart; `bodyType` in JSON when a text
    body's `type` differs from the inferred one; environment `secrets` list in JSON.
  - Normalizations: blank header rows (the editor's placeholder) aren't written; `bodyMode: "raw"`
    reads as no mode; environments come back in file name order. The property test compares
    after these. A URL's query moves to `params` only when the URL rebuilds byte for byte.
  - Model: every struct keeps unknown JSON keys (`extra`); new fields are skipped when empty, so
    existing JSON storage reads and writes unchanged (tests in `model.rs`). Collection-level
    default auth is applied by the runner; disabled headers aren't sent.
  - Fixed on the way: a pre-request script dropped every header it didn't set (9697b6d).
  - `hitop run <folder>` uses only the folder's `environments/`, and only with `--env`. The mock
    test copies `cli/fixtures/sample` to a temp dir, adds `environments/mock.yaml` with the mock
    port and runs `hitop run ./fixtures/sample --env mock` there (`--var` stays in T3).
  - Paths work in `run`, `requests *`, `variables *`, `export` and `send -c`. `collections
    list/add/delete/rename/duplicate` and `import` still manage global storage only (a folder is
    renamed in `hitop.yaml`, deleted with `rm`).
  - CI `cli` job now also runs `cargo clippy --all-targets -- -D warnings`.
  - Tests: hitop-core 84 unit + 4 format (golden x3, proptest 512 cases); hitop 34 + 9 folder.
  - Open for T5: web app storage may hold string ids (`"col-legacy"` in `legacy-storage.json`);
    the Rust model's `u64` ids reject them, so `hitop convert` fails on such an export. The web
    app also drops request fields it doesn't know (`file`, `examples`, `bodyType`, unknown keys)
    when it saves a tab; fix both when the web app works on folders.
  - Open for T26: files are YAML 1.2 (`on`/`yes` unquoted are strings); Yams must read them with
    the 1.2 core schema or quote-sensitive values like `on` change meaning.

#### T2 Secrets in the OS keychain
- `hitop-core/src/secrets.rs` on top of `keyring`: resolve order from section 4. Headless Linux
  without Secret Service must fall back to env vars with a clear message, not a panic.
- CLI: `hitop secret set|get|rm|ls --env <env>`; `--secret NAME=value` on `send` and `run`.
- Mask secret values as `••••` in CLI output, history, reports and runner logs.
- Web app: no keychain in a browser. Secret values stay in localStorage, are masked in the UI and
  are never written to exports (zip or JSON). Say so in the UI next to the secret toggle.
- **Done when:** unit tests with a mock keyring backend; test that no secret value appears in
  history, exports or reports; CI green on Linux (env var path).

#### T3 CI runner: reports, exit codes, flags
- `hitop run <path|name> --reporter cli,junit,json --out <dir>`; per request: status, timings,
  each assertion with pass/fail and message.
- Flags: `--env <name>`, `--env-file <yaml>`, `--var k=v`, `--bail`, `--timeout <ms>`,
  `--iterations <n>`, `--data <csv|json>` (data-driven runs).
- Exit codes: 0 all pass; 1 a request or assertion failed; 2 usage or config error. Document them in
  `--help` and the site guide.
- **Done when:** a schema-shape test checks the JUnit XML (root `testsuites`, `testsuite`/`testcase` attributes, `failure` elements with messages); GitHub's test-report
  action renders it (checked in T8); CI green.

#### T4 Postman migration: environments and pm.*
- Import Postman environment and globals JSON into environment files (secret type → `secret:
  true`, value goes to the keychain on desktop and CLI).
- Import nested folders, collection-level auth and scripts.
- `pm.*` shim as one JS prelude (`shared/pm-shim.js`), used by the CLI and desktop through
  `include_str!` in `scripting.rs` and by the web app's script service: `pm.environment`,
  `pm.collectionVariables`, `pm.variables` get/set; `pm.response.code/json()/text()/headers`;
  `pm.request`; `pm.test` and a chai-style `pm.expect` subset mapped onto `assert.*`.
- Import report: every script line that uses an unsupported `pm.*` call, shown in the web app and
  printed by `hitop import`.
- **Done when:** fixtures from three real-world public Postman collections import and run against
  a mock server; report lists unsupported calls; CI green.

#### T5 Desktop = web app; web folder import/export
- Add a platform adapter in `webapp/src/platform/`: `web` (current fetch/proxy and localStorage)
  and `tauri` (Tauri `invoke` for requests, scripts, storage, environments, secrets, and folder
  open/save through the dialog plugin).
- Point `desktop/src-tauri/tauri.conf.json` at a separate web app build for Tauri. `npm run build`
  bakes in `VITE_HITOP_PROXY=/api/proxy`; the Tauri build must not, or desktop requests go through
  the VM. Keep the bundle id, the MAS
  entitlements and existing user data (same hitop-core storage).
- Desktop works on collection folders directly (open folder, live save).
- Web app: export a collection as `.zip` of the folder, import `.zip`, and "Open folder"
  where `showDirectoryPicker` exists. YAML conversion in the browser must match hitop-core
  exactly: either build the format module to WASM (`cli/crates/hitop-wasm`, wasm-bindgen) or
  share fixtures that both implementations must pass. Prefer WASM.
- Delete `desktop/src` React code and its dependencies only after the owner's macOS check (the
  handoff below) passes: the Mac App Store app is live. Until then keep both, with the Lit build
  behind a config switch.
- CI: desktop job builds the web app with the Tauri adapter and runs `cargo check`; add a
  Playwright run of the web app in Tauri mode against a mocked `invoke`.
- **Done when:** all web e2e suites pass in both modes; desktop builds on the CI Linux runner;
  old desktop data loads (migration test). **Handoff:** owner runs the macOS build and checks
  requests, files and the keychain.

#### T6 Cookie jar and multipart in Rust and CLI
- Cookie jar in the Rust transport (persisted per workspace), used by desktop and CLI.
- CLI multipart and form bodies (`-F name=value`, `-F file=@path`).
- **Done when:** mock-server tests for cookies across requests and multipart file upload; CI green.

#### T7 Release infra: GitHub Releases, Docker image
- `desktop-v*` tag → `tauri-apps/tauri-action` matrix: macOS universal `.dmg`, Windows
  `.msi`, Linux `.AppImage` and `.deb`, all unsigned (macOS ad-hoc signed; owner decision 2026-11-01, no Developer ID), attached to a GitHub Release with notes that
  explain the unsigned-app warnings.
- `cli-v*` tag → crates.io publish (existing `release-cli.yml`) plus CLI binaries for the five
  targets in `cli/Cargo.toml` (regenerate the cargo-dist workflow with the current `dist`, or a
  plain matrix job), and `[package.metadata.binstall]` so `cargo binstall hitop` works.
- `cli-v*` tag → multi-arch `ghcr.io/bohdaq/hitop:<version>` and `:latest` (static musl binary,
  distroless base), pushed with `GITHUB_TOKEN`.
- Site: `/download` page with links to the latest release assets.
- **Done when:** a dry-run release on a `-rc` tag produces every asset and image; the image runs
  `hitop run` on a fixture. **Handoff:** owner sets the ghcr package visibility to public once.

#### T8 GitHub Action and GitLab template
- `bohdaq/hitop-action`: `action.yml` (inputs: `path`, `env`, `env-file`, `reporter`, `version`,
  `secrets` as env vars), runs the Docker image, uploads JUnit as an artifact and writes a job
  summary. Tag `v1` and `v1.0.0`.
- Self-test workflow in the action repo against a fixture collection.
- `site/content/guides/`: "Run hitop in GitHub Actions" and "Run hitop in GitLab CI" (with a
  `.gitlab-ci.yml` template).
- **Done when:** the action's self-test passes on GitHub. **Handoff:** owner ticks "Publish to
  Marketplace" on the release.

#### T9 License notices, DCO, project policies
- Keep MIT for everything that exists today (`LICENSE`, every `license` field). `sync-server/` (T23)
  gets its own `LICENSE` (AGPL-3.0-or-later) and `license = "AGPL-3.0-or-later"`; the format spec
  gets a CC BY 4.0 header.
- DCO: a CI check that every commit in a PR has `Signed-off-by` (for example
  `tim-actions/dco` or a small script job), and `CONTRIBUTING.md` explaining `git commit -s`.
- `SECURITY.md` (private reporting through GitHub security advisories), `TRADEMARK.md` (the name
  "hitop" stays with the owner), `CODE_OF_CONDUCT.md`.
- `cargo deny check licenses` in CI for every Rust workspace (sync server may depend on
  AGPL-compatible crates; MIT crates must not pull GPL or AGPL dependencies).
- **Done when:** `cargo deny check licenses` passes in CI; a test PR without sign-off fails the DCO
  check.
- Notes (done 2026-11-01): `deny.toml` at the root; run it as
  `cargo deny --config deny.toml --manifest-path <ws>/Cargo.toml check licenses` (the `--config`
  flag must come before `check`). Workflows `licenses.yml` (matrix cli, server, desktop) and
  `dco.yml` (script job, bots exempt). Test PR #1 failed on an unsigned commit and passed after
  `--amend -s`; closed, branch deleted. Private vulnerability reporting enabled on the repo. Add
  `sync-server` to the `licenses.yml` matrix in T23 with its own AGPL-aware deny config. Commit
  with `git commit -s` from now on.

#### T10 Opt-in telemetry
- `hitop-core/src/telemetry.rs`: when enabled, at most one ping per week: random install id,
  surface, version, OS. Nothing else.
- Telemetry code sits behind the Cargo feature `telemetry`: on for the CLI and the GitHub Release
  desktop builds, off for Mac App Store builds and absent from the web app. A CI check builds the
  MAS configuration and asserts no ping code or endpoint string is in it.
- CLI `hitop telemetry on|off|status`; a desktop settings toggle; off by default, asked once on
  first run (desktop) with "No" as the default button.
- `server/`: `POST /api/ping` keeps daily aggregate counts only (no IP, no install id stored past
  de-duplication for the week); `GET /api/stats` behind a token from the server env.
- Update `PRIVACY.md` and the site privacy page.
- **Done when:** server tests for de-duplication and that no IP is stored; client test that
  nothing is sent while off; deployed.

#### T11 Launch readiness, release 0.3.0 / desktop 0.2.0
- Launch-gate scenario as an automated test: import a Postman collection and environment, switch
  environment, authenticate, run in CI through the action, get a JUnit report.
- Site: comparison pages (Postman, Bruno, Insomnia, Yaak) with facts and a checked date; guides
  "Migrate from Postman" and "Collections in git"; README rewrite around the positioning line;
  a `vhs` tape (`site/demo/demo.tape`) for the terminal GIF.
- Release CLI 0.3.0 and desktop 0.2.0; deploy.
- Write the launch texts (Show HN, r/rust, r/webdev, This Week in Rust, Product Hunt) to
  `launch/` for the owner.
- **Done when:** scenario test green in CI; releases published; live site updated.
  **Handoffs:** owner records the GIF if `vhs` can't run here, builds the MAS update, and posts
  the launch.

### Phase 2: different (target v1.0)

#### T12 `hitop mcp`
- Stdio MCP server with the official Rust SDK (`rmcp`). Workspace = a collection folder path
  (`hitop mcp ./my-api`) or global storage.
- Tools: `list_collections`, `list_requests`, `get_request`, `run_request`, `run_collection`,
  `create_request`, `update_request`, `list_environments`, `set_environment`. Secrets are never
  returned; outputs mask them. No delete tool.
- Guide: using hitop with Claude Code and Cursor, with a `.mcp.json` example.
- **Done when:** JSON-RPC integration tests for every tool; manual check with an MCP client
  recorded in the tranche notes. **Handoff:** listing in MCP directories (text prepared).

#### T13 GraphQL
- Body type `graphql` (query plus variables JSON) in the format, core, CLI and web app;
  introspection fetch; schema-aware completion in CodeMirror.
- **Done when:** tests against a local GraphQL mock; e2e for the editor; format round-trip.

#### T14 WebSocket
- WebSocket requests in the format; desktop and CLI through Rust (`tokio-tungstenite`), the web
  app through the browser `WebSocket`. Connect, send saved or typed messages, timestamped log.
- CLI `hitop ws <url>` with interactive send.
- **Done when:** tests against a local echo server in Rust and e2e.

#### T15 gRPC
- Unary and server-streaming calls in desktop and CLI (`tonic` + `prost-reflect`), schema from
  server reflection or imported `.proto` files. The web app shows "desktop and CLI only".
- **Done when:** tests against a local tonic test server.

#### T16 Response diff and code snippets
- Compare two responses (history items or saved examples) side by side, with JSON-aware diff.
- Code snippets: curl, JavaScript fetch, Python requests, Go net/http, Java HttpClient, Rust
  reqwest. Generator in hitop-core, used by the web app through `hitop-wasm` and by the CLI as
  `hitop snippet <request> --lang <lang>`.
- **Done when:** snippet fixtures per language; the generated curl runs against a mock server and
  matches.

#### T17 OpenAPI import and sync
- OpenAPI 3.0 and 3.1 (YAML or JSON, file or URL) → collection folder, in hitop-core (WASM for the
  web app).
- Sync: re-import shows added, changed and removed operations and keeps user edits (scripts,
  examples, auth).
- **Done when:** fixtures (Petstore plus two large public specs) import; sync test preserves
  edits.

#### T18 Bring-your-own-key AI
- Dropped by the owner on 2026-11-01. Skip it; nothing later depends on it.

#### T19 v1.0 hardening and release
- Freeze format v1 (spec marked stable, a compatibility test suite).
- `cargo fuzz` targets for the YAML reader, Postman importer and OpenAPI importer, run in CI for a
  short time per push; `cargo audit` and `npm audit` in CI.
- Release CLI 1.0.0 and desktop 1.0.0; deploy.
- **Done when:** fuzzers run clean for 30 minutes locally; releases published. **Handoffs:** MAS
  build 1.0; App Store price change ($9–19) if the owner wants it.

### Phase 3: teams (v1.x)

#### T20 Git UI in desktop
- Status, diff, commit, pull, push, branches and conflict resolution for the open collection
  folder, through `git2` with vendored libgit2 (the MAS sandbox can't run the `git` binary).
- **Done when:** tests on temp repos for commit, branch, merge conflict and resolution.

#### T21 Secret manager integrations
- References like `${secret:vault:path#key}`, `${secret:aws:name}`, `${secret:azure:vault/name}`
  and `${secret:op:item/field}`, resolved at run time in desktop and CLI.
- **Done when:** tests against local dev servers (Vault dev mode, LocalStack) and a stubbed `op`.

#### T22 HTML reports and scheduled runs
- `--reporter html` (a single self-contained file).
- `hitop schedule` writes a scheduled GitHub Actions workflow or a cron entry for a collection.
- **Done when:** HTML report snapshot test; generated workflow validates with `actionlint`.

#### T23 Sync server (AGPL)
- New `sync-server/` (Rust, SQLite by default, Postgres optional), AGPL-3.0.
- End-to-end encryption as in section 2 (HPKE key wrapping, XChaCha20-Poly1305 blobs with
  associated data, no custom primitives). Rotate the workspace key when a member is removed.
  Publish test vectors and `sync-server/THREAT_MODEL.md`.
- Sign-in by email magic link (SMTP from server config) and passkeys.
- Docker image, self-host guide, systemd unit; deploy to the VM at the hostname from section 7.
- **Done when:** server tests for auth, blob versioning and conflicts; test vectors pass in Rust and in the web app; a test that the server never
  sees plaintext; deployed and reachable.
  **Handoff:** external review of the encryption design and code before any public launch.

#### T24 Client sync, teams, cloud runs
- Sync in desktop, CLI and web app; conflict UI; invites; roles owner, editor and viewer.
- Scheduled runs and monitors: on a self-hosted runner, or on the server only when a workspace
  owner explicitly shares a run key (documented trade-off with E2E encryption).
- No billing and no paid accounts.
- **Done when:** two-client sync e2e test; role enforcement tests.

#### T25 SSO and audit log
- OIDC sign-in and an audit log in the sync server, open source.
- **Done when:** tests against a local OIDC provider (for example Keycloak in a container).

#### T26 iOS companion code
- Swift package `HitopKit` (format reader with Yams, runner on URLSession, environments, keychain
  secrets), testable with `swift test` on Linux when a Swift toolchain is available.
- SwiftUI app in `ios/`: open collection folders from Files or iCloud Drive, run requests and
  collections, switch environment, App Intents for Shortcuts, a status widget. No editing.
- **Done when:** `HitopKit` tests pass (on Linux or in the owner's Xcode). **Handoff:** owner
  builds, tests and submits.

---

## 6. Handoffs and open questions

Agent appends here; owner ticks when done.

| Date | Tranche | What the owner needs to do | Done |
| --- | --- | --- | --- |

## 7. Setup the owner does before the agent starts

Checked 2026-11-01:

- [x] Open changes in `webapp/` committed (UI review fixes, 49ceefe).
- [x] Disk: 88 GB free.
- [x] Empty public repo `bohdaq/hitop-action` exists.
- [x] `CARGO_REGISTRY_TOKEN` works (published 0.2.0 on 2026-10-31). Owner confirmed (2026-11-01)
      it is valid until at least mid-2027.
- [x] Deploy path works through the tunnel pod (`deploy/README.md`), used 2026-11-01.
- [x] Before T23 (Phase 3), done 2026-11-01 (DNS verified; SMTP uses the existing `noreply`
      account over `smtp://…@127.0.0.1:587`, so check the certificate against `mail.rws8.tech`): DNS A record `sync.hitop.rws8.tech` → 168.119.17.57, and an SMTP
      mailbox for magic links (for example `noreply@rws8.tech`) with its password placed on the VM
      in `/etc/hitop-sync/env` as `SMTP_URL`.
- [x] Swift 6.4.2 via swiftly (2026-11-01; `swift test` works; non-login shells need
      `. ~/.local/share/swiftly/env.sh`).
