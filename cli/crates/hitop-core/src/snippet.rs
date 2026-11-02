//! Code snippets for a saved request: curl, JavaScript fetch, Python
//! requests, Go net/http, Java HttpClient and Rust reqwest. Pure (no
//! native deps), so the web app runs the same generator through hitop-wasm.
//!
//! Variables are filled in from `variables`, except the names in `secrets`
//! and names with no value: those are written as a `{{NAME}}` placeholder,
//! so a snippet never carries a secret unless the caller put its value in
//! `variables` and left it out of `secrets`. Scripts are not run.
use std::collections::HashMap;

use regex::Regex;

use crate::auth;
use crate::form;
use crate::graphql;
use crate::model::{Auth, Request};

/// A snippet language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Curl,
    Fetch,
    Python,
    Go,
    Java,
    Rust,
}

impl Lang {
    pub const ALL: [Lang; 6] = [Lang::Curl, Lang::Fetch, Lang::Python, Lang::Go, Lang::Java, Lang::Rust];

    /// The id used by `--lang` and the web app: `curl`, `fetch`, `python`,
    /// `go`, `java`, `rust`.
    pub fn id(self) -> &'static str {
        match self {
            Lang::Curl => "curl",
            Lang::Fetch => "fetch",
            Lang::Python => "python",
            Lang::Go => "go",
            Lang::Java => "java",
            Lang::Rust => "rust",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Curl => "curl",
            Lang::Fetch => "JavaScript fetch",
            Lang::Python => "Python requests",
            Lang::Go => "Go net/http",
            Lang::Java => "Java HttpClient",
            Lang::Rust => "Rust reqwest",
        }
    }

    /// File extension of the generated code.
    pub fn extension(self) -> &'static str {
        match self {
            Lang::Curl => "sh",
            Lang::Fetch => "mjs",
            Lang::Python => "py",
            Lang::Go => "go",
            Lang::Java => "java",
            Lang::Rust => "rs",
        }
    }

    /// Reads an id, with a few aliases (`js`, `javascript`, `py`, `golang`, `reqwest`).
    pub fn parse(s: &str) -> Option<Lang> {
        Some(match s.to_ascii_lowercase().as_str() {
            "curl" | "sh" | "shell" => Lang::Curl,
            "fetch" | "js" | "javascript" | "node" => Lang::Fetch,
            "python" | "py" | "requests" => Lang::Python,
            "go" | "golang" => Lang::Go,
            "java" => Lang::Java,
            "rust" | "rs" | "reqwest" => Lang::Rust,
            _ => return None,
        })
    }
}

/// Generated code and the `{{NAME}}` placeholders it contains (secrets and
/// variables with no value), in order of first use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub code: String,
    pub placeholders: Vec<String>,
}

/// The placeholder written for a variable that is a secret or has no value.
pub fn placeholder(name: &str) -> String {
    format!("{{{{{name}}}}}")
}

/// What goes on the wire, with variables filled in.
#[derive(Debug, Clone, PartialEq)]
struct Spec {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    basic: Option<(String, String)>,
    body: Body,
}

#[derive(Debug, Clone, PartialEq)]
enum Body {
    None,
    Raw(String),
    /// Already encoded as hitop sends it (`form::urlencoded`).
    UrlEncoded(String),
    Multipart(Vec<Part>),
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String, String),
    /// Field name and file name: the snippet reads the file from disk.
    File(String, String),
}

/// The snippet for `request` (its own auth, else `collection_auth`).
pub fn generate(lang: Lang, request: &Request, collection_auth: Option<&Auth>, variables: &HashMap<String, String>, secrets: &[String]) -> anyhow::Result<Snippet> {
    if request.is_websocket() {
        anyhow::bail!("'{}' is a WebSocket request: there is no HTTP snippet for it (connect with `hitop ws`)", request.name);
    }
    if request.is_grpc() {
        anyhow::bail!("'{}' is a gRPC request: there is no HTTP snippet for it (call it with `hitop grpc`)", request.name);
    }
    let mut placeholders = Vec::new();
    let spec = build(request, collection_auth, variables, secrets, &mut placeholders)?;
    let code = match lang {
        Lang::Curl => curl(&spec),
        Lang::Fetch => fetch(&spec),
        Lang::Python => python(&spec),
        Lang::Go => go(&spec),
        Lang::Java => java(&spec),
        Lang::Rust => rust(&spec),
    };
    Ok(Snippet { code, placeholders })
}

