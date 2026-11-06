//! A collection's documentation as one self-contained HTML page (`hitop docs`, the web app's
//! "Docs page"): its variables and, per folder, each request's method, URL, headers, body, auth
//! (its kind, never a value that isn't a `${variable}`) and saved examples. Like the run report:
//! inline CSS, no script, a Content Security Policy that loads nothing, every string escaped,
//! and the same collection gives the same bytes.
use std::collections::BTreeMap;
use std::fmt::Write;

use crate::model::{Auth, Collection, Request};

/// HTML text and attribute values (double-quoted), as the run report escapes them.
fn html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() => out.push('\u{FFFD}'),
            _ => out.push(c),
        }
    }
    out
}

const STYLE: &str = r#"
:root{color-scheme:light dark;--bg:#fff;--fg:#1b1d22;--muted:#5d6370;--line:#d9dce3;--card:#f5f6f8;--code:#eef0f4;--accent:#5b3fd1}
@media (prefers-color-scheme:dark){:root{--bg:#121418;--fg:#e6e8ec;--muted:#9aa1ad;--line:#2c3038;--card:#1b1e24;--code:#22262e;--accent:#b4a3ff}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.55 system-ui,-apple-system,"Segoe UI",sans-serif}
.page{max-width:920px;margin:0 auto;padding:32px 20px}
h1{margin:0 0 4px;font-size:28px}h2{margin:36px 0 12px;font-size:20px;border-bottom:1px solid var(--line);padding-bottom:6px}h3{margin:0;font-size:17px}
.muted{color:var(--muted)}
nav ul{list-style:none;padding:0;margin:12px 0;columns:2;column-gap:24px}nav li{padding:2px 0;break-inside:avoid}
a{color:var(--accent)}
.req{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:16px;margin:14px 0}
.line{display:flex;gap:10px;align-items:baseline;flex-wrap:wrap;margin:8px 0}
.method{font:600 12px/1 ui-monospace,monospace;padding:4px 7px;border-radius:6px;background:var(--code)}
code,pre{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:13px}
.url{overflow-wrap:anywhere}
pre{background:var(--code);padding:10px 12px;border-radius:8px;overflow-x:auto;white-space:pre-wrap;word-break:break-word;margin:6px 0}
table{border-collapse:collapse;width:100%;margin:6px 0}th,td{text-align:left;padding:4px 8px;border-bottom:1px solid var(--line);vertical-align:top;overflow-wrap:anywhere}th{font-weight:600;width:30%}
h4{margin:12px 0 4px;font-size:13px;text-transform:uppercase;letter-spacing:.04em;color:var(--muted)}
footer{margin-top:40px;color:var(--muted);font-size:12px}
"#;

/// A value shown as is when it is a `${variable}` (or empty), else hidden.
fn shown(value: &str) -> String {
    let v = value.trim();
    if v.is_empty() || (v.starts_with("${") && v.ends_with('}') && !v[2..].contains("${")) {
        v.to_string()
    } else {
        "••••".into()
    }
}

fn auth_text(auth: Option<&Auth>) -> Option<String> {
    Some(match auth? {
        Auth::None => return None,
        Auth::Bearer { token } => format!("Bearer token {}", shown(token)),
        Auth::Basic { username, password } => format!("Basic, user {}, password {}", shown(username), shown(password)),
        Auth::ApiKey { key, value, location } => format!("API key {key} in the {}: {}", if matches!(location, crate::model::ApiKeyLocation::Query) { "query" } else { "headers" }, shown(value)),
        Auth::OAuth2(o) => format!("OAuth 2.0 ({}), token from {}", if o.grant == "password" { "password grant" } else { "client credentials" }, o.token_url),
        Auth::AwsV4(a) => format!("AWS Signature v4, {} in {}, access key {}", a.service, a.region, shown(&a.access_key)),
    })
}

fn anchor(n: usize) -> String {
    format!("r{n}")
}

/// The folder a request is in (from its file path), "" at the top.
fn folder_of(r: &Request) -> String {
    r.file.as_deref().and_then(|f| f.rsplit_once('/')).map(|(dir, _)| dir.replace('/', " › ")).unwrap_or_default()
}

fn request(h: &mut String, n: usize, r: &Request, default_auth: Option<&Auth>) {
    let method = if r.is_websocket() { "WS".to_string() } else if r.is_grpc() { "GRPC".into() } else { r.method.to_uppercase() };
    let _ = writeln!(h, "<section class=\"req\" id=\"{}\">\n<h3>{}</h3>", anchor(n), html(&r.name));
    let _ = writeln!(h, "<div class=\"line\"><span class=\"method\">{}</span><code class=\"url\">{}</code></div>", html(&method), html(&r.url));
    let headers: Vec<_> = r.headers.iter().filter(|x| !x.name.is_empty() && x.enabled).collect();
    if !headers.is_empty() {
        h.push_str("<h4>Headers</h4>\n<table>");
        for x in headers {
            let _ = write!(h, "<tr><th>{}</th><td><code>{}</code></td></tr>", html(&x.name), html(&x.value));
        }
        h.push_str("</table>\n");
    }
    if let Some(a) = auth_text(r.auth.as_ref().or(default_auth)) {
        let _ = writeln!(h, "<h4>Auth</h4>\n<p>{}</p>", html(&a));
    }
    if r.graphql.is_some() || r.body_type.as_deref() == Some("graphql") {
        let _ = writeln!(h, "<h4>GraphQL</h4>\n<pre>{}</pre>", html(&r.body));
    } else if !r.form_fields.is_empty() && r.body_mode.as_deref().is_some_and(|m| m != "raw") {
        h.push_str("<h4>Form</h4>\n<table>");
        for f in r.form_fields.iter().filter(|f| f.enabled && !f.key.is_empty()) {
            let value = if f.kind == "file" { "(a file)".to_string() } else { f.value.clone() };
            let _ = write!(h, "<tr><th>{}</th><td><code>{}</code></td></tr>", html(&f.key), html(&value));
        }
        h.push_str("</table>\n");
    } else if !r.body.trim().is_empty() {
        let _ = writeln!(h, "<h4>Body</h4>\n<pre>{}</pre>", html(&r.body));
    }
    for e in &r.examples {
        let name = e["name"].as_str().unwrap_or("Example");
        let status = e["status"].as_u64().map(|s| format!(" ({s})")).unwrap_or_default();
        let body = e["body"].as_str().unwrap_or_default();
        let _ = writeln!(h, "<h4>Example: {}{}</h4>\n<pre>{}</pre>", html(name), status, html(body));
    }
    h.push_str("</section>\n");
}

/// The page for `collection`.
pub fn to_html(collection: &Collection) -> String {
    let mut h = String::with_capacity(16 * 1024);
    h.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    h.push_str("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\">\n<meta name=\"referrer\" content=\"no-referrer\">\n");
    let _ = writeln!(h, "<title>{}: API documentation</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main class=\"page\">", html(&collection.name));
    let n = collection.requests.len();
    let _ = writeln!(h, "<h1>{}</h1>\n<p class=\"muted\">{} {}. Made with <a href=\"https://hitop.rws8.tech/\">HITOP</a>.</p>", html(&collection.name), n, if n == 1 { "request" } else { "requests" });
    // Folders in order of first appearance; requests keep the collection's order.
    let mut folders: Vec<String> = Vec::new();
    let mut by_folder: BTreeMap<String, Vec<(usize, &Request)>> = BTreeMap::new();
    for (i, r) in collection.requests.iter().enumerate() {
        let f = folder_of(r);
        if !folders.contains(&f) {
            folders.push(f.clone());
        }
        by_folder.entry(f).or_default().push((i, r));
    }
    h.push_str("<nav aria-label=\"Requests\"><ul>\n");
    for f in &folders {
        for (i, r) in &by_folder[f] {
            let label = if f.is_empty() { r.name.clone() } else { format!("{f} › {}", r.name) };
            let _ = writeln!(h, "<li><a href=\"#{}\">{}</a></li>", anchor(*i), html(&label));
        }
    }
    h.push_str("</ul></nav>\n");
    if !collection.variables.is_empty() {
        let mut vars: Vec<_> = collection.variables.iter().collect();
        vars.sort();
        h.push_str("<h2>Variables</h2>\n<table>");
        for (k, v) in vars {
            let _ = write!(h, "<tr><th><code>${{{}}}</code></th><td><code>{}</code></td></tr>", html(k), html(v));
        }
        h.push_str("</table>\n<p class=\"muted\">Environments (Dev, Staging, Prod) may set other values.</p>\n");
    }
    for f in &folders {
        let _ = writeln!(h, "<h2>{}</h2>", if f.is_empty() { "Requests".to_string() } else { html(f) });
        for (i, r) in &by_folder[f] {
            request(&mut h, *i, r, collection.auth.as_ref());
        }
    }
    h.push_str("<footer>Generated by HITOP from the collection's files.</footer>\n</main>\n</body>\n</html>\n");
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Header;

    #[test]
    fn a_page_with_folders_variables_and_hidden_secrets() {
        let mut c = Collection { name: "Shop <API>".into(), ..Default::default() };
        c.variables.insert("base".into(), "https://shop.test".into());
        c.auth = Some(Auth::Bearer { token: "${token}".into() });
        let mut a = Request { name: "List orders".into(), method: "GET".into(), url: "${base}/orders".into(), file: Some("orders/list".into()), ..Default::default() };
        a.headers.push(Header::new("X-Trace", "1"));
        let b = Request { name: "Login".into(), method: "POST".into(), url: "${base}/login".into(), body: "{\"user\":\"ada\"}".into(), auth: Some(Auth::Basic { username: "ada".into(), password: "hunter2".into() }), ..Default::default() };
        c.requests = vec![a, b];
        let page = to_html(&c);
        assert!(page.contains("<h1>Shop &lt;API&gt;</h1>"));
        assert!(page.contains("<h2>orders</h2>") && page.contains("<h2>Requests</h2>"));
        assert!(page.contains("Bearer token ${token}"), "a variable is shown");
        assert!(page.contains("Basic, user ••••, password ••••") && !page.contains("hunter2"), "a literal credential never");
        assert!(page.contains("<code>${base}</code>") && page.contains("https://shop.test"));
        assert!(!page.contains("<script"));
        assert_eq!(page, to_html(&c), "the same bytes");
    }
}
