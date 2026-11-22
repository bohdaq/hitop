# HITOP for Android: what only the owner can do

Known before the start (2026-11-22):

- [ ] **Google Play developer account** (personal, $25, ID check). A new personal account must run a
      closed test with at least 12 testers for 14 days before it can publish to production: start the
      closed test as soon as A2 works, to overlap with the remaining tranches.
- [ ] **Play Console:** create the app HITOP (`tech.rws8.hitop`), then a service account with release
      rights for CI; its JSON key as GitHub secret (name given by the agent in A7).
- [ ] **Upload key** for signing (the agent writes the exact keytool command); keystore + passwords as
      GitHub secrets. Google Play App Signing holds the app signing key.
- [ ] **Google Cloud OAuth client** of type Android (package `tech.rws8.hitop` + the upload/app signing
      SHA-256) for native Google sign-in, and its id in the sync server's settings.
- [ ] **Sync server deploy** for any server change the agent makes (A2: Android sign-in return / Google
      ID token).
- [ ] **On the phone:** widgets, run notification, Quick Settings tile, Gemini Nano features, real
      sign-in with Google, GitHub and Apple.

The agent adds items below as it meets them.
