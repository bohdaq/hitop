//! The few non-API pages: service info, health, and the page sign-in links open.

use rust_web_server::header::Header;
use rust_web_server::mime_type::MimeType;
use rust_web_server::range::Range;
use rust_web_server::request::Request;
use rust_web_server::response::{Response, STATUS_CODE_REASON_PHRASE};
use rust_web_server::router::PathParams;
use serde_json::json;

use crate::api::{ApiResult, State};

pub fn index(_: &State, _: &Request, _: &PathParams) -> ApiResult {
    Ok((
        200,
        json!({
            "service": "hitop-sync",
            "version": env!("CARGO_PKG_VERSION"),
            "license": "AGPL-3.0-or-later",
            "source": "https://github.com/bohdaq/hitop/tree/main/sync-server",
        }),
    ))
}

pub fn health(state: &State, _: &Request, _: &PathParams) -> ApiResult {
    state.db.tx(|tx| tx.query("SELECT 1", &[]))?;
    Ok((200, json!({ "ok": true })))
}

/// The page a sign-in link opens. The code stays in the URL fragment (never sent to a server or
/// logged); the page only shows it so it can be pasted into hitop.
const SIGNIN_PAGE: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>hitop sync sign-in</title>
<style>body{font:16px/1.5 system-ui,sans-serif;max-width:36rem;margin:3rem auto;padding:0 1rem;color:#1b1b1f;background:#fff}
code{display:block;padding:.75rem;background:#f2f0f4;border-radius:8px;word-break:break-all;font-size:15px}
@media (prefers-color-scheme:dark){body{color:#e4e1e6;background:#1b1b1f}code{background:#303034}}</style></head>
<body><h1>Sign in to hitop sync</h1>
<p id="msg">Paste this code into hitop to finish signing in. It works once, for 15 minutes.</p>
<code id="code"></code>
<script>var c=location.hash.slice(1);document.getElementById('code').textContent=c||'(no code in this link)';</script>
</body></html>
"#;

pub fn signin_page() -> Response {
    let range = Range::get_content_range(SIGNIN_PAGE.as_bytes().to_vec(), MimeType::TEXT_HTML.to_string());
    let mut r = Response::get_response(STATUS_CODE_REASON_PHRASE.n200_ok, None, Some(vec![range]));
    r.headers.push(Header { name: "Content-Security-Policy".into(), value: "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; frame-ancestors 'none'".into() });
    r.headers.push(Header { name: "Referrer-Policy".into(), value: "no-referrer".into() });
    r
}
