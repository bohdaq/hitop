# HITOP in the Microsoft Store (owner steps)

Windows people get HITOP from the Microsoft Store. The Store signs the MSIX package itself, so no
code-signing certificate is needed. The GitHub `.exe`/`.msi` downloads stay as they are: unsigned, and
they update themselves. The Store build has no usage ping and no updater, because the Store updates it.

How the package is made: `.github/workflows/release-msstore.yml` (CLAUDE.md, Desktop, "Microsoft Store").
Every `desktop-v*` tag builds it, and so does a manual run (Actions → Release Microsoft Store → Run
workflow). The package is a **run artifact**, never a release asset:

- `hitop-msstore-<version>-x64` holds `hitop-<version>-x64-store.msix`. It is unsigned and only Partner
  Center can use it.
- `hitop-msstore-<version>-x64-TEST-ONLY` holds a self-signed test package, its `.cer` and
  `README-TEST-ONLY.txt`. The workflow builds this one while the identity variables (step 3) aren't
  set. **Never upload it.**

Artifacts are kept for 30 days. If an artifact has expired, re-run the workflow on the tag.

## 1. Partner Center account

1. Go to <https://partner.microsoft.com/dashboard/registration> (Windows & Xbox / Apps and games) and
   sign up as an **individual** developer. It's free, and Microsoft verifies your identity with an ID
   document and a selfie.
2. Fill in the publisher display name. People see it in the Store as the developer name (for example
   "Bohdan Tsap").

## 2. Reserve the name

Apps and games → New product → **MSIX or PWA app** → reserve **HITOP**. Don't choose "EXE or MSI app":
that route needs a code-signed installer. If "HITOP" is taken, stop and decide on a name: the package's
DisplayName must be exactly the reserved name.

## 3. Identity → repository variables

Open the app → Product management → **Product identity**, then copy these values into GitHub → Settings →
Secrets and variables → Actions → **Variables** (not secrets):

| Product identity field | Repository variable | Looks like |
| --- | --- | --- |
| Package/Identity/Name | `MSSTORE_IDENTITY_NAME` | `12345BohdanTsap.HITOP` |
| Package/Identity/Publisher | `MSSTORE_PUBLISHER` | `CN=1A2B3C4D-…` (the whole value, `CN=` included) |
| Package/Properties/PublisherDisplayName | `MSSTORE_PUBLISHER_DISPLAY_NAME` | `Bohdan Tsap` |

Set all three or none. With only some of them set, the build fails and names the missing ones. From then
on the workflow builds `…-store.msix` with that identity. Also note the **Store ID** (12 characters, like
`9NXXXXXXXXXX`). The optional automatic upload (step 7) needs it.

## 4. Test before the first submission (recommended)

Use a Windows 10 (2004 or later) or Windows 11 machine or VM.

1. Before step 3, or on a fork without the variables, run the workflow and download the
   `…-TEST-ONLY` artifact.
2. Follow `README-TEST-ONLY.txt`: trust the `.cer` in Local Machine → Trusted People, then run
   `Add-AppxPackage`.
3. Work through the checks at the end of this file, then uninstall the package and remove the
   certificate.

The test package has a different identity (`tech.rws8.hitop.test`), so it never collides with the Store
install.

## 5. Upload the package

1. Run the workflow on the release tag, or let the tag push run it. Download `hitop-msstore-<version>-x64`.
2. Partner Center → the app → **Start your submission** → Packages: drop in `hitop-<version>-x64-store.msix`.
   Partner Center checks it, including the identity, the version (4-part, last part 0) and the
   capabilities.
3. **runFullTrust** is a restricted capability. When Partner Center asks why, answer: "HITOP is a
   Win32 desktop app (Tauri, WebView2). It needs full trust to read and write collection folders that
   the user picks, run the user's own command line tools for secret managers (op, az, aws), and open Git
   repositories."
4. Device families: Windows 10/11 Desktop only. Uncheck Xbox, Holographic, Team.
5. A new version later: bump the desktop version as usual, tag, then upload the new `.msix` in a new
   submission. The Store refuses a version that isn't higher than the published one.

## 6. Listing, properties, age rating

- **Pricing and availability**: **Free** (owner decision 2026-11-17, ST1: the store apps are free and
  sell nothing in the app; launch/app-store-free.md). The store build shows no purchase call to action.
- **Properties**: category **Developer tools** (subcategory none). Privacy policy URL
  `https://hitop.rws8.tech/privacy.html`. Website `https://hitop.rws8.tech`. Support contact
  `https://github.com/bohdaq/hitop/issues`.
  System requirements: leave the hardware boxes empty. "Additional system requirements" can say
  "Microsoft Edge WebView2 Runtime (included in Windows 11 and in updated Windows 10)".
- **Age ratings** (IARC questionnaire): a developer tool with no violence, gambling and so on. People can
  share collections with others they invite, and the app sends HTTP requests to any address the user
  types. Answer the "users can interact or share content" and "unrestricted internet access" questions
  honestly (yes), and expect a 3+/Everyone-class rating with an "Unrestricted internet" notice.