fn build(request: &Request, collection_auth: Option<&Auth>, variables: &HashMap<String, String>, secrets: &[String], placeholders: &mut Vec<String>) -> anyhow::Result<Spec> {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let cell = std::cell::RefCell::new(std::mem::take(placeholders));
    let fill = |s: &str| -> String {
        re.replace_all(s, |caps: &regex::Captures| {
            let name = &caps[1];
            match variables.get(name) {
                Some(v) if !secrets.iter().any(|s| s == name) => v.clone(),
                _ => {
                    let mut p = cell.borrow_mut();
                    if !p.iter().any(|n| n == name) {
                        p.push(name.to_string());
                    }
                    placeholder(name)
                }
            }
        })
        .into_owned()
    };
    let method = if request.method.trim().is_empty() { "GET".to_string() } else { request.method.trim().to_ascii_uppercase() };
    // The scheme hitop adds when sending; a URL it can't tell (a placeholder
    // for the host) is written as it is.
    let url = fill(&request.url);
    let url = crate::sending::with_scheme(&url).unwrap_or(url);
    let headers: Vec<(String, String)> = request.headers.iter().filter(|h| h.enabled && !h.name.trim().is_empty()).map(|h| (fill(&h.name), fill(&h.value))).collect();
    let has = |headers: &[(String, String)], name: &str| headers.iter().any(|(k, v)| k.eq_ignore_ascii_case(name) && !v.is_empty());
    let auth = request.auth.as_ref().or(collection_auth);
    let mut basic = None;
    let (url, mut headers) = match auth {
        // Basic stays structured so each language writes it its own way;
        // a hand-set Authorization header wins, as when hitop sends it.
        Some(Auth::Basic { username, password }) if (!username.is_empty() || !password.is_empty()) && !has(&headers, "Authorization") => {
            basic = Some((fill(username), fill(password)));
            (url, headers)
        }
        Some(Auth::Basic { .. }) => (url, headers),
        other => auth::apply(other, url, headers, &|s| Ok(fill(s)))?,
    };
    let body = match request.body_mode.as_deref() {
        Some(mode @ ("urlencoded" | "multipart")) => {
            let rows: Vec<_> = request.form_fields.iter().filter(|f| f.enabled && !f.key.is_empty()).collect();
            if mode == "urlencoded" {
                let pairs: Vec<(String, String)> = rows.iter().map(|f| (fill(&f.key), fill(&f.value))).collect();
                let built = form::urlencoded(&pairs);
                if !has(&headers, "Content-Type") {
                    headers.push(("Content-Type".into(), built.content_type));
                }
                Body::UrlEncoded(String::from_utf8(built.body).unwrap_or_default())
            } else {
                // The multipart Content-Type (and its boundary) comes from the
                // client library; a hand-set one would break the body.
                headers.retain(|(k, _)| !k.eq_ignore_ascii_case("Content-Type"));
                Body::Multipart(
                    rows.iter()
                        .map(|f| {
                            let name = fill(&f.key);
                            if f.kind == "file" {
                                let file = f.extra.get("fileName").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("file.bin");
                                Part::File(name, file.to_string())
                            } else {
                                Part::Text(name, fill(&f.value))
                            }
                        })
                        .collect(),
                )
            }
        }
        _ if request.is_graphql() => {
            let op = request.graphql_operation().unwrap_or_default();
            let (query, variables, name) = (fill(op.query), fill(op.variables), fill(op.operation_name));
            if !has(&headers, "Content-Type") {
                headers.push(("Content-Type".into(), graphql::CONTENT_TYPE.into()));
            }
            // A placeholder can leave the variables invalid JSON
            // (`{"id": {{id}}}`): they go in as written then.
            Body::Raw(graphql::request_body(&query, &variables, &name).unwrap_or_else(|_| graphql_body_unchecked(&query, &variables, &name)))
        }
        _ if request.body.is_empty() => Body::None,
        _ => {
            let body = fill(&request.body);
            // The Content-Type hitop adds when sending a raw body.
            crate::sending::add_content_type(&method, &mut headers, &body);
            Body::Raw(body)
        }
    };
    // hitop sends one value per header name, the last one (a header map).
    let mut unique: Vec<(String, String)> = Vec::new();
    for (k, v) in headers {
        match unique.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(&k)) {
            Some(slot) => *slot = (k, v),
            None => unique.push((k, v)),
        }
    }
    let headers = unique;
    *placeholders = cell.into_inner();
    // An API key in the query is percent-encoded: keep its placeholder readable.
    let mut url = url;
    for name in placeholders.iter() {
        url = url.replace(&format!("%7B%7B{}%7D%7D", form::encode_component(name)), &placeholder(name));
    }
    Ok(Spec { method, url, headers, basic, body })
}

