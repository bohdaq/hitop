# HITOP web app: UI/UX review

Reviewed: `webapp/` at `origin/main` fe54a11 (2026-11-01), built with `npm run build` and served from `dist/` behind a fake `/api/proxy` (canned answers for `https://api.hitop.test/*`, a no-CORS and a CORS local API, a routed WebSocket echo). Driven with Playwright (Chromium 1248) at 1440×900, 1024×768 and 390×844, light and dark. axe-core 4.x ran on six states. All screenshots are in `shots/`. Line numbers are from `webapp/src/` unless a path says otherwise (the 1416bad→fe54a11 diff touched only `platform/index.js`).

Not exercised: File System Access folders (`Open folder`, `Save to folder`, Git) need a real directory picker; gRPC sending (desktop only, the web build says so clearly); the real hitop-server proxy's error bodies.

## Verdict

HITOP's web app is a clean, fast, honest tool: the Material surface is consistent, dark mode is first-class, wording is in plain English, Postman and OpenAPI imports produce an audit trail that Postman itself doesn't, tests/scripts/GraphQL/WebSocket all work end to end, and contrast passes AA everywhere I measured. What holds it back is not polish but a handful of trust-breaking behaviours a Postman/Bruno/Insomnia user hits in the first ten minutes: a 5 MB response silently stops every later save and tabs vanish on reload; "Import" wipes all existing collections with no confirmation; a raw JSON POST goes out without any `Content-Type`; a URL typed without `https://` fetches HITOP's own index.html and records a green 200; closing a tab discards edits with no warning; and there are no keyboard shortcuts at all (Ctrl+Enter does nothing from the body editor). Fix the top five and the app is competitive with Hoppscotch for the "send a request in the browser" use case; the rest of the list is daily-friction work (response pane height, sidebar scale, tab strip, compare defaults).

## Top 10 issues, by user impact

### 1. Blocker: a large response breaks persistence silently; tabs are lost on reload

- **What**: after a 5 MB response, every later `saveTabs` throws `QuotaExceededError`; it's caught and `console.error`'d, the user sees nothing. New tabs and edits made after that point are gone on reload. Reproduced: send `/huge` (5 MB, `Cut at 5 MB` shown), open a new tab, type a URL, reload → both tabs missing (`p3b.log`: "tabs before: … GET /huge | GET /after-huge-edit", "tabs after: …" without them; 3 quota errors).
- **Where**: `store.js:1180-1190` stores the full `response` body on the tab; `store.js:293` saves on `pagehide`; `frontend/src/services/storageService.jsx:48-54` swallows the exception. The proxy caps at 5 MB but localStorage is ~5 MB total for the origin, so one big body fills it.
- **Screenshot**: `shots/p1-huge-body.png` (the state that triggers it), `shots/p3-after-quota-reload.png` (after reload: 11 tabs, the two newest gone).
- **Why it matters**: silent data loss. Users of API clients routinely hit multi-MB responses (logs, exports). The failure is invisible and persists until the big tab is closed.
- **Fix**: don't persist response bodies above ~256 KB (store a `{ truncatedForStorage: true, head: first 64 KB }` marker and show "Response not kept across reloads"); move response bodies out of `hitop_tabs` into IndexedDB (you already open an `hitop` IDB for folder handles, `platform/web.js:86-104`); and surface the write failure as a toast ("Couldn't save your tabs: storage is full. Close the tab with the 5 MB response.") instead of `console.error`.

### 2. Blocker: Import replaces every collection with no confirmation, and is paste-only

- **What**: the Import dialog has a textarea and a primary button "Replace and import". Pasting a Postman collection deleted the existing "Default" collection with the saved "My precious request" instantly; no confirm, no undo (`p2.log`: sidebar before/after). There is no file picker (`input[type=file]` count 0), so a 2 MB Postman export has to go through the clipboard.
- **Where**: `views/hitop-app.js:252-261` (dialog), `store.js:615-636` (`closeAllFolders()` + `this.collections = imported`, lines 629-630). Zip import on the Collections page does add instead (`collection-views.js:63-69`), which is the behaviour users expect.
- **Screenshot**: `shots/p2-import-dialog.png`, `shots/p2-after-import.png`.
- **Why it matters**: the Postman migrant's first action destroys their work. Every competitor (Postman, Insomnia, Bruno, Hoppscotch) imports additively and offers a file picker.
- **Fix**: make JSON import additive like zip/OpenAPI (new collections appended, same-id collections replaced after a per-collection confirm). If "replace all" must remain, make it a checkbox off by default and route it through `confirm({ destructive: true })` listing what will be deleted (you already do this for one collection in `hitop-app.js:172-179`). Add `<input type="file" accept=".json,.yaml,.yml">` and drag-and-drop onto the textarea.

