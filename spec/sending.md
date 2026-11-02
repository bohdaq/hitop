<!--
What hitop adds when it sends a request
Copyright 2026 Bohdan Tsap
This specification is licensed under the Creative Commons Attribution 4.0
International License (CC BY 4.0): https://creativecommons.org/licenses/by/4.0/
The hitop code that implements it is MIT licensed.
-->

# What hitop adds when it sends a request

Status: since hitop 1.2 (CLI, MCP, hitop-core, desktop, web app, HitopKit);
the web app and desktop since 1.1.0. Reference implementation:
`cli/crates/hitop-core/src/sending.rs`. Test vectors: `vectors/send.json`.

This is send behaviour, not collection format: nothing here is written to a
collection, and the format (`collection-format.md`, version 1) is unchanged.
Every hitop surface applies the same two rules to the request as it goes out,
after `${variables}`, auth and the pre-request script, so one collection sends
the same headers from `hitop run`, `hitop send`, `hitop requests send`,
`hitop mcp`, the web app, the desktop app and the iOS companion, and
`hitop snippet` (and the Code dialog) shows them.

## 1. Content-Type of a raw body

A request whose method is `POST`, `PUT` or `PATCH` (any case), whose body is
raw text (not a form, multipart or GraphQL body, or a form body the
pre-request script replaced with text), and that has no `Content-Type` header
with a value, is sent with the Content-Type its text implies. Leading
whitespace is skipped first, then:

| Body starts with | Content-Type |
| --- | --- |
| `{` or `[` | `application/json` |
| `<html` or `<!doctype html` (ASCII letters in any case, one space) | `text/html` |
| any other `<` | `application/xml` |
| anything else | `text/plain` |
| nothing (empty or blank body) | none |

A `Content-Type` header of the request's own, or one its pre-request script
sets, always wins. Post-request scripts see the added header in `pm.request`.
GET, DELETE and other methods get none. The body's `type` in the request file
(`json`, `xml`, `text`) is a label for editors and is not used.

## 2. Scheme of a URL typed without one

The URL as sent, with leading and trailing whitespace removed:

- with a scheme (a letter, then letters, digits, `+`, `.` or `-`, then `:`,
  unless that is a `host:port`), it is sent as it is;
- `localhost`, `127.x.x.x`, `0.0.0.0` or `[::1]` (ASCII case-insensitive),
  with an optional `:port` and then nothing or `/`, `?`, `#`, gets `http://`;
- a host of ASCII letters, digits, `_` and `-` with a dot or a `:port`,
  then optionally `/`, `?` or `#` and no whitespace, gets `https://`;
- anything else is an error, and nothing is sent:
  `"<url>" isn't a URL. Enter a full URL, such as https://api.example.com/users`.

So `api.example.com/users` and `${host}/users` with `host: api.example.com`
are sent to `https://api.example.com/users`, and `localhost:3000` to
`http://localhost:3000`. Snippets add the scheme too, unless the host is a
`{{placeholder}}`. WebSocket and gRPC targets are not changed.

## 3. Whitespace and characters

"Whitespace" is JavaScript's (`String.prototype.trim`): U+0009 to U+000D,
U+0020, U+00A0, U+1680, U+2000 to U+200A, U+2028, U+2029, U+202F, U+205F,
U+3000 and U+FEFF (a byte order mark), but not U+0085. Letters and digits in
the rules are ASCII only. Implementations in other languages must not use
their own Unicode whitespace or case folding.

## 4. Test vectors

`vectors/send.json` holds `contentType.detect` (body → Content-Type or
`null`), `contentType.apply` (method, headers, body → headers as sent) and
`scheme.cases` (URL → URL as sent, or the error text). hitop-core
(`tests/send_vectors.rs`), the web app (`webapp/tests/unit.mjs`, through
hitop-wasm) and HitopKit (`SendRulesTests`) run every case. Cases are only
added.
