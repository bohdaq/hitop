# Browser extension store listings

Texts for the owner to paste when uploading (agents never upload or tag a release). Targets: **Chrome Web Store**
(item `mmbpagencfjfgeicmeobbmeocdpeokdk`) and **Firefox Add-ons** (addons.mozilla.org/addon/hitop).
Not Edge (owner decision 2026-11-10). Firefox ships only with the WebAssembly script runner
(HITOP Next F1).

Name: **HITOP**. Summary / short description: the tagline (brand/BRAND.md), as in the manifests
(every field and image for both stores: "Listing" below):

> The API client your terminal, your CI and your AI agent can all drive.

Build: `./build-chrome-store.sh` → `hitop-chrome-store.zip`; `./build-extension.sh` →
`hitop-firefox-extension.zip`. Manifest version: `extensionVersion` in `webapp/package.json`
(5.0.0 at the A8 switch), written into both manifests by `webapp/scripts/package-extension.mjs`.
Releases go through CI: see "Releasing an update" at the end.

## Listing

Everything a store page shows, ready to paste: text below, images in `launch/store-assets/`.
**Store listing text and images are not part of the release workflow.** `release-extensions.yml`
uploads only the zip (Chrome) or the zip plus source (AMO); it never touches the listing. Set the
fields below once in each dashboard and change them there by hand. The one exception is the
Chrome title and summary, which come from the manifest (see Title).

### Assets (`launch/store-assets/`)

| File | Size | Store | Field |
| --- | --- | --- | --- |
| `screenshot-1-request.png` | 1280 × 800 | Chrome, AMO | Screenshot 1 |
| `screenshot-2-collections.png` | 1280 × 800 | Chrome, AMO | Screenshot 2 |
| `screenshot-3-environments.png` | 1280 × 800 | Chrome, AMO | Screenshot 3 |
| `screenshot-4-run-report.png` | 1280 × 800 | Chrome, AMO | Screenshot 4 |
| `screenshot-5-graphql.png` | 1280 × 800 | Chrome, AMO | Screenshot 5 |
| `promo-small-440x280.png` | 440 × 280 | Chrome | Small promo tile (required) |
| `promo-marquee-1400x560.png` | 1400 × 560 | Chrome | Marquee promo tile (optional) |
| `icon-128.png` | 128 × 128 | Chrome, AMO | Store icon (copy of `brand/png/logo-128.png`) |

All PNG, 24-bit, no alpha in the screenshots and tiles. Remake them with
`webapp/tests/store-shots.cjs` (its header says how: build, copy `dist/` to `deploy/out/www/app`,
run `hitop-server`, then `BASE=http://localhost:7889 node tests/store-shots.cjs`); `TILES=1` remakes
only the tiles and the icon from `promo-tiles.html`. The screenshots use the seeded "shop-api"
workspace, a fixed clock and a mocked proxy, so no real account, address or API appears.

Screenshot captions (AMO shows them under each image; Chrome has no caption field):

1. Send a request: response, timing and the four checks its script ran, all passed.
2. Collections with folders, as they run and as `hitop run` sees them.
3. Environments: hover a ${variable} to see its value and where it comes from.
4. Run a whole collection: per-request results, checks, p95, and the same run as a CLI command.
5. GraphQL with variables, next to REST, WebSocket and SSE requests.

### Title

The manifest `name` (both manifests, `webapp/scripts/package-extension.mjs`) is now **HITOP**.
The Chrome listing still shows the old name (HITOP plus a "HTTP Client" suffix, which brand/check.sh
keeps out of the repository) because that was the manifest name of the
version that is live there; Chrome takes the title from the manifest and has no field for it in
the dashboard, so it becomes "HITOP" once a version built since the rename (any 5.x, such as
`ext-v5.1.0`) passes review and goes live.

Recommendation: keep **HITOP** in both stores. brand/BRAND.md rules out suffixes in the name
(suffixed names are gone), and the words people search for ("API client",
"HTTP", "REST", "Postman") are already in the summary and the description, which both stores
search. On AMO the name is a dashboard field (max 50 characters): set it to `HITOP` too.

### Keywords: no keyword box

