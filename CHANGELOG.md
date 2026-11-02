# Changelog

Releases before 1.2 are listed in ROADMAP.md (section 5) and on the GitHub Releases page.

## 1.2.0 (2026-11-02)

### Same Content-Type on every surface

- `hitop run`, `hitop send`, `hitop requests send`, `hitop snippet`, `hitop mcp` and the iOS
  companion (HitopKit) now do what the web app and desktop have done since 1.1.0: a POST, PUT or
  PATCH with a raw body and no `Content-Type` header is sent with `application/json`,
  `application/xml`, `text/html` or `text/plain`, decided from the body as sent. A header of the
  request's own (or one its pre-request script sets) wins; nothing is written to the collection.
  Post-request scripts see the header in `pm.request`, and snippets show it.
- A URL without a scheme is sent with `https://` (`http://` for `localhost`, `127.x.x.x`,
  `0.0.0.0` and `[::1]`) in the CLI, MCP and HitopKit too, so `base: api.example.com` works in
  every app. Anything else that isn't a URL fails before sending with
  `"<url>" isn't a URL. Enter a full URL, such as https://api.example.com/users`.
- One implementation: hitop-core `sending` (the web app and desktop reach it through hitop-wasm;
  HitopKit follows it), specified in `spec/sending.md` with test vectors in
  `spec/vectors/send.json`. Collection format v1 is unchanged.
- Behaviour change for CLI users: a run that sent a raw body without `Content-Type` now sends one.
  Add your own `Content-Type` header to keep a server that rejects the detected one happy.

### hitop sync: authenticated workspace keys

- Workspace keys are now handed out with authenticated wraps (HPKE Auth mode, made with the
  sending device's key, the one whose fingerprint people compare). Every client (CLI, desktop, web
  app) uses a key only when a device it trusts made the wrap, never takes a different key for an
  epoch it already has, and notices when the server hides a key rotation. A malicious sync server
  can no longer give a new member a workspace key of its own choosing.
- Invite codes carry the inviter's key fingerprint (`<code>.<fingerprint>`), so the invitee's
  hitop checks who sent the key; `hitop sync invite` prints it (also for mailed invites, to send
  yourself). A key from a device you haven't trusted is refused with its fingerprint:
  `hitop sync trust '<fingerprint>'`, or "Trust this device…" on the Sync page.
- Workspaces shared with 1.1 keep working, with a warning; the owner's next sync upgrades their
  keys. hitop 1.1 can't read the new keys: everyone in a shared workspace updates to 1.2, and the
  sync server must be updated first. Runner credentials made with 1.2 need a 1.2 image; 1.1
  credentials keep working with a warning.
- `hitop-sync-crypto` 0.2.0 (new API: `seal_wrap`, `open_wrap`, `accept_wraps`), test vectors
  `vectors/v2.json`. Design: `sync-server/THREAT_MODEL.md`.
