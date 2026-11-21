# HITOP for iOS 26 on the App Store (I6, owner steps)

HITOP for iOS 26 (`ios/HITOP`, plan `ios/HITOP_IOS.md`) ships as version **2.0.0** of the live app
**HiTop HTTP Client** (id6785326628, bundle id `hitop.rws8.tech.HiTop`, team XD7JHZ5DB3). It is
free and sells nothing: the store-mode rule of `launch/app-store-free.md` (no price, no plan to buy,
no "buy on the web"). CI (`.github/workflows/ios.yml`) builds and tests it on every push, makes the
store screenshots (artifact `app-store-screenshots`) and runs the build script below up to an
unsigned Release archive (job `archive`). Signing, upload and App Store Connect are on your Mac.

## Before you start: decide these

1. **Export compliance: encryption = true.** I changed `ITSAppUsesNonExemptEncryption` in
   `ios/HITOP/App/Info.plist` from false to **true** in I6, because the app ships its own
   cryptography (section 9). If you know the Mac filing covers this as exempt and want the old
   answer back, revert that one line (`<true/>` → `<false/>`) before building.
2. **Account deletion: built (branch `account-delete`).** Account → **Delete account** (at the
   bottom of the Account screen; the same in the web and desktop apps) deletes the HITOP Cloud
   account at once (`DELETE /v1/me`, sync-server `account.rs`) and revokes its Sign in with Apple
   token. Before you submit, the sync server with that endpoint (schema 19) must be deployed to
   `sync.hitop.rws8.tech`: an older server answers 404 and the app shows that error.
