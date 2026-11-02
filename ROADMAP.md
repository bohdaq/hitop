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
| T2 | Secrets in the OS keychain | done | accd2a4, 20951a8, 8104002, b15cb14 | 2026-11-01 |
| T3 | CI runner: reports, exit codes, flags | done | 3ee1091, ea5b43c, 8304dcc, aa5f226 | 2026-11-01 |
| T4 | Postman migration: environments and pm.* | done | cee58cb, 56bda1c, c629096, 1eaf04e, e4ed688, 53a0156 | 2026-11-01 |
| T5 | Desktop = web app; web folder import/export | handoff | a1e7704, 767f0d2, c7ae8f5, 37a6385, 54611a7, f527ceb; macOS check in section 6 | 2026-11-01 |
| T6 | Cookie jar and multipart in Rust and CLI | done | a87f4ac, cb20c93, eacc7fb | 2026-11-01 |
| T7 | Release infra: GitHub Releases, Docker image | done | ee41820; dry run cli-v0.3.0-rc.1, desktop-v0.2.0-rc.1; deployed | 2026-11-01 |
| T8 | GitHub Action and GitLab template | done | 52f26f5, 0595081; cli-v0.3.0-rc.2; hitop-action aa873b7, 1223c47, tags v1, v1.0.0 | 2026-11-01 |
| T9 | License notices, DCO, project policies | done | 51cd299; DCO verified on PR #1 | 2026-11-01 |
| T10 | Opt-in telemetry | done | server 032ec68, 15f4b4e (deployed from a112885); client a64c6c6, 4b8b898, released in cli-v0.3.0 / desktop-v0.2.0 | 2026-11-01 |
| T11 | Launch readiness, release 0.3.0 / desktop 0.2.0 | done (handoffs) | bc1fdc0, f8a78e7, 7e2889f, e2f5788, ddcca14, 9327b37, b91e7b9, 31414f3; cli-v0.3.0, desktop-v0.2.0; hitop-action 37701e4, v1.0.1 | 2026-11-01 |
| T12 | `hitop mcp` | done (handoff) | c0a55c8, 1d6986f, faad9aa; MCP directory listing text in section 6 | 2026-11-01 |
| T13 | GraphQL | done | 208583d, a78211d | 2026-11-01 |
| T14 | WebSocket | done | ac00903, 743db7d, 538401a, 2eac6a7, 0e23002, 61ea4c0, 8938060 | 2026-11-01 |
| T15 | gRPC | done | 45b6c8a, c98955c, a018a6d, ac0c0ad, f22e395, c96f81f | 2026-11-01 |
| T16 | Response diff and code snippets | done | 5c550ac, 9b88ff5, 0d4caa8, 8acd583, 344d07d | 2026-11-01 |
| T17 | OpenAPI import and sync | done | 68d6a14, 3628f76, d1878c0, 45ac576, e5292df, ed397f1, a1fd78e, be02942 | 2026-11-01 |
| T18 | Bring-your-own-key AI | dropped | owner decision | 2026-11-01 |
| T19 | v1.0 hardening and release | done (handoffs) | fb37d84, 1b51e9f, 8503242, addb6db, e6babe2, a49eb5b, 27d6ce7, ce257e8, 26715ba, e4ca5e2, 274fb75; cli-v1.0.0, desktop-v1.0.0; hitop-action c623e3b; site not deployed | 2026-11-01 |
| T20 | Git UI in desktop | done (handoff) | 3837aec, e3f5d49, 2229a15; CI 41306134755; macOS check in section 6 | 2026-11-01 |
| T21 | Secret manager integrations | done (handoff) | 47d72f0, b375fb8, bdd1f67, 9d12eaf, 7979fa5; CI 41306134755; real-account check in section 6 | 2026-11-01 |
| T22 | HTML reports and scheduled runs | done | 9c3302e, 7dd53a3, 2cdc097, 0638ca4; CI 41289440630 | 2026-11-01 |
| T23 | Sync server (AGPL) | done (handoff) | 5cd2ac2, 9a8119f, 35f2775, f08bbce, a3b6c7d, ce07adf; deployed https://sync.hitop.rws8.tech | 2026-11-01 |
| T24 | Client sync, teams, cloud runs | done (handoff) | 7aea6c7, cf840bd, 3213817, ab721a2, ae52378, 320eaff, e119f0f, 67dde26, fafab1d, 4a0fb9f, 24f18a0, 533aa7a, 7283511; CI 41323865763; sync server (migration 2) + site deployed 2026-11-01 from b8ae78d | 2026-11-01 |
| T25 | SSO and audit log | done (handoff) | 85d7aa3, 1a8b6bd, b88ecc3, 89644fd, 85b0d45, cfbc6cc, 46aaa5e, 1c2e04d, b5d974d; CI 41331504040; sync server (migration 3) + site deployed 2026-11-01 from 0334d26 (CI 41332721114); hosted-SSO decision in section 6 | 2026-11-01 |
| T26 | iOS companion code | done (handoff) | 981c736, c1bfa7f, 24c00c5, f3332cb; CI 41309724315 (HitopKit on Linux); Xcode build in section 6 | 2026-11-01 |
| Brand | Brand unification (owner request 2026-11-01) | done (handoffs) | b924e03, ebc7438, bc588a1; CI 41317707104; hitop-action 8931565 (untagged) | 2026-11-01 |
| UX | Web app UX review fixes (reviews/2026-11-01-webapp-ux) | done | 2bf3443, 0e55b0b, 6eb61be, c8fdaa5, 8e6c75f, 205c3ec; CI 41411458623; released in desktop-v1.1.0, web app deployed 2026-11-02 | 2026-11-02 |
| 1.1 | 1.1 owner follow-ups: MCP WebSocket/OpenAPI tools, row-by-row OpenAPI sync, secret manager settings | done | cf400c6, 75d5a1a, ef5ab83, bc1d19a; released in cli-v1.1.0 / desktop-v1.1.0 (2026-11-02) | 2026-11-01 |
| 1.1.0 | Release 1.1.0: CLI (hitop, hitop-core 1.1.0, first hitop-sync-crypto 0.1.0 on crates.io), ghcr image, desktop; sync server + site deployed | done | c79dd1c, 4ebb2e8 (release-cli runs the sync tests with the server + Keycloak), 3055102 (ID token iat/nbf/exp checks); rc cli-v1.1.0-rc.1 (failed: no sync server in release tests) / rc.2, desktop-v1.1.0-rc.1; releases 41420105353, 41420109000 | 2026-11-02 |
| 1.2.0 | Release 1.2.0: signed (authenticated) key wraps, same Content-Type on every surface, web app UX deferred items; hitop-sync-crypto 0.2.0 | done | b1233c3; sync server deployed first (no migration), CI 41449435386; rc.1 41452055030 / 41452059390; releases 41454956040 (CLI) / 41454960465 (desktop); site + web app deployed 2026-11-02 | 2026-11-02 |
| 1.2 | Same Content-Type on every surface (owner-approved 2026-11-02, release 1.2.0) | done (handoff) | fb636e1 (hitop-core `sending`), d718e28 (hitop send, CLI + MCP tests), 9505a3c (web app through hitop-wasm), ae47ae1 (HitopKit), edc28c5 (spec/sending.md, README, guide, CHANGELOG), ROADMAP; CI 41436745770; not tagged | 2026-11-02 |
| 1.2 UX | 1.2 Web app UX: deferred items (reviews/2026-11-01-webapp-ux, owner-approved 2026-11-02, release 1.2.0) | done (handoff) | 52598e0 (IndexedDB bodies), d974170 (Cancel, time limit, desktop `send_cancel`), d9b704c (Settings page), 66ed313 (additive import), 81b89e7 (split panes, Mod+P), 8dcac28 (tab ×, History page, request menu, folder groups), e531e29 (docs), 71cc52d (id-less tabs); CI 41442784706; not tagged | 2026-11-02 |
| 1.2 | Signed key wraps: authenticated workspace key wraps in hitop sync (owner-approved 2026-11-02, release 1.2.0) | done (handoff) | e86896d (hitop-sync-crypto 0.2.0), 8af9f85 (sync server), 66610fd (CLI), be8338a (hitop-wasm), 3c82306 (web app + desktop), 607ee2a (docs), 3551e5b (CHANGELOG); CI 41442784706 (on 71cc52d, every job green); not tagged, not deployed | 2026-11-02 |

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
- **Notes (2026-11-01, done):**
  - Crates: `keyring` 4.2 (default `v1` feature: macOS Keychain, Windows Credential Manager, Secret
    Service over `zbus` with `crypto-rust` on Linux, so no libdbus and no C) behind the hitop-core
    feature `keychain` (default on; off for the future wasm build, then every keychain call is
    "unavailable"). `rpassword` 7.5 for the prompt, in the CLI only. Licenses pass `cargo deny`.
    RustSec: no vulnerability in the new crates; three "unmaintained" notices (RUSTSEC-2026-0370,
    -0371, -0372: `zvariant`, `zbus_names`, `zvariant_derive` were folded into zbus 6) that go away
    when keyring's zbus store moves to zbus 6. Checked the pre-existing advisories too:
    webpki-roots 1.0.8 (RUSTSEC-2026-0381) bumped to 1.0.11 in the CLI and desktop locks (8104002);
    bincode (syntect) and number_prefix (indicatif) stay unmaintained notices.
  - Workspace id (spec section 2 and 5): `workspace` in `hitop.yaml` (random 32 hex digits from
    `hitop init`; `Collection.workspace` in JSON), else the collection id in decimal; global
    storage is `global`. Env var names: upper case, every non-alphanumeric → `_`
    (`HITOP_SECRET_MY_ENV_API_KEY`). Keychain account uses the environment name as written.
  - `HITOP_KEYCHAIN=off` skips the keychain (CI runners that shouldn't touch D-Bus, and the CLI
    tests). The keychain is asked only for secrets no env var or `--secret` gave, so a CI run on
    env vars never touches it or prints a note. Checked on this machine (no Secret Service): with
    no bus, a bus without `org.freedesktop.secrets`, or `HITOP_KEYCHAIN=off`, hitop fails or falls
    back in under 50 ms with the reason and the env var to set; no panic.
  - CLI: `hitop secret set|get|rm|ls --env <env> [-c <folder or name>]`; `set` reads the value from
    the argument, a prompt, or stdin, stores it, and marks the variable secret (global
    `environments.json` `secrets`, or only that folder environment file via
    `format::mark_secret`). `rm` removes the keychain value only (the variable stays secret);
    `ls` shows where each value comes from, never the value. `--secret NAME=value` on `send`,
    `run` and also `requests send`. `env show` prints `••••`, `env set` refuses a secret name,
    `env export` writes `secrets` without values, `env import` reads them back.
  - Masking (`secrets::Masker`, also the percent-encoded form): response body and headers, test
    names and details, errors, history `url`; `runner::run_collection_with` masks every step
    before `on_step` and in what it returns (T3 reports build on those). `run_collection` is kept
    for the desktop. Scripts still see the real values.
  - Web app: a Secrets section per environment (`lit-material-key-value-editor` with
    `mask-values`, a "Show secret values" switch, and the note), `secrets: [names]` on the
    environment with values in `variables` (localStorage). Collection run history items and run
    results are masked; the environments export keeps names only. There is no zip export yet:
    T5's must use `exportableEnvironments()` from `webapp/src/features.js`.
  - Deviations: the "secret toggle" is a separate Secrets editor plus a show switch, because the
    key-value editor masks all rows or none. The single request view still shows the response as
    received (the browser keeps no history of interpolated values for it; its history item keeps
    the `${var}` templates). The real keychain round trip (set, get, rm against macOS Keychain or
    a Secret Service) isn't exercised by tests: CI has none and neither has this machine; the
    store logic runs against `MemoryStore`. See the handoff in section 6.
  - Tests: hitop-core 96 unit (secrets 9, runner 1, format 2 new) + 4 format; hitop 34 + 9 folder
    + 7 secrets (send and run masking, env var and flag paths, missing-secret error without a TTY,
    no keychain, `env show/export`, `convert`); web unit checks for export stripping, masking and
    environments stored before T2; e2e checks for the Secrets section, its export and masked run
    history.

#### T3 CI runner: reports, exit codes, flags
- `hitop run <path|name> --reporter cli,junit,json --out <dir>`; per request: status, timings,
  each assertion with pass/fail and message.
- Flags: `--env <name>`, `--env-file <yaml>`, `--var k=v`, `--bail`, `--timeout <ms>`,
  `--iterations <n>`, `--data <csv|json>` (data-driven runs).
- Exit codes: 0 all pass; 1 a request or assertion failed; 2 usage or config error. Document them in
  `--help` and the site guide.
- **Done when:** a schema-shape test checks the JUnit XML (root `testsuites`, `testsuite`/`testcase` attributes, `failure` elements with messages); GitHub's test-report
  action renders it (checked in T8); CI green.
- **Notes (2026-11-01, done):**
  - Core: `report.rs` builds the JSON report and JUnit XML from the runner's masked steps, with no new
    dependency (XML escaping, XML 1.0 character filtering and RFC 3339 timestamps written by hand), so
    `desktop/src-tauri/Cargo.lock` is unchanged. `runner::run_collection_opts` takes `RunOptions`
    (`stop_on_fail`, `timeout`, `overrides`); `run_collection` and `run_collection_with` are thin wrappers,
    `RunStepResult` only gained `duration_ms` (the whole step). `http::execute_with` adds the timeout
    (`execute` unchanged for the desktop). `format::read_environment_file` reads `--env-file`.
  - JSON report: `spec/run-report.md` (CC BY 4.0). `reportVersion: 1`, `tool`, `startedAt`, `durationMs`,
    `passed`, `bailed`, `summary` (collections, iterations, requests and assertions as total/passed/failed),
    then `collections[] → iterations[] (data row) → requests[] (status, responseTimeMs, durationMs, error,
    failure line) → assertions[] (name, passed, message)`. Fields may be added within v1; readers ignore
    unknown keys; breaking changes bump the version. No response bodies.
  - JUnit: `<testsuites>` → one `<testsuite>` per collection and iteration (`<properties>`: iteration,
    environment, `data.<name>`) → one `<testcase>` per request (`classname` = collection). A failed request
    has exactly one child: `<error type="RequestError">` (no response) or `<failure type="AssertionError">`
    / `type="HttpStatus"`; `message` is the failure line, the text lists every assertion. Passing cases list
    them in `<system-out>`. The shape test parses it with `roxmltree` (CLI dev-dependency, MIT/Apache).
  - Exit codes: `main.rs` maps an `Err` from `run` to 2 (clap usage errors were already 2); request and
    assertion failures are 1. In `--help` (`after_help`), the CLI README and the guide.
  - Behaviour change: a run no longer stops at the first failed request by default; `--bail` does that
    (across iterations and `--all` collections too). `--no-stop-on-fail` stays as a hidden, accepted no-op.
  - Decisions: variable order collection < environment (`--env` or `--env-file`, which conflict) < data row
    < `--var` < secrets; `--var` is not masked (documented). `--iterations` and `--data` conflict (one
    iteration per row). Script context starts empty in every iteration. `--timeout` is per request.
    `--out` defaults to `hitop-reports`; the `cli` reporter alone writes no files. `--data`: CSV with a
    header row (RFC 4180, parser in `commands/data.rs`, no crate) or a JSON array of objects (values turned
    into strings).
  - Site guide `site/content/guides/run-collections-in-ci.md` (`/terms/run-collections-in-ci.html`), built,
    not deployed. A GitLab example was left out: the ghcr image is distroless (no shell for `script:`), and
    T8 writes the GitLab template.
  - Tests: hitop-core 103 unit (report 6, runner 1 new) + 4 format; hitop 3 unit (data) + 34 cli + 9 folder
    + 12 run + 7 secrets. `cargo deny` licenses ok. `cargo audit` isn't installed here; the only new crate is
    the dev-dependency `roxmltree` 0.21.2, with no RustSec advisory (checked in rustsec/advisory-db).

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
- **Notes (2026-11-01, done):**
  - Shim: `shared/pm-shim.js` defines `__hitopPm(api)` (pm plus Postman's legacy names) and
    `__hitopPmUnsupported(src)` (the report check, with the supported path list in the same file).
    hitop-core embeds a byte-for-byte copy, `cli/crates/hitop-core/pm-shim.js`
    (`scripting::PM_SHIM`), because `cargo publish` only packages files inside the crate and a
    symlink breaks Windows checkouts; the test `shared_shim_is_the_packaged_copy` fails when the two
    differ (copy `shared/` over). The web app loads `shared/pm-shim.js?raw` (`webapp/src/pm.js`),
    the Tauri mock (`webapp/tests/tauri-mock.cjs`) reads it from disk.
  - The shim loads only when a script mentions `pm`, `postman`, `tests`, `responseBody`,
    `responseCode`, `responseTime`, `environment` or `globals`; then the script runs inside a
    function that receives those names. Every other script runs exactly as before.
  - Variables: Postman's collection, environment, global and local scopes are one set in hitop.
    `PreRequestResult`/`PostRequestResult` gained `variables` (set or `None` = unset); the runner
    applies them for the rest of the run (never written to files). `prepare` keeps a missing
    `${name}` for the pre-request script and fills it after, and rebuilds what the script left
    unchanged when it set variables, so `pm.variables.set('ts', ...)` works in the same request.
    The web app keeps script variables in memory per collection (`store.scriptVariables`) until
    reload. `run_post_request_script_with` + `SentRequest` give post scripts `pm.request`;
    the old functions keep their signatures for the desktop.
  - Beyond the spec, because Conduit needs them: the legacy sandbox (`tests[...]`, `responseBody`,
    `responseCode`, `responseTime`, `environment`, `globals`, `postman.*Variable`), `pm.globals`,
    `pm.response.to.*`, `pm.request` edits. `assert.ok(c, name, detail)` takes a detail (chai's
    message) in scripting.rs and `features.js`. `pm.expect` outside `pm.test` records its own test.
  - Importer: `hitop-core/src/postman.rs` (v2.0 and v2.1); `export::from_postman` delegates to it.
    Folders → `file` paths (`folder/sub/slug`, numbered on clashes) with derived ids; collection
    auth → `Collection.auth`; folder auth inherited by its requests; `noauth` → `{type: none}`;
    collection and folder scripts put in front of the request's own, each in a `{ }` block with a
    `// From ...` line; `{{x}}` → `${x}`; disabled headers and query rows kept off; urlencoded and
    formdata bodies; GraphQL sent as a JSON body; saved responses → `examples`. Reported: script
    lines with unsupported calls (checked where written, collection or folder, not per request),
    unsupported auth types, file form fields, other body modes, `{{$dynamic}}` variables (left as
    written). Environments: `type: "secret"` → `secrets`, values kept apart; disabled values left
    out. Globals have no hitop scope: they become defaults in every environment imported with them
    (the environment wins), or an environment of their own.
  - CLI: `hitop import <files>... [--to <dir>] [--force]`; files detected by content (collection,
    environment, globals, HITOP JSON). `--to` writes the folder (new workspace id; an existing
    folder keeps its id; environments added or replaced by name). Secret values go to the keychain
    (`<workspace>/<env>/<name>`, `global` without `--to`); without one, a note names
    `HITOP_SECRET_<ENV>_<NAME>`. `hitop env import` of a Postman file does the same. The report
    prints after the import; the exit code stays 0.
  - Web app: Postman collections import through hitop-core's importer in WASM (`importPostman` in
    hitop-wasm, with the page's shim check results passed in, so CLI and web give the same report);
    the "Check after import" dialog lists it (copy says "script before/after"). The web app has no
    collection auth, so `forWebApp` copies it into requests without their own. Postman `secret`
    environment values become web secrets (masked, kept in localStorage as for any web secret).
    HITOP and Bruno JSON still import through `collectionService.jsx`.
  - Fixtures (`cli/fixtures/postman/`, README with sources): Conduit from gothinkster/realworld
    (MIT, `dedb6969fe`), newman's `examples/sample-collection.json` (Apache-2.0, `8fbb2eb4`),
    Adyen `BinLookupService-v54.json` (MIT, `50d57dd2`), vendored unchanged with their licenses;
    the environment and globals files next to them are ours (dummy secrets). `tests/postman.rs`
    imports each with `--to` and runs it against httpmock: Conduit 32 requests and over 300 legacy
    checks pass, and its mock only answers with the token and slug earlier scripts set; newman's
    sample passes and fails with chai's message; Adyen sends the secret API key from the globals.
  - Tests: hitop-core 116 unit (scripting 7, postman 4, prepare 1, interpolation 1 new) + 4 format;
    hitop 3 + 34 cli + 9 folder + 5 postman + 12 run + 7 secrets; hitop-wasm 3; web `tests/pm.mjs`
    (shim, import report, three fixtures through WASM, secret env) and e2e `tests/postman.cjs`
    (report dialog, folders and auth, a run where `pm.environment.set` carries a token, against a
    local Node server) in web and Tauri mode. No new crates.
  - Open: (the desktop script variables and `pm.request` gaps were fixed in T6, eacc7fb.) A variable a script sets for a secret name
    isn't masked. chai methods outside the list fail at run time inside `pm.test` (with a
    TypeError message) rather than in the report. The report check reads lines, skipping `//`
    comment lines only: a `pm.foo` inside a string or block comment is reported too.
  - Globals scope added 2026-11-01, owner request (d07a4bf, 3ba2d89, 9da1707, 31dd203, ae9d195): Postman
    globals now import as globals instead of being copied into each environment (the deviation above).
    Order, later ones win: globals < collection < environment < data row < `--var` < script-set values <
    secrets (Postman's global < collection < environment). Storage: `globals.json` next to
    `environments.json` (an `Environment` with id/name `globals`, read leniently); folders:
    `globals.yaml` at the root (spec section 5a, golden file, round trip, hitop-wasm `globals` in the
    folder JSON and `normalize('globals')`). `Folder.globals: None` leaves an existing file alone, so
    writers that don't know globals can't drop it; `plan_sync` already leaves it. A folder uses only its
    own `globals.yaml` (as with environments); global storage has one set for all its collections.
    - Decisions: a secret global's value sits at the globals level (an environment's value of the same
      name still wins; only environment secrets sit on top). Secret globals resolve as the environment
      named `global`: `HITOP_SECRET_GLOBAL_<NAME>`, `--secret`, keychain `<workspace>/global/<name>`,
      prompt; an environment literally named `global` shares those names (documented). Core:
      `prepare::Scopes { globals, upper }` + `prepare_in`; `RunOptions.globals`/`global_secrets`;
      `Secrets::masker_with`; `secrets::GLOBALS_ENV`, `globals_lookup_env`; `postman::merge_into_globals`
      (imported values win; `merge_globals` removed).
    - Shim: `api.globals` + `setGlobal`; `pm.globals`, `postman.*GlobalVariable` and the legacy `globals`
      use that scope; a global set in a script shows through `pm.variables` unless another scope has the
      name (judged by comparing the merged value with the global one). Without `api.globals` (old hosts)
      `pm.globals` is the shared set as before. Scripts get `globals` and return `globals` updates
      (`PreRequestCtx.globals`, `PostRequestCtx.globals`, both results); the runner keeps the layers
      apart and merges them per request.
    - CLI: `hitop globals show|set|unset|import|export [-c <folder>]` (export = `{variables, secrets}`
      without values), `hitop secret set|get|rm|ls --globals [-c]`, `hitop import` writes Postman globals
      to `--to`'s `globals.yaml` or `globals.json`, `hitop env import` of a Postman globals file goes to
      the globals, `hitop convert --globals <file>` both ways; `send`, `run`, `requests send` use them.
    - Web app and desktop: a Globals section on the Environments page (plain and secret editors, show
      switch, Import globals, Export globals without secret values); localStorage `hitop_globals`
      (`{variables, secrets}`, absent in old storage = empty); the environments Import routes a Postman
      globals file to the globals; `store.variables` layers globals lowest; `scriptGlobals` (app-wide, a
      folder collection its own) last until reload; run history masks secret globals. Desktop:
      `load_globals`/`save_globals`, secret values in the keychain as `global/global/<name>`, folder
      `globals.yaml` read (secret values from `<workspace>/global/<name>`) and written back unchanged.
    - Adyen fixture test: its globals now land in `globals.yaml`, and the key comes from
      `HITOP_SECRET_GLOBAL_X_API_KEY`. The collection declares `X-API-Key` and `YOUR_MERCHANT_ACCOUNT`
      with empty values, which hide globals (in Postman too), so the test deletes those two collection
      variables first.
    - Found on the way: Send did nothing when the URL started with `${variable}` (the form ran `type=url`
      validation); the request form is `novalidate` now (31dd203).
    - Open: the web app has no editor for a folder's `globals.yaml` (it is used and written back as it
      came); the zip export of a non-folder collection doesn't include the app's globals; in the
      key-value editors (environments and globals alike), typing a secret's name that starts with an
      existing plain name (`g` then `gs`) briefly claims the plain one and drops it (pre-existing).
    - Tests: hitop-core 135 unit with the CLI features (scripting globals 1 new, postman 1 rewritten) + 4
      format (golden globals, proptest with globals); hitop `tests/globals.rs` 3 (folder globals below
      collection and environment with a secret global masked and pm.globals set/get across requests;
      `hitop globals`, Postman import, `env import`, `send`, export/import into a folder; `convert
      --globals`), Adyen updated; web unit (globals helpers, golden folder with globals); e2e
      `tests/globals.cjs` in both modes.

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

