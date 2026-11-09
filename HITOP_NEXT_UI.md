# HITOP Next: UI redesign (PRD)

Status: proposal, 2026-11-09. Nothing here is built yet.
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
8. **Extensions on the web app** (Firefox + Chrome builds from `webapp/`; storage keys already
   shared with React; extension CSP and the proxy-less send path to check).
9. Site restyle with the same tokens; light theme pass; store screenshots.

**Track B — timing (Rust), starting now**
1. hitop-core: measure DNS / connect / TLS / first byte / download per request.
2. `timings` in hitop-server's proxy answer, the desktop `send_request`, and report.json
   (additive field, `spec/run-report.md` + compat case).
3. The app shows the waterfall when `timings` exist; browser-direct sends show total time only.

## Accessibility and quality bar

- Text contrast ≥ 4.5:1 on every surface (muted `#8A8A94` on `#0A0A0C` passes; check the darker
  greys used for line numbers stay decorative).
- Status never by colour alone: icons and text with every pass/fail/warn.
- Every control reachable by keyboard; palette and side sheets trap focus and restore it.
- Touch targets ≥ 44 px on touch devices.
- No regressions in `npm run test:e2e` and `test:e2e:tauri`.