/// `graphql::request_body` without the variables check.
fn graphql_body_unchecked(query: &str, variables: &str, operation_name: &str) -> String {
    let mut out = format!("{{\"query\":{}", serde_json::Value::String(query.to_string()));
    if !variables.trim().is_empty() {
        out.push_str(&format!(",\"variables\":{}", variables.trim()));
    }
    if !operation_name.trim().is_empty() {
        out.push_str(&format!(",\"operationName\":{}", serde_json::Value::String(operation_name.trim().to_string())));
    }
    out.push('}');
    out
}

fn basic_header(user: &str, pass: &str) -> String {
    use base64::Engine;
    format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}")))
}

// --- escaping -----------------------------------------------------------------

/// A POSIX shell word in single quotes.
fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A double-quoted string for JavaScript, Python, Go (`\xNN` for control
/// characters: all three read it as that code point) or Rust and Java.
fn quoted(s: &str, lang: Lang) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // `${` starts a template only in backticks, but keep `$` plain anyway.
            c if (c as u32) < 0x20 || c == '\u{7f}' => match lang {
                Lang::Rust => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
                // Java turns \uXXXX into the character before it parses the
                // string, so a newline that way would end it: octal instead.
                Lang::Java => out.push_str(&format!("\\{:03o}", c as u32)),
                _ => out.push_str(&format!("\\x{:02x}", c as u32)),
            },
            // Line and paragraph separators end a line in old JavaScript.
            '\u{2028}' | '\u{2029}' if lang == Lang::Fetch => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A Go string: a raw (backtick) literal for multi-line text that allows one, else quoted.
fn go_str(s: &str) -> String {
    if s.contains('\n') && !s.contains('`') && !s.contains('\r') {
        format!("`{s}`")
    } else {
        quoted(s, Lang::Go)
    }
}

/// A Rust string: a raw literal for text with quotes, backslashes or lines.
fn rust_str(s: &str) -> String {
    if !(s.contains('"') || s.contains('\\') || s.contains('\n')) || s.chars().any(|c| (c as u32) < 0x20 && c != '\n') {
        return quoted(s, Lang::Rust);
    }
    let mut hashes = String::from("#");
    while s.contains(&format!("\"{hashes}")) {
        hashes.push('#');
    }
    format!("r{hashes}\"{s}\"{hashes}")
}

/// Content-Type of the request, if one is set by hand or by the body.
fn content_type(spec: &Spec) -> Option<&str> {
    spec.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("Content-Type")).map(|(_, v)| v.as_str())
}

// --- curl ------------------------------------------------------------------------

