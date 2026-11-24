# HITOP for Android: what only the owner can do

Step by step, with every console field: `android/PLAY_STORE.md`. This is the checklist.

## To publish on Google Play

- [ ] **Google Play developer account** (personal, $25, ID check). New personal accounts must run a
      closed test with at least 12 testers for 14 days in a row before production
      (PLAY_STORE.md §3, §7 step 4): start it with the first internal build.
- [ ] **Upload key**: `keytool -genkeypair -v -keystore hitop-upload.jks -alias upload -keyalg RSA
      -keysize 4096 -validity 10000 -dname "CN=HITOP upload key"` (PLAY_STORE.md §1).
- [ ] **GitHub secrets**: `ANDROID_UPLOAD_KEYSTORE_BASE64`, `ANDROID_UPLOAD_KEYSTORE_PASSWORD`,
      `ANDROID_UPLOAD_KEY_ALIAS`, `ANDROID_UPLOAD_KEY_PASSWORD` (optional),
      `HITOP_GOOGLE_SERVER_CLIENT_ID`, `PLAY_SERVICE_ACCOUNT_JSON` (PLAY_STORE.md §2).
- [ ] **Play Console**: create the app HITOP (`tech.rws8.hitop`), Play App Signing (default), store
      listing texts and pictures from `android/store/screenshots/` + `brand/png/logo-512.png`, app
      content declarations, Data safety answers, the dataSync foreground service declaration with a
      short screen recording (PLAY_STORE.md §3, §5, §8, §9).
- [ ] **Service account** with "Release apps to testing tracks" on HITOP (PLAY_STORE.md §6).
- [ ] **First release by hand**: push tag `android-v1.0.0` (release-android.yml makes the signed
      AAB), upload that AAB to Internal testing in Play Console; afterwards Actions → Android Play
      uploads each build (PLAY_STORE.md §7).
- [ ] **Promote** internal → closed → production in Play Console when ready (agents never promote).

## Sign-in

- [ ] **Google Cloud OAuth clients of type Android** (package `tech.rws8.hitop`; one with the app
      signing SHA-256 from Play Console, one with the upload key's SHA-256) (PLAY_STORE.md §4).
- [x] **Sync server** deployed 2026-11-24 (3a30dbe): `POST /v1/auth/google/native/{start,finish}` are there, off.
- [ ] Set `HITOP_SYNC_GOOGLE_ANDROID_CLIENT_IDS=<the Android client ids, comma separated>` in `/etc/hitop-sync/env` and
      restart `hitop-sync`. Until then Google sign-in runs in the browser (it works already).

## Website

- [x] Site pages (privacy policy with the Android sections) deployed 2026-11-24. The web app, portal and proxy were not.

## On a real phone (no emulator can check these)

- [ ] Widgets on the home screen (Last run, Failing, Pinned + Send), the run notification's Stop,
      the Quick Settings tile, long-press shortcuts.
- [ ] Gemini Nano features on a phone that has it (Pixel 9 or newer, Galaxy S25…): Explain,
      Suggest a test, request from a description.
- [ ] Real sign-in with Google (Credential Manager), GitHub and Apple (Custom Tab back to the app).
- [ ] A tablet or foldable: three panes from 1200 dp, two from 600 dp, Ctrl+Enter with a keyboard.
- [ ] OAuth 2.0 with sign-in: Send opens the page, the token is used; the app survives being in the background meanwhile.
- [ ] A user-installed certificate: an internal API works only with Settings → Trust this phone's certificates.
- [ ] Background: sync with the app closed, a 15-minute scheduled run, the notification when it fails.
- [ ] Company SSO against a real provider. Share to HITOP from a browser; a file upload; an import from Downloads.
- [ ] Cold start feels fast (the startup profile is hand-written and wasn't measured).

## Ready to tag?

B0–B7 are done. After the phone checks above: bump nothing (`versionName` is 1.0.0), push tag `android-v1.0.0`.
