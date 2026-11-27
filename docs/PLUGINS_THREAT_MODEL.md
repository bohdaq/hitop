# Plugins: design and threat model

Status: written before any code (owner's rule: plugins get a written threat model first). Plan item C7. Part 1 is built: spec section 5c,
the refusals in every interpolator, and the CLI (`hitop plugin`, the `hitop plugin-run` child, the `value` hook). Part 2a is built: the
web app and both extensions (`platform/wasm-plugins.js`, `plugins-worker.js`, Settings → Plugins). Part 2b is built: the desktop
(hitop-core `plugin_host`, shared with the CLI: the store in `<config>/hitop/plugins/` and the child, which in the desktop is the app's own
executable started with `--hitop-plugin-run`).

## What a plugin is

A plugin is one JavaScript file with a manifest at the top, installed by a person on one device. It adds
pure functions, called with data and answering data:

| Hook | Called with | Answers | Used for |
|---|---|---|---|
| `value` | its arguments (text) | text | `${plugin.name(args)}` in a URL, header or body: signatures, random data, encodings |
| `viewer` | a response (status, headers, body up to 1 MB) | text or a table | showing a response another way (protobuf as text, CSV as a table) |
| `importer` | a file's text | a collection (the JSON model) | formats hitop doesn't read |
| `snippet` | a request as sent (secrets as `{{NAME}}`) | text | code in another language |

A plugin can't send requests, read or write files, see the environment, secrets, other requests or the
library, keep state between calls, or change anything except through its answer, which hitop checks and
shows like its own data.

## Where it runs

In hitop-core's JavaScript engine (boa), the one that runs request scripts:

- web app, Chrome and Firefox extensions: boa compiled to WebAssembly in its own worker (`platform/wasm-plugins.js`,
  `plugins-worker.js`; its own wasm instance, so its engine limits aren't the scripts worker's), one call at a time. The worker
  itself is same-origin and has `fetch`, IndexedDB and `postMessage`; the boundary is boa: the plugin's code runs in the
  interpreter, which has no host functions and imports nothing from the worker. Plugins are kept in IndexedDB (`hitop-plugins`);
- desktop and CLI: a child process (`hitop plugin-run`; the desktop app's own executable with `--hitop-plugin-run`, as the sync server's `monitor-run`): input on stdin, answer on
  stdout, killed at the time limit, memory capped (`setrlimit` on Linux and macOS; a Job object on Windows), its own
  engine limits. Not a thread: boa can't be interrupted, and a string or regex blow-up isn't a loop the loop limit
  counts; nor `scripting::set_limits`, which is process-wide and would limit people's own scripts too;
- iOS, Android: not in the first version.

Each call gets a fresh engine with only the call's input, a loop limit, a call-depth limit and a time limit
(5 s; the worker or the child process is ended when it is passed). Answers are size-limited (1 MB) and validated before use.

## Assets

1. Secrets: environment and global secret values, keychain entries, OAuth tokens.
2. Session and device keys (sign-in, end-to-end encrypted collections).
3. Collections, environments, history, response bodies.
4. The person's machine: files, network position (localhost, intranet), CPU.
5. What hitop shows and sends: a request must be what the person sees.

## Threats and answers

| # | Threat | Answer |
|---|---|---|
| T1 | A malicious plugin reads secrets or keys | It never gets them: no host functions; the engine has only the call's input. `value` gets the arguments as written, after variables are filled in, so a secret reaches a plugin only where the person passes it to one (shown as a warning in the editor). |
| T2 | It sends data out | No network or file APIs in the engine. Its answer is the only output: a `value` answer goes into the request the person sends, and the answers that were sent are shown with the response (part 3 decision 1). Viewer answers are shown as text (escaped, never HTML); importer answers are a collection the person reviews before it's added. |
| T3 | It hides data in its answer (a request that leaks a secret) | Only via T1's case (the person passed the secret to it), which the editor warns about before sending. Every answer that went out is shown with the response, masked when it came from a secret. |
| T4 | It runs forever or uses all memory | Time limit enforced from outside: the worker or the child process is ended. Memory: the child's cap; in the browser the worker's own heap (a tab can still grow until the 5 s limit). |
| T5 | It escapes the engine (a boa bug) | Web and extensions: an escape from boa into the worker's JavaScript would reach what a same-origin worker has (`fetch`, IndexedDB, the app's origin), but not the page's localStorage, DOM or memory; the worker is ended after 5 s or a trap. Desktop and CLI: a child process with only stdin and stdout, no secrets in its environment (the environment is cleared), memory capped; it still runs as the person, so a full escape there could reach their files (residual risk). |
| T6 | A plugin arrives without the person choosing it (in a shared or imported collection, through sync) | Never: plugins are installed from a file by the person, per device, and are not part of collections, sync, the library or exports. A collection that uses `${plugin.x()}` without it installed sends nothing and says which plugin is missing. |
| T7 | A plugin changes after install (an update replaces it) | No auto-update: a new version is a new install, shown with what changed in its manifest. Its SHA-256 is kept and checked before each use. |
| T8 | A plugin pretends to be another (name clash) | Names are unique per device; installing a second with the same name asks to replace the first. |
| T9 | A viewer's answer injects markup | Rendered as text or a table of text, never HTML. |
| T10 | An importer answer breaks hitop (huge, deep, wrong types) | Size and depth limits; parsed by the same code as a HITOP file; shown in the import report. |
| T11 | A `value` plugin turns a secret into something the masker doesn't know (base64, an HMAC), which then shows in history, reports or MCP output | An answer computed from an argument that held a secret value is added to the masker as a secret for that send or run. |
| T12 | A client without plugins sends `${plugin.x()}` as text (released CLI, iOS, monitors, MCP) | The syntax is spec section 5c (format v1 is additive; compat case `plugins`). Checked 2026-11-26: released interpolators already refuse an unknown `${...}` ("variable 'plugin.x()' is not defined"), except the web app's Postman-script mode, which sent it as text, and a variable whose value holds a call (one pass, sent as text everywhere). Since part 1, core, the web app (both modes) and HitopKit refuse both cases and name the plugin; hitop-core refuses when no plugin host is installed, which covers MCP without one, monitors, the Android app and SyncFFI. |