fn curl(spec: &Spec) -> String {
    let mut parts: Vec<String> = Vec::new();
    let has_body = spec.body != Body::None;
    let first = match spec.method.as_str() {
        "HEAD" => format!("curl --head {}", sh(&spec.url)),
        "GET" if !has_body => format!("curl {}", sh(&spec.url)),
        m => format!("curl -X {} {}", sh_word(m), sh(&spec.url)),
    };
    parts.push(first);
    for (k, v) in &spec.headers {
        // `Name;` sends an empty header; `Name:` would remove it.
        let h = if v.is_empty() { format!("{k};") } else { format!("{k}: {v}") };
        parts.push(format!("-H {}", sh(&h)));
    }
    if let Some((u, p)) = &spec.basic {
        parts.push(format!("-u {}", sh(&format!("{u}:{p}"))));
    }
    match &spec.body {
        Body::None => {}
        Body::Raw(text) | Body::UrlEncoded(text) => {
            if content_type(spec).is_none() {
                // curl would add a form Content-Type that hitop doesn't send.
                parts.push(format!("-H {}", sh("Content-Type:")));
            }
            parts.push(format!("--data-raw {}", sh(text)));
        }
        Body::Multipart(rows) => {
            for p in rows {
                match p {
                    // --form-string: a value starting with @ or < stays text.
                    Part::Text(n, v) => parts.push(format!("--form-string {}", sh(&format!("{n}={v}")))),
                    Part::File(n, f) => parts.push(format!("-F {}", sh(&format!("{n}=@\"{}\"", f.replace('\\', "\\\\").replace('"', "\\\""))))),
                }
            }
        }
    }
    let mut out = parts.join(" \\\n  ");
    out.push('\n');
    out
}

/// A method is a token; quote it only when it isn't a plain word.
fn sh_word(s: &str) -> String {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        s.to_string()
    } else {
        sh(s)
    }
}

// --- JavaScript fetch -------------------------------------------------------------

fn fetch(spec: &Spec) -> String {
    let q = |s: &str| quoted(s, Lang::Fetch);
    let mut out = String::new();
    if let Body::Multipart(rows) = &spec.body {
        if rows.iter().any(|p| matches!(p, Part::File(..))) {
            out.push_str("import { openAsBlob } from \"node:fs\";\n\n");
        }
        out.push_str("const form = new FormData();\n");
        for p in rows {
            match p {
                Part::Text(n, v) => out.push_str(&format!("form.append({}, {});\n", q(n), q(v))),
                Part::File(n, f) => out.push_str(&format!("form.append({}, await openAsBlob({}), {});\n", q(n), q(f), q(file_name(f)))),
            }
        }
        out.push('\n');
    }
    let mut headers: Vec<(String, String)> = spec.headers.iter().map(|(k, v)| (q(k), q(v))).collect();
    if let Some((u, p)) = &spec.basic {
        headers.push((q("Authorization"), format!("\"Basic \" + btoa({})", q(&format!("{u}:{p}")))));
    }
    out.push_str(&format!("const response = await fetch({}, {{\n", q(&spec.url)));
    out.push_str(&format!("  method: {},\n", q(&spec.method)));
    if !headers.is_empty() {
        out.push_str("  headers: {\n");
        for (k, v) in &headers {
            out.push_str(&format!("    {k}: {v},\n"));
        }
        out.push_str("  },\n");
    }
    match &spec.body {
        Body::None => {}
        // A string body gets text/plain from fetch; bytes go out with no Content-Type, as from hitop.
        Body::Raw(text) if content_type(spec).is_none() => out.push_str(&format!("  body: new TextEncoder().encode({}),\n", q(text))),
        Body::Raw(text) | Body::UrlEncoded(text) => out.push_str(&format!("  body: {},\n", q(text))),
        Body::Multipart(_) => out.push_str("  body: form,\n"),
    }
    out.push_str("});\n\nconsole.log(response.status);\nconsole.log(await response.text());\n");
    out
}

