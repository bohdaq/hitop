# Free App Store apps (ST1, owner steps)

Owner decision 2026-11-17: the Mac App Store app becomes free (it costs $4.99 today) and so does the
iOS app. Paid HITOP Cloud plans (Team, Business) work in the store app exactly as on the web, but
the app sells nothing: like Slack or Notion, people sign in to an account that already has a plan
(App Store Review Guidelines 3.1.1 and 3.1.3). The Microsoft Store build follows the same rule.
GitHub Release builds and the web app keep everything as it is.

What the code does (docs/archive/HITOP_NEXT_UI.md ST1): a plain `tauri build` (no Cargo feature `github`) is a
store build. Its web app shows the plan's name and usage meters and neutral limit messages, and no
price, no Pay/Choose/Upgrade/trial/seat button, no link to pricing, checkout or the billing portal,
no "see plans", and no word about buying anywhere else. It also takes the "See or change the plan
at …" link out of the sync server's limit messages. The GitHub Release workflow builds with
`--features telemetry,updater,github` and keeps today's screens.

## 1. Price: Free (App Store Connect)

For both apps, in App Store Connect → the app → **Pricing and Availability** → Price Schedule →
change the base price to **Free (0.00)**, starting today. People who bought the app keep it, and
their updates continue as before.

- macOS: **HiTop API Client** (id6785294166), today $4.99
- iOS: **HiTop HTTP Client** (id6785326628), today $2.99. As far as I know a price change needs no new build (unverified). The
  app gets its successor as version 2.0.0 (HITOP for iOS 26, free, store mode): build, listing,
  review notes and privacy answers are in `ios/APP_STORE.md`.

## 2. Build the Mac App Store version from main (on the Mac)

The Mac App Store build is the plain `tauri build` with `desktop/src-tauri/tauri.conf.json`, which
already names your "3rd Party Mac Developer Application" identity and `Entitlements.mas.plist`.
**Never add `--features`**: no `telemetry`, `updater` or `github`, so the build has no usage ping,
no self-update and is in store mode. The web app is built first by `beforeBuildCommand`. It needs
the wasm32 target and wasm-bindgen-cli 0.2.126.

One-time setup on the Mac: rustup's Rust (not Homebrew's), plus

```sh
rustup target add wasm32-unknown-unknown aarch64-apple-darwin x86_64-apple-darwin
cargo install wasm-bindgen-cli --version 0.2.126 --locked
```

and the Mac App Store provisioning profile for `tech.rws8.hitop` (developer.apple.com → Profiles →
Mac App Store Connect, Mac App Distribution certificate; re-save it there when it shows Invalid, e.g.
after the App ID gains a capability). Then, every release:

```sh
cd ~/git/hitop && git pull && (cd webapp && npm ci) && (cd desktop && npm ci)
desktop/scripts/build-mas.sh 1.3.2      # the build number: higher than every earlier upload
```

It builds a universal (Apple silicon + Intel) store-mode app into `~/hitop-mas-target`, checks the
binary, puts the profile in, clears the quarantine flag, signs with `Entitlements.mas.plist` (which
carries the application identifier TestFlight wants) and writes `desktop/HITOP.pkg`. Drag that into
**Transporter** and Deliver. Lessons from the first run (2026-11-18): Homebrew's rustc and an editor's
rust-analyzer in `src-tauri/target` break proc macros (hence the own target dir); `strip = true`
stripped proc macros on that Mac (fixed: `[profile.release.build-override] strip = false`); a failed
upload still uses up its build number.

Before you submit, open the build once and sign in to an account with a Team plan. On the
Organization page the section is now **Plan & usage**: the plan's name, seats and meters, nothing to
buy. A lapsed plan reads "This organization's plan has ended".

## 3. Listing text

- **Name**: rename to **HITOP** if you want (fallback "HITOP API Client"). The window title is
  already HITOP. A name change applies with the version you submit.
- **Subtitle** (≤ 30): `API client for terminal and CI`
- **Promotional text**: `The API client your terminal, your CI and your AI agent can all drive.`
- **Description** (it says the app is free; no prices, no plan names you have to buy, no mention of
  buying on the web):

> HITOP is a free API client for HTTP, GraphQL, WebSocket and gRPC. It's the same tool as the
> hitop command line and the HITOP GitHub Action, so the requests you build here run unchanged in
> your terminal, in CI and from your AI agent.
>
> • Send HTTP and GraphQL requests, open WebSocket connections and call gRPC services.
> • Keep collections as plain YAML folders in your own repository, with a built-in Git panel.
> • Environments and secrets: secret values stay in your Mac's keychain.
> • Pre- and post-request scripts in JavaScript, including Postman-style tests.
> • Import Postman collections, OpenAPI specs and curl commands.
> • Compare responses, keep examples and copy any request as code.
>
> HITOP is free and works without an account. HITOP Cloud sync is available when you sign in with
> an account: your collections and environments on all your devices, and shared collections with
> your team.

- **What's New**: `HITOP is now free. Sign in to use HITOP Cloud sync with your account.`
- **Keywords**: unchanged, or `api,http,rest,graphql,grpc,websocket,curl,postman,openapi,cli`.

## 4. App Review notes

Paste into App Review Information → Notes:

> HITOP is free. Creating an account is free (Sign in → Continue with GitHub or Google). Everything
> in the app works without an account. With one, HITOP Cloud syncs collections. Paid HITOP Cloud
> plans for teams are not sold in the app: there is no purchase, price or link to buy in this
> build. People who already have a plan on their account can use it after signing in. Usage
> meters show the plan's limits.

Don't name a guideline clause in the note unless you have checked its current wording. The rule
followed here is 3.1.1/3.1.3: no purchase and no call to action for buying outside the app.

If the reviewer asks for a demo account: make one on HITOP Cloud (a GitHub or Google account made
for review, signed in once in the app), put its sign-in details under "Sign-in required", and tick
it. A Free account is enough to see every screen. To show a team plan, ask me to put that account's
organization on Team with `hitop-sync-server set-plan <org> team <seats>`.

## 5. Microsoft Store

Same rule. Set the price to **Free** in Partner Center (launch/microsoft-store.md, section 6). The
MSIX from `release-msstore.yml` is a store build (no `--features`), and its exe is checked with
`check-store-binary.sh store`.