- **Chrome Web Store has no keyword or tag field.** Search uses the title, the summary and the
  description, so the words people type ("API client", "REST client", "HTTP request", "Postman
  alternative", "GraphQL", "WebSocket") should appear in them, in sentences; Google's policy forbids
  keyword stuffing and lists of unrelated terms.
- **AMO** has categories and tags (below); search also uses name, summary and description.

### Chrome Web Store

- **Title:** HITOP (from the manifest; nothing to type).
- **Summary (≤ 132 characters):** from the manifest `description`, the tagline, 70 characters:

  > The API client your terminal, your CI and your AI agent can all drive.

  The dashboard has no summary field either. A longer summary would need a new manifest
  `description`, which BRAND.md fixes to the tagline, so leave it.
- **Description (≤ 16,000 characters):** the text under "Description (both stores)" below.
- **Category:** Developer Tools.
- **Language:** English.
- **Homepage URL:** https://hitop.rws8.tech/
- **Support URL:** https://hitop.rws8.tech/guides/ (hitop.rws8.tech has no support or contact page;
  the guides are the help pages). Alternative: https://github.com/bohdaq/hitop/issues, if issues
  are where you want questions to go.
- **Privacy policy URL:** https://hitop.rws8.tech/privacy.html
- **Official URL:** hitop.rws8.tech, if it is verified in Google Search Console under the
  publisher's Google account (it is in Search Console for SEO; the dashboard lists only verified sites).

### Firefox Add-ons (AMO)

- **Name (≤ 50):** HITOP
- **Add-on URL:** https://addons.mozilla.org/firefox/addon/hitop/ (slug `hitop`)
- **Summary (≤ 250 characters):** 227 characters:

  > The API client your terminal, your CI and your AI agent can all drive. Send REST, GraphQL, WebSocket and SSE requests, keep them in collections with environments, and run them with scripts, tests and reports. No account needed.

- **Description:** the text under "Description (both stores)" below. AMO's editor accepts plain
  text with line breaks (and a few HTML tags); paste it as is.
- **Categories:** Web Development, only (AMO lets you pick more than one, but none of its other
  Firefox categories fits, and "Other" adds nothing).
- **Tags (up to 10):** `api`, `api client`, `rest`, `http`, `graphql`, `websocket`, `testing`,
  `developer tools`, `json`, `postman`. If the dashboard offers tags only from its own list
  (rather than free text), take the ones there that match these and leave the rest.
- **Support email:** the address the site already gives for contact (pricing and privacy pages).
- **Support website:** https://hitop.rws8.tech/guides/
- **Homepage:** https://hitop.rws8.tech/
- **Privacy policy:** AMO wants the text of a policy, not only a link. Paste:

  > HITOP's privacy policy is at https://hitop.rws8.tech/privacy.html. In short: collections,
  > environments, history and secret values stay in this browser's storage. Requests to public
  > hosts are relayed by hitop.rws8.tech's proxy to be sent; it stores and logs nothing. Requests
  > to localhost and private networks go straight from the browser. With HITOP Cloud (only when
  > you sign in), collections sync to sync.hitop.rws8.tech; shared ones are end-to-end
  > encrypted. No analytics, no ads, no tracking.

- **License:** the repository's license for the extension source (as chosen at the first
  submission; it doesn't change per version).

### Description (both stores)

Plain text, about 3,300 characters, the same in both stores.

