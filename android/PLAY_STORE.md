# HITOP for Android on Google Play: the owner's guide

Everything to do in Play Console, Google Cloud and GitHub, in order. Agents never upload outside
`android-play.yml`, never promote a track and never push an `android-v*` tag; the steps below are
the owner's. Package `tech.rws8.hitop`, app name **HITOP**, free, no in-app purchases, no ads.

## 1. Upload key (once, on your own machine)

Google Play App Signing keeps the key that signs what people install; you sign uploads with an
upload key. Make it (keep the file and passwords in your password manager; Google can reset a lost
upload key, but it takes days):

```sh
keytool -genkeypair -v -keystore hitop-upload.jks -alias upload \
  -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=HITOP upload key"
keytool -list -v -keystore hitop-upload.jks -alias upload | grep SHA256   # the upload key's SHA-256
base64 -w0 hitop-upload.jks > hitop-upload.jks.b64                         # macOS: base64 -i hitop-upload.jks
```

## 2. GitHub secrets (repository → Settings → Secrets and variables → Actions)

| Secret | Value |
| --- | --- |
| `ANDROID_UPLOAD_KEYSTORE_BASE64` | the contents of `hitop-upload.jks.b64` |
| `ANDROID_UPLOAD_KEYSTORE_PASSWORD` | the keystore password |
| `ANDROID_UPLOAD_KEY_ALIAS` | `upload` |
| `ANDROID_UPLOAD_KEY_PASSWORD` | the key password (leave unset if it is the keystore password) |
| `HITOP_GOOGLE_SERVER_CLIENT_ID` | the **Web** OAuth client id of step 4 (`….apps.googleusercontent.com`) |
| `PLAY_SERVICE_ACCOUNT_JSON` | the service account's JSON key of step 6 (the whole file) |

Delete `hitop-upload.jks.b64` afterwards. The workflows write the key to the runner's temp
directory, use it, and remove it.

## 3. Play Console: the app

1. Play Console → **Create app**: name HITOP, default language English (United States), App,
   Free. Accept the declarations.
2. New personal developer accounts must run a **closed test with at least 12 testers for 14 days in
   a row** before production. Start it as soon as there is a build (step 7).
3. **App integrity → App signing**: keep Google-generated app signing key (the default). Copy the
   **App signing key certificate SHA-256** for step 4.

## 4. Google Cloud: sign in with Google

Project of the sync server's Google sign-in (the one holding `HITOP_SYNC_GOOGLE_CLIENT_ID`):

1. APIs & Services → Credentials → **Create OAuth client ID → Android**: package `tech.rws8.hitop`,
   SHA-256 = the **app signing** certificate (step 3). Make a second Android client with the
   **upload key's** SHA-256 for builds from `release-android.yml` installed outside Play.
