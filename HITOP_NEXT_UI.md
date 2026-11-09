# HITOP Next: UI redesign (PRD)

Status: in progress since 2026-11-09 (owner: "start"). See "Progress" below the plan.
Concept canvas (private, open it from the owner's account): https://claude.ai/artifact/Y5TViNJUgGVkoHnx34Vcwg

## Why

The web app, desktop and extension pages work but look like a Material form app: rounded pills, a
light purple palette, every feature its own dialog. Our users are developers who live in editors and
terminals (Linear, Raycast, VS Code, Warp set their expectations). HITOP Next keeps every feature and
the brand mark, and changes the look and the interaction model to a keyboard-first, dense, dark-first
tool for working with APIs.

Goal: someone who opens the app for the first time should think "this is a serious engineering tool",
and someone who uses it all day should never need the mouse.

Not goals: new protocols, new cloud features, a new framework. The app stays Lit + lit-material on
the same store, platform layer and engines.

## The concept, screen by screen

The canvas has four artboards. Each one is a target, not a pixel spec.

### 1. Workspace

- **Title bar**: mark + `HITOP / <collection>` breadcrumb; a central command field ("Search requests,
  run a command, ask the agent…  ⌘K"); environment chip with a coloured dot; git branch chip with
  ahead/behind; account avatar.
- **Activity rail** (56 px, icons only, with tooltips): Collections, History, Environments, Runs,
  Mocks, Monitors, Agent, Settings. Replaces today's sidebar footer and the menus.
- **Collection tree**: filter, folders, a method tag in mono, a git letter (`M`, `A`, `?`) per
  changed file, the open request marked with an accent bar. Footer: "Files in your repo · no cloud",
  the path, the number of requests and of changed files.
- **Request tabs**: method + name, an unsaved dot, a type tag for WS / GQL / SSE / gRPC.
- **URL bar**: 44 px, method picker, URL in mono with `{{variables}}` highlighted; hovering a variable
  shows its value, where it comes from (environment, collection, global) and an Edit link. Primary
  Send button with its shortcut (`⌘↵`), and a menu for more send options.
- **Split request | response**, resizable, side by side on wide screens and stacked below 1100 px.
  - Request tabs: Params, Headers, Body, Auth, Scripts, Tests, with counts. Body has a JSON / Form /
    Binary switch and shows schema validation against the OpenAPI component when one is linked.
  - Response header line: status pill, time, size, `n/n checks`.
  - **Timing waterfall** (DNS, Connect, TLS, Waiting, Download) above the body.
  - Response tabs: Body (folded JSON), Headers, Checks, Diff vs last, Raw. Checks also listed under
    the body with their expression.
- **Status bar**: file path, unsaved state ("⌘S writes the file"), proxy, TLS/HTTP version, engine,
  `?` for shortcuts.

### 2. Collection run

A run report page: status badge, title (collection · environment), branch, commit, message; actions
(Re-run failed, download JUnit/JSON, Open failing request). Four stat tiles (requests, checks, p95,
wall time, with a delta against main). A timeline table: one row per request with method, name,
code, time and a bar on a shared time axis; filters All / Failed / Slow. A failure card: expected vs
actual, a likely cause from the agent with "Apply fix", and the exact `hitop run` command that
reproduces the run.

### 3. Command palette and agent

- **⌘K palette**: one field over requests, actions, environments and runs, with sections and
  prefixes (`>` commands, `@` environments, `#` runs) and Tab to hand the query to the agent.
- **Agent panel** (right side sheet): it sees the open request and its last response. Changes arrive
  as a diff of the request YAML, are dry-run against the last response, then Apply / Run with
  checks / Discard. Secrets stay masked; the same tools are available to other agents over MCP.

### 4. Foundations

- Colour: graphite surfaces `#0A0A0C` (ground), `#0D0D10` (panel), `#111114` (raised), `#1D1D22`
  (hover), `#1F1F24` (border); text `#EDEDF0` / muted `#8A8A94`; one accent `#A99CFF` (from the mark's
  violet). Semantic: pass `#7FD8B0`, fail `#FF9C9C`, warning and variables `#F2B45C`, info `#6CA8FF`.
  Methods: GET green, POST amber, PUT blue, PATCH violet, DELETE red.
- Type: Geist for UI, Geist Mono for every number, URL, method and code (tabular figures).
- Shape: radius 6 / 9 / 12 / 16, 4-pt spacing, borders instead of shadows, shadows only on floating
  layers (palette, popovers, side sheets). Motion 120 ms ease-out, none with reduced motion.
- A light theme mirrors every token; it is not drawn yet.
- Principles: keyboard first; the file is the truth; show the numbers; CLI parity; the agent
  proposes, you apply; quiet by default.

## What exists today, and the gap

Researched in this repo on 2026-11-09 (webapp ≈ 11.5k lines of JS, `app.css` 633 lines; tokens in
`site/src/theme.css`, shared with the site).

| Concept element | Today | Gap |
| --- | --- | --- |
| Theme, tokens | `site/src/theme.css`: M3 roles via `light-dark()`, Geist already loaded, light + dark | Add the graphite palette and semantic/method tokens as a HITOP theme layer for the app; decide whether the site follows (below) |
| Layout | `hitop-app.js` render: top bar + 280 px sidebar + main; no rail | New shell: rail, collection pane, tabs, status bar. lit-material has `sidebar`, `split-panel`, `toolbar`, `tooltip`, `tree` |
| Command palette | Quick open on Mod+P (`quick-open.js`, fuzzy match, unit tested); Mod+K focuses the URL | Grow quick open into the palette: action registry, sections, prefixes, Tab to the agent. Move "focus URL" to another key (or `/`) |
| Variable highlighting and peek | Variables are resolved, not shown inline | CodeMirror decoration on the URL and the body (CodeMirror is already a dependency) + a `popover` |
| Request/response side by side | Stacked | `split-panel`, remembered per window; stacked under 1100 px |
| Status, time, size | `timeMs` from `web.js` (browser fetch) and `server/src/proxy.rs` | Size is easy. See timing |
| **Timing waterfall** | Only total time. Browser `fetch` cannot see DNS/TLS for cross-origin calls; the proxy (`reqwest` blocking) and hitop-core (`reqwest`) don't expose phases | Real engineering: measure phases in Rust (custom connector/resolver around hyper, or `curl`-style timers) in hitop-core, return `timings` from the proxy, the Tauri engine and the CLI JSON report. Show the bar only when phases are known; the browser-direct mode shows total time only |
| Checks list | Tests run, results shown | Restyle; show the expression next to each check |
| Diff vs last response | Compare responses dialog (`tools-dialogs.js`, `diff.js`): current / examples / history snapshots | Move it into a response tab with "last response" picked by default |
| Git letters in the tree, branch chip | Desktop: `tauri-git.js` panel (status, diff, stage). Web: GitHub sync status only | Desktop: feed status into the tree and the title bar. Web: hide or show sync state instead |
| Run report page | `collection-views.js` `runResults`: a results list | New page from the same results + `spec/run-report.md` data; `stat`, `table`, `chart` packages. p95 and "vs main" need stored runs (Cloud run history exists; local runs need a small history) |
| Agent panel with YAML diff and dry run | "Ask AI" dialog (`ai-view.js`): tests, explain, request from text; own key, secrets masked | Move to a side sheet; return edits as a structured patch of the request, render with `diff`, dry-run checks against the stored response before Apply |
| "Likely cause / Apply fix" on failures | Explain exists | Pass run context (order, previous responses) to Explain; fix = the same patch flow |
| CLI command for a run | CLI flags known | Build the `hitop run …` line from the run's settings; copy button |
| Activity rail entries | History, environments, mocks, monitors, settings exist as views/dialogs | Turn dialogs into rail views; keep the dialogs for small edits |
| Mobile / narrow | Drawer under a breakpoint | Rail becomes a bottom bar or the drawer; panes stack |

### Where the redesign lands

Only the Lit web app (`webapp/`) gets this design. The desktop releases still ship the React UI
(`desktop/src`) until the owner's macOS check switches them to the web app (ROADMAP T5,
`tauri.webapp.conf.json`). The browser extensions run the React `frontend/` app. So:

- Doing T5 first, or alongside phase 2, gets the new look onto desktop instead of only the web.
- The extensions keep the old look unless they move to the web app too, a separate decision.
- `site/src/theme.css` is shared with the site. Add the app's tokens as an app-only layer
  (`webapp/src/app.css` or a new `webapp/src/theme-next.css`) so the site doesn't change by accident.

## First draft of the plan (superseded by "Plan, as decided" below)

Each phase ships on its own and keeps every e2e suite in `webapp/tests` green (they select by role
and label, so markup can change; update selectors where layout moves).

1. **Tokens and theme** (small). HITOP dark/light tokens on top of `theme.css`; method and semantic
   colours; mono for numbers. Dark by default in the app, the theme switch stays.
2. **Shell** (medium). Rail, collection pane with the new tree rows, request tabs, status bar,
   title-bar command field. Pure layout over the existing views.
3. **Request and response** (medium). URL bar, variable highlight + peek, side-by-side split,
   response header line, Checks and Diff tabs.
4. **Command palette** (medium). Action registry (every menu item and shortcut registers once),
   palette UI on Mod+K, prefixes, quick open kept as the request section. Shortcut list generated
   from the same registry.
5. **Run report** (medium). New page, timeline, stat tiles, CLI command; local run history in the
   platform store for p95 deltas.
6. **Agent side sheet** (medium–large). Structured patch output, diff view, dry run, Apply; failure
   explanations with run context.
7. **Timing phases** (large, Rust). hitop-core measures DNS/connect/TLS/TTFB/download; proxy, Tauri
   and CLI JSON report carry `timings`; spec update in `spec/run-report.md`; waterfall in the app.
8. **Desktop git in the tree** (small, desktop only).
9. **Polish**: light theme pass, reduced motion, phone layout, screenshots for stores and site.

Phases 1–4 give the new look; 5–8 add the parts of the concept that need new data.

## Decisions (owner, 2026-11-09)

- **Site too**: hitop.rws8.tech adopts the graphite palette. The tokens therefore go into
  `site/src/theme.css` (shared), not an app-only layer; the site gets its own restyle phase.
- **Dark by default** in the app; the theme switch stays and light follows.
- **Mod+K opens the palette.** URL focus moves to Mod+L; the shortcut list and `tests/ux*.cjs`
  change with it.
- **Timing phases early**: the Rust work runs in parallel with phases 1–4, not after.
- **New components in lit-material** (command palette, timing waterfall, method tag, and any other
  generic piece), released from the lit-material repo, then used by HITOP.
- **Desktop T5 with the shell phase**: desktop ships the Lit web app; release after the owner's
  macOS check.
- **Extensions move to the web app**: the extension pages run the Lit app instead of `frontend/`
  React.

These change "Where the redesign lands" above: the site, the desktop and the extensions all get it.

## Plan, as decided

Two tracks run at once.

**Track A — UI**
1. Tokens in `site/src/theme.css` (graphite, semantic, method colours, mono numbers), dark default
   in the app.
2. lit-material: `command-palette`, `timing-waterfall`, `method-tag` packages (+ docs on
   lit.rws8.tech); release.
3. Shell (rail, tree with git letters on desktop, tabs, status bar, title bar) + **desktop T5 switch**.
4. Request/response (URL bar, variable peek, split, checks, diff tab).
5. Palette on Mod+K (action registry; Mod+L = URL).
6. Run report page.
7. Agent side sheet (patch, diff, dry run).
7b. **Organization area in the app** (S3–S8, S13, S15–S20): members, plan & billing (Stripe checkout/portal; desktop via system browser + deep link), tokens & MCP, runs & monitors, security & audit; unified conflict view (S14); limit dialogs (S18). The portal redirects into the app, then retires.
8. **Extensions on the web app** (Firefox + Chrome builds from `webapp/`; storage keys already
   shared with React; extension CSP and the proxy-less send path to check).
9. Site restyle with the same tokens; light theme pass; store screenshots.

**Track B — timing (Rust), starting now**
1. hitop-core: measure DNS / connect / TLS / first byte / download per request.
2. `timings` in hitop-server's proxy answer, the desktop `send_request`, and report.json
   (additive field, `spec/run-report.md` + compat case).
3. The app shows the waterfall when `timings` exist; browser-direct sends show total time only.

### Progress

| Step | Status | Commit |
| --- | --- | --- |
| A1 tokens + dark default | done: graphite dark roles + `--hitop-method-*`, `--hitop-pass/fail/warn/info`, `--hitop-phase-*` in `site/src/theme.css` (site dark mode changes on its next deploy); app dark when nothing saved, "Match the system" kept under `hitop-theme-system` (`webapp/src/theme.js`). Not deployed | e3b9a1e |
| B1 hitop-core timing phases | done: `timing.rs` (timed resolver + connector layer on the default client), `Timings {dnsMs, connectMs, waitMs, downloadMs, totalMs, reused}` on `http::Response` and `RequestResult`. Connect = TCP + TLS (reqwest can't split them). Not yet in report.json, proxy or desktop (B2) | 963451b |
| B2 timings everywhere | done: report.json request `timings` (optional key, `spec/run-report.md` "Timings"; HitopKit's CLI cross-check ignores it until HitopKit measures with URLSessionTaskMetrics), hitop-server proxy answer `timings` (its guard resolver wrapped by `TimedResolver::wrapping`), desktop `send_request` → `timings`, both web adapters pass it through (`res.timings`). Not deployed | 5c50f4b |

| A0 webapp on lit-material 1.6.0 | done: every `@lit-material/*` 1.0.0 → 1.6.0; added command-palette, timing-waterfall, method-tag, navigation, split-panel, tooltip, popover (kbd and status-bar still 0.0.0-init on npm at the time: not used). test:unit, both builds, check-no-telemetry, Tauri e2e 385 PASS | 66f1a30 |

| A3 shell + T5 | done: icon rail (`lit-material-navigation-rail compact`: Collections, History, Environments, Cookies, desktop pages such as Git, Settings), collection pane (Import/Export at its foot), title bar (collection crumb, search field, tools, account), status bar (request file or place, unsaved, engine), reorderable request tabs (`store.moveTab`), compact density with a fine pointer. Phones: rail and pane are the drawer. T5: `tauri.conf.json` now loads the web app (`webapp/dist-tauri`); the React UI stays as `tauri.react.conf.json` (`npm run dev:react`/`build:react`). **Owner: macOS check before the next desktop release; the Mac App Store build now needs the wasm32 target, wasm-bindgen-cli 0.2.126 and `npm ci` in webapp/.** Tests: unit, both builds, check-no-telemetry, e2e Tauri 399 PASS (new `tests/next.cjs`), web 307 PASS (local hitop-server) | eced16c |

| A4 + B3 request/response | done: the URL's `${variables}` as chips under the URL bar (an undefined one marked; a click opens a popover with the value, secrets masked, and its scope: script, team environment, environment, collection, globals; `store.variableSource`), timing waterfall (`@lit-material/timing-waterfall`) under the response line when the response has `timings` (desktop always; web through the proxy only, so not for localhost), "reused connection" note, a "Diff vs last" response tab (the Compare view in the pane, from the second response on). Request/response stay side by side ≥ 880 px of view (existing split). Not done: inline highlighting inside the URL field itself (chips instead), a separate Checks tab (the existing Tests tab lists them). Tests: unit, both builds, check-no-telemetry, e2e Tauri 405 PASS, web 313 PASS | 8dc3734 |

| A5 command palette | done: `@lit-material/command-palette` on Mod+K (and the title bar's search field): open tabs, every saved request, environments ("Use …"), recent runs from History, and one action registry (`src/commands.js`, shortcut keys from `shortcuts.js` `ACTION_KEYS`); prefixes `>` actions, `@` environments, `#` runs; Tab hands the query to Ask AI as a "Request from text" prompt. Mod+L goes to the URL field (was Mod+K; `/` still works); Mod+P quick open unchanged. Tests: unit, both builds, check-no-telemetry, e2e Tauri 413 PASS, web 321 PASS (local hitop-server) | 60b962e |
| A6 run report | done: `<hitop-run-report>` on the collection page (`views/run-report-view.js`, pure `src/run-report.js`): tiles (requests, checks, p95 with the change against the previous run, wall time), timeline on one time axis with All / Failed / Slow (≥ p95 and ≥ 200 ms), first failure with expected vs actual, the `hitop run` line with Copy, JUnit and report.json downloads in `hitop run`'s shapes (spec/run-report.md, `timings` when the send had them). Run history: `hitop_run_history` in localStorage, 20 summaries per collection (no URLs or bodies). Tests: unit, both builds, check-no-telemetry, e2e Tauri 419 PASS, web 327 PASS (local hitop-server; sync, library, github need servers not started here) | 6c5a602 |
| A7 agent side sheet | done: Ask AI is a right side sheet (`@lit-material/sheet` modal, 520 px; `views/ai-view.js`): Change this request, Write tests, Explain, Request from text. Edits come back as a patch (`src/agent.js`: JSON schema with every field, null = unchanged; validated, unknown fields refused; a value with •••• is never written, a masked header keeps its real value, disabled headers stay) shown as a `@lit-material/diff`; tests are dry-run against the last response (`store.dryRunChecks`, nothing it sets is kept) before Apply / Run with checks / Discard. Palette Tab opens it on Request from text. Tests: unit, both builds, check-no-telemetry, e2e Tauri 429 PASS, web 337 PASS. "Run with checks" not covered by e2e | 7c78247 |
| 7b organization area | done: rail "Organization" → `<hitop-org>` (`views/org-view.js`, pure `src/org.js`) for the org picked in the top bar: General (rename, delete), Members (roles, owner transfer, remove/leave, invites resend/revoke), Plan & billing (usage meters with warn/full levels, plan cards from the server's offers, Stripe Checkout / billing portal on the web; the desktop shows plan and usage and says to change it on the web — no purchase link, App Store rule), Access tokens & MCP (create shown once, revoke, `claude mcp add` line), Runs & monitors, Security & audit for owners/admins of team orgs (SSO status, domains with TXT records and re-check, audit JSONL export; Business-gated answers shown). New organization starts a Team trial (S3) then opens Members and the invite dialog; invites take several addresses, count seats, flag addresses outside your domain (S4); a 402 `limit` opens the limit dialog (S18: what still works, Plan & billing or who can upgrade). One conflict view: `lit-material-conflict-resolver` for GitHub and shared-collection conflicts (S14). Account menu Plan & billing opens the in-app page. App: `@lit-material/kbd` (⌘K hint, ⌘S, ?) and `@lit-material/status-bar` replace the local stand-ins. **Portal**: `/orgs`, `/orgs/:org`, `/orgs/:org/members`, `/orgs/:org/plan`, `/orgs/:org/settings/{general,danger}` and `/tokens` redirect to `hitop.rws8.tech/app/#org=<id>&section=<s>` (`cloud/portal/src/app-redirect.js`; the app opens that org and section); kept on the portal: sign-in, home, workspaces + run history, account (passkeys, sessions), settings tabs sso, scim, admin, audit. `localStorage.hitop_portal_stay=1` keeps the old pages. Tests: unit (incl. the portal↔app link mapping), both builds, check-no-telemetry, e2e Tauri 529 PASS, web 472 PASS (local hitop-server + real sync server for sync, library, github), portal `npm test` 124 ok (old pages via the escape hatch + 7 redirect checks). Not deployed | 2e1bdf2 |
| A8 extensions on the web app | done: `npm run build:extension` (`vite --mode extension`, `.env.extension`: relative paths, no proxy — host permissions let the page fetch any host — the theme boot script moved to `boot.js` since extension pages allow no inline script, a second page `sandbox.html`). Scripts before/after a request run in the manifest's sandboxed page (`src/sandbox-main.js` ↔ `platform/sandbox-scripts.js` by postMessage, plain data), the same code the web build runs in its page (`src/script-run.js`, extracted from store.js); the pm shim loads lazily so the page itself never evaluates code. `webapp/scripts/package-extension.mjs chrome|store|firefox <dir>` writes manifest 5.0.0 (CSP `script-src 'self' 'wasm-unsafe-eval'`, sandbox CSP), background script and icons; `build-chrome-extension.sh`, `build-chrome-store.sh`, `build-extension.sh` use it, `HITOP_EXTENSION_UI=react` keeps the React build. Firefox has no sandboxed extension pages: scripts there fail at once with the reason (as before, the React extension could not run them either). Tests: unit (script-run), web/tauri/extension builds, check-no-telemetry, `tests/extension.cjs` (unpacked Chrome extension in Chromium: opens, dark boot, a host without CORS answers, pre/post scripts in the sandbox, no CSP violation, 7 PASS; needs a full Chromium: `CHROMIUM_PATH`), e2e Tauri 537 PASS, web 472 PASS (local hitop-server + real sync server). Nothing uploaded to stores | 5755a75 |
| A9 site + light theme pass | done: the site's fixed-dark pictures (the app window and terminal on the home page) and its glass/mark tokens move to the graphite palette (`site/src/hitop.css`, `landing.css`, `site.css`); per-scheme `theme-color` (`shell.js`); light values of `--hitop-method-get`/`-post`, `--hitop-pass`, `--hitop-warn` darkened (#24573b, #6f4708) so every `--hitop-*` meaning colour and the text roles reach 4.5:1 on every surface in both themes: new `site/scripts/check-contrast.mjs` (in `npm run check` and CI's Site checks). Reduced motion: the existing `prefers-reduced-motion` rule in `theme.css` covers the site and the app (the app imports it). App: the pass chip on the graphite tokens. Tests: site build (39 pages), check-pages, check-contrast 28 ok, brand/check.sh, webapp unit + both builds + check-no-telemetry, e2e Tauri 537 PASS, web 472 PASS. Not deployed: the site's new look ships with its next deploy | (A9) |

Known gaps: TCP vs TLS not split; concurrent requests to the same host or IP literals may swap
connect times (totals exact); clients installed with `http::install_client` (sync-server monitors)
report no timings. B2 must check HitopKit's `CLICompatibilityTests` before report.json gets
`timings` (URLSessionTaskMetrics has the same phases).

## Accessibility and quality bar

- Text contrast ≥ 4.5:1 on every surface (muted `#8A8A94` on `#0A0A0C` passes; check the darker
  greys used for line numbers stay decorative).
- Status never by colour alone: icons and text with every pass/fail/warn.
- Every control reachable by keyboard; palette and side sheets trap focus and restore it.
- Touch targets ≥ 44 px on touch devices.
- No regressions in `npm run test:e2e` and `test:e2e:tauri`.

## User stories

Canvas pages "User story" (S1–S8) and "More stories" (S9–S20) draw the key ones. Persona: **Maya**,
API lead at a six-person team; **Jonas**, admin; **Priya**, member; **Ana**, guest viewer;
**CI runner**, the team's GitHub Actions credential.

**Decision (owner, 2026-11-09): organization settings, members, plan and billing, tokens, SSO and
audit move inside the app** (an "Organization" settings area with a left menu). The portal at
cloud.hitop.rws8.tech (`cloud/portal`: account, home, members, org, orgs, plan, runs, settings,
tokens, workspaces pages) stays as a redirect into the app, then retires. All app builds (web,
desktop, extensions) get the same pages; desktop opens Stripe checkout and the Stripe portal in the
system browser and comes back through a deep link.

Status: **have** = exists today (maybe restyle), **partial**, **new**.

### Getting started

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U1 | a new user, to send a request without an account | I can try HITOP in seconds | have | — |
| U2 | Maya, to sign in with GitHub, Google, email + password, a mailed code or SSO | my data syncs across devices | have (portal + app) | S1 |
| U3 | Maya, to import Postman collections, environments and globals | I leave Postman in one step | have | S2 |
| U4 | Maya, to open a collection folder in our repo | requests live next to the code | have (desktop, FS Access) | S2 |
| U5 | Maya, to start from an OpenAPI URL and later sync it without losing edits | the spec stays the source | have | S2 |
| U6 | a newcomer, a sample collection and a three-step setup | I learn by doing | partial (sample yes, stepper new) | S2 |
| U7 | Maya, to install the CLI and run the same collection locally | CLI and app agree | have | S9 |

### Everyday work

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U8 | Priya, to build a request with params, headers, body, auth, scripts and checks | one tool covers it | have | concept 1 |
| U9 | Priya, to see variable values and where they come from | no guessing which env applies | new | concept 1 |
| U10 | Priya, to see status, time, size and timing phases | slow parts are obvious | partial (no phases) | concept 1 |
| U11 | Priya, to compare a response with the last one or a saved example | I spot regressions | have (dialog) → tab | concept 1 |
| U12 | Priya, WebSocket, SSE, GraphQL and gRPC in the same app | one client for every API | have (gRPC web via bridge) | — |
| U13 | Priya, to find anything and run any command from ⌘K | I never leave the keyboard | partial (Mod+P) | concept 3 |
| U14 | Priya, the agent to write checks or fix a request as a diff I approve | AI helps without surprises | partial (Ask AI) | concept 3 |
| U15 | Priya, to copy any request as curl, fetch, Python, Go, Java or Rust | I share it with anyone | have | — |

### Environments and secrets

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U16 | Maya, environments per stage with secret values kept on each device | secrets never sync or leak | have | S10 |
| U17 | Maya, team environments shared with the org, secrets still per device | everyone points at the same staging | have | S10 |
| U18 | Maya, `${secret:vault:…}` references to Vault, AWS, Azure, 1Password | secrets stay in our manager | have (desktop, CLI) | S10 |
| U19 | Priya, a clear note when a secret is missing on this device | I know what to fill | partial | S10 |

### Git and files

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U20 | Priya on desktop, changed files marked in the tree | I see what I touched | new | concept 1 |
| U21 | Priya on desktop, to diff, stage, commit, pull, push and switch branches | no terminal needed | have (Git page) | S11 |
| U22 | Maya on the web, to link a Cloud collection to a GitHub repo, branch and folder | every save becomes a commit | have | S12 |
| U23 | Priya, to know where a collection is kept (browser, Cloud, shared, GitHub, folder) | no surprises about who can see it | have (icon) → clearer | S12 |

### Sharing and teams

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U24 | Maya, to create an org and start a Team trial | the team gets a shared space | have (switcher) | S3 |
| U25 | Maya, to invite people with a role and see seats before sending | no billing surprises | have (portal) → app | S4 |
| U26 | Maya, to share one collection end-to-end encrypted and confirm each device | the server can't read it | have | S13 |
| U27 | Jonas, to accept an invite with a code and see the fingerprint | I join the right team | have | S13 |
| U28 | Ana, read-only access as a free guest viewer | stakeholders can look | have (Team) | S8 |
| U29 | Maya, to change roles, resend or revoke invites, remove people | access matches the team | have (portal) → app | S8 |
| U30 | Maya, to transfer ownership | the org survives me leaving | have (admin) → app | S8 |
| U31 | Priya, comments on requests of shared collections | discussion stays with the API | have | — |
| U32 | Priya, version history per request with restore | mistakes are undoable | have | S14 |

### Conflicts

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U33 | Priya, a side-by-side view when a shared collection changed on two devices, with mine / theirs / both per file | nothing is lost | have (People tab) → unified | S14 |
| U34 | Maya, the same view for GitHub-linked collections | one way to resolve everywhere | have (git panel) → unified | S14 |
| U35 | Priya, a merge-conflict view on desktop git | I finish merges in the app | have (Git page) | S14 |
| U36 | Priya, a Cloud library conflict to keep both, never overwrite | my offline edits survive | have ("(this browser)" copy) → explained | S14 |
| U37 | Maya, OpenAPI sync to show what it would change and keep my edits | the spec can't clobber us | have | — |
| U38 | anyone, one badge on the account button counting open conflicts and errors | I see problems at once | have | S14 |

### Runs, CI and automation

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U39 | Priya, to run a collection or folder and read a clear report | failures explain themselves | partial | concept 2 |
| U40 | Maya, to generate a GitHub Actions workflow or crontab line | the suite runs in CI | have (CLI `hitop schedule`) → app | S9 |
| U41 | Maya, a read-only runner credential for CI | CI pulls shared collections safely | have | S9 |
| U42 | Maya, run history from CI in the app | I see trends, not just today | have (Cloud) | S15 |
| U43 | Maya, monitors that run on schedule and mail on fail/recover | we hear before customers do | have (paid) | S15 |
| U44 | Priya, a mock server from saved examples | the frontend can start before the API | have (paid) | S16 |
| U45 | Maya, personal access tokens and the hosted MCP endpoint | agents and scripts use our library | have (portal) → app | S17 |
| U46 | Priya, `hitop mcp` for Claude Code and Cursor | my editor's agent drives HITOP | have | S17 |

### Plans, billing and usage

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U47 | Maya, to compare plans and pick seats | I pay for what we use | have (portal) → app | S5 |
| U48 | Maya, to review and pay on Stripe | the card never touches HITOP | have | S6 |
| U49 | Maya, usage against limits and a warning before a limit | nothing blocks us mid-task | partial (402 on create) | S7, S18 |
| U50 | Priya, a clear message when a limit stops an action, with who can fix it | I'm not stuck | partial | S18 |
| U51 | Maya, invoices, payment method, tax ID, cancel | billing is self-serve | have (Stripe portal) | S7 |
| U52 | Maya, to know a trial or failed payment never deletes data | I trust the product | have (rules) → said in UI | S5, S7 |

### Account, devices and security

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U53 | Maya, a list of my devices with fingerprints, and to remove one | a lost laptop loses access | have | S19 |
| U54 | Maya, a recovery phrase and password reset that keep my keys | I never lose shared data | have | S19 |
| U55 | Maya, to turn the Cloud library off and keep everything local | privacy is my choice | have | S19 |
| U56 | Maya (Business), SSO for our domain with DNS verification | people sign in with company accounts | have (server) → app | S20 |
| U57 | Maya (Business), audit log with export and signed daily anchors | auditors are satisfied | have (server) → app | S20 |
| U58 | Jonas (Business), an admin console: request key rotation, remove devices from org workspaces | we respond to incidents | have (server) → app | S20 |

### Settings and preferences

| # | As … I want … | So that … | Today | Screen |
| --- | --- | --- | --- | --- |
| U59 | Priya, request time limit, theme, shortcuts list, storage usage | the app fits me | have | — |
| U60 | Priya, my own Anthropic key for the agent, kept on this device | AI uses my account | have | — |
| U61 | Priya on desktop, a choice about the weekly usage ping | telemetry is opt-in | have (desktop only) | — |