```text
HITOP is an API client: send HTTP requests, keep them in collections, and run them with scripts and tests. This extension is the HITOP web app in a browser tab. No account needed: your collections stay in this browser unless you turn on HITOP Cloud.

The same collections run in your terminal and in CI with the hitop command line tool, and your AI agent can drive them through hitop's MCP server.

REQUESTS
• REST with any method, query parameters, headers, JSON, form and file bodies
• GraphQL with variables, and schema completion after one introspection
• WebSocket and Server-Sent Events, with a timestamped log of every message
• Timing for each request (DNS, connect, waiting, download), HTTP version, response diff
• Auth presets: bearer, basic, API key, OAuth 2.0 (with PKCE), AWS Signature v4, digest

COLLECTIONS AND ENVIRONMENTS
• Collections with folders; drag requests and folders to reorder them
• Environments (Dev, Staging, Prod…), collection variables and globals; ${variables} anywhere, hover one to see its value and where it comes from
• Secret values are masked on screen and in history, stay in this browser and are never exported

SCRIPTS AND TESTS
• Scripts before and after each request; Postman's pm.* API works, so imported tests keep running
• Tests on every response, with a pass/fail count next to the status

RUNS AND REPORTS
• Run a whole collection or one folder in order; values set by one request reach the next
• A run report with every request, its checks and the p95 latency, compared with the last run
• Download it as JUnit XML or report.json, and copy the same run as a hitop command for CI

IMPORT AND EXPORT
• Import Postman collections, environments and globals, OpenAPI 3.0/3.1 specs (from a file or a URL) and curl commands
• Keep a collection in sync with its OpenAPI spec without losing your edits
• Export to Postman v2.1 or HITOP's folder format (YAML files that diff well in Git)

MORE
• Command palette (Ctrl/Cmd+K) to find any request, environment or action
• Ask AI with your own Anthropic API key
• Optional HITOP Cloud: sync between your devices, end-to-end encrypted sharing with your team, GitHub sync, mock servers and monitors

HOW YOUR REQUESTS TRAVEL
Browsers block many cross-origin requests (CORS). So requests to public hosts go through HITOP's request proxy at hitop.rws8.tech, which sends them and returns the answer: it stores and logs nothing, but the request's headers, tokens and body do pass through it. Requests to localhost and private networks go straight from your browser and never reach the proxy. That is why the extension asks for access to all sites: the requests you send go to hosts you choose. No analytics, no ads, no remote code.

Privacy policy: https://hitop.rws8.tech/privacy.html

WHAT THE EXTENSION DOESN'T DO
Use the free desktop app (macOS, Windows, Linux) or the hitop CLI for these:
• Collection folders on disk in Firefox (Chrome can open a folder; Firefox has no folder access) and the Git panel
• Secret-manager references (Vault, AWS, Azure, 1Password)
• gRPC to private networks, and WebSocket handshake headers to localhost
• Store updates arrive after each store's review, so the extension can be a few days behind the web app at hitop.rws8.tech/app/

Desktop app, CLI and guides: https://hitop.rws8.tech/
```

### Step by step: where each field goes

Chrome Web Store, https://chrome.google.com/webstore/devconsole → the HITOP item:

1. **Store listing** tab:
   - *Description*: paste the description block.
   - *Category*: Developer Tools. *Language*: English.
   - *Graphic assets*: Store icon ← `icon-128.png`; Screenshots ← `screenshot-1…5` in order
     (drag to reorder); Small promo tile ← `promo-small-440x280.png`; Marquee promo tile ←
     `promo-marquee-1400x560.png`. Global promo video: leave empty.
   - *Additional fields*: Official URL (if verified), Homepage URL, Support URL as above.
   - The title and summary are shown read-only from the package: nothing to type.
2. **Privacy** tab: Privacy policy URL; single purpose, permission justification
   (`<all_urls>`), remote code "No" and data use answers from "Permissions" and "Data use" below.
3. **Save draft**. Listing edits are reviewed on their own: "Submit for review" sends them even
   without a new package. They go live after that review.

Firefox Add-ons, https://addons.mozilla.org/developers/ → HITOP → **Edit Product Page**:

1. *Describe Add-on*: Name, Add-on URL, Summary, Description; Categories (Web Development);
   Tags. Save.
2. *Additional Details* (same page): Support email, Support website, Homepage; Save.
3. *Media*: Icon ← `icon-128.png` (or keep the manifest's icon); Screenshots ← the five PNGs, each
   with its caption from the list above. Save.
4. *Technical Details* / *Privacy Policy* (left menu, "Manage Privacy Policy" on older layouts):
   paste the policy text.
5. AMO applies product page edits at once (some may be checked by a reviewer afterwards); they are
   not tied to a version.

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

- Chrome "Data usage" form: Authentication information, Website content (both relayed by the
  proxy) and Personally identifiable information (the account email, HITOP Cloud sign-in only).

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
2026-11-17): `{"required": ["authenticationInfo", "websiteContent"], "optional": ["personallyIdentifyingInfo"]}` (the account email, signed-in people only), set in
`webapp/scripts/package-extension.mjs`. Requests to public hosts go through the hitop.rws8.tech
proxy, so their headers, tokens and bodies leave the device (relayed, never stored); Firefox shows
this at install.