3. **Privacy policy: rewritten (same branch).** `site/src/pages/privacy.js` now describes the HITOP
   Cloud account (what it keeps, what it doesn't, the personal library the server can read, shared
   collections end to end encrypted), how to delete it in the app and what stays afterwards. Deploy
   the site before you submit (the App Privacy answers in section 8 match it). The page says copies
   of the database are deleted at 35 days: off-site backups aren't on yet, but the VM keeps the
   copies made at upgrades (`sync.db.bak-*`, cutover snapshots) with no expiry. Delete those older
   than 35 days and keep doing so (a cron line, or the backup timer's own pruning once it runs)
   before the page goes live, or change that sentence.
4. **France.** The Mac app went out with France excluded until the ANSSI declaration is done. The
   iOS app uses the same cryptography (section 9). Either exclude France for 2.0.0 too (people in France
   who have the old app keep it, but nobody there can download it any more), or wait for ANSSI.
5. **The Explain screenshots** (iPhone 06, iPad 02): CI simulators have no Apple Intelligence model,
   so the explanation and the four suggested checks there are written in advance
   (`FakeAssistant.storeExplanation`, launch argument `-UITestStoreScreens`): facts of the seeded
   Create order answer only, and the checks are validated against it like the model's. They show what
   the feature does, but they are not the model's own words. If you'd rather show real output, take
   those two by hand on a device with Apple Intelligence and a demo API of your own (`-UITestSeed`
   always uses the stand-in), or leave them out (the minimum is one screenshot per device size).

## 1. One-time setup on the Mac

- **Xcode 26** from the App Store (`xcode-select -p` points at it), signed in to the team's Apple ID
  in Xcode → Settings → Accounts.
- **Rust from rustup** (not Homebrew's; same as the Mac App Store build). The script adds the two iOS
  targets itself (`aarch64-apple-ios`, `aarch64-apple-ios-sim`).
- **XcodeGen**: `brew install xcodegen`.
- **Signing**: automatic. With `-allowProvisioningUpdates` Xcode makes the Apple Distribution
  certificate (if there is none) and the App Store profiles for `hitop.rws8.tech.HiTop` and the
  widget extension `hitop.rws8.tech.HiTop.Widgets`. The app group `group.hitop.rws8.tech.HiTop` was
  registered on 2026-11-19; App Groups must be on for both ids (developer.apple.com → Identifiers).

## 2. Build and upload

```sh
cd ~/git/hitop && git pull
ios/HITOP/scripts/build-appstore.sh 2            # → ios/HITOP/build/export/HITOP.ipa, then Transporter → Deliver
ios/HITOP/scripts/build-appstore.sh 2 --upload   # or: straight to App Store Connect
```

The number is the build (CFBundleVersion): higher than every earlier upload of 2.0.0, failed ones
included (2, 3, …; the live 1.0 was build 1). The script builds the Rust library
(`scripts/build-sync-ffi.sh xcframework`, a few minutes), generates the project, archives Release for
a generic iOS device, checks version 2.0.0 and the build number in the app and the widget extension,
iOS 26.0 minimum, arm64, the store-mode check (`scripts/check-store-app.sh`: no StoreKit, no purchase
phrases; a backstop, the UI tests are the real guard), that both targets are signed with the app
group, then exports with `method app-store-connect` (Xcode doesn't change the build number). Logs:
`ios/HITOP/build/archive.log`, `export.log`. When processing is done the build shows under TestFlight;
install it there once before submitting (section 11).

## 3. App Store Connect: the new version

App Store Connect → HiTop HTTP Client → iOS App → **+ Version**: `2.0.0`. Under Build, pick the
uploaded build. Export compliance questions come up for it (section 9).

## 4. Price: Free

Pricing and Availability → Price Schedule → base price **Free (0.00)**, starting today, as in
`launch/app-store-free.md` section 1 (the app costs $2.99 today). People who bought it keep it. In
Availability, France as decided above.

## 5. Name and listing text

- **Name**: `HITOP` (fallback `HITOP API Client` if the name is taken). It applies with 2.0.0.
- **Subtitle** (≤ 30): `HTTP, GraphQL, gRPC, WebSocket` (30)
- **Promotional text**: `The API client your terminal, your CI and your AI agent can all drive.`
- **Description**:

> HITOP is a free API client for iPhone and iPad. Send HTTP and GraphQL requests, open WebSocket
> connections, follow Server-Sent Events and call gRPC services, then read the answer the way you
> want: pretty JSON with folding, headers, cookies, timing and test results.
>
> • Collections with folders, environments, variables and globals. Secret values stay in the Keychain.
> • Tests and pre-request scripts in JavaScript, including Postman-style tests.
> • Run a whole collection and read its report. Follow a run in a Live Activity and the Dynamic Island.
> • Widgets for your last run, failing collections and pinned requests, with a Send button.
> • Siri and Shortcuts: run a collection or send a request by voice or from an automation.
> • Apple Intelligence, on device: explain a response, suggest a test, turn a sentence or a curl
>   command into a request. Nothing leaves your device.
> • iPad: three columns, keyboard shortcuts and pointer support.
>
> HITOP works without an account. Sign in to HITOP Cloud (free to create, with Apple, Google or
> GitHub) to keep your collections and environments on all your devices and to share collections
> with your team, end-to-end encrypted.
>
> HITOP is also a command line tool and a GitHub Action: the collections you build here run unchanged
> in your terminal, in CI and from your AI agent.

- **Keywords** (≤ 100): `api,http,rest,graphql,grpc,websocket,sse,curl,openapi,client,request,test,json,developer` (88).
  No other app's name (Postman is in the description as a script style, not as a keyword).
- **What's New**:

> HITOP 2.0 is a new app, and it's free. Collections, environments, tests and collection runs;
> HTTP, GraphQL, WebSocket, Server-Sent Events and gRPC; widgets, a Live Activity and Siri; Apple
> Intelligence on device; HITOP Cloud sync with your account. Your requests and history from the
> earlier version are kept. HITOP 2.0 needs iOS 26; on iOS 17 to 25 you keep the earlier version.

- Category: Developer Tools (unchanged). Support URL `https://hitop.rws8.tech`, privacy policy URL
  `https://hitop.rws8.tech/privacy.html`.

## 6. Screenshots

Download the artifact `app-store-screenshots` from the newest green iOS run (Actions → iOS → the run
→ Artifacts). Sizes are checked in CI; no alpha. Upload in file-name order:

- **iPhone 6.9" Display** (1320 × 2868, portrait), folder `iphone/`: 01-collections, 02-request,
  03-response, 04-run-result, 05-websocket, 06-explain. App Store Connect scales them down for the
  smaller iPhone sizes.
- **iPad 13" Display** (2752 × 2064, landscape), folder `ipad/`: 01-request (three columns with the
  response), 02-explain, 03-run-result, 04-graphql.

Delete the old app's screenshots in those slots first. Seeded demo data only (`*.shop.example`
hosts, no secret), store mode, status bar 9:41.

## 7. App Review notes

App Review Information → Notes:

> HITOP is free. Everything in the app works without an account. A new install starts with an empty
> "My requests" collection: tap +, enter a URL such as https://httpbin.org/get and tap Send.
> Signing in is optional (Account: the H pill at the top left of Collections; on iPad the person
> icon in the sidebar). Creating a HITOP Cloud account is free: Continue with Apple, Google or
> GitHub. With an account HITOP Cloud syncs collections and environments. Paid HITOP Cloud plans for
> teams are not sold in the app: there is no purchase, price or link to buy in this build. People who
> already have a plan on their account can use it after signing in; usage meters show the plan's
> limits.
>
> Also in this version: widgets (last run, failing collections, pinned requests), a Live Activity
> while a collection runs (Runs tab → Run a collection, or long-press a collection), Siri and
> Shortcuts ("Run <collection> in HITOP"), and Apple Intelligence on device (send a request, then
> Explain in the response; + → New request from text or curl). Apple Intelligence features show only
> on devices where it is available and turned on.
>
> Account deletion: signed in, open Account and tap **Delete account** at the bottom, then Delete
> account in the sheet. The HITOP Cloud account is deleted at once, with its synced library, devices
> and memberships (for Sign in with Apple, the Apple token is revoked); collections saved on the
> device stay.

Check those steps once on a fresh TestFlight install. Don't name a guideline clause in the note
unless you've checked its current wording.

Demo account: as `launch/app-store-free.md` section 4. Every screen works signed out; if the reviewer
asks, make a HITOP Cloud account for review (a Google or GitHub account made for it, signed in once),
put its details under Sign-in required and tick it. A Free account shows every screen.

## 8. App Privacy

App Privacy → Data Types. From the code (I2 sign-in and library sync, I2b shared collections, the
sync server's tables):

- **Contact Info → Email Address**: collected, linked to the person, App Functionality. Only when
  someone signs in (the account is the email address; with Apple the relayed address if they hide it;
  the name Apple sends is not kept).
- **Identifiers → User ID**: collected, linked, App Functionality (the account id and device key ids).
- **User Content → Other User Content**: collected, linked, App Functionality: the personal library
  (collections, environments, globals; the server can read it; secret values sealed under the server's
  host key) and its per-request version history. Only when signed in.
- Not collected: requests and responses (sent from the device straight to the address entered, never
  through HITOP's servers), history, Keychain secrets, Apple Intelligence input (on device), shared
  collections (end-to-end encrypted: the server can't read them), usage data, diagnostics, location.
  The device name sent is a fixed "HITOP on iPhone"/"HITOP on iPad", not the device's own name. The
  security log keeps a shortened IP address and the user agent (privacy page); it isn't used for
  location or tracking.
- **Tracking**: No. No third-party SDKs, no ads, no analytics, no usage ping in the iOS app.
- Deletion: people delete the account in the app (Account → Delete account); the privacy page says
  what stays afterwards (audit entries until a year old, database copies up to 35 days).

## 9. Export compliance

`ios/HITOP/App/Info.plist` now says `ITSAppUsesNonExemptEncryption` = **true** (it said false until
I6): besides Apple's TLS, the app ships its own cryptography for shared collections
(`hitop-sync-crypto` through SyncFFI: HPKE key wraps, XChaCha20-Poly1305), the same code as the Mac
app. So App Store Connect asks the export compliance questions for each build. Answer them exactly as
you did for the Mac app 1.3.0 on 2026-11-18, which went with the self-classification report you
emailed to BIS and NSA that day (annual, next by Feb 1): check whether that report names the iOS app
too; if not, include it in the next one. France: as decided at the top. If App Store Connect gives
you an export compliance code after the documents, add it as `ITSEncryptionExportComplianceCode` in
Info.plist so later builds skip the questions.

## 10. Age rating

Answer **None / No** to every content question: the app shows only what the person's own requests
return, has no user-to-user content, no web browser view (no WKWebView: bodies show as text or an
image preview) and no purchases. That gives the lowest rating. If a reviewer counts "send a request to
any URL" as unrestricted web access, the rating goes up; answer as they ask.

## 11. Check on a device, then submit

Install the TestFlight build on an iPhone with iOS 26 (and an iPad if you have one), on top of the old
app with some data:

- The old app's requests and history are there (one-time import).
- Sign in with Apple, Google and GitHub against https://sync.hitop.rws8.tech; sync a collection with
  the web app; the Account screen shows nothing to buy.
- Account → Delete account with a throwaway Sign in with Apple account: signed out, collections kept;
  appleid.apple.com → Sign in with Apple no longer lists HITOP (the token was revoked).
- I3: add the three widgets; tap Send on the pinned-requests widget; run a collection with the Lock
  Screen showing (Live Activity, Dynamic Island); "Run shop-api in HITOP" (or your collection's name)
  with Siri; the Control Center control.
- I5 (on a device with Apple Intelligence): Explain a few real responses (JSON, HTML, an error);
  Suggest a test and run it; New request from text with a sentence; turn Apple Intelligence off in
  Settings and check the reasons; VoiceOver, the largest text size, Increase Contrast, Reduce Motion.

Then Add for Review → Submit. iOS 26 is the minimum: people on iOS 17–25 can't update and keep the
last 1.0 build; App Store Connect handles that by itself (no setting).