- **Notes (2026-11-01, done; React removal waits for the macOS check):**
  - Switch: `desktop/src-tauri/tauri.webapp.conf.json` points Tauri at `webapp/dist-tauri`
    (`npm run build:tauri`: `VITE_HITOP_PLATFORM=tauri`, base `./`, no `VITE_HITOP_PROXY`; the bundle
    has no `/api/proxy` string). Use it with `cd desktop && npm run build:webapp` / `dev:webapp`
    (`tauri … --config src-tauri/tauri.webapp.conf.json`; for GitHub builds add
    `--config src-tauri/tauri.github.conf.json`). Without it the React app builds as before, so
    `release-desktop.yml` is unchanged. Bundle id (`tech.rws8.hitop`, CLAUDE.md said
    `com.bohdaq.hitop`), MAS entitlements (user-selected read-write covers folders) and storage files
    are the same in both. Delete `desktop/src` once the macOS check passes.
  - 2026-11-01, owner decision: GitHub desktop builds switch to the web app now. `tauri.github.conf.json`
    carries the webapp `build` block and `release-desktop.yml` runs `npm ci` in `webapp/`; released as
    desktop 1.0.1 (1.0.0 still had the React UI). The Mac App Store build (plain `tauri build`) keeps
    `desktop/src` until the owner's macOS check; React code stays in the repo until then.
  - Adapters (`webapp/src/platform/`): `web` (fetch or proxy, localStorage, File System Access) and
    `tauri` (`window.__TAURI_INTERNALS__.invoke`, no npm dep). `main.js` awaits `initPlatform()`
    (the desktop preloads storage from Rust) before the app starts; Vite drops the other adapter.
    Desktop: requests through `send_request` (the proxy's envelope both ways, so the app and the
    tests see one shape; bytes for multipart, repeated `Set-Cookie`), scripts in boa
    (`run_pre_script`/`run_post_script`), storage in hitop-core's JSON files (writes deduped),
    secret values of global environments in the keychain as `global/<env>/<name>` (what `hitop
    secret` uses), folder environments under the folder's workspace id, exports through the save
    dialog and `write_file`. The app's cookie jar handles every desktop request until T6's Rust jar.
  - WASM: `cli/crates/hitop-wasm` (its own workspace and lock, `publish = false`) exports
    `readFiles`, `toFiles`, `planSync` and `normalize` over JSON. Toolchain: wasm-bindgen crate
    `=0.2.126` plus the same CLI version (`cargo install wasm-bindgen-cli --version 0.2.126
    --locked`; CI uses `taiki-e/install-action`), no wasm-pack, no wasm-opt (670 KB, loaded only when
    a folder or zip is used). `webapp/scripts/build-wasm.mjs` runs before `build`, `build:tauri`,
    `dev` and `test:unit`, and fails with the install command when the CLI is missing or another
    version. hitop-core got a default feature `native` (reqwest, boa, dirs; modules http, scripting,
    prepare, runner, report, storage behind `#![cfg(feature = "native")]`): new core modules that
    use them need the same line, or the web build breaks. In-memory folders in
    `format::files` (reuses the writer, so bytes match; tests compare with `read_folder`/
    `write_folder` on the golden folder). The web unit checks run hitop-core's golden folder
    through WASM byte for byte (shared fixtures as a second guard).
  - Folders: desktop opens a folder (dialog), keeps it out of `collections.json`, remembers paths
    in the window's localStorage (`hitop_folders`) and writes changes as they happen
    (`write_folder`: changed files only, `.yml` kept, stale files removed). Web: "Open folder" where
    `showDirectoryPicker` exists, live save while connected, handle in IndexedDB and "Reconnect
    folder" after a reload (the folder wins); the collection also stays in localStorage. "Save to
    folder" turns any collection into a folder; closing a folder collection leaves its files.
    Folder environments join the environment list as "<env> (<collection>)" and are written back;
    like the app's own, the active one applies to every request (the CLI uses a folder's
    environments for that folder only). Zip: "Export as zip" (`<slug>/` + the folder, environments
    without secret values; a folder collection takes its own environments, others the app's) and
    "Import zip" (finds the top `hitop.yaml`, adds the collection and environments with new names),
    with `fflate` 0.8.2.
  - T1 open items fixed: ids read leniently in Rust (digit strings as numbers, other strings as
    FNV-1a of `legacy-id/<s>`, spec section 6), so `hitop convert` takes old web exports; the desktop
    import gives string ids new numbers. Saving a tab keeps fields the editor doesn't know
    (`examples`, `file`, unknown keys; also on a move to another collection and on duplicate), and a
    folder's `bodyType` label is dropped when the body changes.
  - Found on the way: the React desktop never kept history (`save_history` rejected its web-shaped
    items, `load_history` fell back to empty). `HistoryItem` now keeps extra keys; the adapter maps
    `timestamp`/`statusCode` to Rust's fields (`platform/history.js`), and CLI history shows.
  - Tests: hitop-core 111 unit + 4 format (new: string ids, history extra, `http::send` against a
    one-shot TCP server, four `format::files`); hitop-wasm 2; web unit checks (golden folder through
    WASM, zip, history mapping, old desktop files through Rust's model). e2e: the four suites run
    in both modes through `tests/harness.cjs`; Tauri mode (`HITOP_MODE=tauri`, `npm run
    test:e2e:tauri`) serves `dist-tauri` at `http://tauri.localhost/` and mocks invoke in Node
    (`tests/tauri-mock.cjs`: storage files through hitop-core in WASM, Node fetch, the boa preamble in
    `node:vm`, temp folders, in-memory keychain), seeded with old desktop files
    (`tests/desktop-storage/`, the migration test) and asserting nothing goes through the proxy.
    New `tests/folders.cjs` (both modes, local mock API only): open `cli/fixtures/sample`, run it with
    its environment, live save, rename, unknown fields kept, zip export and import, close.
  - CI: `cli` job also clippies hitop-core without `native` and tests hitop-wasm (native and
    wasm32); `web` job installs the wasm target and CLI and runs the Tauri-mode e2e after the web
    one; `desktop` job checks the React build, builds `webapp` with the Tauri adapter and runs
    `cargo check` again with `TAURI_CONFIG` = the overlay; `licenses.yml` covers hitop-wasm.
  - Not done here: security-scoped bookmarks, so in the sandboxed MAS build a folder from an
    earlier launch may need "Open folder" again (the app says so); the Vite build targets Safari
    16.4, so macOS 12 with an older WebKit may not run the new UI (see the handoff).

#### T6 Cookie jar and multipart in Rust and CLI
- Cookie jar in the Rust transport (persisted per workspace), used by desktop and CLI.
- CLI multipart and form bodies (`-F name=value`, `-F file=@path`).
- **Done when:** mock-server tests for cookies across requests and multipart file upload; CI green.
- **Notes (2026-11-01, done):**
  - Core: `cookies.rs` (native) ports the web app's jar (`features.js` `parseSetCookie`,
    `mergeCookies`, `cookieHeader`): host-only vs `Domain`, default path = request directory, `Path`,
    `Secure`, `Expires` (HTTP dates parsed by hand) and `Max-Age` (wins), past expiry deletes, longest
    path first; the same JSON shape (camelCase, `expires` in ms or null), so the web app's cookies load
    as they are. The unit test repeats `webapp/tests/unit.mjs`'s cases. Persisted per workspace (the
    keychain's workspace id: `global` for global storage, a folder's `workspace`) in
    `<config>/hitop/cookies/<workspace>.json`; an empty jar removes the file. `http::execute_opts` /
    `send_with` take `Options { timeout, jar }` (old functions unchanged); the jar adds `Cookie` only
    when the request has none and reads every `Set-Cookie` of the final response with its final URL.
    `form.rs`: `urlencoded` (`encodeURIComponent` rules), `multipart` (random boundary checked against
    the parts), `request_body`/`outgoing` for saved `bodyMode` `urlencoded`/`multipart` requests.
    `runner::run_collection_jar` takes the jar; `run_collection_opts` is unchanged. No new crates.
  - CLI: `send -F name=value`, `-F name=@path[;type=mime]` (type guessed from the extension otherwise),
    `--urlencoded`; `-F` without `-X` means POST; values get `${variables}`, files are read as bytes.
    `run` and `requests send` now send saved form and multipart bodies (they went out empty before,
    including Postman `urlencoded`/`formdata` imports). Cookies: `send`, `run` (across requests and
    iterations; the jar is saved after each collection) and `requests send` use the workspace's jar;
    `--no-cookies` turns it off; `hitop cookies ls|clear [-c <collection|folder>]`.
  - Desktop (Lit web app in Tauri): requests use the Rust jar (`global` workspace, so the desktop and
    `hitop send` without `-c` share it). `send_request` takes `cookies: bool` and returns the jar;
    `cookies_load`/`cookies_save` back the Cookies page (platform `cookies`, `cookiesNote`); the old
    `hitop_cookies` localStorage jar moves to Rust once. The web app keeps its own jar (proxied requests).
  - T4 desktop gaps fixed: `run_pre_script`/`run_post_script` return `variables`, `run_post_script`
    takes the sent `request` for `pm.request` (`tauri.js` passes `sent`); the Tauri mock does both; the
    `[tauri]` Postman check now expects the token to reach the next request.
  - Open: cookies set on a redirect reqwest follows are lost (only the final response counts, as with
    the web proxy); folder collections in the desktop share the `global` jar rather than their own
    workspace's; file rows of saved multipart requests have no file in a collection, so `hitop run`
    sends an empty part named after `fileName` (as the web app does after a reload).
  - Tests: hitop-core 131 unit (cookies 4, form 3 new) + 4 format; hitop 3 + 34 cli + 3 cookies_forms
    (jar across `send`s, `cookies ls/clear`, `--no-cookies`, own Cookie header wins; a folder run with a
    urlencoded login, cookie, multipart text fields and JSON; `-F` text + CSV + binary file, `;type=`,
    `--urlencoded`, errors) + 9 folder + 5 postman + 12 run + 7 secrets; e2e both modes, new `[tauri]`
    checks that the Rust jar (not the page) sends the cookie and keeps it in `cookies/global.json`.

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

- Notes (done 2026-11-01): `release-cli.yml` (crates.io, five binaries: Linux builds are static
  musl instead of the gnu targets in the old cargo-dist metadata, which was removed; GitHub
  Release; ghcr image from `cli/docker/Dockerfile` on distroless static, smoke test
  `cli/docker/smoke.sh`) and `release-desktop.yml` (tauri-action; `tauri.github.conf.json`
  overlays ad-hoc signing and drops the MAS entitlements for GitHub builds). Tags with a suffix
  are dry runs: `cargo publish --dry-run`, pre-release, image tagged with the rc version only.
  Both rc runs passed first time; every asset and the image are there, and the image ran the
  smoke collection in CI and locally. The ghcr package was already public, so that handoff is
  not needed. `/download/` lists the assets (direct links filled in from the GitHub API, the
  releases page without JS). Desktop asset names carry the tauri.conf.json version (0.1.0 in
  the rc); bump it to 0.2.0 in T11.

#### T8 GitHub Action and GitLab template
- `bohdaq/hitop-action`: `action.yml` (inputs: `path`, `env`, `env-file`, `reporter`, `version`,
  `secrets` as env vars), runs the Docker image, uploads JUnit as an artifact and writes a job
  summary. Tag `v1` and `v1.0.0`.
- Self-test workflow in the action repo against a fixture collection.
- `site/content/guides/`: "Run hitop in GitHub Actions" and "Run hitop in GitLab CI" (with a
  `.gitlab-ci.yml` template).
- **Done when:** the action's self-test passes on GitHub. **Handoff:** owner ticks "Publish to
  Marketplace" on the release.
- **Notes (2026-11-01, done; Marketplace handoff open):**
  - `bohdaq/hitop-action` (MIT): composite action. `run.sh` runs `docker run --network host --user
    $(id -u):$(id -g) -v $GITHUB_WORKSPACE:/work ghcr.io/bohdaq/hitop:<version> run ...` (user so the
    reports belong to the runner user; `HOME=/tmp`, `HITOP_KEYCHAIN=off`), always exits 0 and writes the
    outputs `exit-code`, `passed`, `report-dir`, `junit-file`, `report-file`; then `actions/upload-artifact@v7`
    uploads `junit.xml` (artifact `hitop-junit`) and `actions/github-script@v9` runs `summary.js`, which
    writes the job summary from `report.json` (reads only `reportVersion` 1, says so otherwise) and calls
    `setFailed` unless the exit code is 0. Inputs: `version` (required, the image tag; no default so users
    pin), `path`, `env`, `env-file`, `reporter` (`cli,junit,json`), `out`, `secrets` (lines `NAME=value` →
    `--secret`, `::add-mask::`, shown as `••••` in the echoed command), `vars` (→ `--var`), `args`,
    `artifact-name`, `summary`. Every `HITOP_*` env var of the step (so `HITOP_SECRET_<ENV>_<NAME>`) is
    passed into the container by name. Linux runners only (needs Docker); README says to use the release
    binary on macOS and Windows.
  - Self-test `.github/workflows/self-test.yml` (push, PR, dispatch with a version): a Python mock server
    (`test/mock_server.py`) on 127.0.0.1:8765, fixtures `test/fixtures/api` (secret via the input and via
    `HITOP_SECRET_CI_TOKEN`, `--var`, `--timeout`) and `test/fixtures/failing` (assertion failure, HTTP 500,
    connection refused, and `--bail`), plus exit code 2 (unknown env, empty version). Asserts outputs,
    report contents, that no secret value reaches the reports, and that reports are removable. Green:
    https://github.com/bohdaq/hitop-action/actions/runs/41268052937 (first run 41267938196 also green).
  - JUnit rendering on GitHub (T3's deferred check), both on hitop's real `junit.xml`:
    `dorny/test-reporter@v3` (`java-junit`): one suite per collection, one test per request, and each failure
    as `AssertionError: status is 201: got 200`, `HttpStatus: HTTP 500`, `RequestError: <connection error>`.
    v3 writes to the job summary by default; `use-actions-summary: false` makes a check run (needs `checks:
    write`). `mikepenz/action-junit-report@v6`: a check run with 3 annotations titled
    `<collection>.<request>` with the failure line as message; its summary table lists failed tests by their
    message. Annotations sit on path `<collection>` line 1, since testcases have no `file` attribute (open item
    below). No change needed to the JUnit shape.
  - `--bail` and `<skipped/>`: decided not to add skipped testcases. The JUnit file records what ran, the JSON
    report has `bailed: true`, and the action summary says the run stopped early; listing never-run requests
    would need the runner to report the remaining order, a core change while T4 works in `cli/`. Documented in
    the GitHub Actions guide. Revisit if a user asks.
  - GitLab: the distroless image has no shell, so GitLab can't run `script:` in it. `release-cli.yml` now also
    pushes `ghcr.io/bohdaq/hitop:<version>-debug` (and `:debug` on non-rc releases) from
    `cli/docker/Dockerfile.debug` (distroless `static-debian12:debug`: busybox `sh`, root, so it can write
    reports into the GitLab build dir), and the image job runs the smoke test on it plus `sh -c` with an
    empty entrypoint. Template: `image: {name: ghcr.io/bohdaq/hitop:<version>-debug, entrypoint: [""]}`.
    The guide also shows the static release binary in `alpine` with the `.sha256` check. Checked locally with
    rc.2 the way GitLab runs it (`--entrypoint ""`, `sh -c`, project mounted): exit 0, reports written.
  - `cli-v0.3.0-rc.2` on 52f26f5 (Release CLI run 41267320514 green): the first image with T3's reporters and
    the first `-debug` image. Its binary still says `hitop 0.2.0` (workspace version is bumped in T11).
  - Guides `site/content/guides/run-hitop-in-github-actions.md` and `run-hitop-in-gitlab-ci.md`
    (`/terms/run-hitop-in-github-actions.html`, `/terms/run-hitop-in-gitlab-ci.html`), linked from
    `run-collections-in-ci.md`; built, not deployed. The action README links the GitHub guide, so that link
    works after the next site deploy. Docs pin `0.3.0-rc.2`; T11 changes them to `0.3.0` (and the action
    self-test default `TEST_HITOP_VERSION`).
  - (Closed in T11: `file` on `<testcase>` and in report.json.) Open for T11/T19: add a `file` attribute (the request's YAML path) to `<testcase>` so annotations land on
    the request file in a pull request; spec section 4 would gain it as an addition within v1.

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
- **Notes (2026-11-01, client done; the coordinator deploys):**
  - Server half: 032ec68 (`server/src/telemetry.rs`, deployed), privacy page 15f4b4e. The client matches
    the page as written; no page change was needed.
  - Core `telemetry.rs` behind hitop-core feature `telemetry` (= `native`, off by default, so hitop-wasm
    and the desktop default build don't have it). Settings `<config>/hitop/telemetry.json`, per surface
    (`{"cli": {...}, "desktop": {...}}`: `enabled`, `asked`, `id`, `lastPing`), so `hitop telemetry on`
    never turns on the desktop and the reverse. On makes the id (32 hex digits from
    `secrets::new_workspace_id`), off deletes the id and the last ping time. The body is exactly
    `{"id","surface","version","os"}` (os `macos|linux|windows|other`), POST with reqwest, 3 s timeout,
    no Origin or Cookie. The attempt is recorded before sending, so an offline machine tries once a week,
    not on every command. `HITOP_TELEMETRY_URL` overrides the endpoint (tests).
  - CLI: feature `telemetry` on by default (`--no-default-features` builds have no code). `hitop
    telemetry on|off|status`; status prints the endpoint and the exact body (a placeholder id while off),
    the last and next date. The ping runs in a thread at the start of any other command (not `telemetry`
    or `completions`); `main` waits up to 3 s for it at the end. A command that exits through
    `process::exit` (a failed `send` assertion) may cut it off; it then counts as that week's. `main`
    now maps errors to exit 1 itself (same `Error: ...` text) so it can wait.
  - Desktop: crate feature `telemetry` (not default: a plain `tauri build`, the MAS build, has no ping);
    `release-desktop.yml` passes `--features telemetry`. Commands `telemetry_status` / `telemetry_set`
    exist in both builds and fail without the feature; the startup ping runs in a thread from `setup`.
    The version is tauri.conf.json's. Web app (Tauri adapter only): `platform/tauri-settings.js`, a
    Settings page in the sidebar (switch plus the exact body) and a first-run `confirm` (No has the
    focus; Escape and a click outside count as No; either answer is stored). The page and question show
    only when `telemetry_status` answers, so the MAS build embeds them dormant; the endpoint string lives
    only in Rust. Generic adapter hooks: `pages`, `onStart`.
  - Checks: `webapp/scripts/check-no-telemetry.mjs` (CI web job): `dist/` has no `api/ping`, `telemetry`,
    `usage ping` or `hitop-settings`; `platform/web.js` has no telemetry path; control: `dist-tauri` has
    the page and no endpoint. CI desktop job builds the MAS configuration (no feature) and fails if the
    binary has `hitop.rws8.tech/api/ping`, `/api/ping`, `HITOP_TELEMETRY_URL` or `telemetry.json`, then
    builds with `--features telemetry` and requires them (control).
  - The GitHub Release desktop builds still ship the React UI (T5 switch pending the macOS check), which
    has no toggle, so nothing can turn the ping on there until the switch; that matches the page.
  - Tests: hitop-core 3 telemetry unit (off by default and per surface, once a week, dates); hitop 2
    `tests/telemetry.rs` against httpmock: nothing sent while off (also after on then off, and no file
    written while off), and on: `telemetry` itself sends nothing, the next command sends once with the
    exact body, three more commands send nothing, 8 days later it sends again, a failed ping isn't
    retried that week. e2e `tests/settings.cjs` (both modes): web has no page and no question; Tauri:
    the question, No focused, Escape = No, asked once, the page shows and toggles, a build without the
    ping has neither.

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
- **Notes (2026-11-01, done; handoffs open):**
  - Launch gate, two layers. `cli/crates/hitop/tests/launch_gate.rs`: `hitop import` of a Postman v2.1
    collection (`cli/fixtures/launch-gate/shop.postman_collection.json`, written for hitop) with two
    Postman environments (Staging, Local; `password` of type secret), then a run per environment against
    its own httpmock server: Log in posts the secret, its test script does `pm.environment.set("token", ...)`,
    and the collection's Bearer auth `{{token}}` authenticates List orders and Get order (the mocks only
    answer with that token and only on that environment's server); JUnit checked with roxmltree (suite
    environment property, testcases, URLs, `file`), the password in neither report nor stdout; a wrong
    password fails all three with exit 1. CI job `launch-gate` (ci.yml): musl build, image
    `ghcr.io/bohdaq/hitop:launch-gate` built on the runner from `cli/docker/Dockerfile`, import in the container,
    then `bohdaq/hitop-action@v1` once per environment (secret through the `secrets` input, then through
    `HITOP_SECRET_LOCAL_PASSWORD`) against `mock_server.py` on 127.0.0.1:8781/8782, `check_junit.py` on each
    junit.xml. Green on main (CI 41277643609, 41278740884). No external network. pm.environment.set →
    collection auth worked as is: no launch-gate gap found.
  - hitop-action v1.0.1 (37701e4): `run.sh` uses an image already on the runner when its tag isn't on
    ghcr.io (notice in the log), so hitop's CI can run the action on a just-built image; self-test job
    `local-image` (also a tag in neither place → exit 2). Tagged v1.0.1, `v1` moved, release created
    with `gh`. After the release: self-test default and README to 0.3.0 (docs only, no retag).
  - `file` (T8 open item): each `<testcase>` gets `file="<folder as given>/requests/<file>.yaml"` (no
    leading `./`, `.yml` when that is the file on disk; absent for global storage and `--all`), and
    `report.json` requests get `"file"` (string or null). Deviation: added to the JSON report too, as an
    addition within reportVersion 1 (spec sections 2, 3 and 4 say "added in hitop 0.3.0").
  - Site: `hitop-vs-postman|bruno|insomnia|yaak.md` (guides at `/terms/*.html`), each "Checked on
    2026-11-01" with a Sources list of the official pricing pages, repositories and docs fetched that day;
    claims not on those pages were left out (Postman's license, Bruno's app account, Inso reporters).
    `postman-alternative.md` had "$0 to $49" and "account mandatory"; corrected to the current pricing page.
    Guides `migrate-from-postman.md`, `collections-in-git.md`; CI guides pin 0.3.0. Home page: `hitop mcp`
    line and link to T12's guide; CI FAQ answer. README rewritten around the full positioning line (T12
    landed before the tag, so "your AI agent" holds); crate README intro, `hitop mcp` row and the `file`
    attribute.
  - `site/demo/demo.tape`: hidden setup copies the launch-gate fixtures to a temp dir and starts the mock;
    shows import, a passing run, a failing run (exit 1) and the testcase `file` attributes. Commands
    dry-run by hand; vhs isn't installed here, so the GIF is a handoff.
  - Versions: workspace 0.3.0 (and the `[workspace.dependencies] hitop-core` version), desktop 0.2.0 in
    tauri.conf.json, Cargo.toml, package.json and package-lock.json; the three Cargo locks edited to the new
    versions only (`cargo update` also reshuffled windows-sys entries in the desktop lock, so those were
    hand-edited and checked with `cargo metadata --locked`). tokio 1.52.3 → 1.54.1 in the cli and desktop
    locks (RUSTSEC-2026-0386), the same entry as server/ and sync-server/.
  - Audits before tagging: `cargo deny check advisories` (no cargo-audit here): cli only unmaintained
    (bincode, number_prefix, zbus_names, zvariant*); desktop also quick-xml 0.39.4 via plist/tauri-utils
    (RUSTSEC-2026-0194, -0195; no newer version in range) and unmaintained proc-macro-error, unic-*; server
    clean. `npm audit --omit=dev`: site and desktop clean, webapp fflate 0.8.2 moderate (GHSA-px8p-9vwx-vf98).
    No high or critical; left for T19.
  - Releases on 31414f3 (CI 41278740884 green): Release CLI https://github.com/bohdaq/hitop/actions/runs/41278983546
    green first time: crates.io hitop and hitop-core 0.3.0, GitHub Release `cli-v0.3.0` with the five archives and
    their `.sha256`, `ghcr.io/bohdaq/hitop:0.3.0`, `:latest`, `:0.3.0-debug`, `:debug` (amd64 and arm64;
    `docker run ghcr.io/bohdaq/hitop:0.3.0 --version` → `hitop 0.3.0`). Release desktop
    https://github.com/bohdaq/hitop/actions/runs/41278983311 green: `desktop-v0.2.0` with the universal dmg and
    app.tar.gz, msi, setup.exe, deb, rpm and AppImage, all named 0.2.0. hitop-action self-test on 0.3.0
    (81264f1): https://github.com/bohdaq/hitop-action/actions/runs/41279497845 green.
  - Site deployed from ec1b9cf on 2026-11-01 (with T12's guide); every home-page link returns 200. Still
    open: the handoffs in section 6.
  - Launch drafts in `launch/` (README with where and when, Show HN, r/rust, r/webdev, This Week in Rust,
    Product Hunt). Nothing posted.

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
- **Notes (2026-11-01, done):**
  - SDK: `rmcp` 3.8.0 (Apache-2.0, `cargo deny ... check licenses` ok), features `server`, `macros`,
    `transport-io` only, plus `tokio` and `schemars` 1, all in the `hitop` binary crate (hitop-core, the desktop
    and hitop-wasm locks are unchanged). `commands/mcp.rs`; `Source::global` opens a collection by name only, so a
    folder with the same name in the working directory never wins in global mode.
  - Contract: tool names as specified; inputs snake_case, outputs camelCase (run results reuse the report.json
    `RequestReport` shape; `run_collection` returns the whole report, reportVersion 1). Requests are found by name
    or folder file path. `run_request` adds `headers`, `body` (cut at 64 KiB, `bodyTruncated`) and `environment`.
    Default time limit 30 s per request (`timeout_ms`), `iterations` 1 to 100. No delete tool.
  - Behaviour: runs go through `runner::run_collection_jar` (one-request collection for `run_request`), so
    precedence is globals < collection < environment < call `variables` (the `--var` slot) < script-set < secrets and
    the workspace cookie jar is loaded and saved per call, as in `hitop run`. `set_environment` is session-only
    (environments.json `active` untouched); the session starts with the active environment in global storage, none
    for a folder. Tool bodies run in `spawn_blocking` behind one mutex (blocking reqwest; no interleaved saves).
  - Secrets: resolved like `hitop run` with no flags and no prompt (env vars, keychain); the missing-secret error
    drops the `--secret` hint. Every value resolved in the session is masked in each JSON string (keys too) and in
    the final text, for results and errors; `list_environments` gives secret names only; `create_request` and
    `update_request` refuse input holding a known secret value. Checked that nothing reachable prints to stdout
    (core has no print; secret notes go to stderr; the usage ping thread is silent).
  - Tests: `cli/crates/hitop/tests/mcp.rs`, 5 tests driving the binary over stdio JSON-RPC against httpmock: every
    tool, precedence, cookies across calls, global storage, bail/iterations, refusals, a missing secret (tool error,
    server keeps answering), and no secret value in the whole stdout stream nor in the folder's files (secret echoed in
    response body and header, in a failing assertion message and in a connection error).
  - Manual check with the MCP inspector CLI (`npx @modelcontextprotocol/inspector@0.17.2 --cli target/debug/hitop mcp
    fixtures/sample ...`): `tools/list` returned the nine tools; `tools/call get_request request="Get user"` returned
    the saved request; `run_request` with `environment=prod`, `HITOP_SECRET_PROD_TOKEN`/`_APIKEY` set and
    `variables={"base":"http://127.0.0.1:18431"}` against a local Python server that echoes `Authorization` returned
    200, both assertions passed, body `{"name":"Ada","echo":"Bearer ••••"}`; the secret value appeared nowhere in the
    output; without `_APIKEY` it returned `isError: true` naming `HITOP_SECRET_PROD_APIKEY`.
  - Guide: `site/content/guides/use-hitop-with-claude-code-and-cursor.md` (site build ok, `/terms/` lists it).
  - Open: README and the site home page don't mention `hitop mcp` yet (T11 rewrites the README; add one line
    after it lands). `update_request` can't clear a request's auth back to the collection default (give
    `{"type":"none"}` to send none); additive later if needed.

