# Changelog

Releases before 1.2 are listed in ROADMAP.md (section 5) and on the GitHub Releases page.

## 1.2.0 (unreleased)

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