/// A name in a Content-Disposition header, quoted as hitop quotes it (`form::multipart`).
fn disposition(s: &str) -> String {
    s.replace('"', "%22").replace(['\r', '\n'], " ")
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

// --- Python requests ---------------------------------------------------------------

fn python(spec: &Spec) -> String {
    let q = |s: &str| quoted(s, Lang::Python);
    let mut out = String::from("import requests\n\n");
    out.push_str(&format!("url = {}\n", q(&spec.url)));
    let mut args = vec![q(&spec.method), "url".to_string()];
    if !spec.headers.is_empty() {
        out.push_str("headers = {\n");
        for (k, v) in &spec.headers {
            out.push_str(&format!("    {}: {},\n", q(k), q(v)));
        }
        out.push_str("}\n");
        args.push("headers=headers".into());
    }
    match &spec.body {
        Body::None => {}
        Body::Raw(text) | Body::UrlEncoded(text) => {
            // Bytes, so requests sends the text as UTF-8 and adds no Content-Type.
            out.push_str(&format!("data = {}.encode(\"utf-8\")\n", q(text)));
            args.push("data=data".into());
        }
        Body::Multipart(rows) => {
            out.push_str("files = [\n");
            for p in rows {
                match p {
                    Part::Text(n, v) => out.push_str(&format!("    ({}, (None, {})),\n", q(n), q(v))),
                    Part::File(n, f) => out.push_str(&format!("    ({}, ({}, open({}, \"rb\"))),\n", q(n), q(file_name(f)), q(f))),
                }
            }
            out.push_str("]\n");
            args.push("files=files".into());
        }
    }
    if let Some((u, p)) = &spec.basic {
        args.push(format!("auth=({}, {})", q(u), q(p)));
    }
    out.push_str(&format!("\nresponse = requests.request({})\n", args.join(", ")));
    out.push_str("print(response.status_code)\nprint(response.text)\n");
    out
}

// --- Go net/http -----------------------------------------------------------------------

fn go(spec: &Spec) -> String {
    let q = |s: &str| quoted(s, Lang::Go);
    let mut imports = vec!["fmt", "io", "net/http"];
    let mut body = String::new();
    let reader = match &spec.body {
        Body::None => "nil".to_string(),
        Body::Raw(text) | Body::UrlEncoded(text) => {
            imports.push("strings");
            body.push_str(&format!("\tbody := strings.NewReader({})\n", go_str(text)));
            "body".into()
        }
        Body::Multipart(rows) => {
            imports.extend(["bytes", "mime/multipart"]);
            body.push_str("\tvar body bytes.Buffer\n\tform := multipart.NewWriter(&body)\n");
            for (i, p) in rows.iter().enumerate() {
                match p {
                    Part::Text(n, v) => body.push_str(&format!("\tform.WriteField({}, {})\n", q(n), q(v))),
                    Part::File(n, f) => {
                        if !imports.contains(&"os") {
                            imports.push("os");
                        }
                        body.push_str(&format!(
                            "\tfile{i}, err := os.Open({})\n\tif err != nil {{\n\t\tpanic(err)\n\t}}\n\tpart{i}, _ := form.CreateFormFile({}, {})\n\tio.Copy(part{i}, file{i})\n\tfile{i}.Close()\n",
                            q(f),
                            q(n),
                            q(file_name(f))
                        ));
                    }
                }
            }
            body.push_str("\tform.Close()\n");
            "&body".into()
        }
    };
    imports.sort();
    let mut out = String::from("package main\n\nimport (\n");
    for i in &imports {
        out.push_str(&format!("\t\"{i}\"\n"));
    }
    out.push_str(")\n\nfunc main() {\n");
    out.push_str(&body);
    out.push_str(&format!("\treq, err := http.NewRequest({}, {}, {reader})\n\tif err != nil {{\n\t\tpanic(err)\n\t}}\n", q(&spec.method), q(&spec.url)));
    for (k, v) in &spec.headers {
        out.push_str(&format!("\treq.Header.Add({}, {})\n", q(k), q(v)));
    }
    if matches!(spec.body, Body::Multipart(_)) {
        out.push_str("\treq.Header.Set(\"Content-Type\", form.FormDataContentType())\n");
    }
    if let Some((u, p)) = &spec.basic {
        out.push_str(&format!("\treq.SetBasicAuth({}, {})\n", q(u), q(p)));
    }
    out.push_str(
        "\n\tresp, err := http.DefaultClient.Do(req)\n\tif err != nil {\n\t\tpanic(err)\n\t}\n\tdefer resp.Body.Close()\n\tout, _ := io.ReadAll(resp.Body)\n\tfmt.Println(resp.Status)\n\tfmt.Println(string(out))\n}\n",
    );
    out
}

// --- Java HttpClient -------------------------------------------------------------------

/// Headers Java's HttpClient refuses to set (it throws).
const JAVA_RESTRICTED: &[&str] = &["connection", "content-length", "expect", "host", "upgrade"];

fn java(spec: &Spec) -> String {
    let q = |s: &str| quoted(s, Lang::Java);
    let mut imports = vec!["java.net.URI", "java.net.http.HttpClient", "java.net.http.HttpRequest", "java.net.http.HttpResponse"];
    let mut pre = String::new();
    let publisher = match &spec.body {
        Body::None if matches!(spec.method.as_str(), "GET" | "DELETE") => None,
        Body::None => Some("HttpRequest.BodyPublishers.noBody()".to_string()),
        Body::Raw(text) | Body::UrlEncoded(text) => Some(format!("HttpRequest.BodyPublishers.ofString({})", q(text))),
        Body::Multipart(rows) => {
            imports.extend(["java.io.ByteArrayOutputStream", "java.nio.charset.StandardCharsets"]);
            pre.push_str("        String boundary = \"----hitop\" + System.nanoTime();\n");
            pre.push_str("        ByteArrayOutputStream body = new ByteArrayOutputStream();\n");
            for p in rows {
                match p {
                    Part::Text(n, v) => {
                        let head = format!("\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{v}\r\n", disposition(n));
                        pre.push_str(&format!("        body.write((\"--\" + boundary + {}).getBytes(StandardCharsets.UTF_8));\n", q(&head)));
                    }
                    Part::File(n, f) => {
                        if !imports.contains(&"java.nio.file.Files") {
                            imports.extend(["java.nio.file.Files", "java.nio.file.Path"]);
                        }
                        let head = format!("\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n", disposition(n), disposition(file_name(f)));
                        pre.push_str(&format!(
                            "        body.write((\"--\" + boundary + {}).getBytes(StandardCharsets.UTF_8));\n        body.write(Files.readAllBytes(Path.of({})));\n        body.write(\"\\r\\n\".getBytes(StandardCharsets.UTF_8));\n",
                            q(&head),
                            q(f)
                        ));
                    }
                }
            }
            pre.push_str("        body.write((\"--\" + boundary + \"--\\r\\n\").getBytes(StandardCharsets.UTF_8));\n");
            Some("HttpRequest.BodyPublishers.ofByteArray(body.toByteArray())".to_string())
        }
    };
    if spec.basic.is_some() {
        for i in ["java.nio.charset.StandardCharsets", "java.util.Base64"] {
            if !imports.contains(&i) {
                imports.push(i);
            }
        }
    }
    imports.sort();
    let mut out = String::new();
    for i in &imports {
        out.push_str(&format!("import {i};\n"));
    }
    out.push_str("\npublic class Main {\n    public static void main(String[] args) throws Exception {\n");
    out.push_str(&pre);
    out.push_str("        HttpClient client = HttpClient.newHttpClient();\n");
    out.push_str("        HttpRequest request = HttpRequest.newBuilder()\n");
    out.push_str(&format!("            .uri(URI.create({}))\n", q(&spec.url)));
    for (k, v) in &spec.headers {
        if JAVA_RESTRICTED.contains(&k.to_ascii_lowercase().as_str()) {
            out.push_str(&format!("            // {k} is set by HttpClient itself\n"));
            continue;
        }
        out.push_str(&format!("            .header({}, {})\n", q(k), q(v)));
    }
    if matches!(spec.body, Body::Multipart(_)) {
        out.push_str("            .header(\"Content-Type\", \"multipart/form-data; boundary=\" + boundary)\n");
    }
    if let Some((u, p)) = &spec.basic {
        out.push_str(&format!(
            "            .header(\"Authorization\", \"Basic \" + Base64.getEncoder().encodeToString({}.getBytes(StandardCharsets.UTF_8)))\n",
            q(&format!("{u}:{p}"))
        ));
    }
    match publisher {
        None if spec.method == "GET" => {}
        None => out.push_str("            .DELETE()\n"),
        Some(p) => out.push_str(&format!("            .method({}, {p})\n", q(&spec.method))),
    }
    out.push_str("            .build();\n");
    out.push_str("        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());\n");
    out.push_str("        System.out.println(response.statusCode());\n        System.out.println(response.body());\n    }\n}\n");
    out
}

// --- Rust reqwest --------------------------------------------------------------------------

fn rust(spec: &Spec) -> String {
    let q = |s: &str| quoted(s, Lang::Rust);
    let multipart = matches!(spec.body, Body::Multipart(_));
    let features = if multipart { "[\"blocking\", \"multipart\"]" } else { "[\"blocking\"]" };
    let mut out = format!("// Cargo.toml: reqwest = {{ version = \"0.12\", features = {features} }}\nfn main() -> Result<(), Box<dyn std::error::Error>> {{\n");
    if let Body::Multipart(rows) = &spec.body {
        out.push_str("    let form = reqwest::blocking::multipart::Form::new()");
        for p in rows {
            match p {
                Part::Text(n, v) => out.push_str(&format!("\n        .text({}, {})", q(n), q(v))),
                Part::File(n, f) => out.push_str(&format!("\n        .file({}, {})?", q(n), q(f))),
            }
        }
        out.push_str(";\n");
    }
    out.push_str("    let client = reqwest::blocking::Client::new();\n    let response = client\n");
    let call = match spec.method.as_str() {
        m @ ("GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD") => format!("        .{}({})\n", m.to_ascii_lowercase(), q(&spec.url)),
        m => format!("        .request(reqwest::Method::from_bytes({}.as_bytes())?, {})\n", q(m), q(&spec.url)),
    };
    out.push_str(&call);
    for (k, v) in &spec.headers {
        out.push_str(&format!("        .header({}, {})\n", q(k), q(v)));
    }
    if let Some((u, p)) = &spec.basic {
        out.push_str(&format!("        .basic_auth({}, Some({}))\n", q(u), q(p)));
    }
    match &spec.body {
        Body::None => {}
        Body::Raw(text) | Body::UrlEncoded(text) => out.push_str(&format!("        .body({})\n", rust_str(text))),
        Body::Multipart(_) => out.push_str("        .multipart(form)\n"),
    }
    out.push_str("        .send()?;\n    println!(\"{}\", response.status());\n    println!(\"{}\", response.text()?);\n    Ok(())\n}\n");
    out
}

/// The `Authorization` value basic auth sends, for tests and callers that
/// compare a snippet with what hitop sends.
pub fn basic_authorization(user: &str, pass: &str) -> String {
    basic_header(user, pass)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ApiKeyLocation, FormField, Header};

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn secrets_and_missing_variables_become_placeholders() {
        let r = Request {
            method: "get".into(),
            url: "${base}/u?k=${key}".into(),
            headers: vec![Header::new("X-Token", "${token}")],
            auth: Some(Auth::Bearer { token: "${token}".into() }),
            ..Default::default()
        };
        let v = vars(&[("base", "https://api.test"), ("token", "s3cr3t")]);
        let s = generate(Lang::Curl, &r, None, &v, &["token".into()]).unwrap();
        assert!(!s.code.contains("s3cr3t"), "{}", s.code);
        assert!(s.code.contains("https://api.test/u?k={{key}}") && s.code.contains("Bearer {{token}}"), "{}", s.code);
        assert_eq!(s.placeholders, vec!["key".to_string(), "token".to_string()]);
    }

    #[test]
    fn no_snippet_for_websocket_requests() {
        let r = Request { name: "Feed".into(), url: "wss://x".into(), kind: Some("websocket".into()), ..Default::default() };
        assert!(generate(Lang::Curl, &r, None, &HashMap::new(), &[]).unwrap_err().to_string().contains("WebSocket"));
        let r = Request { name: "Call".into(), url: "localhost:50051".into(), kind: Some("grpc".into()), ..Default::default() };
        assert!(generate(Lang::Curl, &r, None, &HashMap::new(), &[]).unwrap_err().to_string().contains("gRPC"));
    }

    #[test]
    fn shell_quoting_survives_quotes() {
        assert_eq!(sh("it's"), r"'it'\''s'");
        assert_eq!(quoted("a\"b\\c\n\u{1}", Lang::Java), "\"a\\\"b\\\\c\\n\\001\"");
        assert_eq!(quoted("\u{1}", Lang::Rust), "\"\\u{1}\"");
        assert_eq!(rust_str("say \"#hi\""), "r##\"say \"#hi\"\"##");
    }

    #[test]
    fn hand_set_authorization_wins_over_basic() {
        let r = Request {
            method: "GET".into(),
            url: "https://x".into(),
            headers: vec![Header::new("Authorization", "Token abc")],
            auth: Some(Auth::Basic { username: "u".into(), password: "p".into() }),
            ..Default::default()
        };
        let s = generate(Lang::Python, &r, None, &HashMap::new(), &[]).unwrap();
        assert!(!s.code.contains("auth=") && s.code.contains("Token abc"), "{}", s.code);
    }

    #[test]
    fn api_key_in_query_and_multipart_content_type() {
        let r = Request {
            method: "POST".into(),
            url: "https://x/up".into(),
            headers: vec![Header::new("Content-Type", "multipart/form-data")],
            auth: Some(Auth::ApiKey { key: "api_key".into(), value: "a b".into(), location: ApiKeyLocation::Query }),
            body_mode: Some("multipart".into()),
            form_fields: vec![FormField { key: "n".into(), value: "@v".into(), ..Default::default() }],
            ..Default::default()
        };
        let s = generate(Lang::Curl, &r, None, &HashMap::new(), &[]).unwrap();
        assert!(s.code.contains("'https://x/up?api_key=a%20b'") && s.code.contains("--form-string 'n=@v'") && !s.code.contains("Content-Type"), "{}", s.code);
    }

    #[test]
    fn graphql_variables_with_a_placeholder_go_in_as_written() {
        let r = Request {
            method: "POST".into(),
            url: "https://x/graphql".into(),
            body: "{ me }".into(),
            body_type: Some("graphql".into()),
            graphql: Some(crate::model::Graphql { variables: "{\"id\": ${id}}".into(), ..Default::default() }),
            ..Default::default()
        };
        let s = generate(Lang::Curl, &r, None, &HashMap::new(), &[]).unwrap();
        assert!(s.code.contains(r#"--data-raw '{"query":"{ me }","variables":{"id": {{id}}}}'"#) && s.code.contains("Content-Type: application/json"), "{}", s.code);
    }

    #[test]
    fn languages_parse_from_their_ids() {
        for l in Lang::ALL {
            assert_eq!(Lang::parse(l.id()), Some(l));
        }
        assert_eq!(Lang::parse("JS"), Some(Lang::Fetch));
        assert_eq!(Lang::parse("cobol"), None);
    }
}