#### T13 GraphQL
- Body type `graphql` (query plus variables JSON) in the format, core, CLI and web app;
  introspection fetch; schema-aware completion in CodeMirror.
- **Done when:** tests against a local GraphQL mock; e2e for the editor; format round-trip.
- **Notes (2026-11-01, done):**
  - Format (additive to v1): `body.type: graphql` with `content` (the query), `variables` (JSON text, a block scalar, not a YAML
    map, so `{"limit": ${limit}}` works and round-trips exactly) and `operationName`. JSON model: `bodyType: "graphql"`, query in
    `body`, new `graphql: {variables, operationName}` (skipped when empty; `{}` reads as none). Both keys may stay on another body
    type (kept, not sent). Readers up to 0.3.0 refuse a file that has them (unknown keys in `body` are an error) and read every
    file without them. Spec section 3 "GraphQL" and section 6 updated. Golden `graphql-me` now has variables and operationName;
    the proptest generates `graphql` on any request (512 cases).
  - Sending (`hitop-core/src/graphql.rs`, no native deps): `${var}` substitution in query, variables and operation name first, then
    `{"query":...,"variables":...,"operationName":...}` in that order (query JSON-encoded, variables text as typed after checking it
    is a JSON object, empty parts left out) and `Content-Type: application/json` unless the request sets one. A Postman script that
    sets a variable the GraphQL variables use still works (the variables wait for the script, as for other fields).
  - API: `prepare::Draft` has a new field `graphql` and a constructor `Draft::of(request, collection_auth)` (runner and
    `requests send` use it); `Request` has `graphql`. Struct literals of `Draft` outside the crate break, so the next hitop-core
    release is a minor bump (0.4.0), not 0.3.x.
  - CLI: `hitop send <url> --graphql <file|-> [--graphql-vars JSON|@file] [--operation-name NAME]` (POST by default; conflicts with
    `-b`/`-F`). `run`, `requests send` and MCP `run_request`/`run_collection` send saved GraphQL requests; MCP `get_request` shows
    `graphql`. Postman `graphql` bodies now import as GraphQL requests (they were flattened to a raw JSON body) and export as Postman
    `mode: graphql`. Tests: `cli/crates/hitop/tests/graphql.rs` (5, httpmock as the GraphQL endpoint matching the exact JSON body),
    core unit tests (body shapes, bad variables, Content-Type, script-set variables, Postman import/export).
  - hitop-wasm: `graphqlBody(query, variables, operationName)`, so the web app and desktop send the bytes the CLI sends.
  - Web app and desktop: Body type "GraphQL" with a query editor, a variables (JSON) editor and the operation name; "Fetch schema"
    sends the introspection query through `store.dispatch` (proxy or direct on the web, `send_request` in Rust on the desktop; the
    request's headers, auth and cookies, not its script) and turns on completion (fields, arguments, types) and schema checks.
    Packages: `cm6-graphql` 0.2.2 (1.0 is still rc), `graphql` 16.14.2, `@codemirror/autocomplete` 6.20.3 (6.20.4 was under two
    weeks old), all MIT, as are `graphql-language-service`, `nullthrows`, `debounce-promise`, `vscode-languageserver-types`. The
    GraphQL editor is its own lazy chunk (`code-editor-graphql`, 120 kB, 36 kB gzipped; graphql-js shared chunk 79 kB), so the other
    editors don't load it. The checks skip a query holding `${...}` (not GraphQL until filled in). Schemas stay in memory per tab.
  - e2e `webapp/tests/graphql.cjs` against a local graphql-js server: introspection, completion of root fields and of `User`'s
    fields, the exact JSON body with variables filled in, response shown, saved shape, reload, switch to raw, a failed
    introspection, invalid variables not sent; 14 checks on the web, 16 in Tauri mode (sent through Rust, nothing via the proxy).
    Full suites green locally: web 97 PASS, Tauri 120 PASS; unit checks (wasm `graphqlBody`, Rust keeps a GraphQL request,
    schema from an introspection answer); `cargo deny ... check licenses` ok; no new Rust dependency.
  - Deviation: variables use hitop's `${var}` syntax, not `{{var}}` (Postman's `{{var}}` is converted on import as before).
  - Open: history items keep the query only (reopening one doesn't bring back variables); GraphQL over GET, subscriptions and a
    CLI introspection command are not done; the site comparison pages and `launch/` texts still say GraphQL is planned (update
    with the next release). Seen, not from T13: the URL field shows "Please enter a URL." for URLs starting with `${...}`;
    `npm audit --omit=dev` reports fflate 0.8.2 (moderate, GHSA-px8p-9vwx-vf98, fixed in 0.8.3). No owner handoff.

#### T14 WebSocket
- WebSocket requests in the format; desktop and CLI through Rust (`tokio-tungstenite`), the web
  app through the browser `WebSocket`. Connect, send saved or typed messages, timestamped log.
- CLI `hitop ws <url>` with interactive send.
- **Done when:** tests against a local echo server in Rust and e2e.
- **Notes (2026-11-01, done):**
  - Format (additive to v1, as T13): `kind: websocket` on a request plus a `websocket` section, `subprotocols` (list) and `messages`
    (rows of `name`, `content`; rows keep unknown keys, unknown keys directly under `websocket` are an error). URL, params, headers
    and auth are the request's own; `method`, `body` and scripts are kept but unused. JSON model: `Request.kind` (`"http"`/empty read
    as none) and `Request.websocket` (`{}` reads as none). Both are top-level keys, so 0.3.0 keeps them as unknown keys (and would
    send the request as a GET), unlike T13's `body` keys which it refuses. Spec section 3 "WebSocket" and section 6. Golden
    `live-prices.yaml`, the proptest generates `kind` and `websocket` on any request, hitop-wasm round trip, format unit test.
    Struct literals of `Request` outside the crate break again: still the 0.4.0 bump T13 booked.
  - hitop-core `websocket.rs`: `target()`/`message()` fill in `${variables}` (no native deps; `http(s)://` read as `ws(s)://`);
    `connect()` behind a new default feature `websocket`: tokio-tungstenite 0.30 (MIT) over rustls 0.23 with the ring provider and
    webpki-roots (no aws-lc, same roots as reqwest), a task that streams events (message, closed with code and reason, error) on an
    mpsc channel and a cloneable `Sender` (text, binary, close; dropping it closes). Close handshake bounded at 5 s. The runner skips
    WebSocket requests. Locks: cli and desktop updated (tokio-tungstenite, tungstenite, sha1, data-encoding, webpki-roots 0.26
    shim, tokio-macros); hitop-wasm lock unchanged. `cargo deny ... check licenses` ok.
  - CLI `hitop ws <url>` or `hitop ws <request> -c <collection>`: `-H`, `--protocol`, auth flags, `-m/--message <saved>` (first)
    then `-s/--send <text>`; without either, stdin lines are sent (blank lines skipped). `-n/--count`, `--wait` (default 1000 ms,
    30000 with --count), `--timeout` (handshake, 30 s), `--json` (one object per event: open, sent, received, closed, error, with
    `time` in RFC 3339 ms). Received messages on stdout (binary as hex; `< ` prefix on a TTY), the rest on stderr; secrets masked
    everywhere. Exit 1 on a failed connection or a `--count` not reached. `requests list` shows `WS`, `requests show` the
    subprotocols and messages; `requests send`, `hitop snippet` and MCP `run_request` refuse WebSocket requests with a pointer to
    `hitop ws`; MCP `get_request`/`list_requests` show `kind`/`websocket`; no new MCP tools.
  - Desktop: `ws_connect` (Tauri `ipc::Channel` for events, state `Sockets`), `ws_send`, `ws_close` in `commands.rs`. Checked with
    `cargo clippy` in a Debian container with the GTK/WebKit dev packages (none on this machine) and by CI's desktop job.
  - Web app: method "WebSocket" in the method select (tab `kind`, `subprotocols`, `wsMessages`, `wsDraft`); Connect/Disconnect,
    Params, Auth, Headers (+ subprotocols), Messages (composer, saved messages with name/content/send/remove, "Save as message"),
    and a log with times (`HH:MM:SS.mmm`), direction, binary as hex and base64, Clear. The log and connection live in
    `store.sockets` (memory only: the log holds filled-in tokens). Browser adapter: `WebSocket` with subprotocols only; the Headers
    tab says headers and header auth can't be sent from a browser, and the log names the headers left out; an https page can only
    reach `wss://` or `ws://localhost` (said on failure). Desktop adapter: Rust over a channel (ordered by index, held until
    `ws_connect` answers). Sidebar and collection lists show `WS`; collection runs skip them; closing a tab disconnects.
  - Tests: core 3 (echo server with headers, bearer, subprotocol choice, text, binary, server close 4000, client close, refused) +
    format/runner/snippet unit tests; CLI `tests/ws.rs` 5 (flags until count, stdin + JSON lines, count not reached / server close /
    refused / bad URL, saved request with secret masked and real values at the server, run skips and requests send refuses) + an
    MCP test. e2e `webapp/tests/websocket.cjs` against a hand-written Node echo server: web 19 checks (served from `dist/` locally;
    CI runs it behind hitop-server), Tauri 21 (headers and auth reach the server through the mocked Rust side, which uses Node's
    WebSocket with headers). Full Tauri e2e suite green locally; unit checks green (Rust keeps a saved WebSocket request). CI run
    41285646054 green on 8938060 (CLI, desktop compile, web and Tauri e2e including `websocket.cjs`).
  - Deviations: the composer and saved messages are text only (binary is shown, not sent; `ws_send` accepts base64 for later). The
    method column of the request line is 160 px (was 140) so "WebSocket" fits. `--message` messages go before `--send` ones
    (clap loses the order between the two flags).
  - Open: MCP WebSocket tools (not added, by the tranche's scope; added in 1.1 as `run_websocket`, section "1.1 Owner follow-ups"); history doesn't record WebSocket sessions; no ping/keepalive
    control or auto-reconnect; the browser can't send handshake headers (platform limit, documented); Postman export writes a
    WebSocket request as a plain GET (Postman v2.1 has no WebSocket items); pre/post scripts don't run for WebSocket requests.

#### T15 gRPC
- Unary and server-streaming calls in desktop and CLI (`tonic` + `prost-reflect`), schema from
  server reflection or imported `.proto` files. The web app shows "desktop and CLI only".
- **Done when:** tests against a local tonic test server.
- **Notes (2026-11-01, done):**
  - Format (additive to v1, as T14): `kind: grpc` plus a top-level `grpc` section: `service` (`package.Service`), `method`,
    `message` (JSON text, a block scalar like T13's GraphQL variables, so `${var}` works anywhere), `protos` and `importPaths`
    (relative to the collection folder; none = server reflection), `plaintext`. Unknown keys under `grpc` are an error; 0.3.0
    keeps both keys as unknown top-level keys. Golden `say-hello.yaml`, proptest generates `grpc`, hitop-wasm round trip, spec
    section 3 "gRPC" and section 6. Deviation: no separate `metadata` field: the request's `headers` rows are the metadata and
    `auth` applies (bearer/basic/header API key become metadata; an API key `in: query` is dropped), the T14 way.
  - hitop-core `grpc.rs`: `target()` (no native deps) and, behind the new default feature `grpc` (not in hitop-wasm): tonic 0.14.6
    (`channel`, `tls-ring`, `tls-webpki-roots`; pinned to 0.14.6 in the cli and desktop locks because 0.14.7–0.14.9 are days old),
    tonic-reflection 0.14.6 (generated v1 and v1alpha clients), prost 0.14, prost-reflect 0.16.5 (`serde`: proto3 JSON mapping),
    protox 0.10.0 (parses .proto at run time; well-known types built in; no protoc). All MIT/Apache; `cargo deny ... check
    licenses` ok, no aws-lc or OpenSSL in the tree. `connect`, `schema` (protos, or reflection v1 then v1alpha on UNIMPLEMENTED;
    dependencies the server leaves out are fetched by file name), `services`, `describe` (.proto-like text), `template` (every field,
    first enum value, one list element, first oneof field, nested messages to depth 4, well-known types in their JSON form), `call`
    (unary and server streaming through one `server_streaming` code path so trailers are read either way; events Headers, Message,
    Status{code, name, message, trailers}; client-streaming/bidi refused before calling), `DynamicCodec`. Responses show default
    fields; 64-bit ints are strings. Runner skips gRPC requests, `snippet` refuses them.
  - CLI `hitop grpc <target> <package.Service/Method> [-d JSON|@file|-] [-H "name: value"] [--proto f]... [-I dir]... [--plaintext]
    [--connect-timeout ms] [--max-time ms] [-v] [--json]`, `hitop grpc <target> list [service]`, `describe <symbol>` (with a message
    template), saved requests with `-c` (protos relative to the folder; `-d`, a method argument, `-H`, `--proto` override). Messages
    pretty JSON on stdout, status/trailers (and headers with -v) on stderr, `--json` = JSON lines; exit 1 on a non-OK status or an
    unreachable server (UNAVAILABLE). Secrets masked in all output (incl. message bodies echoing them). `requests list` shows
    `GRPC`, `requests show` the section; `requests send` and MCP `run_request` refuse, MCP `get_request` shows `grpc`. The shared
    variable setup of `hitop ws` moved into `ws::session_variables` (used by both).
  - Desktop: `grpc_list`, `grpc_describe`, `grpc_call` (Tauri channel; state `GrpcCalls`), `grpc_cancel`. Checked with
    `cargo clippy --locked --all-targets` in a Debian bookworm container with the GTK/WebKit dev packages (only the existing unused
    `SecretStore` import warning in commands.rs, not from T15) and by CI's desktop job.
  - Web app: method "gRPC" (tab `kind: "grpc"`, `grpcService/Method/Message/Protos/ImportPaths/Plaintext`); tabs Schema
    (reflection or .proto files + import paths, plaintext switch), Auth, Metadata, Message (service and RPC method pickers after
    "Load schema", text fields before; "Template" fills the message from the input type; a note for client-streaming methods);
    Send/Cancel; a timestamped log of sent, headers, messages, status with trailers, errors (`store.grpcTabs`, memory only). Desktop
    folders give the base dir for relative .proto paths. In the browser: "gRPC is available in the desktop app and CLI
    (`hitop grpc`)", Send and Load schema off; requests are still edited, saved and reopened. Runs skip them; sidebar shows `GRPC`.
  - Tests: core `tests/grpc.rs` 9 against `tests/support/grpc_server.rs` (a hand-written tonic service on dynamic messages,
    reflection v1, v1alpha or none, from `tests/fixtures/grpc/greeter.proto` + `include/common.proto`): reflection list/describe/
    template, unary with metadata (headers, `-bin`, auth), server streaming, NOT_FOUND with trailers, v1alpha fallback, .proto files
    (missing import, missing file), refusals (bidi, bad JSON, unknown method, bad base64), refused connection and TLS to a plaintext
    server, saved request → call; plus unit tests (target, endpoints, method names), format, runner and snippet tests. CLI
    `tests/grpc.rs` 7 (the same server via `#[path]`) + an MCP test. e2e `webapp/tests/grpc.cjs`: web 9 checks (the desktop-only
    message, Send off, saved shape, reload), Tauri 22 (reflection pickers, template, unary, streaming, error status, bidi refused,
    bad JSON, no reflection → .proto files and import paths, saved shape, nothing via the proxy) against a canned Greeter in
    `tauri-mock.cjs`. Full Tauri e2e suite green (204 PASS) after rebasing over T17; unit checks green (Rust keeps a gRPC request).
  - Open: MCP has no gRPC tools; client-streaming and bidirectional calls; history doesn't record gRPC calls; no `--insecure`
    (self-signed TLS) or client certificates; the desktop reloads the schema on every call (no cache); Postman export writes a gRPC
    request as a plain GET; scripts don't run for gRPC requests; the web app's gRPC e2e in Tauri mode uses a canned mock (the real
    client path is covered by the Rust tests). No owner handoff beyond the open question in section 6.

#### T16 Response diff and code snippets
- Compare two responses (history items or saved examples) side by side, with JSON-aware diff.
- Code snippets: curl, JavaScript fetch, Python requests, Go net/http, Java HttpClient, Rust
  reqwest. Generator in hitop-core, used by the web app through `hitop-wasm` and by the CLI as
  `hitop snippet <request> --lang <lang>`.
- **Done when:** snippet fixtures per language; the generated curl runs against a mock server and
  matches.

- **Notes (2026-11-01, done):**
  - Core `hitop-core/src/snippet.rs` (no native deps, builds for wasm32): `generate(lang, request, collection_auth, variables,
    secrets) -> {code, placeholders}` for curl, JavaScript fetch (Node, top-level await), Python requests, Go net/http, Java
    HttpClient and Rust reqwest (blocking). Auth (basic structured per language, bearer and API key through `auth::apply`), enabled
    headers, query, raw, urlencoded (encoded as `form::urlencoded` does), multipart (text parts and file parts read from disk) and
    GraphQL bodies (T13's JSON POST, `graphql::request_body`). Written the way hitop sends: one value per header name (hitop's
    header map keeps the last), no Content-Type hitop wouldn't send (curl gets `-H 'Content-Type:'` for a raw body without one,
    fetch sends such a body as bytes), `--data-raw`/`--form-string` so a leading `@` stays text. Escaping per language (POSIX single
    quotes; Java octal escapes for control characters, since `\uXXXX` is unescaped before parsing; Rust raw strings with enough
    `#`; Go raw strings for multi-line text).
  - Secrets: names in `secrets` and variables with no value are written as `{{NAME}}` (also readable inside an API key in the
    query). The CLI never resolves secrets by default (no keychain access, no prompt) and drops stale secret values from JSON
    environments; `--show-secrets` resolves them like `requests send`. stderr lists the placeholders. Web app: "Include secret
    values" checkbox, off by default, with a warning when on.
  - CLI `hitop snippet <collection> <request> --lang <lang> [-e env] [--show-secrets] [--secret N=V]` (default curl; addressing as
    `requests send`; variables globals < collection < environment; scripts not run). README section "Code snippets".
  - Tests: golden files for 7 cases x 6 languages (`hitop-core/tests/golden/snippets/`, `UPDATE_GOLDEN=1`), syntax-checked with
    bash, node, python3, javac and gofmt where installed; `hitop/tests/snippet.rs` runs `requests send` and then the generated
    snippet against a local TCP capture server for 7 requests (bearer + query, JSON + basic with quotes, `$`, backticks and
    non-ASCII, urlencoded + API key in the query, multipart + API key header, raw text without Content-Type, a custom method with a
    repeated header, GraphQL) and compares method, path, query, headers and body (multipart boundary normalised): curl always,
    plus fetch (node), Python requests, Java (`java Main.java`) and Go (`go run`) when installed; all five matched locally. Rust
    snippets are not compiled (reqwest isn't available offline in a test).
  - hitop-wasm: `snippet(input, lang)`, `snippetLanguages()`; clippy clean on the host and wasm32.
  - Web app and desktop: a Code button next to Save (language picker, the code block's copy button, placeholder list, CLI
    equivalent). History items now keep `response` (status, headers, body up to 100 kB with `truncated`, duration; secret values
    masked); items stored before have none, still load (e2e seeds the old React/desktop storage) and are not offered for
    comparison. "Save as example" on a saved request's response appends `{name, status, headers, body}` (masked) to its
    `examples` (written to the folder file like any example). "Compare" (response bar and History dialog) picks Before/After from
    the current response, the request's examples and history: status, headers (names without case), JSON bodies by structure
    (key order ignored, arrays by position) with added/removed/changed paths plus a side-by-side of the canonical JSON, other
    bodies by line (LCS, with a size cap). `webapp/src/diff.js` (unit checks), `views/tools-dialogs.js`.
  - e2e `webapp/tests/diff-code.cjs` (local Node server only): old history loads, Code panel in all six languages with the secret as
    `{{key}}`, "Include secret values", example saved and masked, compare example vs current (one header, two JSON paths, key order
    ignored), sources (old history left out), same vs same, history keeps masked responses, History → Compare; 20 checks web, 21
    Tauri. `graphql.cjs` gained a Code panel check (curl posts the GraphQL JSON body). Full suites locally: web 118 PASS, Tauri 142
    PASS, 0 FAIL; `cargo test` and clippy (`--all-targets`, `--no-default-features`, `--all-features`, hitop-wasm host and wasm32)
    clean. No new dependency in any workspace.
  - Deviation: snippets use `{{NAME}}` placeholders, the shape the task suggested (hitop's own `${NAME}` would expand in a shell).
  - Open: examples saved in the web app can't be renamed or deleted there yet (edit the folder file or JSON); the history
    response snapshot is cut at 100 kB, so a diff of larger bodies covers only the start; no PHP/C#/HTTPie snippets. No owner
    handoff.

#### T17 OpenAPI import and sync
- OpenAPI 3.0 and 3.1 (YAML or JSON, file or URL) → collection folder, in hitop-core (WASM for the
  web app).
- Sync: re-import shows added, changed and removed operations and keeps user edits (scripts,
  examples, auth).
- **Done when:** fixtures (Petstore plus two large public specs) import; sync test preserves
  edits.
- **Notes (2026-11-01, done):**
  - Core `hitop-core/src/openapi/` (pure Rust, no `native`, in hitop-wasm): `refs.rs` parses JSON or YAML (yaml_serde
    `Value`, merge keys applied, non-string keys such as `200:` stringified) and follows local `$ref`s as JSON pointers
    (`~0`/`~1`, percent-decoding; loops of refs are an error, refs into other files reported as skipped). `example.rs`
    builds examples from schemas: `example`, 3.1 `examples`, `const`, `default` (not an empty `{}`/`[]`), `enum`, then
    by type; `allOf` merged, first non-null `oneOf`/`anyOf`, 3.1 type lists, `readOnly` left out of request bodies; a
    `$ref` back into itself stops (depth 12, 4000 values cap). Parameters get explicit values only (example, default,
    enum), never `string`/`0`. Swagger 2.0 and non-3.x are refused with a message.
  - Mapping: title → name; first server (variables at defaults; relative URL resolved against the spec's URL, or a
    warning) → `baseUrl`; first tag → folder (`--flat`: one level), in the spec's tag order then path order; name =
    summary, else operationId, else `METHOD /path`; path params `${name}` + collection variables; required query in
    the URL, optional ones `disabledParams`; header params (Accept, Content-Type, Authorization skipped) on when
    required; cookie params skipped. Bodies: JSON (`+json`, and `*/*` with a schema as Kubernetes has) with a
    `Content-Type` header, urlencoded and multipart (binary → file field), XML/text from a string example; optional
    DELETE/GET bodies left out; other types skipped. Security: top-level → collection auth, operation-level when it
    differs (`security: []` → `{type: none}`); with no top-level `security` and every operation using the same one, it
    becomes the collection's. Variables `${<scheme>}` (`${<scheme>Username}`/`Password` for basic). Schemes several
    at once, OAuth 2, OpenID Connect, cookie keys, mutual TLS reported per scheme with use counts; unused schemes left
    out. Response `example`/`examples` → saved examples `{name: "<code> <summary|description>", status, body, source:
    openapi}`; `externalValue` reported. Webhooks and callbacks reported.
  - Record (spec section 3 "OpenAPI source", additive like T13/T14 top-level keys: 0.3.0 keeps it as an unknown key):
    request `openapi: {operation, operationId, synced: {method, url, params, headers, body, auth, examples}}`, collection
    `openapi: {source (URLs only), version, flat, synced: {baseUrl, auth}}`. `synced` values are FNV-1a 53-bit hex of each
    field normalized (blank header rows and row extras dropped), so a web app save or folder round trip doesn't look
    like an edit.
  - Sync (`openapi::sync`, three-way per field): match by operationId, then `METHOD /path`; spec changed + request still
    at base → updated; spec changed + edited → kept (listed); base becomes the new spec either way. Names, files,
    scripts, user examples (no `source: openapi`), environments untouched; spec examples replaced, user ones kept after
    them. Added operations get unique files next to the existing ones; removed ones listed, deleted only with `prune`.
    Collection: `baseUrl` and auth by the same rule, new path variables added (never removed), record updated.
    `SyncDiff {added, changed (updated/kept), removed, pruned, unchanged, collection}`.
  - CLI: `hitop import openapi <file|url> [--to] [--force] [--flat]` (a subcommand of `import` with
    `args_conflicts_with_subcommands`, so `hitop import a.json b.json` parses as before) and `hitop import <spec>` by
    content; `hitop openapi sync <file|url> <folder> [--dry-run] [--prune]` (+ `hitop openapi import`). URLs through
    `http::send_with` (60 s). Report: mapped line, skipped, to check; exit 0. Sync writes only `hitop.yaml` and
    `requests/` (`write_collection`).
  - Web app and desktop: Import dialog accepts a pasted spec (wasm `isOpenapi`) and has "Load an OpenAPI spec from a
    URL" (dialog `openapi-import`, URL through `platform().send`: proxy for public hosts on the web, Rust on the
    desktop); OpenAPI adds a collection (Postman/HITOP JSON still replaces), with `inlineAuth` because the web app has
    no collection auth, and shows the report (mapped, Not imported, To check). Collections with a record get "Sync with
    OpenAPI" (URL prefilled from the record, paste, prune checkbox) → Preview (counts and +/~/- rows with kept edits) →
    Apply (or Back). Request ids travel to Rust as positions and come back, so string web ids and open tabs survive.
    Code in `webapp/src/views/openapi-dialogs.js`; no new Tauri commands.
  - Fixtures (`cli/fixtures/openapi/`, README with URL, commit and license): swagger-petstore `openapi.yaml` (3.0.4,
    Apache-2.0, 19 operations), Adyen `CheckoutService-v71.yaml` (3.1.0, MIT, 805 KB, 28 operations, 143 examples),
    Kubernetes `apis__apps__v1_openapi.json` (3.0.0, Apache-2.0, 919 KB, 77 operations), vendored unchanged.
  - Tests: hitop-core 10 unit (`openapi/tests.rs`: mapping of a spec with every feature, YAML int keys, cycles and ref
    loops, pointer escapes, Swagger refusal, relative servers, flat + inline auth, folder round trip of the record, sync
    keeping name/file/scripts/assertion/auth/example/body edits while applying URL/header/example/baseUrl changes,
    prune) + `tests/openapi.rs` 3 (each fixture: counts, key mappings, folder round trip unchanged, no-op sync); CLI
    `tests/openapi.rs` 3 (Petstore `--to`, `--force` needed, detection, run against httpmock with the API key header;
    URL import with recorded source, 404, global storage; sync of edited files: dry run writes nothing, apply keeps
    renamed file + scripts + auth + body, adds, reports removed, second sync clean, `--prune`, non-imported folder
    refused); hitop-wasm 1; web `tests/pm.mjs` (the three fixtures through wasm, same counts, no-op sync); e2e
    `tests/openapi.cjs` (URL import via the dialog link and report, pasted YAML added, edits, sync preview/apply,
    prune, run of the synced collection against a local Node server) in web and Tauri mode. Full e2e suite green in
    both modes locally; clippy (workspace, core without default features, wasm32) and `cargo deny check licenses` clean.
    No new crates. CI run 41287086462 green on be02942 (all jobs, incl. desktop compile and web + Tauri e2e); Licenses
    run 41287086529 green.
  - Guide `site/content/guides/import-an-openapi-spec.md` (/terms/import-an-openapi-spec.html); CLI README "From OpenAPI".
  - Deviations: JSON maps are sorted (serde_json without `preserve_order`), so requests within a folder follow path
    order and generated example bodies list keys alphabetically (the same on every surface). Field-level merge: adding
    one header of your own made the spec's later header changes "kept" for that request (1.1 merges header and query
    rows one by one). A file path source isn't
    recorded (only URLs), so the web app's sync dialog is prefilled only for URL imports.
  - Open: external `$ref`s (other files/URLs) aren't fetched; OAuth 2 flows aren't set up (no OAuth in hitop); XML
    bodies aren't generated from schemas; `hitop mcp` has no OpenAPI tool (added in 1.1: `import_openapi`, `sync_openapi`); `cargo fuzz` target for the importer is T19's.

#### T18 Bring-your-own-key AI
- Dropped by the owner on 2026-11-01. Skip it; nothing later depends on it.

#### T19 v1.0 hardening and release
- Freeze format v1 (spec marked stable, a compatibility test suite).
- `cargo fuzz` targets for the YAML reader, Postman importer and OpenAPI importer, run in CI for a
  short time per push; `cargo audit` and `npm audit` in CI.
- Release CLI 1.0.0 and desktop 1.0.0; deploy.
- **Done when:** fuzzers run clean for 30 minutes locally; releases published. **Handoffs:** MAS
  build 1.0; App Store price change ($9–19) if the owner wants it.
- **Notes (2026-11-01, done; handoffs in section 6):**
  - Format freeze: `spec/collection-format.md` and `spec/run-report.md` are **stable**; spec section 7 states the promise
    (additive only within v1; unknown keys kept at the top level and on rows, an error inside `body`/`scripts`/`websocket`/
    `grpc`/enabled params; unknown auth `type` is an error, unknown keys inside `auth` dropped; breaking = `format: 2`, refused
    by v1 readers). Compatibility suite `spec/compat/v1/` (CC BY 4.0, README): minimal, http, graphql, websocket, grpc, openapi
    (with its `spec.yaml`), secrets (environments, globals, `${secret:...}` refs), examples, unknown-keys, quirks (non-canonical:
    `.yml`, flow style, numbers/booleans as strings, hidden and non-YAML files, a blank header row) and `run-report/report.json`.
    `hitop-core/tests/compat.rs`: read each folder from disk and in memory = `expected.json`, write + read back, byte-identical
    write for canonical cases, later `format` refused, v1 report keeps every key and type (native only). Case list only grows.
  - Fuzzing: `cli/fuzz` (own workspace, nightly, hitop-core without `native`): `collection_yaml` (in-memory files; a folder that
    reads must write and read back), `postman` (collection + environment/globals), `openapi` (import + sync, options from byte 0).
    `fuzz/seed.sh` builds corpora from the repo fixtures (corpora/artifacts gitignored; one small seed in `fuzz/seeds/openapi`).
    Workflow `fuzz.yml`: 60 s per target per push on nightly-2026-10-24, crash input uploaded. One crash found and fixed:
    request files differing only in case (`liVe-prices.yaml` + `live-prices.yaml`) read fine but couldn't be written back →
    both readers now refuse them (1b51e9f). Local 30-minute runs (nightly 2026-10-24, -O, 8-core box, all three in parallel),
    all clean: collection_yaml 1801 s, 2,436,097 execs, peak RSS 346 MB (rerun from scratch after the fix); postman 1801 s,
    1,699,348 execs, 561 MB; openapi 1801 s, 322,075 execs, 516 MB.
  - Audits: `licenses.yml` now runs `cargo deny check licenses advisories` for cli, hitop-wasm, server, desktop (sync-server
    already did) and `npm audit --omit=dev --audit-level=low` for desktop, webapp, site. Fixed: fflate 0.8.3 (GHSA-px8p-9vwx-vf98,
    the one open Dependabot alert, #237; the "2 moderate" were this one plus closed ones — all others are `fixed`), plist 1.10.3 →
    quick-xml 0.42 in the desktop (RUSTSEC-2026-0194/-0195), indicatif 0.18 (drops unmaintained number_prefix). tokio already
    1.54.1 (latest) everywhere. Ignored with reasons in deny.toml, all "unmaintained", no vulnerability: bincode (syntect's bundled
    dumps), zvariant/zvariant_derive/zbus_names (zbus 5 via keyring; merged into zbus 6), proc-macro-error and unic-* (Tauri 2
    build-time). sync-server keeps its RSA Marvin ignore.
  - Flaky WebSocket test fixed (fb37d84): the socket task dropped its channel ends in an unspecified order, so `events` could end
    while the inbox still took a send. Reproduced 25/300 under load; after the fix 0/300.
  - MAS bookmarks (a49eb5b): `desktop/src-tauri/src/bookmarks.rs` (objc2 0.6 + objc2-foundation 0.3, macOS only): inside the sandbox
    (`APP_SANDBOX_CONTAINER_ID`) `read_folder`, `write_folder` and `git` first resolve the folder's security-scoped bookmark and start
    accessing it once per launch (renewed when stale), or save one for a folder just picked (`<app data>/folder-bookmarks.json`);
    outside the sandbox nothing happens; closing a folder collection calls `folder_forget`. Entitlement
    `com.apple.security.files.bookmarks.app-scope` only in Entitlements.mas.plist (tauri.github.conf.json drops MAS entitlements).
    Checked by a cross `cargo clippy --target aarch64-apple-darwin` of the module here and by the new CI job `desktop-macos`
    (check + clippy on macos-14). Not run on a Mac: handoff. Only matters for the web-app overlay build (the React UI has no folders).
  - Docs: comparison pages, README, home page (three tiles), launch drafts: GraphQL, WebSocket (CLI, web app without handshake
    headers), gRPC (CLI) supported, the released desktop gets WebSocket/gRPC/Git with the new UI; OpenAPI, snippets, diff, secret
    managers, HTML reports, `hitop schedule` listed. Guides pin 1.0.0. Site built into the repo only: **not deployed** (needed).
  - Versions: workspace 1.0.0 (+ hitop-core dep; cli, hitop-wasm, fuzz, desktop locks hand-edited, `cargo metadata --locked` ok),
    desktop 1.0.0 (tauri.conf.json, Cargo.toml, package.json, package-lock.json). Before tagging: cli `cargo test --locked` 351
    passed, clippy (all targets; core `--no-default-features`), hitop-wasm test + clippy (native, wasm32), server tests 10, cargo
    deny all workspaces, npm audit all three, `cargo publish --dry-run -p hitop-core`.
  - Dependabot: the two moderate alerts were #237 fflate (webapp, fixed by 0.8.3) and #190 glib 0.18 (GHSA-wrw7-89jp-8q8g,
    unsound `VariantStrIter`, fixed in glib 0.20) in the desktop lock: glib comes from Tauri 2's GTK 3 stack (gtk-rs 0.18) on
    Linux and Tauri has no release on newer gtk-rs, and hitop never uses `VariantStrIter`; it stays open until Tauri moves
    (cargo-deny reports it as informational "unsound", which doesn't fail the check).
  - Releases on 1416bad (CI 41312937372 green; on 88d7c74 the e2e job failed once on jsonplaceholder and passed on rerun,
    run 41311581124): Release CLI https://github.com/bohdaq/hitop/actions/runs/41312973145 green: crates.io hitop and
    hitop-core 1.0.0, GitHub Release `cli-v1.0.0` with the five archives and `.sha256`, `ghcr.io/bohdaq/hitop:1.0.0`, `:latest`,
    `:1.0.0-debug`, `:debug` (amd64 + arm64; `docker run ghcr.io/bohdaq/hitop:1.0.0 --version` → `hitop 1.0.0`). The first
    cli-v1.0.0 run (41312515815, on 88d7c74) stopped in its test job before publishing anything: release-cli.yml's check job
    lacked actionlint and the Vault/LocalStack services the tests need on CI since T21/T22; fixed in 274fb75 and the
    unpublished tag moved to 1416bad. Release desktop https://github.com/bohdaq/hitop/actions/runs/41312516061 green (tag on
    88d7c74): dmg (universal), app.tar.gz, msi, setup.exe, deb, rpm, AppImage, all 1.0.0. hitop-action c623e3b: README and
    self-test default 1.0.0 (no action code change, so no v1.0.2 and `v1` stays on v1.0.1).
  - Deviations: no cargo-audit binary here, cargo-deny advisories is the audit (same RustSec DB). Desktop clippy on Linux couldn't
    run locally (no GTK dev libs); CI's desktop jobs cover it. `desktop-v1.0.0` on GitHub still ships the React UI
    (release-desktop.yml unchanged until the owner's T5 check), so it has no folders, WebSocket, gRPC or Git panel.

### Phase 3: teams (v1.x)

#### T20 Git UI in desktop
- Status, diff, commit, pull, push, branches and conflict resolution for the open collection
  folder, through `git2` with vendored libgit2 (the MAS sandbox can't run the `git` binary).
- **Done when:** tests on temp repos for commit, branch, merge conflict and resolution.
- **Notes (2026-11-01, done; handoff: try it on macOS, section 6):**
  - `cli/crates/hitop-git` (new, `publish = false`, in the cli workspace; no hitop-core dependency): git2 0.21.0 with
    `vendored-libgit2` (libgit2 1.9.7 via libgit2-sys 0.18.8, built from source, no system libgit2 and no `git` binary). Default feature `net` =
    git2 `https` + `ssh` + `vendored-openssl`. TLS by OS (read from libgit2-sys's build.rs): HTTPS uses WinHTTP on Windows,
    Secure Transport on macOS, OpenSSL on Linux; SSH is libssh2 over WinCNG on Windows and OpenSSL elsewhere. OpenSSL
    (3.6.3 via openssl-src, Apache-2.0) is vendored, so Linux and macOS builds need perl and make but no system OpenSSL. Without `net` only
    local and file:// remotes work (what the e2e mock builds). `cargo deny ... check licenses` passes for cli and desktop
    without a deny.toml change: cargo-deny reads the crates' fields (git2, libgit2-sys, libssh2-sys `MIT OR Apache-2.0`,
    openssl-src `MIT/Apache-2.0`, openssl-sys `MIT`). The vendored C library libgit2 is GPL-2.0 with the linking exception,
    which allows linking it into MIT code and shipping the result under any terms; no exception entry was needed.
  - API (every function opens the repository that contains the folder, so a collection may be a subfolder of a repo):
    `status` (branch, short head, upstream, ahead/behind, state, merge message, files with index/worktree change or
    conflicted, identity from git config, remotes, the folder's prefix), `diff(file, staged)` (hunks of context/add/del/note
    lines; untracked files as all new; binary flagged), `stage` (deleted files staged as removals; staging a conflicted file
    resolves it), `unstage`, `discard` (back to HEAD, untracked deleted), `commit` (author and committer = git config
    user.name/email; refuses an empty message, nothing staged and unresolved conflicts; during a merge it is the merge commit
    with MERGE_HEAD as second parent), `set_identity` (repo-local user.name/email, for when the global config can't be read,
    as in the MAS sandbox), `branches` (local with current/upstream/merged-into-current, remote without HEAD),
    `create_branch` (optionally switching), `switch_branch` (safe checkout: local changes the switch would overwrite stop it;
    a remote-only branch gets a tracking local branch), `delete_branch` (only merged into the current branch, never the
    current one), `merge_branch`, `fetch`, `pull` (fetch, then fast-forward, else a merge commit, else stop with conflicts;
    never rebases; a merge needs no uncommitted changes to tracked files; a branch without an upstream takes
    `<remote>/<branch>`), `push` (to the upstream or `<remote>/<branch>`, which becomes the upstream; a remote with commits
    the branch lacks is refused with "pull first"), `conflict(file)` (base, ours, theirs from the index stages and the
    working-tree text with markers), `resolve(file, ours | theirs | content{text} | worktree)` (worktree refuses a file that
    still has `<<<<<<<`/`>>>>>>>` lines), `abort_merge` (hard reset to HEAD). `json::call(cmd, args, auth)` is the single
    entry point (camelCase command names, `{ok}`/`{error}` from `json::answer`).
  - Remote auth (`Auth {token, username}` passed in; `auth::choose` is a pure function, unit-tested): SSH: user name
    (URL's or `git`), then the SSH agent; HTTPS: the token as password (user name: saved one, the URL's, or
    `x-access-token`), then git's credential helpers (`credential.helper`, run as programs); then the system default. Each
    way is tried once, then the operation fails with a message naming what was tried (libgit2 asks again after every
    failure, so without this a bad token loops). Limits: SSH host keys must already be in `~/.ssh/known_hosts` (libgit2
    checks it; there is no prompt for a new host); no SSH key files or passphrases (agent only); no proxy settings beyond what
    libgit2 reads; the MAS sandbox can't reach the SSH agent socket or `~/.ssh`, can't run credential helpers and can't read
    `~/.gitconfig` (set the identity in the panel, use HTTPS with a token there). LFS, submodules, hooks, rebase, stash,
    signing and partial (hunk) staging aren't supported.
  - Desktop (`desktop/src-tauri/src/git.rs`, not in commands.rs): `git {cmd, args}` (async) → `hitop_git::json::call`;
    fetch/pull/push read the token for the current remote's host from the OS keychain (service `hitop`, account
    `git/<host>/token`, optional `git/<host>/username`; `hitop_git::token_account`). `git_token_status {path}` →
    `{host, saved, username}` (host null for local/file:// remotes), `git_token_set {path, token, username}`,
    `git_token_delete {path}`. Tokens never touch files. Checked with `cargo clippy --locked --all-targets` in a Debian
    bookworm container (rust 1.91 + GTK/WebKit dev packages): clean apart from the existing unused `SecretStore` import in
    commands.rs.
  - Web app: `platform/tauri-git.js` (imported only by tauri.js, so neither the page nor its icons are in the web build;
    `platform.git` is null in web.js): a "Git" sidebar page (and a Git button on a folder collection's page) for the open
    folder collections (picker when there are several): branch bar with upstream, ↑ahead ↓behind, Fetch/Pull/Push;
    Conflicts, Staged and Changes lists (Stage/Unstage/Discard per file, Stage all/Unstage all, click for the diff); diff
    view; conflict view with ours/theirs/base, Take ours/Take theirs, an editable "Resolved text" (refuses markers) or Mark
    resolved after editing the file elsewhere; merge banner with Abort merge; commit box (author shown; name/email form when
    unset; "Commit merge" during a merge, prefilled with the merge message); branches (Switch, Merge into current, Delete when
    merged, Check out for remote-only, New branch); Remote access (token for the HTTPS host, saved in the keychain). Before
    each action the store writes pending folder edits (`store.flushFolder`); after actions that change files (pull, switch,
    merge, resolve, discard, abort) it reads the folder again (`store.reloadFolder`, the folder wins).
  - Tests: hitop-git 15 (temp repos, a local bare remote, global/system git config pointed at an empty dir): status with
    untracked/modified/deleted, diffs, stage/unstage/discard, commit author and message, identity required, subfolder
    collections, branch create/switch/delete-merged (unmerged and current refused, a switch blocked by local changes),
    fetch + behind, pull fast-forward, rejected push then pull merge commit and push, dirty tree refused, new branch pushed
    and tracked, remote-only branch checked out, conflict resolved by taking theirs (state, base/ours/theirs, commit
    refused, markers refused, merge commit with two parents, push), conflict resolved by an edit in the file, given text,
    ours and abort; auth chooser, host parsing, keychain accounts; json entry point. e2e `webapp/tests/git.cjs`: web mode
    checks the panel is absent; Tauri mode (41 checks) runs the real hitop-git through the mock (`tauri-mock.cjs` `git`
    spawns `cli/target/debug/examples/git-json`, built `--no-default-features`; `HITOP_GIT_JSON` overrides) on a copy of
    `cli/fixtures/sample` with a bare remote and a teammate clone driven by the git CLI: status, edit in the app → diff,
    stage, unstage, commit as the configured author, a README that isn't a collection file survives, push, branch create and
    switch, fetch/behind, pull fast-forward that reloads the collection, a conflict resolved with Take theirs and Commit
    merge, a second resolved with edited text (markers refused first), teammate pulls the merge, delete merged branch, token
    saved to and removed from the keychain only. CI web job builds the example before the Tauri-mode e2e (rust-cache adds
    `cli`).
  - Open: opening a folder and editing one request also rewrites `hitop.yaml` (the writer drops an `order:` list that
    matches the default order), an unrelated change in `git status` (format writer, T5 behaviour); open tabs keep their
    content after a pull or switch changes the request underneath (the sidebar and collection reload); no YAML-aware
    diff (plain lines); no commit history view; third-party notices (OpenSSL's Apache-2.0 NOTICE, libgit2's license) are
    not yet bundled with the desktop app (nothing in the repo collects notices today); the MAS build links vendored OpenSSL
    for libssh2 (export-compliance answers are the owner's, as for rustls today).

#### T21 Secret manager integrations
- References like `${secret:vault:path#key}`, `${secret:aws:name}`, `${secret:azure:vault/name}`
  and `${secret:op:item/field}`, resolved at run time in desktop and CLI.
- **Done when:** tests against local dev servers (Vault dev mode, LocalStack) and a stubbed `op`.
- **Notes (2026-11-01, done):**
  - Syntax (spec §5b): `${secret:vault:<mount>/<path>[#field]}`, `${secret:aws:<name|ARN>[#jsonKey]}`,
    `${secret:azure:<vault>/<name>[/<version>]}`, `${secret:op:<vault>/<item>/[<section>/]<field>}` or `${secret:op://...}`.
    The inner text is a variable name, so interpolation, `prepare`, auth, GraphQL, snippets and every `Masker` needed no change:
    a resolved reference goes into `Secrets` (`Origin::SecretManager`). References inside variable values
    (`token: ${secret:vault:...}` in an environment) are filled by `Secrets::expand_refs` (the runner calls it; CLI sites through
    `secret::fill_refs`). Scripts and saved examples are never scanned.
  - Core: `secret_refs.rs` (find/parse/expand, no deps, compiled in hitop-wasm too) and `secret_managers.rs` behind the default
    feature `secret-managers` (= `native` + `ring`). Vault/OpenBao: `sys/internal/ui/mounts/<path>` for mount + KV version (as the
    vault CLI), fallback on `/data/` in the path when the token may not ask; VAULT_ADDR/TOKEN/NAMESPACE, BAO_*, `~/.vault-token`; one GET
    per secret for several fields. AWS: SigV4 written here on `ring` (already in all three locks through rustls), checked against
    the aws-sig-v4-test-suite `get-vanilla` vector and a GetSecretValue request signed by botocore 1.34; credentials env → `~/.aws`
    profile keys or `credential_process` → `aws configure export-credentials --format process` (SSO, assumed roles); region from ARN,
    AWS_REGION, AWS_DEFAULT_REGION, profile; AWS_ENDPOINT_URL[_SECRETS_MANAGER]. Azure: client credentials
    (AZURE_TENANT_ID/CLIENT_ID/CLIENT_SECRET, AZURE_AUTHORITY_HOST) or `az account get-access-token --resource https://vault.azure.net`,
    then REST `api-version=7.4`. 1Password: `op read <uri> --no-newline`. Subprocesses: stdin null (MCP's stdin is the protocol),
    stdout read on a thread, killed at the timeout, errors carry stderr only. Timeouts 15 s HTTP / 60 s CLIs
    (`HITOP_SECRET_MANAGER_TIMEOUT`). Cache per `Resolver` (one per run, per MCP tool call, per desktop send/run).
  - Binary size/licenses: no new crate in any lock (cli lock +3 edges: ring for hitop-core, httpmock dev for hitop-core, reqwest dev for
    hitop; desktop +1 edge), so no measurable growth beyond the code; `cargo deny ... check licenses` passes for cli and desktop.
    The official AWS SDK was not used (it would add aws-smithy/aws-lc-rs and a C build to the static musl binaries).
  - CLI: every command that fills variables (`send`, `run`, `requests send`, `ws`, `grpc`, `snippet --show-secrets`, `mcp`)
    resolves the references its request(s), collection variables and auth, environment, globals, `--var` and data rows use, before
    sending. `--no-secret-managers` lives on `SecretFlags` (so every such command) and on `hitop mcp`; `HITOP_SECRET_MANAGERS=off`;
    either one makes a request that uses a reference an error listing them (exit 2 for run). `hitop secret check [-c] [--env]
    [--list]`: every reference with where it is used, ✓/✗ with the reason, never a value; exit 1 when one fails. Snippets: references
    become `{{secret:...}}` placeholders (also inside variable values) unless `--show-secrets`. MCP: resolves only references in the
    workspace files, not in a tool call's `variables` (an AI client can't read arbitrary paths); values remembered for masking and for
    `refuse_secrets` on create/update.
  - Desktop: Tauri command `secret_refs_resolve {refs}` → `{inner: value}`, error = one line per failed reference. Web app:
    `store.resolveRefs()` (in `prepare`, `wsConnect`, `grpcCall`, now async) gathers refs from the request (JSON walk, scripts
    skipped) and the variables; values in `store.refValues` (memory, reset at each send and run), merged by `variables()`, masked via
    `secretValuesFor()` in run results, history and response snapshots. The Code dialog always keeps references as placeholders.
  - Tests: hitop-core unit 15 new (refs parse/find/expand 3; resolver: dates, SigV4 ×2, fields, AWS profiles/regions, missing config,
    Vault KV v2/v1/fallback/namespace/token file against httpmock, Azure client credentials + `az` stub + 404, `op` stub, AWS
    credential chain incl. credential_process and stub `aws`, command timeouts; runner 1). `hitop/tests/secret_managers.rs` 8: run
    with op + Azure and the masking check across CLI output, junit.xml, report.json, report.html, history/config dir and the folder
    files; send + requests send with history; errors naming references, `--no-secret-managers`, `HITOP_SECRET_MANAGERS=off`, unknown
    manager; `secret check` (ok, `--list`, a failing one → exit 1); snippets; MCP run_request/run_collection; Vault dev (KV v2 with
    and without `data/`, a KV v1 mount, `~/.vault-token`, wrong token) and LocalStack (JSON key, plain, ARN, missing secret). CI `cli`
    job runs `hashicorp/vault:1.20.4` (dev mode is the image's default command) and `localstack/localstack:4.4` (no auth token
    needed) as service containers; those two tests fail when `CI` is set without them, skip locally. Web: unit checks for
    `secretRefs`/`withSecretRefs`; e2e `tests/secret-refs.cjs` (web: refused with the note, nothing sent; tauri: values sent, one resolve
    call, nothing in history or storage files, environment keeps the reference, failing reference named).
  - Deviations: "the browser shows them unresolved with a clear note" is implemented as: the request isn't sent and the note names the
    references (sending `${secret:...}` text would be useless). The single request view shows the response as received, and the
    WebSocket/gRPC logs show the URL/message as sent, as for environment secrets (T2 deviation). The React desktop (`desktop/src`,
    `run_collection_cmd`) doesn't resolve references: it is frozen until the T5 switch. On macOS a Dock-launched desktop app (until 1.1's Settings section) doesn't see
    shell env vars (VAULT_ADDR...), and the MAS sandbox can't run `op`/`az`/`aws`; documented in the guide, not tested (no Mac).
    Real Azure, AWS, 1Password and HCP Vault were not exercised (no accounts): stubs, the mock and the dev servers stand in.
  - Pre-existing flake seen once locally: hitop-core `websocket::client::tests::echoes_text_and_binary_with_headers_auth_and_subprotocols`
    (`late` send after close succeeded); passed on rerun, not touched.

#### T22 HTML reports and scheduled runs
- `--reporter html` (a single self-contained file).
- `hitop schedule` writes a scheduled GitHub Actions workflow or a cron entry for a collection.
- **Done when:** HTML report snapshot test; generated workflow validates with `actionlint`.
- **Notes (2026-11-01, done):**
  - HTML: `Report::to_html` in hitop-core `report.rs` (no new dependency, so no lock changes), `--reporter html` →
    `<out>/report.html`. One page: summary cards, `--bail` note, every collection, iteration (data row table), request
    (`<details>`, failed ones open: method, status, duration bar, URL, file, response time, error, failure line,
    assertions). Inline CSS, **no script**: the "Show failed requests only" switch is CSS `body:has(#failures-only:checked)`;
    CSP `default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'`, so it loads nothing and
    works offline; light/dark via `prefers-color-scheme`. Every report string goes through one escaper (`& < > " '`,
    C0 controls → U+FFFD); the only value in a `style` attribute is a computed integer. Masking: built from the same
    masked `Report` as JSON/JUnit (CLI test `secrets_are_masked_in_every_report` now checks report.html too).
  - Determinism: the page is a pure function of the `Report` (no clock), so the test hook is the fixture:
    `hitop-core/tests/report_html.rs` renders `tests/golden/report/report.json` (2 collections, data iterations,
    assertion/HTTP/connection failures, `bailed`, null/set environment and file) and compares with `report.html`
    (`UPDATE_GOLDEN=1`). Escaping test: a hostile string (`"'><img onerror>…<script>…<!--&amp;\0`) appended to every
    string field and every data key; asserts no raw payload, the exact escaped form, every tag on the page in an
    allowlist, and even quote counts per tag. Checked visually in headless Chromium, light and dark, 1100 and 420 px.
  - `hitop schedule <folder> --github`: workflow at the git root (nearest `.git` above the folder; error outside a repo),
    `hitop-<slug>.yml` (slug of `--name`, else collection name + env), `actions/checkout@v6` (persist-credentials false),
    `bohdaq/hitop-action@v1` with `version` = this hitop's (`--hitop-version`), `path` relative to the root, `env`,
    `reporter: cli,junit,json` (plus `html` and an `actions/upload-artifact@v7` step `if: always()` for versions after
    0.3.0, since 0.3.0 images lack it), `permissions: contents: read`, concurrency group, `timeout-minutes: 30`. Secrets:
    `HITOP_SECRET_<ENV>_<NAME>: ${{ secrets.<same> }}` on the step (`secrets::env_var`), default = the env's and the
    folder `globals.yaml`'s secret names, or `--secrets a,b` (needs `--env`; `NAME=value` refused). User strings go
    through a YAML double-quote function that refuses `${{`. Refuses an existing file without `--force`. Prints the
    `gh secret set` lines. `--cron` alone prints one crontab line (`current_exe()`, single-quoted absolute paths, `%` →
    `\%`, `--out <log-dir>/<UTC time>`, appends to `<log-dir>/hitop.log`, default `$XDG_STATE_HOME` or
    `~/.local/state/hitop/runs/<slug>`) and secret hints on stderr. Errors exit 2 (`main.rs`, like `run`).
  - Cron validation (`commands/schedule.rs`, unit tests): 5 fields, `*`, numbers, `a-b`, `*/n`, `a-b/n`, lists, a lone
    month/day name; bounds 0-59/0-23/1-31/1-12/0-6 (7 refused: "Sunday is 0"); `@daily` and `a/n` refused with a hint.
    With `--github` the shortest gap must be ≥ 5 minutes (actionlint flags faster schedules).
  - actionlint 1.7.12 (2026-03-30, sha256 `8aca8db9…a3d8` from the release's checksums file): CI `cli` job downloads and
    checks it, exports `HITOP_ACTIONLINT`; `tests/schedule.rs` lints every generated workflow (skips locally without
    it, fails when `CI` is set) and a CI step generates two workflows from `cli/fixtures/sample` and lints them.
  - Docs: `spec/run-report.md` section 5 (HTML report), CLI README ("Scheduled runs", `html`), guides
    run-hitop-in-github-actions (html artifact, "Run on a schedule"), run-collections-in-ci, run-hitop-in-gitlab-ci;
    built into the site on next deploy (not deployed). CLI-only by nature (reports and schedules belong to `hitop run`).
  - Tests: hitop-core report_html 3 (golden, self-contained, escaping) + report unit 7; hitop schedule unit 6 (cron valid/invalid,
    intervals, slugs/versions, YAML quoting, crontab line) + tests/schedule.rs 5 (exact workflow + actionlint, html variant,
    secrets, 10 usage errors, crontab); tests/run.rs 13 (html in the masking test, `html_report_is_one_escaped_page`). Full
    `cargo test --locked`, clippy (`--all-targets`, core `--no-default-features`) clean. CI run 41289440630 green, its actionlint
    steps included.
  - Open: the html reporter reaches the image and the action with the next CLI release; until then generated workflows
    pin 0.3.0 without html. hitop-action could upload `report.html` itself (an `html-artifact-name` input) — not done,
    the guide shows the extra step.
  - For T19: `hitop schedule` pins `CARGO_PKG_VERSION`, so a build between a version bump and its tag writes a workflow
    for an image that isn't published yet; tag promptly after a bump (`--hitop-version` is the escape hatch).

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
- **Notes (2026-11-01, done; email sign-in waits on the SMTP handoff in section 6):**
  - Layout: `sync-server/` is its own Cargo workspace: `hitop-sync-server` (AGPL-3.0-or-later,
    `sync-server/LICENSE` is the GNU text) and `crates/hitop-sync-crypto` (MIT, its own LICENSE).
    The crypto crate lives here, not in `cli/`, so T23 never touched the cli lock other agents were
    editing; T24 adds it to the clients (a path dependency, or move it into the cli workspace).
    The server links it only as a dev-dependency (tests play the clients);
    `tests/no_crypto_in_server.rs` fails if the server ever depends on it, `hpke`,
    `chacha20poly1305` or `x25519-dalek`.
  - Crates: `hpke` 0.15.1 (features x25519 + chacha only; no paid audit, Cloudflare reviewed 0.8),
    `chacha20poly1305` 0.11 (NCC audit 2020), `webauthn-rs` 0.6.2 (MPL-2.0; passkeys, UV
    required), `rusqlite` 0.41 bundled, `postgres` 0.19 (feature `postgres`), `lettre` 0.11 with
    rustls + webpki-roots (no OpenSSL), rust-web-server 18.19 like hitop-server. Dev: the
    `softpasskey` authenticator from `webauthn-authenticator-rs` for a real passkey round trip.
  - Construction (THREAT_MODEL.md, byte layouts in `vectors/v1.json`): blob envelope `0x01 || u32
    epoch || 24-byte nonce || XChaCha20-Poly1305 ct`, AAD = `"hitop-sync/v1/blob"` + length-prefixed
    workspace id and blob id + u64 version + u32 epoch; key wrap = HPKE base mode
    DHKEM(X25519, HKDF-SHA256) / HKDF-SHA256 / ChaCha20-Poly1305 with info =
    `"hitop-sync/v1/wrap"` + workspace id + epoch. Removing a member is one call that must carry the
    next epoch's key wrapped for exactly every remaining device; old-epoch writes then get 409
    `stale_epoch`. Writes are compare-and-swap on `version = current + 1` plus a version vector that
    must descend from the stored one; otherwise 409 with the current state.
  - Test vectors: blobs both ways, wraps in the open direction (DeriveKeyPair recipients), and
    failure cases; checked by `published_test_vectors` and, once by hand, by `@noble/ciphers` 1.3
    and `@hpke/core` 1.7 in Node (all matched). The web app run of them is T24.
  - Tests: crypto 6 (+1 ignored generator); server 3 unit, auth 9 (magic link once/expiry/rate
    limit/hashes only, sessions, passkey register + sign-in + replay + wrong origin, no files
    served, CORS), sync 5 (wraps for every device, versions and conflicts, malformed writes and
    quotas, roles, removal with rotation), plaintext 1 (database files and every answer hold no
    plaintext, workspace key or device secret), dependency 1, Postgres 1 (CI service container).
  - Deploy (VM, all backed up): `/usr/local/bin/hitop-sync-server`, `hitop-sync.service`
    (DynamicUser, DB in `/var/lib/private/hitop-sync`, 127.0.0.1:7881), `/etc/hitop-sync/env`
    (created; public URL, rp id `hitop.rws8.tech`, CORS for the web app, `SMTP_TLS_SERVER_NAME`,
    `SMTP_FROM`). Front rws: upstream + two routes (rate limits 20/min on `/v1/auth/*`, 600/min
    otherwise) and the ACME name; backups `rws.config.toml.bak-hitop-20261101-032639`,
    `cert.pem`/`key.pem.bak-sync-20261101-032639`. `rws --check` needs the service's
    `admin.env` loaded (the admin token), otherwise it fails even on the current config. Cert move
    with `rws-mail-cert.path` stopped: new certificate in ~18 s, then mail reloaded; rws8.tech,
    git, hitop, php, mta-sts, mail 465/587/993 all checked before and after. Reachability:
    checked through the front rws with `--resolve` on the VM; neither this workstation nor the
    stage cluster can reach 168.119.17.57 directly (existing sites included), but Let's Encrypt
    validated the name from outside.
  - Deviations: `/etc/hitop-sync/env` had no `SMTP_URL` (the directory didn't exist), so email
    sign-in answers 503 `mail_off` on the VM until the owner adds it (handoff). STARTTLS to
    127.0.0.1:587 verifies as mail.rws8.tech, so only the line is missing. The licenses check is a
    separate `sync-server` job (two policies: AGPL for the server, the root MIT `deny.toml` with
    `--exclude hitop-sync-server` for the crypto crate) instead of a matrix entry.
    `sync-server/deny.toml` ignores RUSTSEC-2023-0071 (rsa Marvin attack, through webauthn-rs):
    the server only verifies RSA signatures and holds no RSA private key. Docker image: Dockerfile
    only (built and smoke-tested locally, 34 MB, distroless); no publish workflow yet.
  - Open for T24: key fingerprints and trust-on-first-use pinning in the clients (the server
    hands out public keys; THREAT_MODEL.md, "accepted risks"); random blob ids; re-encrypt old
    blobs after a rotation; passkey sign-in from the web app needs `hitop.rws8.tech` as rp id
    (set) and the web app origin (set).

#### T24 Client sync, teams, cloud runs
- Sync in desktop, CLI and web app; conflict UI; invites; roles owner, editor and viewer.
- Scheduled runs and monitors: on a self-hosted runner, or on the server only when a workspace
  owner explicitly shares a run key (documented trade-off with E2E encryption).
- No billing and no paid accounts.
- **Done when:** two-client sync e2e test; role enforcement tests.
- **Notes (2026-11-01, done; deploy and the external review are handoffs):**
  - Layout: `hitop-sync-crypto` moved to `cli/crates/` (cli workspace, publishable 0.1.0; release-cli.yml publishes it before
    hitop-core when crates.io doesn't have its version); sync-server dev-depends on it by path, still never links it. Client
    decisions are `hitop_core::sync` (no deps, so desktop's lock didn't change): unit = a collection folder's files, one blob per
    file under a random 128-bit id, plaintext `{"v":1,"path","text"}`; `plan(local, state, remote, me, ids)` → writes, removes,
    pushes, conflicts, merged, skipped; three-way against the last synced text; `hitop.yaml`/environments/globals merge key by key
    (request `order` merges as a list); a request changed on both sides is a conflict and never pushed until resolved; create
    races converge on the smaller blob id (the loser's client merges and tombstones it); remote paths must pass `is_synced_path`
    (no `..`, hidden, absolute, other files); own pushes coming back and older versions are ignored; a same-text rewrite (re-encryption)
    keeps a local edit. Core tests 9 (an in-memory server with the real CAS rules), wasm 1 + web unit (vectors in wasm).
  - Crypto additions: `fingerprint(pk)` = first 16 bytes of SHA-256("hitop-sync/v1/fingerprint" || pk), 8×4 hex (zero key pinned in a
    test, cross-checked with Python); `random_hex`. hitop-wasm exposes plan/pushed/resolve and keygen/wrap/unwrap/seal/open/
    fingerprint/checkVectors (getrandom 0.4 `wasm_js`, no rustflags needed); `webapp/tests/unit.mjs` runs `vectors/v1.json` in WASM.
  - CLI `hitop sync login|logout|whoami|ls|init|link|unlink|status|pull|push|resolve --mine|--theirs|--show|members|invite|accept|
    add|remove|role|trust|runner create` (commands/sync/). Keychain `sync/<host>` (or `HITOP_SYNC_KEY_FILE`), TOFU pins (wraps only
    for pinned keys: `--trust <fp>` or a TTY prompt; a changed key for a pinned id is refused), `remove` rotates and re-encrypts every
    live blob, `members` hands older and current epoch keys to trusted new devices, `runner create` prints `hitop-runner-1.<b64url>`
    (token + device secret; `HITOP_SYNC_RUNNER` makes `pull` read-only for that workspace, and the server wins: a CI checkout's
    older copy is overwritten and stray request files go, no state kept). Exit 1 while a conflict is open.
    `hitop schedule --sync`: GitHub = a docker `sync pull` step with `secrets.HITOP_SYNC_RUNNER` before the action (actionlint
    clean); cron = pull as this device, then run.
  - Desktop + web: one implementation (`webapp/src/sync.js`, Sync page `views/sync-view.js`), no new Tauri commands: HTTP via fetch
    (web; the server's CORS) or `send_request` (desktop), device secret via `folderSecrets` (keychain on the desktop, localStorage
    on the web). A shared collection is a folder collection: its disk folder, or app storage under the ref `hitop-sync:<ws>`
    (`withSyncFolders` wraps `platform().folders`; Open folder/Save to folder now check `folders.pick`). Conflicts: View both /
    Keep mine / Take theirs; members with fingerprints, Trust… dialog, Add disabled until trusted, roles, remove; invites by mail or
    link; runner credential shown once with the runner-vs-server trade-off text. Closing or deleting a shared collection marks its
    link closed (a reload of its files re-links it) and the next start drops its local copy.
  - Server (migration 2, SQLite and Postgres, applied on start): `users.runner_of`, `invites`; `PUT .../members/:user {role}`
    (one owner kept, runners stay viewers), `POST/GET/DELETE .../invites` (mailed via the existing SMTP path or a link, token hashed,
    7 days, 50 open, 20 mails/h per workspace, email-bound when mailed), `POST /v1/invites/accept`, `POST .../runners` (10 per
    workspace, viewer, `<id>@runner.invalid`, one-year session); runner accounts can't sign in, add keys, look up users, accept
    invites, register passkeys or create workspaces; removing one deletes it; deleting a workspace deletes its runners and invites;
    `/invite` page (code in the fragment). A test opens a T23-shaped database (migration 1 + an account) and checks it migrates.
  - Tests: sync-server roles 2 (every role × read/changes/blob/wraps/write/delete/wrap-for-others/add/role/invite/list/delete invite/
    runner/rotate/remove/delete workspace for owner, editor, viewer, runner, outsider, removed member; a removed member can't open
    anything written after the rotation, not even from the database bytes), teams 5, postgres 2 (teams flow added); CLI tests/sync.rs
    2 (two clients + a viewer + a runner against the real server binary: create, edit, delete, conflict both ways, invite/accept/
    trust/add, remove with rotation and re-encryption, no plaintext in the server's files), schedule +1; web e2e tests/sync.cjs in web
    and Tauri modes (two browser contexts, 24 checks incl. reload, a deleted shared collection staying deleted, keychain-only device
    key on the desktop, no plaintext on the server, the CLI pulling with a credential the web app made when HITOP_BIN is set). Every
    other e2e suite still passes in both modes. CI builds the sync server in the cli and web jobs
    (`HITOP_SYNC_SERVER_BIN`, required when `CI` is set).
  - Deviations: server-side runs are not implemented. The spec allowed a minimal server path; it would need a decrypting key on the
    server (the server binary links no crypto by design, tests/no_crypto_in_server.rs), so T24 ships the runner path fully and
    documents the trade-off (Sync page, guide, THREAT_MODEL "Teams"): a self-hoster who wants it runs a runner credential on the
    host as its own service. Web-app collections without a folder sync collection files only (they have no environments of their
    own); folder collections sync their environments and globals too. Desktop folder collections are synced through the canonical
    writer (a hand-formatted file is pushed once in canonical form); that disk-folder path (`applyFiles` → `folders.write`) has no
    e2e (tests/sync.cjs shares app-storage collections), so it is in the macOS check below. Removal can't take back what a member already synced (documented).
  - Open: authenticated wraps (HPKE base mode doesn't authenticate the wrapper: a server can hand a new member a key of its choice;
    THREAT_MODEL item 2); signed manifest; device recovery; the web app's device key in localStorage; `hitop sync` reaches users
    with the next CLI release (first to publish hitop-sync-crypto); generated `--sync` workflows need that image (> 1.0.0).

#### T25 SSO and audit log
- OIDC sign-in and an audit log in the sync server, open source.
- **Done when:** tests against a local OIDC provider (for example Keycloak in a container).
- **Notes (2026-11-01, done; deploy and the hosted-SSO decision are handoffs):**
  - Server (AGPL, `sync-server/src/oidc.rs`, `audit.rs`; migration 3 for SQLite and Postgres: `oidc_identities`, `audit_log`,
    `audit_chain`). SSO: the server is a confidential OIDC client (authorization code + PKCE S256, discovery with exact issuer match,
    JWKS cached 1 h and refetched on an unknown `kid` at most once a minute, `iss`/`aud`/`azp`/`exp`/`iat`/`nonce` checked, asymmetric
    algorithms only, `none`/HS* refused). `POST /v1/auth/sso/start {challenge=SHA-256(verifier), returnTo?, email?}` → provider URL;
    `GET /v1/auth/sso/callback` exchanges the code (client secret by HTTP Basic + PKCE) and hands a one-time code to the browser only
    (302 to a loopback `returnTo`, to an allowed web origin in the fragment, or a paste page); `POST /v1/auth/sso/finish {code, verifier}`
    → session. Accounts: (issuer, subject) first, else link by a *verified* address, else a new account; unverified or missing
    `email_verified` refused (`HITOP_SYNC_OIDC_ASSUME_VERIFIED=1` for Entra, only with allowed domains). Env: `HITOP_SYNC_OIDC_ISSUER`,
    `_CLIENT_ID`, `_CLIENT_SECRET` or `_CLIENT_SECRET_FILE`, `_ALLOWED_DOMAINS`, `_REQUIRED_GROUPS`/`_GROUPS_CLAIM`, `_SCOPES`, `_NAME`,
    `HITOP_SYNC_SSO_REQUIRED` (no sign-in mails, no passkey sign-in for those domains; checked at mail request, link use and passkey).
    Crates: jsonwebtoken 10 (aws-lc-rs backend, already in the tree via rustls; was 9.3 until 2026-11-02) and ureq 3 (rustls + webpki roots); cargo-deny clean.
  - Audit log: 19 event types (`audit::EVENTS`: signin with method, signin_failed with reason, signout, passkey_added, device_key_added,
    sso_linked, sso_config_error; workspace_created/deleted, member_added/removed, role_changed, key_rotated, key_wrapped,
    invite_created/deleted/accepted, runner_created, session_revoked), written inside the action's transaction (failures in their own),
    with actor, target, small JSON detail, time, IP truncated to /24 or /48 (`HITOP_SYNC_AUDIT_IP=off` drops it; decision: truncate,
    not hash, since a hashed IPv4 is reversible), user agent ≤ 200 chars; client IP from the last `X-Forwarded-For` entry only when the
    peer is in `HITOP_SYNC_TRUSTED_PROXIES` (default loopback; the front rws appends it). One SHA-256 chain per workspace plus the
    account chain (`""`); `audit_chain` keeps head and purge anchor. `GET /v1/workspaces/:ws/audit` (owner; limit/before/event/actor/
    since/until), `.../audit/verify`, `GET /v1/me/audit`, `hitop-sync-server audit-verify` (every chain, exit 1 when broken); purge at
    start and hourly (`HITOP_SYNC_AUDIT_RETENTION_DAYS`, default 365). Rows send `detailJson` (the hashed text) for client checks.
  - Clients: CLI `hitop sync login --sso [--no-browser] [--email hint]` (loopback listener on 127.0.0.1:0, `xdg-open`/`open`/rundll32,
    `HITOP_NO_BROWSER` to print the link; `--no-browser` = paste the code), `hitop sync audit [-c dir] [--json] [--verify] [--mine]
    [--event] [--actor] [--limit]`; `--verify` recomputes the chain locally, asks the server's check and pins the head in
    `audit-heads.json`. Web/desktop Sync page: "Sign in with SSO" (new tab on the web, a link on the desktop; paste the code: no new
    Tauri command), Members → "Show audit log" (owners) with the server's chain check.
  - Tests: sync-server unit 7 (+ hash vector pinned and cross-checked with Python, IP truncation, migration-2 → 3 upgrade with a
    team in it); sso 9 (mock IdP in `tests/common/idp.rs`, P-256 keys generated per run: flow, code to the starting client only,
    returnTo rules, 16 bad-token cases incl. wrong aud/iss, expired, future iat, nonce replay, azp, foreign key, alg none, HS256 with
    the client secret, tampered signature, unverified/missing email_verified; state single use and expiry; JWKS cache and rotation;
    domains/groups/SSO-required; config errors audited); keycloak 7 (Keycloak 26.4.2 dev mode, realm `tests/fixtures/keycloak/
    hitop-realm.json`, browser login driven over HTTP: login, linking to a mailed account, unverified never linked, domain and group
    restriction, SSO-required, replayed redirect/stolen code, real-token checks for nonce replay/bad signature/swapped payload/wrong
    aud/wrong iss (Keycloak names itself after the host)/expired, wrong issuer and wrong secret as config errors); audit 5 (every event
    type and no secret in any row, owner-only read and filters/pages, edit/delete-first/middle/last/reorder/rehash detected, purge
    keeps the chain verifiable, IP rules); postgres +1 (SSO link, audit page, tamper, purge on Postgres). CLI tests/sync.rs: audit
    after the team flow, `login --sso` through Keycloak with the secret in a file and SSO required. Web tests/sync.cjs (web and Tauri):
    audit log + chain check, SSO error without SSO, and the full SSO sign-in in a real browser tab with Keycloak. CI runs Keycloak as a
    `docker run` step in the sync-server, cli and web jobs (services can't pass `start-dev --import-realm`).
  - Deviations: the web app and desktop take the SSO code by paste, not a redirect back into the app (one path for both, no Tauri
    command; the CLI gets the loopback redirect). Existing sessions survive turning SSO on (documented with the SQL to end them).
    Tamper evidence only: someone who writes the database can rewrite a whole chain; clients that pinned a head notice (THREAT_MODEL
    open item 7: anchor heads outside the server). jsonwebtoken stayed on 9.3 here; moved to 10 with the aws-lc-rs
    backend (already linked through rustls) on 2026-11-02 for GHSA-h395-gr6q-cpjc.

#### T26 iOS companion code
- Swift package `HitopKit` (format reader with Yams, runner on URLSession, environments, keychain
  secrets), testable with `swift test` on Linux when a Swift toolchain is available.
- SwiftUI app in `ios/`: open collection folders from Files or iCloud Drive, run requests and
  collections, switch environment, App Intents for Shortcuts, a status widget. No editing.
- **Done when:** `HitopKit` tests pass (on Linux or in the owner's Xcode). **Handoff:** owner
  builds, tests and submits.
- **Notes (2026-11-01, done; handoff in section 6):**
  - `ios/HitopKit` (Swift package, Swift 6, iOS 17 / macOS 14, MIT): reader for format v1 folders
    (`hitop.yaml`, `requests/**`, `environments/`, `globals.yaml`) with hitop-core's defaults, ids,
    path checks and errors; Yams 6.2.2 pinned exactly (`Package.resolved` committed) with
    `Resolver.basic` and the YAML 1.2 core schema done in `YAML.swift`, which closes the T1 note
    (`on`/`yes`/`no` stay strings). Same JSON model field names where they apply; the companion
    drops what only editors need (unknown keys, examples, OpenAPI hashes) and keeps each request's
    path. Variables: globals < collection < environment < run values < script-set < secrets, one-pass
    `${name}` as in the core. Runner on URLSession (ephemeral session per run, so cookies carry within
    a run only) with auth presets, GraphQL bodies (variables spliced as typed), urlencoded and
    multipart bodies; WebSocket and gRPC requests are left out of runs as `hitop run` leaves them
    out (the app marks them "not run on iOS"). Scripts: the core's pre/post preambles and
    `assert.*` verbatim plus the `pm.*` shim (copy in `Resources/`, a test checks it equals
    `shared/pm-shim.js`), on JavaScriptCore on Apple platforms; with no engine (Linux library use)
    scripts are skipped and reported in an added `notes` key. `report.json` shape with every
    documented key (nulls kept), `file` = `<folder name>/requests/<path>.yaml`. Secrets: env var,
    given values, then a `SecretStore` (Keychain service `hitop`, account `<workspace>/<env>/<name>`,
    globals as env `global`); no prompt (the app has a Secrets sheet). `${secret:...}` references fail
    the request with a message naming the reference. `RunStatus`/`RunStatusStore` hold the last run
    per collection (counts and first failure line, no response data) for the widget and Shortcuts.
  - Tests (`swift test` on Linux, 42): the T19 compatibility suite (`spec/compat/v1`: every case reads
    as its `expected.json` for the fields HitopKit keeps; the report has every key and type of the
    v1 example report), YAML core schema, folder reading and error cases, the core's
    params/interpolation/auth/GraphQL/form unit cases, scripts on Node (the same JS JavaScriptCore
    runs; `vm.runInNewContext`), secrets and masking, the runner against `mock_server.py` (sample
    fixture, globals, overrides, failures, bail, cookies, wire format), and `CLICompatibilityTests`:
    the sample fixture plus a GraphQL/API-key/globals/pm.test request run by both `hitop run` and
    HitopKit against the same mock server; the requests on the wire (minus each client's own headers)
    and the reports (minus timings) are equal. CI job `hitopkit`: builds hitop, then `swift test` in
    `swift:6.4.2-noble@sha256:bca121f6…` with python3 and nodejs from apt; fails on any skipped test.
  - The reader also refuses request paths that differ only in case (hitop-core 1b51e9f).
  - `ios/HitopCompanion`: XcodeGen `project.yml` (the .xcodeproj is generated and git-ignored), app
    target (Files/iCloud Drive folder picker, security-scoped bookmarks in the app group, coordinated
    reads that first download iCloud placeholders, collection and request lists, run one request or
    the collection, environment picker, report view and `report.json` share, Secrets sheet to the
    keychain; no editing), App Intents (Run hitop collection, Get last hitop run status, App Shortcuts)
    and a WidgetKit extension (last run of pinned collections). Not compiled here (no Apple SDK on
    Linux): everything not UI is in HitopKit and tested.
  - Relation to the existing app: `ios/HiTop` (App Store "HiTop HTTP Client", $2.99, an editor with its
    own JSON storage and a small JavaScriptCore script service) is untouched and stays frozen. The
    companion is a separate app with its own bundle id; whether it ships as a new App Store app, or
    replaces HiTop as its next version (same bundle id `hitop.rws8.tech.HiTop`), is the owner's choice
    (section 6). HiTop could also adopt HitopKit later to read folders.
  - Deviations: `responseTimeMs` is the time until URLSession returns the whole response (no
    headers-only timing); explicit YAML tags (`!!str 8080`) aren't honoured (Yams resolves tags lazily);
    floats print through Swift's formatter (`1e+16` → `1e16`, otherwise as serde_json); URLSession may
    refuse a body on GET (the CLI sends it); no cookie jar across runs (the CLI keeps one per workspace).

#### Brand unification (owner request 2026-11-01)
- Owner request: one product name, one tagline and one logo on every release (site, web app, desktop
  GitHub and MAS builds, iOS apps, extensions, CLI help and READMEs, Docker labels, hitop-action).
  Overrides the "don't touch frontend/, extension/, chrome-extension/, chrome-store-package/" rule for
  names, titles, descriptions and icons only (no functional change there).
- **Notes (2026-11-01, done; handoffs in section 6):**
  - Name **HITOP** wherever an app name shows; `hitop` stays the command and every identifier (bundle
    ids `tech.rws8.hitop`, `hitop.rws8.tech.HiTop`, `hitop.rws8.tech.HitopCompanion`, extension ids,
    crates, paths, Xcode targets). Before → after: Firefox extension "HITOP - HTTP API Testing Tool" /
    action "HITOP API Tester" / popup "HITOP - API Tester" → "HITOP"; Chrome manifests ("HITOP - HTTP
    API Testing Tool", "HITOP - HTTP Client", "Open HITOP HTTP Client", "HITOP HTTP Client") → "HITOP";
    PWA manifest "HITOP - HTTP Client" → "HITOP"; extension page `<title>`/OG "HITOP - HTTP Client |
    Test REST APIs ..." → "HITOP"; Tauri window "HITOP — HTTP API Testing Tool" → "HITOP" (productName
    was already HITOP); iOS app display name and sidebar "HiTop" → "HITOP"; companion "hitop" and
    widget "hitop runs" → "HITOP Runs" (it is a separate App Store app by its bundle id today; two
    home-screen icons both called HITOP would be indistinguishable; becomes "HITOP" if the owner ships
    it as the next version of the iOS app, T26 handoff); Shortcuts "Run hitop collection" → "Run HITOP
    collection"; CLI `--help` about, crate descriptions and READMEs, guide titles "Run/Use hitop ..." →
    "HITOP"; Docker `image.title=HITOP` added.
  - Tagline (T11 positioning line, 70 characters): "The API client your terminal, your CI and your AI
    agent can all drive." in every description field: extension and PWA manifests, Tauri
    shortDescription/longDescription, crate descriptions, `hitop --help`, Docker description.
  - Logo: `brand/logo.svg` = the master (`site/public-assets/favicon.svg`, gradient #667eea → #764ba2).
    Replaced the red-circle icons with renders at the same names and sizes: site favicon-16x16,
    favicon-32x32, favicon.png (192), apple-touch-icon (180); frontend/public favicon.ico (16–256, six
    images as before), logo192, logo512; desktop/src/logo192.png; extension/icons icon-48, icon-96
    (192 × 192 as it always was); chrome-store-package favicon.ico, icons 16/48/128, logo192, logo512.
    The site now serves the master as favicon.svg (it wrote the flat header mark); the flat
    `#6b55d6` mark stays in the header and 404 (design system: no gradients). Tauri icons already
    matched a fresh render (< 0.1 % pixels, anti-aliasing): left as they are. iOS app icons (purple,
    opaque): left. Social preview already shows the purple mark and "HITOP": left.
  - `brand/`: logo.svg, logo-mono.svg, png/ (16–1024, 300 for the Edge store logo), BRAND.md, export.sh
    (resvg 2.6.2 pinned in brand/package-lock.json, so renders are byte-identical on every machine;
    `--tauri` regenerates the desktop set with Tauri's generator), check.sh. CI job `brand`:
    `export.sh --check` byte compare of every icon and a search for the old names. Build scripts copy
    brand/png instead of `sips`; create-favicon.sh calls export.sh.
  - hitop-action 8931565: action.yml name "Run HITOP collection", description, branding color purple
    (was blue), README title. Not tagged (task scope), so the Marketplace shows it after the next release.
  - Checked: extension build (`npm run build:extension`), `./deploy/build.sh` (site, web app, server
    tests), web app unit + e2e (web and Tauri modes), `cargo test -p hitop`, desktop `cargo check`.

#### 1.1 Owner follow-ups (2026-11-01)
- The owner answered three open questions of section 6 with "yes" on 2026-11-01: MCP tools for WebSocket and
  OpenAPI (T14, T17), OpenAPI sync merging header rows one by one (T17), a desktop Settings field for the secret
  manager environment (T21). They ship in the next CLI and desktop release, 1.1.0 (the owner pre-approved the tags;
  not tagged here: the coordinator releases).
- **Notes (2026-11-01, done):**
  - Row-by-row sync (cf400c6): the request record gains `openapi.rows` (additive: per-row FNV hashes of headers, keyed by
    lower-case name, and of `params` rows, keyed as written; `name#2` for a repeat). When both sides changed `headers` or
    `params`, sync merges by row: a row you added/edited/removed stays (outcome `yours`), each row the spec
    added/changed/removed applies where you didn't touch it (`updated`, a new row goes after the spec's row before it),
    a row both changed keeps yours (`kept`). `OpChange.rows` (`RowChange {field, name, change, outcome}`) is in the diff;
    the field still appears in `updated`/`kept`, so older consumers work. A 1.0 record (no `rows`) keeps an edited
    field whole once, then has row hashes. CLI prints the rows under the request (`~ header X changed by the spec:
    updated`, `= header Y added by you: kept`); the web app's sync preview lists them. Spec section "OpenAPI source"
    and a new compat case `spec/compat/v1/openapi-rows` (HitopKit's CompatSuiteTests read it too: swift test green).
  - MCP (75d5a1a): `run_websocket` (saved WebSocket request or a ws:// URL; saved messages by name then texts; until
    `count` or `wait_ms` (default 1000, 10000 with count), then close; at most 100 messages, 256 KiB, 60 s; handshake
    10 s; events with ms since connect; binary as base64). A URL gets no collection auth (the collection's credentials
    only go to its own requests); `${secret:...}` refs come only from workspace files, as for the other tools.
    `import_openapi` (URL or a file inside the workspace → a new collection folder `to` inside it, never over a
    collection, not the served folder or its requests/ or environments/; with global storage and no `to`, a global
    collection named like the CLI does). `sync_openapi` (preview by default, `apply` writes, `prune` only with `apply`;
    source defaults to the recorded URL). Paths: relative to, and canonicalized inside, the served folder (the server's
    working directory with global storage); `..` refused. `import_to_folder`/`import_to_storage` print to stdout, so
    MCP writes the collection itself. INSTRUCTIONS and the guide say the one exception to "no delete tool"
    (`sync_openapi` with `apply` + `prune`). `run_request` now points WebSocket requests at `run_websocket`. The echo
    server of tests/ws.rs moved to `tests/support/ws_echo.rs` (shared with tests/mcp.rs).
  - Secret manager settings (ef5ab83, docs bc1d19a): core `secret_managers::Settings` (`<config>/hitop/secret-managers.json`: vaultAddr,
    vaultNamespace, awsRegion, awsProfile, azureTenantId, azureClientId, opPath, azPath, awsPath; unknown keys kept; no
    file when empty) and keychain items `secret-managers/<NAME>` for VAULT_TOKEN, AZURE_CLIENT_SECRET,
    AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN. `Resolver::configured(store)` layers them under the
    process env through the resolver's own lookup (nothing is put into the process environment). Precedence: the
    environment wins a group at a time (`GROUPS`: VAULT_ADDR/BAO_ADDR, VAULT_TOKEN/BAO_TOKEN,
    VAULT_NAMESPACE/BAO_NAMESPACE, AWS_REGION/AWS_DEFAULT_REGION, AWS_PROFILE + the three AWS keys, the AZURE_* three,
    each HITOP_*_BIN), then the saved values, then hitop's own lookups (~/.vault-token, ~/.aws, `az`, `op`). The CLI has
    no per-manager flags, so "CLI flags/env > config file" is "env > file" there; `--no-secret-managers` still turns
    all off; `hitop secret check` notes the file. Desktop: Tauri `secret_manager_settings` (settings, which secrets
    are saved, which variables the app's env sets; never a value) and `secret_manager_settings_set` (text stores,
    null removes, left out stays); `secret_refs_resolve` uses the configured resolver. Web app: the Settings page
    (`platform/tauri-settings.js`) now shows in every desktop build with a "Secret managers" section; the usage ping
    part only where the build has it.
  - Tests: hitop-core 2 openapi (row merge both ways incl. a later spec removal, 1.0 record fallback) + 5
    secret_managers (layering and groups, keychain reads only where the env is silent, settings file round trip and
    error, save keeps secrets out of the file, a Vault ref with address from settings and token from the "keychain");
    compat case openapi-rows. CLI: tests/openapi.rs sync test extended (you add X-Mine, the spec changes
    X-Api-Version: dry-run output lists both rows, the file has both); tests/mcp.rs 2 new (run_websocket: saved request
    with messages + texts + count, binary, URL mode without auth, wait, server close 4001, refusals, connection error,
    real secret at the server and none on stdout; import/sync: 8 path refusals, file and URL import, preview writes
    nothing, apply keeps X-Mine and takes X-Api-Version 2, prune only with apply, up to date, global storage) and the
    tool list (12); tests/secret_managers.rs 1 (settings file supplies opPath and vaultAddr; env HITOP_OP_BIN and
    BAO_ADDR win; broken file named). Web: tests/openapi.cjs row lines in the preview and both header outcomes;
    tests/settings.cjs (Tauri: values to secret-managers.json trimmed, token only in the keychain and in no file,
    blank keeps, Remove deletes, env-set variables named; build without the ping: page with secret managers only,
    no question; web: no page). Full e2e green in both modes locally (web served by hitop-server on 127.0.0.1);
    clippy -D warnings clean (cli workspace, hitop-core --no-default-features, hitop-wasm host and wasm32, desktop in
    a Debian container with GTK/WebKit); swift test (HitopKit) green.
  - Deviations: the e2e assertion "build without the ping: no Settings page" became "Settings page with the secret
    managers only" (the section must exist in the Mac App Store build too). The MAS build's settings file and keychain
    items live in its sandbox container, so the CLI doesn't see what the MAS app saved (documented). No per-manager
    CLI flags were added.
  - Open: no long-lived MCP WebSocket session (connect/read/close across calls), by design for now; MCP gRPC tools
    (T15 question) still open.

#### 1.2 Same Content-Type on every surface (2026-11-02)
- In 1.1.0 the web app and its desktop build started adding a Content-Type to raw bodies and a scheme to URLs
  typed without one (UX review, 0e55b0b); the CLI, MCP and HitopKit didn't, so one collection sent different
  headers per surface. Owner approved making them the same in 1.2.0 (section 6 handoff of 2026-11-02, closed).
- **Notes (2026-11-02, done):**
  - One implementation: hitop-core `sending.rs` (pure, in hitop-wasm): `raw_content_type`, `add_content_type`
    (POST/PUT/PATCH, any case; no Content-Type header with a value; the body as sent, after variables and the
    pre-request script) and `with_scheme`. Called from `form::outgoing` (run, requests send, MCP, a form body a
    script replaced), `hitop send` (raw `-b`), `prepare_in` (scheme, both return points) and `snippet::build`
    (header on the filled body; scheme leniently: a `{{placeholder}}` host is left as written). The web app's
    `features.js` copies are gone: `dispatch`/`send` call hitop-wasm `rawContentType`/`withScheme`, the Headers note
    reads the module loaded at start (`store.format`), and the Code dialog no longer adds the header itself (core
    does). HitopKit `Sending.swift` follows the same rules (`FormBody.outgoing`, `Prepare`).
  - Exactness: the rules are the 1.1.0 regular expressions, so whitespace is JavaScript's (U+FEFF in, U+0085 out),
    `\w`/`\d` and `/i` are ASCII. Hand-written matchers in Rust and Swift (no Unicode regex classes or case
    folding). Shared vectors `spec/vectors/send.json` (25 detect, 10 apply, 39 scheme cases), all checked against
    the 1.1.0 JS first; tested by hitop-core `tests/send_vectors.rs`, `webapp/tests/unit.mjs` (through the wasm
    build) and HitopKit `SendRulesTests`. Before committing, 60,000 random strings from an adversarial alphabet were
    run through the 1.1.0 JS, Rust and Swift: no difference (throwaway test, not committed).
  - Scheme decision: core and CLI match the web app. A schemeless URL was a hard reqwest error in the CLI ("relative
    URL without a base"), so no working run changes; a collection with `base: api.example.com` now runs the same in
    every app, and only towards https (http only for loopback, as curl's local default). A typo like `users`
    still fails, now before sending and with the web app's message. A CLI user typing `hitop send api.x.dev/v1`
    gets https, which is what the web app does with the same text. WebSocket and gRPC targets are untouched.
  - Reports and history: report.json v1 and the JUnit shape carry no request headers (spec/run-report.md), and
    CLI `HistoryItem` has none, so nothing to record without a format change; `pm.request` in post-request scripts
    (CLI, MCP, HitopKit) sees the header as sent; the web app's collection-run history already stores the sent
    headers. The web app's single-send history keeps `tab.headers` on purpose: items reopen into tabs, so adding the
    header there would turn it into a saved header on reopen. Report/history `url` is the URL with its scheme.
  - Behaviour change for CLI users (CHANGELOG): a raw body that went out without Content-Type now has one; the
    golden `secrets-placeholders` snippets changed accordingly (curl's `-H 'Content-Type:'` became text/plain).
    `hitop send -b` without `-X` is still a GET and gets no header (the web rule); the method default wasn't changed.
  - Edge (documented, not special-cased): a snippet detects on the body as written in the snippet, so a body that
    is only `${secret}` shows `{{secret}}` → application/json while the real value decides when sending.
  - Tests: hitop-core 1 prepare test + 2 unit + 3 vector tests, 6 golden files; CLI `tests/sending.rs` (send:
    JSON/XML/own/GET, pm.request sees it, schemeless 127.0.0.1:port, `users` refused; run + requests send + snippet
    on a folder with a schemeless base: lowercase `post`, HTML, own header, script-replaced form body, nothing saved);
    `tests/mcp.rs` (run_request/run_collection send it, get_request shows the request as saved); webapp unit
    (vectors + Code snippet through wasm); e2e ux, ux12, diff-code, e2e, graphql and the rest green in Tauri mode
    locally; HitopKit swift test green (SendRulesTests incl. the runner on the mock server, CLICompatibilityTests
    with a raw XML PATCH so the CLI and HitopKit wire requests match with the header). Collection format v1 and
    spec/compat/v1 unchanged.
  - Not done here: version numbers stay 1.1.0 (no bump, no tag, no deploy: the release is the owner's/coordinator's
    step). The React desktop UI (`desktop/src`) and the frozen extensions aren't changed.

#### 1.2 Web app UX: deferred items (2026-11-02)
- The items the 1.1.0 UX review deferred (REVIEW.md "Status (1.2.0)" has the table and what is left).
- **Notes (2026-11-02, done):**
  - Large responses: IndexedDB `hitop-responses` (own DB, so the web adapter's `hitop` DB of folder handles keeps
    version 1), opened in each adapter's `init()` (`platform().responses`, null without IndexedDB or when it doesn't
    open in 3 s). Bodies over 64 KB go there by tab id with the raw body; `hitop_tabs` keeps the first 16 KB with
    `responseCut` + new flag `responseStored` (old readers see a body and the note). Restored after load only when
    the tab still holds that head. A refused write (quota) = one toast, then the 1.1 rule for the rest of the page.
    Migration: 1.1 cut tabs keep their note (nothing to recover); 1.1 inline bodies of 64–256 KB move on the next
    save; bodies of closed tabs are deleted. Tested with IndexedDB removed and with `put` throwing QuotaExceededError.
  - Cancel: per-tab AbortController (`store.inflight`). Web: a direct fetch is aborted; a proxied request can't be
    (frontend's `sendRequest` makes its own fetch and frontend/ is frozen), so web.js races it and drops the answer.
    Desktop (`desktop/src-tauri`, minimal): `SendArgs` gets optional `id` and `timeoutMs`; with an id the exchange
    runs on its own thread and `send_cancel {id}` answers the waiting command at once; the blocking reqwest client
    can't stop mid-exchange, so that thread finishes (bounded by the time limit) and its response and cookie-jar save
    are dropped. A cancelled request shows "Cancelled" and isn't in History. Time limit (Settings, `hitop_settings`)
    for Send, collection runs and schema fetches; Rust gets it as hitop-core's `http::Options.timeout`.
  - Settings: `views/settings-view.js` in both builds. The desktop's secret managers + usage ping became sections of
    it (`platform().settingsSections`), so check-no-telemetry still holds (element `hitop-app-settings`, no
    "telemetry"/"usage ping" text in shared code). Follow-redirects, TLS verification and a proxy toggle don't exist
    in hitop, so the page states the facts instead of switches. Theme via lit-material `setColorScheme` with the
    existing `hitop-theme` key; the top bar toggle and shortcuts button stay as quick access.
  - Import: "Add to my collections" default (ids in use renumbered, names get " (2)"), "Replace all" = 1.1 path with
    its confirm. features.cjs/ux.cjs, which replaced, now pick Replace all explicitly.
  - Panes: layout by the request view's width (ResizeObserver), not the viewport: columns ≥ 880 px, rows with a
    height handle ≥ 600 px, stack below. One per-viewer split and fold (`hitop_layout`), not per tab. A folded
    response stays folded when a new one arrives (its bar shows the status).
  - Mod+P: preventDefault always (also with a dialog open), so Print never opens from inside the app; the browser
    menu still prints. Mod+K unchanged.
  - Sidebar: folder groups from `file` in collection order; reordering only within a run of requests between
    folders; right-click menus open after pointerup (popover light dismiss closed them at once on Linux).
  - Tests: `tests/ux12.cjs` (new, in `test:e2e`, both modes), `tests/unit-ux.mjs` (new, in `test:unit`: import merge,
    folder tree, split math and keys, fuzzy match and ranking); ux.cjs, features.cjs, settings.cjs, e2e.cjs,
    diff-code.cjs adjusted; tauri-mock.cjs knows `send_cancel` and `timeoutMs`. Locally green in both modes: ux12, ux,
    e2e, diff-code, features, postman, settings, folders, openapi, editing, extras, globals, websocket, grpc, graphql
    (git and sync left to CI). The desktop crate can't build on the agent's machine (no GTK); CI compiles it.
  - Collection format v1, spec/compat and the React app's storage shapes unchanged (new keys and an optional flag only).

#### 1.2 Signed key wraps (2026-11-02)
- Close the top finding of the sync crypto (THREAT_MODEL, T24 open item): workspace key wraps were HPKE base mode, so a
  malicious sync server could hand a new member (or anyone, at a fake next epoch) a workspace key of its choosing and
  read what they then wrote. Owner-approved for 1.2.0.
- **Notes (2026-11-02, done; deploy, release and the external review are handoffs):**
  - Decision: **HPKE Auth mode**, not Ed25519 signatures (THREAT_MODEL "Why HPKE Auth mode"). The wrapping device's
    X25519 key, already pinned and fingerprinted since T24, authenticates the wrap; a separate signing key would need
    binding to the pinned encryption key, a second round of trust on first use and new fingerprints. Costs written
    down: only the recipient can verify, key-compromise impersonation (compromised devices out of scope), no signed
    rotation/member-list record. The status row says "signed" as asked; the wraps are authenticated, not signed.
  - `hitop-sync-crypto` 0.2.0 (MIT, crates.io API reworked): format 2 wraps `seal_wrap`/`open_wrap`/`wrap_header`
    (`0x02 || u32 head || u16 len || sender key id || enc || ct`; info `"hitop-sync/v2/wrap"` binds workspace, epoch,
    head = the sender's current epoch, sender and recipient key ids), `rotate(sender, ...)`, `Device {key_id, keys}`;
    format 1 `wrap_key`/`unwrap_key` kept (documented as legacy); `accept_wraps` (`keyring.rs`) = the one acceptance
    policy (pinned senders, inviter fingerprint pinning, strict mode, `key_commitment` per epoch, rollback via the
    max epoch and wrap heads, notices) with `WorkspaceMemory` (feature `serde`); `Error` gained UntrustedSender,
    Forged, KeyChanged, Rollback, LegacyRefused, MissingKey (`#[non_exhaustive]`). `vectors/v1.json` untouched; new
    `vectors/v2.json` (3 wraps, 10 failures, 2 commitments), cross-checked with `@hpke/core` 1.7.4 Auth mode + node
    crypto in a throwaway script. Crate tests 20 (+2 ignored generators).
  - Sync server (no migration): a wrap is 80 bytes (format 1) or format 2; shape checked, a format 2 sender must be one
    of the caller's device keys; a format 2 wrap replaces a format 1 wrap in `POST .../wraps` (upsert with
    `length(wrapped) = 80`, SQLite and Postgres), never the reverse (`stored` in the answer); `GET /v1/workspaces/:ws`
    lists `legacyEpochs` per key; invite accept ignores a `.fingerprint` suffix. Tests `tests/wraps_v2.rs` 3, Postgres
    teams flow +upgrade (run locally against postgres:17-alpine), everything else unchanged and green; the server still
    links no crypto.
  - CLI: every wrap is format 2; keys only through `accept_wraps` with `pins.json` and a new `keys.json`
    (workspace → WorkspaceMemory). `invite` prints `<token>.<fingerprint>` (link and, for mailed invites, a code to send
    yourself); `accept` parses it, makes the workspace strict and records the inviter; `init` makes it strict;
    `remove` unpins the removed member's devices; owners' `pull`/`push`/`members` upgrade format 1 wraps of pinned,
    non-runner devices; `trust` finds keys in every workspace when the folder isn't linked; runner credentials carry
    `pins` (creator + pinned owner/editor devices) and are strict, 1.1 credentials (no pins) keep legacy with a
    warning (their wraps are never upgraded). Refusals name the fingerprint and `hitop sync trust`, security ones
    start with SECURITY. tests/sync.rs: the team flow (code with fingerprint; a mailed code needs `trust`), plus
    `a_malicious_server_cant_hand_out_its_own_workspace_key` (edits the server's SQLite: unauthenticated wrap, forged in
    the owner's name, a device the server adds to the owner's account (also refused by the owner's `remove`), swapped
    key for a known epoch, hidden rotation, old wrap replayed as the new epoch, fake next epoch; nothing written or
    pushed) and `a_hitop_1_1_workspace_keeps_working_and_is_upgraded` (rewrites all wraps to format 1, drops memory and
    the inviter pin: warning, owner upgrade, re-sent known key, strict afterwards, 1.1 runner credential, trust for the
    next rotation). New dev-dependency rusqlite 0.41 (bundled) in the hitop crate.
  - Web app + desktop (`webapp/src/sync.js`, Sync page): same rules through hitop-wasm (`syncSealWrap`,
    `syncWrapHeader`, `syncAcceptWraps` returning `{ok, ...}`/`{ok: false, error: {kind}}`, `syncNormalizeFingerprint`,
    `syncCheckVectorsV2`; `syncWrap`/`syncUnwrap` → `syncWrapLegacy` for tests). localStorage `hitop_sync.memory`;
    `KeyError` (kind, fingerprint); the page shows key warnings, "Security warning:" errors, "Trust this device…"
    after comparing, the invite code with the fingerprint, legacy keys in Members. tests/sync.cjs (web and Tauri):
    invite code carries the fingerprint, the inviter is trusted from it, and a malicious server via `node:sqlite`
    (unauthenticated wrap, forged in the owner's name, a made-up device with its fingerprint and the Trust button);
    unit.mjs runs v2 vectors and accept_wraps cases in WASM. `node:sqlite` needs Node 22.13+ (CI: setup-node 22).
  - HitopKit (Swift) has no sync (no sync code in `ios/HitopKit`): not applicable, nothing to hand off.
  - Compatibility: hitop 1.1 clients can't open format 2 wraps, and a 1.1 server refuses them. Order: deploy the sync
    server first, then release CLI/desktop and deploy the web app; once a workspace is upgraded, every member needs
    1.2. Format v1 and spec/compat untouched (sync is outside format v1).
  - Docs: THREAT_MODEL (format 2, why Auth mode, "Accepting keys", upgrade of 1.1 workspaces, vectors, residual
    risks under "What a malicious server can still do", open item 2 done with the review focus), sync-server README
    (wire formats, `legacyEpochs`, `stored`, invite codes), team guide (invite codes with fingerprints, "Fingerprints
    and warnings", runner credentials, version note). CLAUDE.md facts.
  - Remaining limits (THREAT_MODEL): the first fingerprint comparison; members from 1.1 accept format 1 wraps until
    upgraded; a pinned (or removed but still pinned) member colluding with the server can hand out a key it knows;
    the member list is unauthenticated (a signed manifest would close the last two).

---

## 6. Handoffs and open questions

Agent appends here; owner ticks when done.

| Date | Tranche | What the owner needs to do | Done |
| --- | --- | --- | --- |
| 2026-11-01 | T2 | On your Mac (and a Windows or desktop Linux box if handy), check the keychain once: `hitop env add t2 && hitop secret set token --env t2` (macOS asks to allow access), then `hitop secret get token --env t2`, `hitop secret ls --env t2`, `hitop secret rm token --env t2`. The agent's machine and CI have no keychain, so only the fallback path is tested there. | |
| 2026-11-01 | T3 | Open question, no action needed now: masking replaces every occurrence of a secret value, so a very short value (one to three characters) also masks those characters elsewhere in logs and reports (`token=t` turns every `t` into `••••`). Should hitop warn about, or refuse to mask, secrets shorter than some length? Until decided it masks them (safe side). | |
| 2026-11-01 | T8 | Publish the action to the GitHub Marketplace: open https://github.com/bohdaq/hitop-action/releases/tag/v1.0.0, Edit, tick "Publish this Action to the GitHub Marketplace" (accept the agreement once), primary category **Testing**, secondary **Continuous integration**, then Update release. The name "Run hitop collection" (from `action.yml`) must be unique on the Marketplace; if taken, change `name:` in `action.yml` and retag v1. Listing text, if a field asks for it: "Run hitop API collections from your repository in GitHub Actions: pinned hitop image, JUnit artifact, job summary, secrets from repository secrets, and a failing check when a request fails." || done 2026-11-01: https://github.com/marketplace/actions/run-hitop-collection |
| 2026-11-01 | T5 | On your Mac, build the desktop with the web app: `cd webapp && npm ci && cd ../desktop && npm ci && npm run build:webapp` (needs `rustup target add wasm32-unknown-unknown` and `cargo install wasm-bindgen-cli --version 0.2.126 --locked`; for the MAS package use your usual signing, the overlay only changes the frontend). Check: (1) your existing collections, environments and history from the React app and the CLI show up; (2) send to a public host and to `http://localhost:<port>` (no CORS errors), a multipart file upload, a cookie set then sent back; (3) a post script with `assert.*` and `setContext` across a collection run; (4) Environments: add a secret, quit and reopen, the value is still there and `hitop secret ls --env <env>` shows it in the Keychain (macOS asks for access once); (5) Collections → Open folder on a `hitop init` folder, edit and save a request, check the YAML changed on disk (`git diff`); quit and reopen: the folder comes back (in the sandboxed MAS build it may ask you to open it again: tell me if so); (6) Export as zip and Import zip; Export dialog saves JSON; (7) the app runs on the oldest macOS you support (12, Safari 16.4 WebKit needed; if 12 without updates fails, say so). If all pass, say so and the React `desktop/src` gets deleted and `release-desktop.yml` switched to the overlay. | |
| 2026-11-01 | T23 | Put the SMTP password on the VM: `/etc/hitop-sync/env` exists (root, 0600) but has no `SMTP_URL` line, so email sign-in at https://sync.hitop.rws8.tech answers 503. Add `SMTP_URL=smtp://noreply%40rws8.tech:<password>@127.0.0.1:587` (or `smtp://noreply:<password>@...` if the account logs in without the domain; percent-encode special characters in the password), then `sudo systemctl restart hitop-sync`. `SMTP_TLS_SERVER_NAME=mail.rws8.tech` is already there (STARTTLS on 127.0.0.1:587 verifies as mail.rws8.tech). Test: `curl --resolve sync.hitop.rws8.tech:443:127.0.0.1 -XPOST https://sync.hitop.rws8.tech/v1/auth/email -d '{"email":"<you>"}'` and `journalctl -u hitop-sync` shows "sign-in mail sent". || done 2026-11-01: owner added SMTP_URL; test sign-in mail to noreply@rws8.tech delivered (HTTP 202, postfix local delivery) |
| 2026-11-01 | T10 | Nothing to do now; for the MAS build after the T5 switch: build it as today (plain `tauri build`, no `--features telemetry`), so it has no usage ping and the App Store "Data Not Collected" label stays true. CI checks that configuration on every push. | |
| 2026-11-01 | T23 | Before any public launch of hitop sync: get an external review of the encryption design (`sync-server/THREAT_MODEL.md`) and code (`sync-server/crates/hitop-sync-crypto`, plus the T24 clients). Top item for the reviewer: the server hands out members' public keys, so key substitution is the open risk until clients pin fingerprints. | |
| 2026-11-01 | T24 | Before any public launch of hitop sync (replaces nothing above, adds to it): the external review now covers the clients too: `cli/crates/hitop-sync-crypto`, `cli/crates/hitop-core/src/sync.rs`, `cli/crates/hitop/src/commands/sync/`, `webapp/src/sync.js` and `sync-server/THREAT_MODEL.md` (sections "Clients", "Teams"). Top items: unauthenticated key wraps (a server can hand a new member a workspace key of its choice; fix = HPKE Auth mode or signed wraps) and the first fingerprint comparison (TOFU is implemented). Also decide whether the web app may hold device keys in localStorage for real workspaces. | |
| 2026-11-01 | T24 | Next CLI release: release-cli.yml now publishes `hitop-sync-crypto` 0.1.0 to crates.io first (a new crate under your token; it skips the step when that version exists). `hitop schedule --github --sync` workflows need an image with `hitop sync` (that release, `--hitop-version`). On your Mac with the T5 check: Sync page in the desktop build: sign in, share a collection, see the device key in Keychain (`security find-generic-password -s hitop -a hitop-sync/sync.hitop.rws8.tech`), sync with `hitop sync link` in a terminal; then share a collection opened with Open folder and check that a pull rewrites the YAML on disk and Git shows it. Open question: should the server ever run monitors itself (T24 shipped runners only, see the T24 notes)? | |
| 2026-11-01 | T12 | Optional, when you want it listed: submit `hitop mcp` to MCP directories (for example the official MCP Registry, mcp.so, Smithery, PulseMCP, Glama). Text: **Name** hitop. **Short description** "Run and edit your HTTP API collections from your AI tools: list, read, run, create and update requests in a hitop collection folder, with secrets masked." **Long description** "hitop mcp is a local stdio MCP server for hitop, the open source (MIT) HTTP API client. Point it at a collection folder in your repository (`hitop mcp ./my-api`) or at hitop's global storage. Tools: list_collections, list_requests, get_request, run_request, run_collection (returns the hitop run report), create_request, update_request, list_environments, set_environment, run_websocket (one bounded WebSocket exchange), import_openapi, sync_openapi. Requests run like `hitop run`: the same variables, scripts, assertions and cookie jar. Secret values come from the OS keychain or HITOP_SECRET_* env vars and are shown as •••• in every result; there is no delete tool (sync_openapi removes requests of dropped operations only with apply and prune)." (Tool list updated for 1.1.) **Install** `cargo install hitop` or a binary from https://github.com/bohdaq/hitop/releases. **Config** `{"mcpServers": {"hitop": {"command": "hitop", "args": ["mcp", "./my-api"]}}}`. **Docs** https://hitop.rws8.tech/terms/use-hitop-with-claude-code-and-cursor.html. **License** MIT. **Categories** developer tools, API testing. | |
| 2026-11-01 | T11 | Record the terminal GIF (vhs isn't installed on the agent's machine): install vhs (https://github.com/charmbracelet/vhs, needs ttyd and ffmpeg) and hitop 0.3.0 on PATH, then from the repository root `vhs site/demo/demo.tape`; it writes `site/demo/demo.gif` using the local launch-gate mock (nothing leaves the machine). Commit the GIF, or send it and the agent wires it into the README, home page and launch posts. | |
| 2026-11-01 | T11 | Mac App Store update at 0.2.0: on your Mac, from tag `desktop-v0.2.0`, build as today with plain `tauri build` (no `--features telemetry`, so the "Data Not Collected" label stays true), sign and upload with Transporter. tauri.conf.json now says 0.2.0. Whether this build uses the React UI or the web app overlay depends on the T5 check above. | |
| 2026-11-01 | T11 | Post the launch when you're ready (agents never post): drafts in `launch/` (Show HN, r/rust, r/webdev, This Week in Rust as a PR, Product Hunt), with where and when in `launch/README.md`. Best after the site deploy and the GIF. | |
| 2026-11-01 | T11 | Optional: the hitop-action Marketplace listing still shows v1.0.0 as newest; open https://github.com/bohdaq/hitop-action/releases/tag/v1.0.1, Edit, tick "Publish this Action to the GitHub Marketplace", Update release. `v1` already points at v1.0.1. | |
| 2026-11-01 | T14 | Nothing to do now. Open question for later: should `hitop mcp` get WebSocket tools (for example connect, send, read the next messages, close)? T14 left them out as scoped; MCP `run_request` refuses WebSocket requests and points to `hitop ws`. On your Mac, when you do the T5 check, a WebSocket request in the desktop app sends its headers and auth (the browser build can't). | answered 2026-11-01: yes; MCP `run_websocket` (75d5a1a), 1.1 follow-ups |
| 2026-11-01 | T15 | Nothing to do now. Open questions for later: should `hitop mcp` get gRPC tools (list/describe/call, like the WebSocket question under T14), and should hitop support client-streaming and bidirectional gRPC calls and `--insecure` TLS? When you do the T5 check on your Mac, a gRPC request (method "gRPC") against a local server with reflection shows the service pickers and streams responses. | |
| 2026-11-01 | T17 | Nothing to do now. Open question for later: should sync also merge header rows one by one (today a request whose headers you edited keeps all of them when the spec changes its headers), and should `hitop mcp` get an import or sync tool for OpenAPI? | answered 2026-11-01: yes to both; row-by-row sync (cf400c6), MCP `import_openapi`/`sync_openapi` (75d5a1a), 1.1 follow-ups |
| 2026-11-01 | T22 | Nothing to do now. `--reporter html` and the html artifact in `hitop schedule --github` workflows reach users with the next CLI release (0.3.0 images don't have it); the guides already say "releases after 0.3.0". The guide changes go live with the next site deploy. | |
| 2026-11-01 | T20 | When you do the T5 macOS check (web app overlay build), try the Git panel: open a collection folder that is a git clone with an HTTPS remote (for example a private GitHub repo), make sure it is in a clean state, then: (1) Collections → the folder → Git (or the sidebar's Git); the branch, upstream and ↑/↓ show; (2) edit a request, see it under Changes with its diff, Stage, write a message, Commit (author = your git config; in the sandboxed MAS build the app may not see `~/.gitconfig`: it then asks for a name and email and keeps them in the repo's `.git/config`); (3) Push: in the MAS build it fails without a token and says so (outside the sandbox git's credential helper, for example osxkeychain, may answer); paste a fine-grained token (contents read/write) under Remote access, Push again (macOS may ask for Keychain access once; `security find-generic-password -s hitop -a git/github.com/token` shows it); (4) make a commit on GitHub's web editor to the same line, Pull: a conflict appears; Take theirs or edit the text, Commit merge, Push; (5) create a branch, switch back, delete it after merging. Outside the MAS build, an SSH remote should work through your ssh-agent (the host must be in `~/.ssh/known_hosts`). Tell me what fails, especially anything the sandbox blocks. | |
| 2026-11-01 | T21 | Optional, with accounts you have: put one reference per manager you use into a test folder's environment (`token: ${secret:vault:secret/x#token}`, `${secret:aws:name#key}`, `${secret:azure:<vault>/<name>}`, `${secret:op:<vault>/<item>/<field>}`), sign in as usual (`vault login`, `aws sso login`, `az login`, `op signin`) and run `hitop secret check -c <folder>`: each should show ✓ (no value printed); tell me any that fails and its message. Agents had only Vault dev mode, LocalStack, stubs and mocks. On the Mac with the T5 check: does the desktop app opened from the Dock resolve a Vault reference (it doesn't see shell env vars like VAULT_ADDR; `~/.vault-token` alone isn't enough without the address), and does the MAS build run `op`/`az` (expected: sandbox blocks them)? Open question: should the desktop get a Settings field for VAULT_ADDR and friends? | Settings question answered 2026-11-01: yes; desktop Settings "Secret managers" + CLI reads the same file (ef5ab83), 1.1 follow-ups; the real-account check stays open |
| 2026-11-01 | T19 | Mac App Store 1.0: on your Mac, from tag `desktop-v1.0.0`, build as before with plain `tauri build` (no `--features telemetry`), sign with your MAS identity and `Entitlements.mas.plist` (it now also has `com.apple.security.files.bookmarks.app-scope`), upload with Transporter. If the T5 check passed, build the web app overlay (`npm run build:webapp`) so the folders, WebSocket, gRPC and Git features are in it. | |
| 2026-11-01 | T19 | Check the security-scoped bookmarks in the sandboxed MAS (overlay) build: Collections → Open folder on a `hitop init` folder, edit a request, quit, reopen: the folder comes back without asking and saves still work (also a Git panel action on it). Then rename or move the folder in Finder and relaunch: it should still open (stale bookmark renewed) or ask to open it again, never crash. `~/Library/Containers/tech.rws8.hitop/Data/Library/Application Support/tech.rws8.hitop/folder-bookmarks.json` should list the folder; closing the collection removes it. Tell me what the Console shows for `hitop:` lines if it fails. | |
| 2026-11-01 | T19 | Optional: the App Store price change for 1.0 ($9–19 was the plan; today Mac $4.99, iOS $2.99), in App Store Connect, if you want it. Agents don't change prices. | |
| 2026-11-01 | T19 | Deploy the site (`deploy/deploy.sh`): the comparison pages, README-linked guides and the home page now describe 1.0 (protocols, OpenAPI, snippets, secret managers, HTML reports, schedule) and pin 1.0.0. Not deployed by this tranche on request. Optional: the hitop-action Marketplace listing still shows v1.0.0/v1.0.1; nothing changed in the action. | |
| 2026-11-01 | Brand | **App Store Connect, Mac app** (id6785294166, now "HiTop API Client"): with the next version, set App Name **HITOP** (if Apple says the name is taken, "HITOP API Client"), Subtitle (≤ 30) **API client for terminal and CI** (30 characters; your call), Promotional Text "The API client your terminal, your CI and your AI agent can all drive." The icon comes from the build (`desktop/src-tauri/icons/icon.icns`, already the purple mark); the next MAS build also carries the window title "HITOP". **iOS app** (id6785326628, now "HiTop HTTP Client"): App Name **HITOP** (fallback "HITOP HTTP Client"), same subtitle and promotional text; the home-screen name "HITOP" (`ios/HiTop` Info.plist) reaches devices with the next build you upload; icon unchanged (already purple). Name changes only apply when a new version is submitted. Afterwards tell me the final listing names and I update CLAUDE.md and the site links if the store URLs change. | |
| 2026-11-01 | Brand | **Chrome Web Store** (item mmbpagencfjfgeicmeobbmeocdpeokdk): the listing name and summary come from the manifest: now "HITOP" and "The API client your terminal, your CI and your AI agent can all drive." The store rejects an unchanged version, so set `"version"` in `chrome-store-package/manifest.json` to 4.0.1, then `cd chrome-store-package && zip -r ../hitop-chrome-store.zip . -x '*.DS_Store' README.txt PRIVACY.md` and upload it (Package → Upload new package). Store icon: `brand/png/logo-128.png` (= `chrome-store-package/icons/icon-128.png`). Listing description: start it with the tagline. **Firefox AMO** (addons.mozilla.org/addon/hitop): set `"version"` in `extension/manifest.json` to 4.0.1, run `./build-extension.sh` (builds frontend/ and writes `hitop-firefox-extension.zip`), upload as a new version; listing Name "HITOP", Summary = the tagline, icon `brand/png/logo-128.png`. **Edge Add-ons**: the repository links no Edge listing; if one exists, upload the same Chrome zip, Name "HITOP", short description = the tagline, store logo `brand/png/logo-300.png`. Tell me the versions you uploaded so the manifests in the repo match. | |
| 2026-11-01 | Brand | **GitHub repo social preview** (bohdaq/hitop → Settings → General → Social preview): upload `site/public-assets/social-preview.png` (1280 × 640, purple mark and "HITOP") if it isn't there already. Optional: repository About description = the tagline. **hitop-action Marketplace**: main now says "Run HITOP collection" with a purple badge (hitop-action 8931565), untagged. To show it on the Marketplace: tag `v1.0.2` on hitop-action main and move `v1` (or let an agent do it), then open the release, Edit, tick "Publish this Action to the GitHub Marketplace", Update release. The listing URL `.../marketplace/actions/run-hitop-collection` should stay (same name, other case); if GitHub asks for a new name, keep "Run HITOP collection". | |
| 2026-11-01 | Brand | iOS companion naming (with the T26 build): it is called **HITOP Runs** (app and widget) because its bundle id makes it a second App Store app. If you ship it as the next version of the iOS app instead (T26 option), also set `CFBundleDisplayName` to `HITOP` in `ios/HitopCompanion/HitopCompanion/Info.plist`. For a new listing: App Name "HITOP Runs", subtitle "Run your API collections" (24). Shortcuts now say "Run HITOP collection" and "Get last HITOP run status". | |
| 2026-11-01 | T26 | Build and ship the iOS companion (agents can't build for Apple platforms). On a Mac with Xcode 16+: (1) `cd ios/HitopKit && swift test` (42 tests; JavaScriptCore runs the scripts there; `HITOP_BIN=<path to hitop> swift test` adds the CLI comparison, `HITOP_TEST_KEYCHAIN=1` also writes and deletes one real keychain item). (2) `brew install xcodegen && cd ios/HitopCompanion && xcodegen && open HitopCompanion.xcodeproj`. (3) Decide: a new App Store app (keep bundle ids `hitop.rws8.tech.HitopCompanion` and `hitop.rws8.tech.HitopCompanion.Widget`, create them and the app group `group.hitop.rws8.tech.companion` in the developer portal, enable App Groups on both ids) or the next version of "HiTop HTTP Client" (then set `PRODUCT_BUNDLE_IDENTIFIER` to `hitop.rws8.tech.HiTop` and the widget to `hitop.rws8.tech.HiTop.Widget` in `project.yml`, and keep `ios/HiTop` only as history). Team is `XD7JHZ5DB3` as in the HiTop project. If you change the app group, change it in both `.entitlements` files and `Shared/Library.swift` (`AppGroup.id`). (4) On a device: open a collection folder from iCloud Drive (for example a `hitop init` folder or `cli/fixtures/sample` with a `local` environment pointing at a server you can reach), run one request and the collection, switch environment, set a secret in Secrets… and run `prod`, pin it (swipe) and add the "hitop runs" widget, run "Run hitop collection" and "Get last hitop run status" in Shortcuts. Check that a folder still opens after a relaunch and after the folder is renamed in Files. (5) App Review: the app has `NSAllowsArbitraryLoads` (as HiTop does; an API client calls hosts the user names) and a local network usage string; privacy label "Data Not Collected" holds (no telemetry, no network calls but the user's requests). Both targets carry a `PrivacyInfo.xcprivacy` (no tracking, no collected data; required-reason APIs: app-group UserDefaults `1C8F.1`, file metadata in the user-picked folder `3B52.1`): check the reasons against Apple's current list when you archive. The icon is a copy of HiTop's. Tell me what fails to compile: the app sources were written without an Apple SDK. | |
| 2026-11-01 | T25 | Decide whether the hosted test server (sync.hitop.rws8.tech) gets SSO. Agent's recommendation: leave it off. It serves individuals with sign-in mails and passkeys; SSO is for organizations running their own server with their own provider, and turning it on here means registering a client at one provider (for example Google with `HITOP_SYNC_OIDC_ALLOWED_DOMAINS` set to your domain) and keeping its secret on the VM. If yes: register a web client with redirect URI `https://sync.hitop.rws8.tech/v1/auth/sso/callback`, add `HITOP_SYNC_OIDC_ISSUER`, `HITOP_SYNC_OIDC_CLIENT_ID`, `HITOP_SYNC_OIDC_CLIENT_SECRET` (and the domains) to `/etc/hitop-sync/env` (root, 0600), `sudo systemctl restart hitop-sync`, and test with `hitop sync login --sso`. Leave `HITOP_SYNC_SSO_REQUIRED` off on a shared test server. | |
| 2026-11-01 | T25 | Before any public launch (adds to the review above): the reviewer also covers `sync-server/src/oidc.rs` (token checks, the code handoff, account linking) and `sync-server/src/audit.rs` (chain, purge), and THREAT_MODEL sections "Single sign-on" and "Audit log". Open: audit heads anchored outside the server (THREAT_MODEL item 7). | |
| 2026-11-02 | UX | Eye-check the review fixes on the web app and desktop build: top bar, sidebar fold/filter, tab strip (tabs now at most 16rem wide, so one tab no longer spans the strip), theme toggle now cycles light/dark/system. (The Content-Type question of this row was answered "port it" by the owner and is done in 1.2, row below.) | |
| 2026-11-02 | 1.2 | Nothing blocking. When releasing 1.2.0: bump the versions as usual (still 1.1.0) and keep the CHANGELOG's behaviour-change note in the release text (CLI raw bodies now get a Content-Type; schemeless URLs get https:// instead of failing). Optional check: `hitop snippet` and the web app's Code dialog for one raw JSON POST should show the same `Content-Type: application/json` line. HitopKit's rules run on Linux in CI; on a Mac the companion uses the same code (`Sending.swift`), nothing extra to do. | |
| 2026-11-02 | 1.2 UX | Eye-check the 1.2 web app items in a browser and in the desktop build (macOS WKWebView; Windows WebView2 if handy): (1) the drag handle between request and response: feel, cursor, keyboard (Tab to it, arrows, Enter resets), the stacked layout in a narrow window, none on a phone; (2) hide/show the response; (3) Cmd+P / Ctrl+P opens quick open and never the print dialog, Cmd+K still the URL field; (4) Cancel on a slow request in the desktop app (`https://httpbin.org/delay/10`): stops at once; (5) a multi-MB response, then reload/relaunch: the whole body is back (IndexedDB in WKWebView), in a private browser window the 1.1 note instead; (6) Settings: theme, time limit, Clear stored responses; on the desktop the secret managers and (release build) usage ping are sections of the same page; (7) Import with "Add to my collections" next to existing ones; (8) folder groups after a Postman import, right-click menu, the × on tabs, the History page. Screenshots: `reviews/2026-11-01-webapp-ux/shots/after12-*.png` (local). | |
| 2026-11-02 | 1.2 signed key wraps | **Deploy the sync server before releasing 1.2 clients** (a 1.1 server refuses format 2 wraps with 400 "wrapped key ... must be 80 bytes"): build `sync-server` from main and install it as before (`deploy/install.sh`); no migration, no env change; existing workspaces keep working. Then release CLI/desktop 1.2.0 (hitop-sync-crypto **0.2.0** is published first by release-cli.yml: it is a new version under your token) and deploy the web app. Check on the test server: (1) an existing 1.1 workspace syncs from the 1.2 CLI with a "handed out without authentication" warning; as its owner, `hitop sync pull` prints "Upgraded N workspace key wrap(s)" and a second pull is quiet; (2) a member then sees "re-sent the workspace key you already had" and, after the next rotation, must `hitop sync trust '<owner fingerprint>'`; (3) a fresh invite: `hitop sync invite --role editor -c <dir>` prints a link ending in `.<your fingerprint>`; the invitee accepts it and `hitop sync link` works with no trust step; (4) a 1.1 runner credential still pulls with a warning; make a new one with `hitop sync runner create`; workflows that use 1.2 credentials need image >= 1.2.0. Tell members of shared workspaces to update: hitop 1.1 can't open the new wraps. | |
| 2026-11-02 | 1.2 signed key wraps | External review (replaces nothing above, adds to it): workspace key wraps are now authenticated (HPKE Auth mode with the pinned device key) and the server-substituted-key finding is closed. Point the reviewer at `cli/crates/hitop-sync-crypto/src/keyring.rs` (`accept_wraps`) and its two callers (`cli/crates/hitop/src/commands/sync/client.rs` `refresh_keys`, `webapp/src/sync.js` `workspaceKeys`), the format 2 `info`/wire layout and `vectors/v2.json`, the legacy path (who still accepts format 1 wraps; the owner's upgrade, `upgrade_wraps`/`upgradeWraps`), runner credential pins, invite-code fingerprints, and the residual risks in THREAT_MODEL "What a malicious server can still do" (first comparison, 1.1 members until upgraded, a pinned or removed-but-pinned member colluding with the server, the unauthenticated member list). Open question for you: should removing a member also tell the other members' clients to unpin that member's devices (needs a signed member list, i.e. the signed manifest of THREAT_MODEL item 3)? | |

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