### 3. Major: raw JSON body is sent without any `Content-Type`

- **What**: POST with body type "Raw (JSON, XML, text)" and `{"a": 1}` goes out with headers `[]` (`p4.log`: "POST raw JSON, no explicit header → sent headers: []"). The curl snippet shows the same hole as `-H 'Content-Type:'` with an empty value (`shots/p1-code-dialog.png`). Most frameworks (Express `json()`, Rails, Spring) then return 400/415 or ignore the body.
- **Where**: `store.js:1116-1126` adds `Content-Type` only for `multipart`/`urlencoded`; raw gets nothing. The body editor already detects JSON for highlighting (`views/request-view.js:192`).
- **Screenshot**: `shots/p1-code-dialog.png`, `shots/p1-post-body.png`.
- **Why it matters**: the first POST a new user sends fails for a reason the app hides; Postman, Insomnia and Bruno all set `application/json` when the raw sub-type is JSON.
- **Fix**: split "Raw" into JSON / XML / Text / Custom (a second select next to Body type, as Postman does), and in `dispatch()` push `Content-Type: application/json` / `application/xml` / `text/plain` unless the user set one. Show the auto header greyed in the Headers tab as "added automatically" so there's no magic. Fix the snippet generator to skip empty-valued headers.

### 4. Major: a URL without a scheme fetches HITOP's own page and shows a green 200