- **Store listing** (English): text from `launch/store-listings.md`, kept true to the desktop app. Field
  limits:
  - Description: up to 10,000 characters, plain text, no URLs.
  - Short description: shown above the description, 270 characters show without a click.
  - Product features: up to 20, at most 200 characters each, no bullets of your own.
  - What's new: leave it empty for the first submission; at most 1,500 characters after that.
- **Screenshots**: desktop screenshots must be **at least 1366 × 768** PNG (up to 3840 × 2160, 50 MB
  each, up to 10). The extension screenshots in `launch/store-assets/` are 1280 × 800, **too small**.
  Make the same set at twice the size: build and serve the web app as `webapp/tests/store-shots.cjs`
  says, then run `SCALE=2 BASE=http://localhost:7889 node tests/store-shots.cjs ../launch/store-assets/msstore`
  in `webapp/`. That gives five 2560 × 1600 PNGs, sharp, not upscaled. They show the web app, which is
  the desktop UI. Desktop-only pages (Git, secret managers) need screenshots taken on Windows if you
  want them.
- **Store logos**: upload `brand/png/logo-300.png` as the "1:1 App tile icon (300 × 300)". Poster and
  box art are for games; skip them.

Then submit. Certification usually takes a few business days.

## 7. Optional: automatic upload to a draft

To have each release tag upload the package for you, register an Azure AD (Entra ID) application in
Partner Center: Account settings → User management → Azure AD applications, role Manager. Then add:

- secrets `MSSTORE_TENANT_ID`, `MSSTORE_CLIENT_ID`, `MSSTORE_CLIENT_SECRET`, `MSSTORE_SELLER_ID`
  (Account settings → Legal info → Seller ID);
- variable `MSSTORE_PRODUCT_ID` (the Store ID from step 3).

Job `submit` then runs Microsoft's `msstore publish … --noCommit` on every non-rc tag, and on a manual
run with "submit" ticked. The package goes into a **draft** submission; you review it and press Submit
in Partner Center. Without the secrets the job prints a notice and does nothing. The submission API
works only after the first submission was made by hand (step 5). `msstore publish` replaces a pending
draft with a fresh one made from the last published submission, so don't leave half-done listing
edits in a draft when you push a tag. Nobody has run this job yet, so watch its first run.

## Manual checks on Windows (agents can't run Windows)

Install the test package or the Store build, then check:

1. **Starts, looks right**: the HITOP icon in Start, the taskbar (no plate behind it), the title bar and
   Alt+Tab, at 100 % and 200 % display scaling. Settings has no Updates section and no usage ping
   section, there's no update bar, and the first run asks no usage ping question.
2. **App data**: create a collection, environment and history in app storage. Quit and restart: it's all
   there. MSIX keeps AppData in a light container (Windows 10 1903 and later): a file the Store app
   *creates* under `%APPDATA%` or `%LOCALAPPDATA%` goes to its private copy, which only it sees and which
   uninstalling deletes; a file that *already existed* (made by the CLI or the GitHub build) is read and
   written in place, so it stays shared. Check both on a machine that has the CLI:
   - Fresh machine: install the Store app first, save a collection, then run `hitop requests` with the
     CLI. Expected: the CLI doesn't see it (`collections.json` was created privately).
   - CLI or GitHub build used first: the Store app shows the same collections, cookies and globals, and
     edits made in one show in the other.
   - Run the GitHub build and the Store app at the same time: both start, and neither breaks the
     other's files.
3. **Folder collections**: open a folder in Documents, on another drive and on a network share; edit,
   save, restart, reopen. Also check a folder the CLI uses (`hitop run <folder>` sees the app's edits).
4. **Keychain**: save a secret variable and a Git token, restart, and check they're still there. They
   live in Windows Credential Manager (the `hitop` service's entries), shared with the CLI and the GitHub build.
5. **Git panel**: on a cloned repository, check status, diff, commit, branch, then pull and push over
   HTTPS with a token.
6. **Network**: HTTP to a public API and to `http://localhost:<port>` (a full-trust app needs no
   loopback exemption); a WebSocket echo (`wss://` and `ws://localhost`); gRPC with reflection against a
   local server and a TLS one; a request through a corporate proxy if you have one.
7. **Sign in**: Account → Sign in with GitHub/Google opens the browser, and you paste the code back.
   The desktop has no deep link or loopback redirect, so nothing here depends on MSIX. Links (Plan &
   billing, docs) open in the default browser.
8. **Secret managers**: with `op`, `az` or `aws` installed, `${secret:...}` references resolve. These
   are child processes started from inside the package.
9. **File dialogs**: Import (open) and Export (save) work. Saving to a file writes where you choose.
10. **Uninstall** from Settings → Apps, then install the GitHub `.exe`. It still works side by side and
    updates itself.