2. The **Web** client id the server already uses for Google sign-in is what the app asks Google for
   (the ID token's audience): it goes in the GitHub secret `HITOP_GOOGLE_SERVER_CLIENT_ID`.
3. Sync server env (`/etc/hitop-sync/env`): `HITOP_SYNC_GOOGLE_ANDROID_CLIENT_IDS=<android client id 1>,<id 2>`,
   then deploy the server built from this tree (it has `POST /v1/auth/google/native/{start,finish}`)
   and restart. Until then the app offers Google sign-in through the browser instead
   (`/v1/auth/methods` has no `googleNative`).

GitHub and Apple sign-in return to `hitop://sign-in`; the server allows that return address since
the iOS app (nothing to add).

## 5. Store listing (Grow → Store presence → Main store listing)

- **App name:** HITOP
- **Short description (80):** The API client your terminal, your CI and your AI agent can all drive.
- **Full description:** below.
- **App icon (512 × 512):** `brand/png/logo-512.png`
- **Feature graphic (1024 × 500):** `android/store/screenshots/feature-graphic.jpg`
- **Phone screenshots (1080 × 1920):** `android/store/screenshots/phone/1-collections.jpg` … `5-environments.jpg`, in that order
- **7-inch tablet (2880 × 1800):** `android/store/screenshots/tablet-7/*.jpg`
- **10-inch tablet (3840 × 2400):** `android/store/screenshots/tablet-10/*.jpg`
- **Category:** Tools. **Tags:** Developer tools. **Email:** your support address. **Website:** https://hitop.rws8.tech
- **Privacy policy:** https://hitop.rws8.tech/privacy.html (has the Android sections once the site is deployed from this tree)

The pictures are rendered from the app's own sample data by
`./gradlew :app:testDebugUnitTest --tests '*StoreScreenshots*' -Phitop.storeScreenshots=$PWD/store/screenshots`
(in `android/`), so they follow the app; the same test checks their sizes on every CI run.

Full description:

```
HITOP is an HTTP API client for your phone that speaks the same collections as the HITOP command line, desktop app and CI action.

• Collections, folders, environments and secret variables (secrets stay in the Android Keystore)
• REST, GraphQL (schema completion), WebSocket, Server-Sent Events and gRPC
• Pre-request and test scripts, including Postman's pm.* API
• Run a whole collection with a live notification, then read or share its report (HTML, JUnit, JSON, the same as hitop run)
• Widgets for the last run, failing requests and pinned requests; a Quick Settings tile and shortcuts
• Paste a curl command to make a request
• On phones with Gemini Nano: explain a response and suggest tests that pass, on the device
• Tablet and foldable layouts, keyboard shortcuts (Ctrl+Enter sends)

Optional HITOP Cloud account (Google, GitHub or Apple): keep your library in sync with the web and desktop apps, and share collections with your team end-to-end encrypted.

No ads, no tracking, no analytics. Requests go straight from your phone to the address you enter.
```

## 6. Service account for CI uploads

1. Google Cloud (any project) → IAM → Service accounts → create `hitop-play-upload`; Keys → Add key → JSON.
2. Enable the **Google Play Android Developer API** in that project.
3. Play Console → **Users and permissions → Invite new users**: the service account's email; app
   HITOP; permissions **Release apps to testing tracks** (and View app information). Nothing else.
4. The JSON file's contents → GitHub secret `PLAY_SERVICE_ACCOUNT_JSON`.

## 7. First upload

Play takes uploads through its API only after the app's first release was made in Play Console:

1. Get a signed AAB: push the tag of section 10 (`android-v1.0.0`) and download
   `hitop-android-1.0.0.aab` from that GitHub Release (or run `android/scripts/build-release.sh`
   locally with the variables of section 2 set by hand).
2. Play Console → Testing → **Internal testing → Create new release** → upload the AAB → Save →
   Review → Start rollout. Testers tab: an email list with yourself.
3. From then on: **Actions → Android Play → Run workflow** (status `completed`; `draft` while the
   app has never been reviewed) uploads each build to the internal track. The version code is
   1000 + the workflow's run number unless you give one; it must be higher than every earlier upload.
4. Closed test (the 12 testers for 14 days): Testing → Closed testing → Create track, add the
   testers' emails or a Google Group, then **Promote release** from internal testing to it.

## 8. App content (Policy → App content)

| Declaration | Answer |
| --- | --- |
| Privacy policy | https://hitop.rws8.tech/privacy.html |
| Ads | No ads |
| App access | All functionality is available without special access. (HITOP Cloud is optional; reviewers can sign in with any Google or GitHub account.) |
| Content rating | Questionnaire, category **Utility, productivity, communication or other**: no violence, sexuality, language, controlled substances, gambling; users can't interact or share content publicly (sharing is with invited people only); no location sharing; no purchases. Expected: Everyone / PEGI 3 |
| Target audience | 18 and over (a developer tool; not designed for children) |
| News app | No |
| Health / Financial features / Government | None / No / No |
| Foreground service | **Data sync** (`FOREGROUND_SERVICE_DATA_SYNC`): "Runs a collection of HTTP requests the user started (Runs → Run), with a notification showing progress and a Stop button; it ends when the run ends." Attach a short screen recording: Collections → ▶ on a collection → Start run → the notification → Stop |
| Data safety | Section 9 |

## 9. Data safety answers

Does the app collect or share any of the required user data types? **Yes** (only with a HITOP
Cloud account; everything else stays on the phone and is not "collected" in Play's sense).

- Is all collected data encrypted in transit? **Yes** (HTTPS).
- Do you provide a way to request deletion? **Yes**: in the app (Account → Delete account), and by
  email; the web URL field: https://hitop.rws8.tech/privacy.html (its "Deleting your account" section
  says how, including by email without the app).
- Shared with third parties: **None** for every type below (HITOP Cloud is HITOP's own service).

| Data type | Collected | Optional? | Purposes | Notes |
| --- | --- | --- | --- | --- |
| Personal info → Email address | Yes | Optional (only with an account) | Account management, App functionality | the sign-in address |
| Personal info → User IDs | Yes | Optional | Account management, App functionality | account id, Google/GitHub/Apple account ids |
| App activity → Other user-generated content | Yes | Optional | App functionality | synced collections, environments, globals (library); shared collections are end-to-end encrypted |
| App info and performance | **No** | | | no crash reports, no diagnostics |
| Device or other IDs | **No** | | | the device's public key and "HITOP on <model>" are account data, not a device id |
| Location, Financial info, Health, Messages, Photos, Audio, Files, Calendar, Contacts, Web browsing | **No** | | | |

Not collected: requests and responses (they go from the phone to the address entered), request
history, secret values on the phone, anything the on-device model reads (Gemini Nano runs on the
phone). The app has no analytics or advertising SDK and no usage ping.

## 10. Releases

- **Testing builds:** Actions → Android Play → Run workflow (internal track). Promote internal →
  closed → production in Play Console yourself (Release → Promote release).
- **Signed APK for people outside Play:** bump `versionName` in `android/app/build.gradle.kts`,
  commit, push, then push tag `android-v<versionName>` (`-rc.N` for a pre-release).
  `release-android.yml` builds, checks and attaches `hitop-android-<v>.apk`, `.aab` and `SHA256SUMS`
  to a GitHub Release (never marked Latest).
- Every CI run (`android.yml`, job App) already builds the release AAB and APK with a throwaway key
  and runs `bundletool validate`, the signature check and the 16 KB page check
  (`android/scripts/build-release.sh`, `check-release.sh`).
- Target API: Play requires each year's new target API by August 31; `targetSdk` is 37.