- **What**: typing `api.hitop.test/users` or even `not a url` and pressing Send issues a relative fetch to HITOP's own origin (`/app/not%20a%20url`). In my harness the static server's SPA fallback answered with `index.html`, shown as `200 OK · 1.2 KB · HTML` and recorded in History as a 200; on hitop.rws8.tech it is whatever the static server returns for that path (likely a 404 page), still presented as a real response rather than a URL error (`shots/p1-err-badurl.png`, `shots/p1-history.png` rows "not a url", "api.hitop.test/users"). The field simultaneously shows the browser's "Please enter a URL." validation text, so the UI contradicts itself.
- **Where**: `views/request-view.js:462-476` (`novalidate`, `type="url"` keeps the native error text), `store.js:1154-1159` (`send()` only checks for empty), `frontend/src/services/transportService.jsx:33-43` (`usesProxy` returns false for anything that isn't `http(s):` so it falls to a relative `fetch`).
- **Screenshot**: `shots/p1-err-badurl.png`.
- **Why it matters**: first-time users type hostnames without a scheme constantly; a wrong, confident 200 is worse than an error.
- **Fix**: after variable interpolation, if the URL has no scheme prepend `https://` when it looks like a host (`/^[\w.-]+(:\d+)?(\/|$)/`), otherwise refuse with an inline error under the field ("Enter a full URL, e.g. https://api.example.com/users"). Never issue a relative fetch. Drop `type="url"` (it produces the stray "Please enter a URL." text) in favour of your own message.

### 5. Major: closing a tab discards edits to a saved request with no warning and no unsaved marker

- **What**: open "Get post", change the URL, ⋯ → Close tab: the tab closes, the edit is gone, the saved request still has the old URL (`p5.log`). Tabs never show a dirty dot; the title changes only for unsaved drafts.
- **Where**: `store.js:412-420` (`closeTab`), `views/hitop-app.js:460` (menu item), tab titles `hitop-app.js:449`.
- **Screenshot**: `shots/p3-unsaved.png` (edited saved request "Users p2", tab unchanged).
- **Why it matters**: Postman/Insomnia users rely on the dirty indicator and the "save changes?" prompt; losing a half-written script hurts.
- **Fix**: compute `dirty = loadedRequestId && snapshot(tab) !== snapshot(request)` in the store; render `●` after the tab title and in the sidebar row; on Close tab / Delete request, `confirm({ headline: "Discard changes to Get post?", confirmLabel: "Discard", destructive: true })`; add "Save and close".

### 6. Major: no keyboard shortcuts

- **What**: Ctrl/Cmd+Enter does not send from the body or script editors (`p1.log`: "ctrl+enter in body sent? 0"); there is no Cmd+S, no Ctrl+T/W, no Ctrl+1..9 tab switching, no `/` to focus the URL. Enter sends only when focus is in the URL field. The only keydown handlers in the app are `request-view.js:475`, `websocket-view.js:48`, `grpc-view.js:53` and the dialog's Enter (`hitop-app.js:103-108`).
- **Why it matters**: every competitor has Ctrl+Enter / Cmd+S; power users send 100+ requests a day and the mouse round-trip to Send after editing a body is the single most repeated action.
- **Fix**: one `keydown` listener on `hitop-app` (`hitop-app.js:49`, `firstUpdated`): `Mod+Enter` → `store.send()`, `Mod+S` → save (silent overwrite when `loadedRequestId`, dialog otherwise), `Mod+T` new tab, `Mod+W` close tab (preventDefault only when a request tab is active), `Mod+Shift+[`/`]` switch tabs, `Mod+K` focus URL, `?` → a shortcuts sheet. Show the shortcut in the Send button's `title`.

### 7. Major: "Compare" opens comparing the current response with itself

- **What**: after sending `?page=1` then `?page=2` (bodies differ), Compare says "The responses are the same." because "Before" defaults to the newest history item, which is the current response (`shots/p3-compare.png`). Users conclude the diff is broken.
- **Where**: `store.js:773-789` (`compareSources`: current first, then history newest-first, so `sources[1]` is the same response), `views/tools-dialogs.js:87-88` (`left = sources[1]`, `right = sources[0]`).
- **Fix**: skip history entries whose `response` equals the current one (same `id` or deep-equal status+body) when picking the default `left`; prefer the most recent history entry for the same method+URL; and label the options "Current response" / "Previous (page=1, 3:48 PM)".

### 8. Major: response pane gets ~300 px on a 900 px screen; no raw/pretty, wrap, search or download

- **What**: editor and response are stacked; with the Headers tab open the response body starts at y≈600 at 1440×900 (`shots/p1-response-desktop-light.png`), and with a script or body editor open it starts at y≈790 (`shots/p3-tests-result.png`, `shots/p1-post-body.png`). There is no collapse, no draggable split, no side-by-side option. The viewer has Copy only: no pretty/raw toggle, no "wrap long lines", no in-body search, no "Save to file", no HTML preview, no JSON path/collapse. Invalid JSON with a JSON content type is shown as plain text with no warning (`shots/p1-err-badjson.png`, `store.js:208-214`).
- **Where**: `views/request-view.js:385-443`, `app.css:90,97,103` (`max-height: 60vh` on the code block).
- **Why it matters**: reading the response is the point of the tool; Postman/Insomnia/Bruno all give the response half the screen and a search box.
- **Fix**: make `.editor` collapsible (remember per tab) and give `.main` a two-pane layout above 1200 px (`grid-template-columns: minmax(420px, 1fr) minmax(420px, 1fr)` with a drag handle; stacked below). In the response header add: Pretty | Raw | Preview (HTML), Wrap toggle, a search field (`Ctrl+F` inside the pane), "Download" (`platform().saveFile`), and a `lit-material-alert variant="warning"` when `content-type` says JSON but parsing failed ("Server says JSON but the body doesn't parse: unexpected `,` at position 23").

### 9. Major: the sidebar doesn't scale: at 32 requests the whole rail scrolls and Environments/History/Import fall off-screen

- **What**: with one 32-request collection the aside scrolls as one block; the footer (Environments, Cookies, History, Export, Import) sits at y=1629 on a 900 px viewport (`shots/p4-sidebar-30-requests.png`; `p4.log`). Collections cannot be collapsed, there is no filter/search box, every collection shows a permanent "Variables" row, and there is no right-click menu on a request (`p5.log`).
- **Where**: `app.css:35-40` (`.sidebar { overflow-y: auto }` with the footer inside the scroller), `views/hitop-app.js:380-427` (flat tree, no `aria-expanded`), `hitop-app.js:418-427` (footer inside `aside`).
- **Fix**: make `nav.tree` the only scroller (`flex: 1; min-height: 0; overflow-y: auto`) and pin `.sidebar-footer`; add a filter input above the tree (type-to-filter by name/URL, `Ctrl+P` quick-open); make collection rows disclosure buttons (`aria-expanded`, remember state); move "Variables" into the collection row's hover menu; add a context menu on request rows (Open in new tab, Rename, Duplicate, Delete).

### 10. Major: tab strip hides the active tab, names every draft "New request", hides its scrollbar

- **What**: with 14 tabs the 14th (active) is off-screen and the strip doesn't scroll it into view (`p5.log`: `activeIndex 13, visible false`, `shots/p5-active-tab-hidden.png`); the tablist scrolls but `scrollbar-width: none` leaves no affordance; all drafts are titled "New request" so they're indistinguishable (`shots/p4-13-tabs.png`); tabs have no close "×" (close is in the ⋯ menu or right-click), and the editor's sub-tabs on phone overflow with "Script before/after" unreachable without knowing to swipe (`shots/p4-phone-request-post-light.png`, `sw 664 / cw 358`).
- **Where**: `app.css:81-83`, `views/hitop-app.js:439-456`, `store.js:405,477` (title), editor tabs `request-view.js:348-350`.
- **Fix**: call `tab.scrollIntoView({ inline: "nearest" })` when `currentTab` changes; show a thin scrollbar or fade edges; title drafts from method+host once a URL exists (you already do "GET /users/1" after a send; do it on input); add a close "×" on hover/active and middle-click to close; on phone use `lit-material-tabs` scrollable variant with fade edges, or a `select` for the editor sections.

## Findings by area

Severity: **blocker** / **major** / **minor** / **polish**.

### Information architecture and navigation
- **major** Postman folders are flattened: "Account" folder → "Me", "Users page 2" appear as top-level siblings in sidebar and collection page; the folder survives only as `file: account/me` (`shots/p2-after-import.png`, `p2.log`). OpenAPI tags likewise ("3 requests in 1 folder" in the report, flat in the UI). Postman users navigate by folder. Render `file` path segments as collapsible groups in `hitop-app.js:397-411` and `collection-views.js:153-165`.
- **minor** Two "HITOP" marks on desktop: the sidebar brand (`hitop-app.js:363`) and the topbar which otherwise holds only a link to the marketing site and the theme toggle; the 56 px topbar is mostly empty (`shots/p1-firstrun-desktop-light.png`). Fold the theme toggle and site link into the sidebar footer and drop the topbar above 900 px, giving the response 56 px back.
- **minor** "History" and "Export"/"Import" are modal dialogs while Environments/Cookies are pages; `aria-current` highlights pages but the dialog rows never show state. Make History a page (or a side panel) with search and per-item delete; it's capped at 50 (`frontend/src/services/historyService.jsx:66`) with no way to pin.
- **minor** "Collections" heading row and the per-collection "Variables" leaf look like requests (same `.tree-row` style, `hitop-app.js:381-396`). Use a disclosure chevron for collections and a settings glyph on the collection row for variables.
- **polish** Legacy data names the request "New Request" while new drafts are "New request" (`shots/p3-legacy-loaded.png`). Normalise on load.

### First run and empty states
- **good** First run lands directly in a request editor with a helpful placeholder and a proxy explanation (`shots/p1-firstrun-desktop-light.png`).
- **minor** The explanation "Requests to public APIs go through the HITOP proxy" is the only place the proxy is mentioned. There's no per-request indicator (proxied vs direct) and no opt-out; `usesProxy` is build-time (`transportService.jsx:12`). A user sending a bearer token to a third-party relay should see "via HITOP proxy" next to the status chip and have a toggle (direct fetch, accept CORS). Add a small chip in `.response-meta` (`request-view.js:416-428`) and a Settings entry.
- **minor** No Settings page exists in the web build (`platform().pages` only in Tauri, `hitop-app.js:424-426`, `platform/web.js`). Users look for: follow redirects, timeout, SSL verify (proxy side), proxy on/off, font size, default headers. Even a read-only "What the web app can and can't do" page would stop the hunt.
- **polish** The "Send a request to see the response" empty state is boxed with a dashed border and stays visible under long editors; fine.

### Request editor
- **major** Code (`</>`) icon button wraps under the Method select at every width because `.request-line` declares 4 grid columns for 5 children (`app.css:86`; measured: Save at x=1376 y=139, Code at x=304 y=199, `p5.log`). Add an `auto` column (and on phone put Save/Code together after Send).
- **major** `Content-Type` not auto-set for raw JSON (top 10 #3).
- **minor** Header name field has no autocomplete for common headers (`shots/p4-headers-row.png`; `lit-material-key-value-editor` renders plain text fields). Add a `datalist` of ~30 common names and `application/json` etc. for values.
- **minor** The URL field is a plain `<input>`; `${var}` isn't highlighted and unresolved variables are only reported after Send, as a toast with developer wording: "The request wasn't sent: Variable 'missing' not found in variables object" (`store.js:1216`, `shots/p5-unresolved-variable.png`). Highlight `${...}` tokens (resolved = tinted, unresolved = red underline) via a mirrored overlay, and phrase the error "`${missing}` isn't defined in Default, the active environment or globals. Define it or pick another environment."
- **minor** Switching the method select to WebSocket keeps the Headers section open with a note that headers aren't sent in the browser (`p3b.log`, `request-view.js:94-102` only jumps for gRPC). Jump to Messages as you do for gRPC.
- **minor** `Send` becomes disabled "Sending" with a skeleton; there is no Cancel (`shots/p1-loading.png`). Keep an `AbortController` and turn the button into "Cancel".
- **minor** Params hint "Values are sent as typed, so encode special characters yourself" (`request-view.js:183`) pushes the encoding burden to the user; competitors encode by default with an "Encode automatically" switch.
- **minor** Saving an already-saved request always opens the Save dialog (`shots/p5-save-overwrite-dialog.png`, `request-view.js:480`, `hitop-app.js:243-251`). Save silently when `loadedRequestId` is set; keep "Save as…" in the ⋯ menu.
- **polish** The six editor sections are Material "fixed" tabs stretched across 1100 px (`shots/p1-firstrun-desktop-light.png`); left-aligned scrollable tabs read better for a tool and leave room for a "Collapse" control.
- **good** Auth tab with inline explanation of what is sent; Params ↔ URL sync with disable checkboxes; multipart/urlencoded modes with per-field enable, file picker and the honest note about files not surviving reload (`shots/p6-multipart.png`); GET body tab explains itself (`shots/p4-get-body-empty.png`); gRPC on the web says what it can't do and still lets you author (`shots/p6-grpc-web.png`).

### Response viewer
- **major** Pane height and missing raw/pretty/wrap/search/download (top 10 #8).
- **minor** Invalid JSON shown as text silently (`store.js:208-214`).
- **minor** `Cut at 5 MB` warning is a plain span in the meta row (`request-view.js:420`); make it an alert with "Download full response" if the proxy can stream it, or at least explain the cap.
- **minor** HTML responses are shown as source only; no Preview tab (Postman/Insomnia/Hoppscotch have one). Render in a sandboxed `iframe srcdoc` with scripts off.
- **minor** Response `pre` is scrollable but not focusable (axe `scrollable-region-focusable`, `request-view.js:436`); add `tabindex="0"` via `lit-material-code-block` part or wrap.
- **polish** 600 KB JSON renders in ~0.9 s and 5 MB in ~1.8 s with highlighting skipped above 500 KB (`request-view.js:49`); acceptable, but show "Highlighting off for large bodies" so users don't think it's a bug (`shots/p1-huge-body.png`).
- **good** Status chip colours, time and size up front; `Tests (2/3)` tab with Passed/Failed and "got …" detail (`shots/p3-tests-tab.png`); Set-Cookie preserved as separate lines in Headers.

### Environments, globals, secrets
- **good** Environment page is clear: chips per environment, "active" tag, Use/Deactivate, separate Secrets editor masked by default, "Secret values stay in this browser … never written to exports" (`shots/p2-environments-imported.png`). Postman environment import worked including `type: secret` → Secrets.
- **minor** The sidebar select has "Manage environments…" as a fake option (`hitop-app.js:370-379`), which needs the value reset hack; it's fine in practice but screen readers announce it as a choice. Use a separate icon button next to the select.
- **minor** The Environments page shows "Globals" below the fold on phone and tablet with no anchor; add a secondary tab (Environments | Globals).
- **minor** No "which value wins" preview: a user with `base` in collection vars, env and globals can't see the effective value. Add a resolved-variables panel (Postman's eye icon).

### Collections sidebar, collection page, runner
- **major** Sidebar scale and folders (top 10 #9, IA above).
- **minor** Run results on the collection page show `{"error":"not found","code":"E_NOT_FOUND"}` raw for the failed request and no summary alert; the summary alert exists only on the Collections overview after "Run all" (`collection-views.js:75-81` vs `149`). Show the summary on the collection page too, and make each run row expandable to the response/tests.
- **minor** Runner always "stops at the first failure" (`collection-views.js:148`) with no option to continue, no iteration count, no data file, no delay. Add "Continue on failure" and iterations before users ask for Postman's runner.
- **minor** `New request` always lands in the first collection (`store.js:497-499`); after deleting "Default" it silently targets whatever is first. Use the collection of the current tab, falling back to a picker.
- **polish** Collection cards show "4 requests · 1 variable" and a drag handle; nice. Request rows on the collection page have only a trash icon; add Duplicate/Rename there too.
- **good** Drag-to-reorder with keyboard (Space) and a visible hint; the "Run collection" / "Variables" / "Sync with OpenAPI" header is tidy (`shots/p3-openapi-collection.png`).

### History
- **minor** Modal, 50 items, no search, no filter by status/method, no delete-one, timestamps only (`shots/p1-history.png`, `hitop-app.js:311-334`). Replace with a page or a side panel; group by day.
- **minor** Schemeless "requests" get recorded as 200 (top 10 #4).

### Import / export
- **blocker** Replace-all + paste-only (top 10 #2).
- **good** The "Check after import" report is better than Postman's own: unsupported `pm.*` calls listed per line with the source line shown (`shots/p2-import-report.png`); OpenAPI report lists operations, variables, auth and "To check" items (`shots/p3-openapi-report.png`); "Sync with OpenAPI" previews before saving (`shots/p3-openapi-sync.png`).
- **minor** Export dialog dumps the whole JSON into a code block with "Show more" before the Download button (`shots/p2-export.png`); for 32 requests this is a wall of text. Show a summary (N collections, M requests, format) and keep "Preview" as a disclosure.
- **minor** Bad-JSON error text is good ("That isn't valid JSON. Paste the whole of a collection file, from the first { or [ to the last.") but the button stays "Replace and import" (`shots/p2-import-badjson.png`).

### Settings and keyboard
- **major** No shortcuts (top 10 #6); no Settings page in the web build.
- **minor** Code editors (CodeMirror) bind Tab to indent (`views/code-editor.js:64`, `indentWithTab`); Esc-then-Tab leaves the editor (verified, `p4.log`) but nothing tells the user. Show "Esc then Tab to leave" in the editor label on focus, as GitHub does.

### Accessibility
- **major** axe `aria-required-children` (critical) on every `lit-material-sortable[role=list]`: the component adds a `[role=status]` child inside the list (`hitop-app.js:399`, `collection-views.js:83`; `@lit-material/sortable` 0.2.1). Move the live region outside the list in the component, or drop `role="list"` on the sortable and put it on an inner `ul`.
- **minor** axe `page-has-heading-one` on the request view: no `h1` (`request-view.js:457-488`). Give the tab title an `h1` (visually styled as the tab) or a visually-hidden `h1` "Request: Get post".
- **minor** axe `link-name` (serious) on phone: `.topbar-link` hides its text span below 900 px (`app.css:215`, `hitop-app.js:513`) so the link has no name. Add `aria-label="HITOP website"`.
- **minor** axe `scrollable-region-focusable` on the response `pre` (above).
- **minor** Touch targets: sidebar rows are 36 px tall, sort handles 28×28 (`p4.log` phone list; `app.css:47`). Raise rows to 44 px on `(hover: none)`.
- **minor** Dialog focus cycles through `body` between Save and the first field (`p4.log` "dialog tab order"); it isn't a hard trap but focus briefly leaves the dialog. Keep the loop inside `lit-material-dialog`.
- **minor** Sidebar rows use `aria-current="page"` on `button`s; acceptable, but the tree lacks `aria-expanded` and the request rows combine method + name into one label ("GET New Request") without a separator; add `aria-label="GET, New Request"`.
- **good** Contrast: every sampled pair passes AA in both schemes (on-surface-variant text such as the response meta and hints 7.6:1 light / 9.9:1 dark, status chips 8.1/10.1, method tags 4.7–8.8, inactive tab labels 15.8); focus rings visible on handles and rows; every icon button has an `aria-label`; skip link present; `aria-busy` on loading response; `role="status"` on run results and socket status.

### Error states
- **good** Direct-fetch CORS failure explains itself: "Couldn't reach the server. Requests to localhost and private networks go straight from your browser, so the server must allow CORS from this site; the desktop and mobile apps don't need it." (`shots/p1-err-cors-direct.png`). Proxy DNS failure shows the proxy's message. WebSocket failure explains that browsers give no detail and calls out `wss://` mixed content (`platform/web.js:160-163`, `shots/p3-ws-error.png`). Script errors show as a failed test *and* a toast (`shots/p3-script-error.png`).
- **minor** "Request failed" has no Retry and no link to "Use the desktop app" when the cause is CORS.
- **minor** A 4xx/5xx is shown exactly like a 2xx apart from chip colour; a short "404 from api.hitop.test" line above the body would help, and `Save as example` is offered for errors too (fine, but label it "Save 404 as example").
- **minor** Empty URL → toast "Enter a URL first." (`store.js:1158`) while Send stays enabled; disable Send with a tooltip instead, or focus the field with inline error.

### Performance feel
- Fine for a browser tool: first paint fast (bundled Lit + CodeMirror), 600 KB JSON in <1 s, 5 MB in <2 s, 32 requests + 14 tabs no visible lag. The only perf-adjacent problem is #1 (storage quota).

### Consistency: wording, spacing, iconography
- **good** Wording is consistently plain ("Script before/after", "Save to a collection", "Check after import"); icons are one family (Phosphor) and every icon button has a label; spacing tokens are uniform; dark mode is a full token swap, not an overlay (`shots/p1-response-desktop-dark.png`, `shots/p4-dark-environments.png`).
- **polish** Primary buttons: "Replace and import" (destructive) is styled the same as "Create"; the `destructive` class is applied but reads as the normal brand colour in the screenshot (`shots/p2-import-dialog.png`). Use the error colour for destructive primaries.
- **polish** Toasts sit top-centre over the topbar and the "Enter a URL first" one lingered across three screenshots (5 s); fine, but they cover the tab strip on phone.
- **polish** "Save as example" and "Compare" in the response meta are text buttons in brand colour, visually as prominent as the status chip; demote to icon+text at `on-surface-variant`.

### Mobile (390×844)
- **good** Drawer sidebar with scrim and a close "×" in the topbar; the request line reflows (URL full width, Method + Send below); dialogs fit; response reads well; no horizontal scroll (`shots/p4-phone-response-light.png`, `shots/p4-phone-import-dialog-light.png`).
- **minor** Editor sub-tabs overflow with no fade/scroll affordance ("Bod…" clipped, `shots/p4-phone-request-post-light.png`).
- **minor** Collection page action buttons wrap over three rows (`shots/p4-phone-collection-light.png`); move the icon actions into a ⋯ menu below 600 px.
- **minor** "Save to folder" shows on phones that will never have the directory picker; gate on `platform().folders` *and* a pointer/hover media query.

### Dark mode
- **good** Correct everywhere sampled: sidebar, dialogs, code blocks, chips, GraphQL editor, 404 state (`shots/p4-dark-404.png`, `shots/p4-dark-collections.png`); the toggle persists in `hitop-theme` and the inline script in `index.html` prevents a flash.
- **polish** No "System" option in the toggle; cycling light→dark only means users on auto-dark OSes can't return to "follow system" without clearing storage.

### Compared with Postman / Bruno / Insomnia / Hoppscotch
HITOP already matches or beats them on: honest proxy/CORS explanation, import reports, OpenAPI sync, secrets masking and export hygiene, test output readability, YAML-per-request folders for git, WebSocket log with timestamps. It is behind on the table stakes those users expect on day one: keyboard shortcuts, dirty-state + save prompts, folder trees and a sidebar filter, auto `Content-Type`, response search/raw/preview and a resizable split, additive file-based import, and a settings page. None of these need new backend work.

## Owner note: move the logo into the top bar (2026-11-01)

![Live app, dark mode: empty top bar, logo in the sidebar](shots/owner-topbar-live.png)

On https://hitop.rws8.tech/app/ the top bar is an empty dark strip, while the logo and "HITOP" sit at the top of the sidebar
above "+ New request". The owner wants the logo in the top bar. Proposal:

- **Top bar, left:** logo + "HITOP" (links to the site, `aria-label="HITOP website"`), the same on every page of the app.
- **Top bar, right:** the environment picker, since it applies to every request; the theme toggle next to it.
- **Sidebar:** starts with "+ New request", then collections. This frees about 150 px for the request list and helps issue 9.
- **Phone width:** logo + a menu button that opens the sidebar.

The same screenshot also shows quick win 1 (the `</>` Code button wrapped onto its own row under Method) and issue 10 (every
draft named "New Request").

## Quick wins (under an hour each)

1. `app.css:86`: `grid-template-columns: 160px minmax(0, 1fr) auto auto auto` so the Code button stops wrapping.
2. `store.js:1116-1126`: push `Content-Type: application/json` for raw bodies that parse as JSON when no header is set; skip empty-valued headers in the snippet input (`store.js:793-`).
3. `store.js:send()`: prepend `https://` when the interpolated URL has no scheme; never fetch a relative URL; drop `type="url"` on the field.
4. `tools-dialogs.js:87`: default `left` to the first source whose response differs from `current`.
5. `storageService.jsx:48-54` + `store.js`: on `QuotaExceededError` emit `notify("Couldn't save your tabs: storage is full…")`.
6. `hitop-app.js:443-451`: `scrollIntoView` the selected `lit-material-tab` on change; title drafts "METHOD host/path" on URL input.
7. `request-view.js:94-102`: when `kind` becomes `websocket`, set `section = BODY` as for gRPC.
8. `hitop-app.js:513`: `aria-label="HITOP website"` on the topbar link; `request-view.js`: visually-hidden `h1`.
9. `hitop-app.js:460` / `store.js:412`: route "Close tab" through `confirm()` when the tab differs from its saved request.
10. `hitop-app.js:49`: a global `keydown` for `Mod+Enter` → send and `Mod+S` → save (even before the full shortcut map).
11. `app.css:35-40`: make `nav.tree` the scroller and pin `.sidebar-footer`.
12. `import` dialog: add `<input type="file">` that fills the textarea.

## What's already good (keep)

- Zero-setup first run: the editor is there, the placeholder teaches variables, the empty state explains the proxy.
- Plain-English microcopy throughout, including error messages that say *why* and *what to do* (CORS, WebSocket mixed content, bad JSON paste, script failures).
- Postman import report with per-line unsupported `pm.*` calls and the source line; OpenAPI import report and preview-before-apply sync.
- Secrets: masked by default, separate editor, kept out of exports and snippets unless explicitly included (with a warning).
- Tests output: pass/fail list with the actual value on failure; counts in the tab label and meta row.
- GraphQL: schema fetch status, completion from introspection, variables pane, operation name with explanation.
- WebSocket: connect/disconnect in the request line, timestamped log with direction icons, saved messages.
- Dark mode and contrast; consistent Material tokens; Phosphor icons with labels; keyboard-reorderable lists with a visible hint.
- Multipart/urlencoded editors that say exactly what will be sent and what won't survive a reload.
- gRPC in the web build: authoring allowed, sending refused with a clear, non-nagging note.
- Storage compatibility with the older React app (legacy fixture loads unchanged).
