# HITOP Development Plan

## Current state

HITOP covers the core API testing workflow: all HTTP methods, collections, variables, pre/post-request JS scripts, request history, Postman import/export, and multi-platform delivery (web, desktop, iOS, CLI, Chrome/Firefox extensions).

---

## Quick wins — high impact, low effort

### 1. URL query param editor
Table-based key/value editor that builds the query string automatically. Currently users must type params directly into the URL.

- Parse existing URL params on load and populate the table
- Sync table ↔ URL field bidirectionally
- Toggle individual params on/off without deleting them

### 2. Auth tab
Dedicated auth section with preset modes:
- **Bearer token** — injects `Authorization: Bearer <token>` header
- **Basic auth** — username/password fields, base64-encodes automatically
- **API Key** — choose header name and value; optionally inject as query param

No OAuth flow needed initially — just the header-injection helpers that cover 90% of real usage.

### 3. Environment switcher
Multiple named variable sets (Dev / Staging / Prod) that share the same `${varName}` syntax already in place.

- Environment selector in the top bar
- Active environment variables override collection variables
- Quick-add/edit environment panel
- Import/export environments as JSON

### 4. Assertion helpers in post-request scripts
Built-in `assert.*` functions available in post-request scripts, with results surfaced in the collection runner.

```js
assert.status(200);
assert.bodyContains("id");
assert.header("Content-Type", "application/json");
assert.responseTime(500); // ms
```

Runner shows pass/fail per request with assertion detail, total pass/fail count at the end.

---

## Medium effort

### 5. Multipart/form-data builder
Key/value table for `multipart/form-data` bodies. File field type for binary uploads.

### 6. Cookie jar
Persist and display cookies returned by responses. Allow manual editing. Send cookies automatically on subsequent requests to the same origin.

### 7. iOS feature parity
The iOS app currently lacks scripts editor, collection runner, and import/export. Bring it up to the same level as the web and desktop apps.

### 8. CLI interactive mode
A `hitop shell` REPL for ad-hoc requests without a collection file. Tab-completion for method, URL, headers.

---

## Longer term

### 9. WebSocket support
Open/close connections, send messages, display message log with timestamps. Separate from the standard request/response model.

### 10. GraphQL support
Detect `application/graphql` content type, show query/variables split editor, schema introspection via `__schema` query.

### 11. Response diff
Compare two saved responses side by side. Useful when tracking API changes across environments.

### 12. Syntax highlighting in script editor
Replace plain `<textarea>` in the scripts tab with a lightweight code editor (e.g. CodeMirror) with JS syntax highlighting and basic error marking.

---

## Platform gaps summary

| Feature | Web | Desktop | iOS | CLI |
|---|---|---|---|---|
| URL param editor | — | — | — | — |
| Auth tab | — | — | — | — |
| Environments | — | — | — | — |
| Assertions | — | — | — | — |
| Script editor highlight | — | — | — | n/a |
| Collection runner | ✓ | ✓ | — | ✓ |
| Import / Export | ✓ | ✓ | — | ✓ |
| Multipart body | — | — | — | — |
| Cookie jar | — | — | — | — |
| WebSocket | — | — | — | — |
| GraphQL | — | — | — | — |