## Part 3 decisions (written before its code)

1. **Answers are shown after the send, not before.** A preview that called the plugin and a send that called it again would show
   different answers for any plugin that isn't pure (a nonce, a timestamp, a signature over the time), so "shown before sending" would
   be false. Instead the response shows each call and the answer that was sent ("Plugins: `b64(cred)` → ••••"), from the same answers
   the send used; masked when computed from a secret (T11); memory only, never in history, exports or sync. Before sending, the editor
   warns where a secret is passed to a plugin (T1).
2. **The T1 warning uses the same test as masking.** A call whose argument is a secret variable by name (environment or globals) or
   holds a secret value (`fromSecret`, the T11 test) is named in a note above the request, as missing secrets are.
3. **Viewer input is masked, its output never stored.** A viewer gets the response with secret values already replaced by `••••` (the
   body the person sees), so it can't re-encode an echoed secret past the masker; its answer is shown, never kept in history, exports,
   reports or sync.
4. **Snippet input never holds a secret value.** The `snippet` hook gets the request as the Code dialog builds it, secrets as `{{NAME}}`;
   `--show-secrets` is refused with a plugin language.
5. **Input size.** A call's input (plugin text plus arguments, as JSON) is at most 8 MB, checked by the parent before the child starts
   (a clear refusal naming the plugin), and the child reads at most that much; a viewer gets at most 1 MB of body.
6. **One validator.** Hook answers are checked by pure hitop-core code (`plugins.rs`; also in hitop-wasm): `value` and `snippet` =
   text up to 1 MB; `viewer` = text, or `{columns, rows}` with at most 100 columns and 10 000 rows, every cell made text; `importer` =
   a collection read by the HITOP import code, with its size and depth limits, then the import report.
7. **Desktop child, tested.** CI runs the built desktop binary as its own plugin child (`desktop/scripts/plugin-child-check.sh`) on
   Linux and macOS: one answer, and a plugin that grows without end is stopped by the child's own limits.

## Left out on purpose

- No network, file or secret access, ever, in this design; a plugin that needs them is a different feature
  with its own threat model.
- No marketplace or remote install: a file, chosen by the person.
- No plugins on iOS and Android in the first version (store rules on code download, and no sandbox for boa).
- No plugin hooks in hosted runs (C8): plugins stay on the person's device.

## Residual risks

- Desktop and CLI: an escape from boa in the child process runs as the person (files). Boa is pure Rust with little
  `unsafe`; request scripts there have the same exposure today, in process.
- Browser: boa has no heap limit; a plugin can make a tab use a lot of memory for up to 5 s. An escape from boa there runs as
  the app's origin in a worker (T5).
- Web app, Postman-script mode: a plugin call's arguments are read before the pre-request script runs, so an argument the script
  sets is reported as missing (the CLI reads them after the script).
- Released clients refuse a plugin call as an unknown variable (T12), except a call in a variable's value and the web app's Postman-script
  mode before part 1, which send it as text.
- Mac App Store and Microsoft Store builds: the child is the app's own executable started again inside the store sandbox (MAS: it
  inherits the app's sandbox); not yet tried on either store build (owner checklist).
- Windows: the child has no memory cap yet (a Job object); the 5 s limit still ends it, and the engine's loop and depth limits apply.
- macOS: the cap is `RLIMIT_DATA`, which macOS enforces loosely; the 5 s limit is the stop that always holds.
- A person can still pass a secret to a `value` plugin; hitop warns but doesn't stop it.
- A plugin answer is seen only after it was sent (part 3 decision 1); the T1 warning is what stands before the send.
- Web app: a plugin call in a WebSocket message sent from the log (not the connect URL or headers) is refused.

## Tests the code must have

- A plugin that reads `localStorage`, `fetch`, `document`, `require`, `process`, `globalThis` finds nothing (each surface).
- Endless loop, deep recursion, a huge answer: stopped with a message, the app stays usable.
- A collection using a plugin that isn't installed: not sent, the message names it; plugins never in exports, sync or the library.
- A changed plugin file (hash) is refused until installed again.
- A viewer answer with `<script>` shows as text.
- Desktop and CLI: `"a".repeat(1e9)` and a catastrophic regex are stopped at the time limit; the child sees no environment variables.
- A value computed from a secret (base64 of it) is masked in history, reports and MCP output.
- Every interpolator refuses a request with a plugin it can't run, and names it.
