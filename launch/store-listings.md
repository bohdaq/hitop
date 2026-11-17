# Browser extension store listings

Texts for the owner to paste when uploading (agents never upload or tag a release). Targets: **Chrome Web Store**
(item `mmbpagencfjfgeicmeobbmeocdpeokdk`) and **Firefox Add-ons** (addons.mozilla.org/addon/hitop).
Not Edge (owner decision 2026-11-10). Firefox ships only with the WebAssembly script runner
(HITOP Next F1).

Name: **HITOP**. Summary / short description: the tagline (brand/BRAND.md), as in the manifests:

> The API client your terminal, your CI and your AI agent can all drive.

Build: `./build-chrome-store.sh` → `hitop-chrome-store.zip`; `./build-extension.sh` →
`hitop-firefox-extension.zip`. Manifest version: `extensionVersion` in `webapp/package.json`
(5.0.0 at the A8 switch), written into both manifests by `webapp/scripts/package-extension.mjs`.
Releases go through CI: see "Releasing an update" at the end.

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

## Releasing an update

`.github/workflows/release-extensions.yml` builds both packages from a tag and submits them.

1. **Bump.** Raise the version, which every store requires to be higher than the last one:
   `cd webapp && npm pkg set extensionVersion=5.0.1`. Commit and push to main.
2. **Rehearse (optional).** Either push `ext-v5.0.1-rc.1`, which makes a GitHub pre-release with
   the three zips and submits nothing, or run the workflow by hand from the Actions tab: with
   "dry run" ticked it only builds, and the zips are the run's artifacts.
3. **Tag.** `git tag ext-v5.0.1 && git push origin ext-v5.0.1`. The workflow checks that the tag,
   `extensionVersion` and both built manifests agree, runs `web-ext lint`, and attaches these to
   the GitHub Release `ext-v5.0.1`:
   - `hitop-chrome-5.0.1.zip`
   - `hitop-firefox-5.0.1.zip`
   - `hitop-firefox-5.0.1-source.zip`, the source code with BUILD.md
   It then submits both stores:
   - **Chrome Web Store:** it uploads the zip through the Web Store API and publishes it. That
     submits the version for review. It goes live after approval, with the item's dashboard
     settings for visibility and rollout.
   - **Firefox Add-ons:** `web-ext sign --channel listed` uploads the zip and the source
     package. Mozilla requires the source package for code that a bundler (Vite) and Rust/wasm
     produced: reviewers rebuild from BUILD.md and compare. Signing, and the listed update,
     happen after review.
4. **Review.** Review usually takes from a few hours to a few days per store. Firefox users
   keep the old version until the new one is approved. A rejection comes by email from the
   store: fix it, bump again (a version number can't be reused) and tag again. Watch the
   Developer Dashboard (Chrome) and the add-on's "Manage Status & Versions" page (AMO).

A store whose credentials aren't set is skipped with a notice, and the job stays green. Upload
that store's zip from the Release by hand, plus the source zip on AMO. To re-run a submission,
run the workflow by hand on the tag with "dry run" unticked. Uploading the same version twice is
refused, so a version a store already took needs a new version.

Credentials (repository Settings → Secrets and variables → Actions):

| Name | Kind | Where it comes from |
| --- | --- | --- |
| `CWS_CLIENT_ID`, `CWS_CLIENT_SECRET` | secret | A Google Cloud project with the Chrome Web Store API enabled. APIs & Services → Credentials → OAuth client ID, type "Web application", redirect URI `https://developers.google.com/oauthplayground` |
| `CWS_REFRESH_TOKEN` | secret | OAuth Playground (gear → use your own credentials), scope `https://www.googleapis.com/auth/chromewebstore`. Sign in as the Google account that owns the item (2-step verification on), then exchange the code |
| `CWS_PUBLISHER_ID` | variable | Chrome Web Store Developer Dashboard → Publisher → Settings (the API v2 URLs need it) |
| `CWS_EXTENSION_ID` | variable, optional | Defaults to `mmbpagencfjfgeicmeobbmeocdpeokdk` |
| `AMO_JWT_ISSUER`, `AMO_JWT_SECRET` | secret | addons.mozilla.org → Tools → Manage API Keys (developer hub). The JWT issuer (`user:…`) and the JWT secret |

The API can't create items, so the first Chrome version and any visibility change go through the
Dashboard. On AMO the add-on id is the Firefox manifest's gecko id (`hitop@example.com`). If no
add-on with that id is listed yet, the first listed version needs listing metadata: submit it by
hand on AMO first.

Data collection (Firefox `browser_specific_settings.gecko.data_collection_permissions`, owner decision
2026-11-17): `{"required": ["authenticationInfo", "websiteContent"]}`, set in
`webapp/scripts/package-extension.mjs`. Requests to public hosts go through the hitop.rws8.tech
proxy, so their headers, tokens and bodies leave the device (relayed, never stored); Firefox shows
this at install.
