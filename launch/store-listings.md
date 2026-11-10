# Browser extension store listings

Texts for the owner to paste when uploading (agents never upload). Targets: **Chrome Web Store**
(item `mmbpagencfjfgeicmeobbmeocdpeokdk`) and **Firefox Add-ons** (addons.mozilla.org/addon/hitop).
Not Edge (owner decision 2026-11-10). Firefox ships only with the WebAssembly script runner
(HITOP Next F1).

Name: **HITOP**. Summary / short description: the tagline (brand/BRAND.md), as in the manifests:

> The API client your terminal, your CI and your AI agent can all drive.

Build: `./build-chrome-store.sh` → `hitop-chrome-store.zip`; `./build-extension.sh` →
`hitop-firefox-extension.zip`. Manifest version 5.0.0 (`webapp/scripts/package-extension.mjs`).

## Description (both stores)

HITOP sends HTTP requests, keeps them in collections and runs them in order, with scripts before
and after each request (Postman's pm.* works) and tests on every response. This extension is the
HITOP web app in a browser tab: no account needed, and your collections stay in this browser
unless you sign in to HITOP Cloud.

What it does:
- REST, GraphQL, WebSocket, Server-Sent Events and gRPC requests
- Collections, environments, globals and secret values (kept in this browser)
- Pre-request and post-request scripts with the pm.* API, tests on every response
- Import from Postman, OpenAPI and curl; export to Postman and HITOP's YAML folder format
- Timing for each request (DNS, connect, waiting, download), response diff, run reports
- The command palette (Ctrl/Cmd+K) and the HITOP agent, with your own Anthropic API key
- Optional HITOP Cloud: sync between your devices, end-to-end encrypted sharing, GitHub sync,
  mock servers and monitors

How requests travel: requests to public hosts go through HITOP's request proxy at
hitop.rws8.tech (that is what gives timing phases, WebSocket handshake headers and gRPC);
requests to localhost and private networks go straight from your browser.

What the extension doesn't do (use the desktop app or the `hitop` CLI for these):
- Secret-manager references (`${secret:vault:…}`, AWS, Azure, 1Password)
- The Git panel for collection folders
- Collection folders on disk in Firefox (Chrome can open a folder; Firefox has no folder access)
- gRPC to private networks, and WebSocket handshake headers to localhost
- Updates reach you only after each store reviews them, so the extension can be a few days
  behind the web app at hitop.rws8.tech/app/

Desktop app, CLI and docs: https://hitop.rws8.tech/

## Permissions (store questionnaires)

- `host_permissions: <all_urls>`: the requests you send go to hosts you choose, including
  localhost and private networks, without CORS getting in the way.
- No other permissions. No analytics, no remote code: every script and the WebAssembly modules
  ship inside the package.

## Data use (store questionnaires)

- Collections, environments, history and secret values stay in the browser's storage.
- Requests to public hosts pass through hitop.rws8.tech's proxy to be sent; it logs nothing
  (privacy policy: https://hitop.rws8.tech/privacy.html).
- With HITOP Cloud turned on (signed in), collections sync to sync.hitop.rws8.tech; shared ones
  are end-to-end encrypted.
