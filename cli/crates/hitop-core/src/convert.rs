//! Insomnia exports (v4 JSON or YAML, v5 YAML), Bruno collections (a folder of `.bru` files, or the
//! exported JSON) and HAR files, turned into Postman's collection and environment shapes: the Postman
//! importer (src/postman.rs) does the rest, so all of them import alike. What can't be carried over
//! is an [`Issue`].
use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::postman::Issue;

/// What [`convert`] makes of other tools' files.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Converted {
    /// `insomnia`, `bruno` or `har`.
    pub format: String,
    /// Postman collections (v2.1).
    pub collections: Vec<Value>,
    /// Postman environments.
    pub environments: Vec<Value>,
    pub issues: Vec<Issue>,
}

impl Converted {
    fn new(format: &str) -> Self {
        Converted { format: format.into(), ..Default::default() }
    }

    fn issue(&mut self, location: &str, what: impl Into<String>, text: impl Into<String>) {
        self.issues.push(Issue { location: location.into(), line: None, what: what.into(), text: text.into() });
    }
}

/// One file of another tool as text: None when it isn't one of them.
pub fn convert_text(text: &str) -> Result<Option<Converted>> {
    let text = text.trim_start_matches('\u{feff}');
    let Some(v) = parse(text) else { return Ok(None) };
    if v["log"]["entries"].is_array() {
        return har(&v).map(Some);
    }
    if v["_type"] == "export" || v["__export_format"].is_number() {
        return insomnia4(&v).map(Some);
    }
    if v["type"].as_str().is_some_and(|t| t.contains("insomnia.rest/")) {
        return insomnia5(&v).map(Some);
    }
    // Bruno's export: items of its own kinds, no Postman `info`.
    if v["items"].is_array() && v["name"].is_string() && (v["version"] == "1" || v["brunoConfig"].is_object()) {
        return Ok(Some(bruno(&v)));
    }
    Ok(None)
}

/// A Bruno collection folder (path with `/` → text): None without a `bruno.json`. The folder may be
/// inside another one (a zip's top folder).
pub fn convert_files(files: &BTreeMap<String, String>) -> Result<Option<Converted>> {
    let Some(root) = files.keys().filter(|p| *p == "bruno.json" || p.ends_with("/bruno.json")).min_by_key(|p| p.len()) else { return Ok(None) };
    let root = root.trim_end_matches("bruno.json").to_string();
    let config: Value = serde_json::from_str(files[&format!("{root}bruno.json")].trim_start_matches('\u{feff}')).context("bruno.json")?;
    let mut issues = vec![];
    let mut dirs: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut folder_files: HashMap<String, Value> = HashMap::new();
    let mut environments = vec![];
    for (path, text) in files {
        let Some(path) = path.strip_prefix(&root).filter(|p| p.ends_with(".bru")) else { continue };
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
        if dir.split('/').any(|d| d == "node_modules" || d == ".git") {
            continue;
        }
        let stem = file.trim_end_matches(".bru");
        let blocks = match bru(text) {
            Ok(b) => b,
            Err(e) => {
                issues.push(Issue { location: path.into(), line: None, what: format!("not read: {e:#}"), text: String::new() });
                continue;
            }
        };
        if dir == "environments" {
            environments.push(bru_environment(stem, &blocks));
        } else if path == "collection.bru" || file == "folder.bru" {
            folder_files.insert(dir.to_string(), bru_request(stem, &blocks));
            // A folder exists by its file too.
            dirs.entry(dir.to_string()).or_default();
        } else {
            dirs.entry(dir.to_string()).or_default().push(bru_request(stem, &blocks));
        }
    }
    // Every folder above one with requests.
    for d in dirs.keys().cloned().collect::<Vec<_>>() {
        let mut up = d.as_str();
        while let Some((p, _)) = up.rsplit_once('/') {
            dirs.entry(p.to_string()).or_default();
            up = p;
        }
    }
    fn items(dir: &str, dirs: &BTreeMap<String, Vec<Value>>, folder_files: &HashMap<String, Value>) -> Vec<Value> {
        let mut out: Vec<Value> = vec![];
        for d in dirs.keys().filter(|d| !d.is_empty() && d.rsplit_once('/').map_or("", |(p, _)| p) == dir) {
            let own = folder_files.get(d).cloned().unwrap_or(json!({}));
            let name = own["name"].as_str().filter(|n| !n.is_empty() && *n != "folder").unwrap_or(d.rsplit('/').next().unwrap_or(d)).to_string();
            out.push(json!({ "type": "folder", "name": name, "seq": own["seq"], "root": own, "items": items(d, dirs, folder_files) }));
        }
        out.extend(dirs.get(dir).cloned().unwrap_or_default());
        out
    }
    let v = json!({
        "name": config["name"].as_str().unwrap_or("Bruno"),
        "version": "1",
        "root": folder_files.get("").cloned().unwrap_or(json!({})),
        "items": items("", &dirs, &folder_files),
        "environments": environments,
    });
    let mut c = bruno(&v);
    issues.append(&mut c.issues);
    c.issues = issues;
    Ok(Some(c))
}

/// JSON, or YAML that is a mapping.
fn parse(text: &str) -> Option<Value> {
    if text.trim_start().starts_with(['{', '[']) {
        return serde_json::from_str(text).ok();
    }
    yaml_serde::from_str::<Value>(text).ok().filter(Value::is_object)
}

fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn list(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

fn header_row(name: &str, value: &str, disabled: bool) -> Value {
    json!({ "key": name, "value": value, "disabled": disabled })
}

fn has_header(headers: &[Value], name: &str) -> bool {
    headers.iter().any(|h| h["disabled"] != true && h["key"].as_str().is_some_and(|k| k.eq_ignore_ascii_case(name)))
}

/// A URL with the enabled query rows added (those it doesn't have) and the disabled ones apart.
fn url_with(raw: &str, query: &[(String, String, bool)]) -> Value {
    let mut url = raw.to_string();
    let mut rows = vec![];
    for (k, v, on) in query {
        if *on {
            let pair = format!("{k}={v}");
            if !url.split_once('?').is_some_and(|(_, q)| q.split('&').any(|p| p == pair)) {
                url.push(if url.contains('?') { '&' } else { '?' });
                url.push_str(&pair);
            }
        } else {
            rows.push(json!({ "key": k, "value": v, "disabled": true }));
        }
    }
    json!({ "raw": url, "query": rows })
}

fn script_event(listen: &str, code: &str) -> Value {
    json!({ "listen": listen, "script": { "exec": code.lines().collect::<Vec<_>>() } })
}

fn auth(kind: &str, fields: &[(&str, &str)]) -> Value {
    let map: Map<String, Value> = fields.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    json!({ "type": kind, kind: map })
}

fn collection(name: &str, item: Vec<Value>) -> Value {
    json!({ "info": { "name": name, "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" }, "item": item })
}

fn environment(name: &str, vars: &[(String, String, bool)]) -> Value {
    let values: Vec<Value> = vars.iter().map(|(k, v, secret)| json!({ "key": k, "value": v, "enabled": true, "type": if *secret { "secret" } else { "default" } })).collect();
    json!({ "name": name, "values": values })
}

/// `{a: {b: 1}}` as `a.b` = `1`.
fn flatten(v: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
    for (k, x) in v.as_object().into_iter().flatten() {
        let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        if x.is_object() {
            flatten(x, &key, out);
        } else {
            out.push((key, text(x)));
        }
    }
}

// ---- Insomnia ---------------------------------------------------------------

/// `{{ _.name }}` → `{{name}}`.
fn insomnia_text(s: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{-?\s*(?:_\.)?([\w.\-]+)\s*-?\}\}").unwrap()).replace_all(s, "{{$1}}").into_owned()
}

fn insomnia_script(s: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\binsomnia\.").unwrap()).replace_all(s, "pm.").into_owned()
}

fn insomnia_auth(a: &Value, location: &str, c: &mut Converted) -> Option<Value> {
    let kind = a["type"].as_str().filter(|k| !k.is_empty())?;
    if a["disabled"] == true {
        return None;
    }
    let get = |k: &str| insomnia_text(&text(&a[k]));
    let pair = |k: &'static str, v: &str| (k, v.to_string());
    let fields: Vec<(&str, String)> = match kind {
        "none" => return Some(json!({ "type": "noauth" })),
        "bearer" => {
            if !get("prefix").is_empty() && !get("prefix").eq_ignore_ascii_case("bearer") {
                c.issue(location, "the bearer prefix", get("prefix"));
            }
            vec![pair("token", &get("token"))]
        }
        "basic" | "digest" => vec![pair("username", &get("username")), pair("password", &get("password"))],
        "apikey" => vec![pair("key", &get("key")), pair("value", &get("value")), pair("in", if get("addTo").to_lowercase().contains("query") { "query" } else { "header" })],
        "iam" => {
            let (id, key, token) = (get("accessKeyId"), get("secretAccessKey"), get("sessionToken"));
            let (region, service) = (get("region"), get("service"));
            return Some(auth("awsv4", &[("accessKey", &id), ("secretKey", &key), ("sessionToken", &token), ("region", &region), ("service", &service)]));
        }
        "oauth2" => {
            let grant = match get("grantType").as_str() {
                "client_credentials" => "client_credentials",
                "password" => "password_credentials",
                "authorization_code" => "authorization_code_with_pkce",
                _ => "unsupported",
            };
            vec![
                pair("grant_type", grant),
                pair("accessTokenUrl", &get("accessTokenUrl")),
                pair("authUrl", &get("authorizationUrl")),
                pair("clientId", &get("clientId")),
                pair("clientSecret", &get("clientSecret")),
                pair("scope", &get("scope")),
                pair("username", &get("username")),
                pair("password", &get("password")),
                pair("redirect_uri", &get("redirectUrl")),
                pair("client_authentication", if a["credentialsInBody"] == true { "body" } else { "header" }),
            ]
        }
        // Reported by the Postman importer.
        _ => vec![],
    };
    let refs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Some(auth(kind, &refs))
}

fn insomnia_rows(rows: &Value) -> Vec<(String, String, bool)> {
    list(rows).iter().filter(|r| !text(&r["name"]).is_empty()).map(|r| (insomnia_text(&text(&r["name"])), insomnia_text(&text(&r["value"])), r["disabled"] != true)).collect()
}

fn insomnia_body(b: &Value, headers: &mut Vec<Value>, location: &str, c: &mut Converted) -> Value {
    let mime = b["mimeType"].as_str().unwrap_or("");
    let body = insomnia_text(&text(&b["text"]));
    let params = |c: &mut Converted| -> Vec<Value> {
        list(&b["params"])
            .iter()
            .filter(|p| !text(&p["name"]).is_empty())
            .map(|p| {
                let file = p["type"] == "file";
                if p["multiline"].is_string() && file {
                    c.issue(location, "a file field with text", text(&p["name"]));
                }
                json!({ "key": insomnia_text(&text(&p["name"])), "value": insomnia_text(&text(&p["value"])), "type": if file { "file" } else { "text" }, "disabled": p["disabled"] == true })
            })
            .collect()
    };
    match mime {
        "application/x-www-form-urlencoded" => json!({ "mode": "urlencoded", "urlencoded": params(c) }),
        "multipart/form-data" => {
            // hitop writes the header with its own boundary.
            headers.retain(|h| !h["key"].as_str().is_some_and(|k| k.eq_ignore_ascii_case("content-type")));
            json!({ "mode": "formdata", "formdata": params(c) })
        }
        "application/graphql" => {
            let g: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({ "query": body }));
            json!({ "mode": "graphql", "graphql": { "query": g["query"], "variables": g["variables"] } })
        }
        "application/octet-stream" if b["fileName"].is_string() => {
            c.issue(location, "file body: attach the file again", text(&b["fileName"]));
            Value::Null
        }
        _ => {
            if !mime.is_empty() && !body.is_empty() && !has_header(headers, "content-type") {
                headers.push(header_row("Content-Type", mime, false));
            }
            json!({ "mode": "raw", "raw": body })
        }
    }
}

fn insomnia_request(r: &Value, path: &str, c: &mut Converted) -> Value {
    let name = text(&r["name"]);
    let location = if path.is_empty() { name.clone() } else { format!("{path} / {name}") };
    let mut headers: Vec<Value> = insomnia_rows(&r["headers"]).iter().map(|(k, v, on)| header_row(k, v, !on)).collect();
    let body = insomnia_body(&r["body"], &mut headers, &location, c);
    let mut url = insomnia_text(&text(&r["url"]));
    for p in list(&r["pathParameters"]) {
        url = path_param(&url, &text(&p["name"]), &insomnia_text(&text(&p["value"])));
    }
    let query = insomnia_rows(&r["parameters"]);
    let mut events = vec![];
    for (listen, code) in [("prerequest", &r["scripts"]["preRequest"]), ("prerequest", &r["preRequestScript"]), ("test", &r["scripts"]["afterResponse"]), ("test", &r["afterResponseScript"])] {
        if !text(code).trim().is_empty() {
            events.push(script_event(listen, &insomnia_script(&text(code))));
        }
    }
    let whole = r.to_string();
    if whole.contains("{%") {
        c.issue(&location, "template tags ({% ... %})", "");
    }
    let mut req = json!({ "method": r["method"].as_str().unwrap_or("GET"), "url": url_with(&url, &query), "header": headers, "body": body });
    if let Some(a) = insomnia_auth(&r["authentication"], &location, c) {
        req["auth"] = a;
    }
    json!({ "name": name, "request": req, "event": events })
}

/// `:name` in a URL's path as `value`.
fn path_param(url: &str, name: &str, value: &str) -> String {
    let (path, query) = url.split_once('?').map_or((url, None), |(p, q)| (p, Some(q)));
    let path: Vec<&str> = path.split('/').map(|s| if !name.is_empty() && s.strip_prefix(':') == Some(name) { value } else { s }).collect();
    query.map_or(path.join("/"), |q| format!("{}?{q}", path.join("/")))
}

/// Folder variables have no place of their own: the collection's, when the name is free.
fn folder_environment(v: &Value, location: &str, vars: &mut Vec<(String, String)>, c: &mut Converted) {
    let mut own = vec![];
    flatten(v, "", &mut own);
    for (k, value) in own {
        let value = insomnia_text(&value);
        match vars.iter().find(|(n, _)| *n == k) {
            Some((_, x)) if *x == value => {}
            Some(_) => c.issue(location, "a folder variable with a name already in use", k),
            None => {
                c.issue(location, "a folder variable: now a collection variable", k.clone());
                vars.push((k, value));
            }
        }
    }
}

/// The base environment as collection variables, each sub environment as an environment.
fn insomnia_environments(base: &Value, subs: &[(String, Value)], c: &mut Converted) -> Vec<(String, String)> {
    let mut vars = vec![];
    flatten(base, "", &mut vars);
    for (_, v) in vars.iter_mut() {
        *v = insomnia_text(v);
    }
    for (name, data) in subs {
        let mut own = vec![];
        flatten(data, "", &mut own);
        let own: Vec<(String, String, bool)> = own.into_iter().map(|(k, v)| (k, placeholders(&insomnia_text(&v)), false)).collect();
        c.environments.push(environment(name, &own));
    }
    vars
}

fn finish(c: &mut Converted, name: &str, items: Vec<Value>, vars: Vec<(String, String)>) {
    let mut col = collection(name, items);
    col["variable"] = vars.iter().map(|(k, v)| json!({ "key": k, "value": v })).collect();
    c.collections.push(col);
}

fn insomnia5(v: &Value) -> Result<Converted> {
    let mut c = Converted::new("insomnia");
    let kind = text(&v["type"]);
    if !(kind.starts_with("collection.insomnia.rest/5") || kind.starts_with("spec.insomnia.rest/5") || kind.starts_with("environment.insomnia.rest/5")) {
        bail!("this kind of Insomnia file isn't supported: {kind} (a collection, a design document or environments, export version 5)");
    }
    let env = &v["environments"];
    let subs: Vec<(String, Value)> = list(&env["subEnvironments"]).iter().map(|s| (text(&s["name"]), s["data"].clone())).collect();
    let mut vars = insomnia_environments(&env["data"], &subs, &mut c);
    if kind.starts_with("environment.") {
        let base: Vec<(String, String, bool)> = vars.into_iter().map(|(k, v)| (k, placeholders(&v), false)).collect();
        c.environments.insert(0, environment(&text(&v["name"]), &base));
        return Ok(c);
    }
    fn items(list_: &[Value], path: &str, vars: &mut Vec<(String, String)>, c: &mut Converted) -> Vec<Value> {
        let mut sorted: Vec<&Value> = list_.iter().collect();
        sorted.sort_by(|a, b| a["meta"]["sortKey"].as_f64().unwrap_or(0.0).total_cmp(&b["meta"]["sortKey"].as_f64().unwrap_or(0.0)));
        let mut out = vec![];
        for x in sorted {
            let name = text(&x["name"]);
            let here = if path.is_empty() { name.clone() } else { format!("{path} / {name}") };
            if x["children"].is_array() || (x["url"].is_null() && x["method"].is_null()) {
                folder_environment(&x["environment"], &here, vars, c);
                let children = items(list(&x["children"]), &here, vars, c);
                out.push(insomnia_folder(x, &name, &here, children, c));
            } else if x["method"].is_null() {
                c.issue(&here, "a WebSocket, gRPC or other non-HTTP request", "");
            } else {
                out.push(insomnia_request(x, path, c));
            }
        }
        out
    }
    let found = items(list(&v["collection"]), "", &mut vars, &mut c);
    let suites = list(&v["testSuites"]).len();
    if suites > 0 {
        c.issue("collection", "unit test suites", suites.to_string());
    }
    let name = text(&v["name"]);
    finish(&mut c, if name.is_empty() { "Insomnia" } else { &name }, found, vars);
    Ok(c)
}

fn insomnia_folder(f: &Value, name: &str, here: &str, children: Vec<Value>, c: &mut Converted) -> Value {
    let mut out = json!({ "name": name, "item": children });
    if let Some(a) = insomnia_auth(&f["authentication"], here, c) {
        out["auth"] = a;
    }
    let headers = insomnia_rows(&f["headers"]);
    if !headers.is_empty() {
        add_headers(&mut out, &headers);
    }
    let mut events = vec![];
    for (listen, code) in [("prerequest", &f["scripts"]["preRequest"]), ("prerequest", &f["preRequestScript"]), ("test", &f["scripts"]["afterResponse"]), ("test", &f["afterResponseScript"])] {
        if !text(code).trim().is_empty() {
            events.push(script_event(listen, &insomnia_script(&text(code))));
        }
    }
    out["event"] = json!(events);
    out
}

/// A folder's headers in front of each request's own (not those it sets itself).
fn add_headers(node: &mut Value, headers: &[(String, String, bool)]) {
    if let Some(items) = node["item"].as_array_mut() {
        for i in items {
            add_headers(i, headers);
        }
        return;
    }
    let own = list(&node["request"]["header"]).to_vec();
    let mut all: Vec<Value> = headers.iter().filter(|(k, _, on)| *on && !has_header(&own, k)).map(|(k, v, _)| header_row(k, v, false)).collect();
    all.extend(own);
    node["request"]["header"] = json!(all);
}

fn insomnia4(v: &Value) -> Result<Converted> {
    if v["__export_format"] != 4 {
        bail!("Insomnia export format {} isn't supported: export again as Insomnia v4 or v5", v["__export_format"]);
    }
    let mut c = Converted::new("insomnia");
    let mut children: HashMap<&str, Vec<&Value>> = HashMap::new();
    for r in list(&v["resources"]) {
        if let Some(p) = r["parentId"].as_str() {
            children.entry(p).or_default().push(r);
        }
    }
    for kids in children.values_mut() {
        kids.sort_by(|a, b| a["metaSortKey"].as_f64().unwrap_or(0.0).total_cmp(&b["metaSortKey"].as_f64().unwrap_or(0.0)));
    }
    let of = |id: &str| children.get(id).cloned().unwrap_or_default();
    type Kids<'a> = HashMap<&'a str, Vec<&'a Value>>;
    fn items(id: &str, path: &str, children: &Kids, vars: &mut Vec<(String, String)>, c: &mut Converted) -> Vec<Value> {
        let mut out = vec![];
        for r in children.get(id).into_iter().flatten() {
            let name = text(&r["name"]);
            let here = if path.is_empty() { name.clone() } else { format!("{path} / {name}") };
            match r["_type"].as_str().unwrap_or("") {
                "request" => out.push(insomnia_request(r, path, c)),
                "request_group" => {
                    folder_environment(&r["environment"], &here, vars, c);
                    let inner = items(r["_id"].as_str().unwrap_or(""), &here, children, vars, c);
                    out.push(insomnia_folder(r, &name, &here, inner, c));
                }
                "websocket_request" | "grpc_request" | "socketio_request" => c.issue(&here, "a WebSocket, gRPC or other non-HTTP request", ""),
                _ => {}
            }
        }
        out
    }
    for ws in list(&v["resources"]).iter().filter(|r| r["_type"] == "workspace") {
        let id = ws["_id"].as_str().unwrap_or("");
        let mut base = json!({});
        let mut subs = vec![];
        for e in of(id).into_iter().filter(|r| r["_type"] == "environment") {
            base = e["data"].clone();
            subs.extend(of(e["_id"].as_str().unwrap_or("")).into_iter().filter(|r| r["_type"] == "environment").map(|s| (text(&s["name"]), s["data"].clone())));
        }
        let mut vars = insomnia_environments(&base, &subs, &mut c);
        let found = items(id, "", &children, &mut vars, &mut c);
        let suites = of(id).iter().filter(|r| r["_type"] == "unit_test_suite").count();
        if suites > 0 {
            c.issue("collection", "unit test suites", suites.to_string());
        }
        if found.is_empty() && vars.is_empty() && subs.is_empty() {
            continue;
        }
        let name = text(&ws["name"]);
        finish(&mut c, if name.is_empty() { "Insomnia" } else { &name }, found, vars);
    }
    if c.collections.is_empty() {
        bail!("this Insomnia export has no requests");
    }
    Ok(c)
}

// ---- Bruno --------------------------------------------------------------------

/// A `.bru` file's blocks: name → its lines, two spaces of indent off.
fn bru(s: &str) -> Result<Vec<(String, Vec<String>)>> {
    let mut out: Vec<(String, Vec<String>)> = vec![];
    let mut open: Option<(String, char, Vec<String>)> = None;
    for line in s.trim_start_matches('\u{feff}').lines() {
        match &mut open {
            Some((_, end, lines)) => {
                if line.trim_end() == end.to_string() && line.starts_with(*end) {
                    let (name, _, lines) = open.take().unwrap();
                    out.push((name, lines));
                } else {
                    lines.push(line.strip_prefix("  ").unwrap_or(line).to_string());
                }
            }
            None => {
                let l = line.trim();
                if l.is_empty() || l.starts_with("//") || l.starts_with('#') {
                    continue;
                }
                let (name, end) = match (l.strip_suffix('{'), l.strip_suffix('[')) {
                    (Some(n), _) => (n, '}'),
                    (_, Some(n)) => (n, ']'),
                    // `name { }` on one line, or `name [a, b]`.
                    _ => match l.split_once(['{', '[']) {
                        Some((n, rest)) if rest.ends_with(['}', ']']) => {
                            let inner = &rest[..rest.len() - 1];
                            out.push((n.trim().into(), inner.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()));
                            continue;
                        }
                        _ => bail!("unexpected line: {l}"),
                    },
                };
                open = Some((name.trim().to_string(), end, vec![]));
            }
        }
    }
    if let Some((name, ..)) = open {
        bail!("the block {name} isn't closed");
    }
    Ok(out)
}

/// A dictionary block's rows: (key, value, enabled). Annotation lines are left out.
fn bru_rows(lines: &[String]) -> Vec<(String, String, bool)> {
    let mut out = vec![];
    let mut it = lines.iter();
    while let Some(line) = it.next() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let multiline = |l: &str, it: &mut std::slice::Iter<String>| -> Option<String> {
            let (_, start) = l.split_once("'''")?;
            if start.contains("'''") {
                return None;
            }
            let mut text = vec![];
            if !start.trim().is_empty() {
                text.push(start.to_string());
            }
            for next in it.by_ref() {
                if let Some((last, _)) = next.split_once("'''") {
                    if !last.trim().is_empty() {
                        text.push(last.trim_start().to_string());
                    }
                    break;
                }
                text.push(next.strip_prefix("  ").unwrap_or(next).to_string());
            }
            Some(text.join("\n"))
        };
        if l.starts_with('@') {
            multiline(l, &mut it);
            continue;
        }
        let (l, on) = l.strip_prefix('~').map_or((l, true), |l| (l, false));
        let (key, value) = match l.strip_prefix('"').and_then(|r| r.split_once("\":")) {
            Some((k, v)) => (k, v),
            None => l.split_once(':').unwrap_or((l, "")),
        };
        let value = if value.trim_start().starts_with("'''") { multiline(value, &mut it).unwrap_or_else(|| value.replace("'''", "").trim().to_string()) } else { value.trim().to_string() };
        out.push((key.trim().to_string(), value, on));
    }
    out
}

fn bru_environment(name: &str, blocks: &[(String, Vec<String>)]) -> Value {
    let mut vars = vec![];
    for (block, lines) in blocks {
        match block.as_str() {
            "vars" => vars.extend(bru_rows(lines).into_iter().map(|(k, v, on)| json!({ "name": k, "value": v, "enabled": on }))),
            "vars:secret" => vars.extend(lines.iter().flat_map(|l| l.split(',')).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).map(|n| {
                let on = !n.starts_with('~');
                json!({ "name": n.trim_start_matches('~'), "value": "", "enabled": on, "secret": true })
            })),
            _ => {}
        }
    }
    json!({ "name": name, "variables": vars })
}

/// A `.bru` request, `folder.bru` or `collection.bru` in the shape of Bruno's JSON export.
fn bru_request(stem: &str, blocks: &[(String, Vec<String>)]) -> Value {
    let rows = |name: &str| -> Vec<(String, String, bool)> { blocks.iter().filter(|(n, _)| n == name).flat_map(|(_, l)| bru_rows(l)).collect() };
    let raw = |name: &str| blocks.iter().find(|(n, _)| n == name).map(|(_, l)| l.join("\n")).unwrap_or_default();
    let map = |name: &str| -> Map<String, Value> { rows(name).into_iter().map(|(k, v, _)| (k, json!(v))).collect() };
    let table = |name: &str| -> Vec<Value> { rows(name).into_iter().map(|(k, v, on)| json!({ "name": k, "value": v, "enabled": on })).collect() };
    let meta = map("meta");
    const METHODS: [&str; 9] = ["get", "post", "put", "delete", "patch", "options", "head", "trace", "connect"];
    let verb = blocks.iter().map(|(n, _)| n.as_str()).find(|n| METHODS.contains(n) || *n == "http");
    let call = verb.map(map).unwrap_or_default();
    let method = match verb {
        Some("http") => text(call.get("method").unwrap_or(&Value::Null)),
        v => v.unwrap_or("get").to_string(),
    };
    let mode = call.get("auth").cloned().or_else(|| map("auth").get("mode").cloned()).unwrap_or(Value::Null);
    let mut a = json!({ "mode": mode });
    for (n, _) in blocks {
        if let Some(kind) = n.strip_prefix("auth:") {
            a[kind] = Value::Object(map(n));
        }
    }
    let mut body = json!({ "mode": call.get("body").cloned().unwrap_or(json!("none")) });
    for (mode, block) in [("json", "body:json"), ("text", "body:text"), ("xml", "body:xml"), ("sparql", "body:sparql")] {
        body[mode] = json!(raw(block));
    }
    if body["json"] == "" {
        body["json"] = json!(raw("body"));
    }
    body["graphql"] = json!({ "query": raw("body:graphql"), "variables": raw("body:graphql:vars") });
    body["formUrlEncoded"] = json!(table("body:form-urlencoded"));
    body["multipartForm"] = json!(table("body:multipart-form"));
    body["file"] = json!(table("body:file"));
    let mut params: Vec<Value> = vec![];
    for (kind, block) in [("query", "params:query"), ("query", "query"), ("path", "params:path")] {
        params.extend(table(block).into_iter().map(|mut p| {
            p["type"] = json!(kind);
            p
        }));
    }
    let seq = meta.get("seq").and_then(Value::as_str).and_then(|s| s.parse::<f64>().ok());
    json!({
        "type": match meta.get("type").and_then(Value::as_str) { Some("graphql") => "graphql-request", None | Some("http") => "http-request", Some(_) => "other" },
        "name": meta.get("name").and_then(Value::as_str).unwrap_or(stem),
        "seq": seq,
        "request": {
            "url": call.get("url").cloned().unwrap_or(Value::Null),
            "method": method.to_uppercase(),
            "headers": table("headers"),
            "params": params,
            "body": body,
            "auth": a,
            "script": { "req": raw("script:pre-request"), "res": raw("script:post-response") },
            "vars": { "req": table("vars:pre-request"), "res": table("vars:post-response") },
            "assertions": table("assert"),
            "tests": raw("tests"),
        },
    })
}

fn bruno_auth(a: &Value) -> Option<Value> {
    let mode = a["mode"].as_str().unwrap_or("");
    let g = |k: &str| text(&a[mode][k]);
    Some(match mode {
        "" | "inherit" => return None,
        "none" => json!({ "type": "noauth" }),
        "bearer" => auth("bearer", &[("token", &g("token"))]),
        "basic" | "digest" => auth(mode, &[("username", &g("username")), ("password", &g("password"))]),
        "apikey" => auth("apikey", &[("key", &g("key")), ("value", &g("value")), ("in", if g("placement").contains("query") { "query" } else { "header" })]),
        "awsv4" => auth("awsv4", &[("accessKey", &g("accessKeyId")), ("secretKey", &g("secretAccessKey")), ("sessionToken", &g("sessionToken")), ("region", &g("region")), ("service", &g("service"))]),
        "oauth2" => {
            // `.bru` files write snake_case, the JSON export camelCase.
            let either = |x: &str, y: &str| if g(x).is_empty() { g(y) } else { g(x) };
            let grant = match either("grant_type", "grantType").as_str() {
                "password" => "password_credentials",
                "client_credentials" => "client_credentials",
                "authorization_code" => "authorization_code_with_pkce",
                _ => "unsupported",
            };
            auth("oauth2", &[
                ("grant_type", grant),
                ("accessTokenUrl", &either("access_token_url", "accessTokenUrl")),
                ("authUrl", &either("authorization_url", "authorizationUrl")),
                ("redirect_uri", &either("callback_url", "callbackUrl")),
                ("clientId", &either("client_id", "clientId")),
                ("clientSecret", &either("client_secret", "clientSecret")),
                ("scope", &g("scope")),
                ("username", &g("username")),
                ("password", &g("password")),
                ("client_authentication", if either("credentials_placement", "credentialsPlacement") == "body" { "body" } else { "header" }),
            ])
        }
        other => auth(other, &[]),
    })
}

/// `eq` and `neq` of a status or a field of a JSON body; None for anything else.
fn bruno_assertion(expr: &str, value: &str) -> Option<String> {
    let (op, operand) = match value.split_once(' ') {
        Some((op @ ("eq" | "neq"), v)) => (op, v.trim()),
        _ => ("eq", value.trim()),
    };
    const OPS: [&str; 24] = ["gt", "gte", "lt", "lte", "in", "notIn", "contains", "notContains", "length", "matches", "notMatches", "startsWith", "endsWith", "between", "isEmpty", "isNotEmpty", "isNull", "isUndefined", "isDefined", "isTruthy", "isFalsy", "isJson", "isNumber", "isString"];
    let first = value.split(' ').next().unwrap_or("");
    if operand.contains("{{") || OPS.contains(&first) || ["isBoolean", "isArray"].contains(&first) {
        return None;
    }
    let title = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let not = if op == "neq" { ".not" } else { "" };
    if expr == "res.status" {
        let code: u16 = operand.parse().ok()?;
        return Some(format!("pm.test({}, () => pm.expect(pm.response.code).to{not}.equal({code}));", title(&format!("status {op} {code}"))));
    }
    let path = expr.strip_prefix("res.body.")?;
    let ident = |s: &str| !s.is_empty() && !s.starts_with(|c: char| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !path.split('.').all(ident) {
        return None;
    }
    let quoted = |q: char| operand.len() >= 2 && operand.starts_with(q) && operand.ends_with(q);
    let literal = if quoted('"') || quoted('\'') {
        title(&operand[1..operand.len() - 1])
    } else {
        match serde_json::from_str::<Value>(operand) {
            Ok(v) if v.is_number() || v.is_boolean() || v.is_null() => v.to_string(),
            _ => title(operand),
        }
    };
    Some(format!("pm.test({}, () => pm.expect(pm.response.json().{path}).to{not}.eql({literal}));", title(&format!("{path} {op} {operand}"))))
}

/// The events of a request (or folder): assertions as tests; Bruno's own scripts stay as comments.
fn bruno_events(r: &Value, location: &str, c: &mut Converted) -> Vec<Value> {
    let mut pre = String::new();
    let mut post = String::new();
    let comment = |what: &str, code: &str, into: &mut String, c: &mut Converted| {
        if code.trim().is_empty() {
            return;
        }
        c.issue(location, format!("{what}: kept as a comment, Bruno's script API isn't HITOP's"), "");
        into.push_str(&format!("// From Bruno ({what}), not converted:\n"));
        for l in code.lines() {
            into.push_str(&format!("// {l}\n"));
        }
    };
    comment("pre-request script", &text(&r["script"]["req"]), &mut pre, c);
    for a in list(&r["assertions"]).iter().filter(|a| a["enabled"] != false) {
        let (expr, value) = (text(&a["name"]), text(&a["value"]));
        match bruno_assertion(&expr, &value) {
            Some(code) => post.push_str(&format!("{code}\n")),
            None => c.issue(location, "an assertion", format!("{expr}: {value}")),
        }
    }
    comment("post-response script", &text(&r["script"]["res"]), &mut post, c);
    comment("tests", &text(&r["tests"]), &mut post, c);
    for (kind, what) in [("req", "a pre-request variable"), ("res", "a post-response variable")] {
        for v in list(&r["vars"][kind]).iter().filter(|v| v["enabled"] != false) {
            c.issue(location, what, text(&v["name"]));
        }
    }
    let mut out = vec![];
    for (listen, code) in [("prerequest", pre), ("test", post)] {
        if !code.is_empty() {
            out.push(script_event(listen, &code));
        }
    }
    out
}

fn bruno_rows(v: &Value) -> Vec<(String, String, bool)> {
    list(v).iter().filter(|r| !text(&r["name"]).is_empty()).map(|r| (text(&r["name"]), text(&r["value"]), r["enabled"] != false)).collect()
}

fn bruno_item(item: &Value, path: &str, c: &mut Converted) -> Option<Value> {
    let name = text(&item["name"]);
    let here = if path.is_empty() { name.clone() } else { format!("{path} / {name}") };
    if item["type"] == "folder" || item["items"].is_array() {
        let root = &item["root"]["request"];
        let mut out = json!({ "name": name, "item": bruno_items(&item["items"], &here, c), "event": bruno_events(root, &here, c) });
        if let Some(a) = bruno_auth(&root["auth"]) {
            out["auth"] = a;
        }
        add_headers(&mut out, &bruno_rows(&root["headers"]));
        return Some(out);
    }
    if !matches!(item["type"].as_str(), None | Some("http-request" | "graphql-request" | "http" | "graphql")) {
        c.issue(&here, "a WebSocket, gRPC, script or other non-HTTP request", "");
        return None;
    }
    let r = &item["request"];
    let mut headers: Vec<Value> = bruno_rows(&r["headers"]).iter().map(|(k, v, on)| header_row(k, v, !on)).collect();
    let mut url = text(&r["url"]);
    let mut query = vec![];
    for p in list(&r["params"]) {
        let (k, v, on) = (text(&p["name"]), text(&p["value"]), p["enabled"] != false);
        if p["type"] == "path" {
            url = path_param(&url, &k, &v);
        } else if !k.is_empty() {
            // Bruno's URL has the enabled ones already (perhaps written differently).
            if on && url.split_once('?').is_some_and(|(_, q)| q.split('&').any(|p| p.split('=').next() == Some(&k))) {
                continue;
            }
            query.push((k, v, on));
        }
    }
    let b = &r["body"];
    let mode = b["mode"].as_str().unwrap_or("none");
    let typed = |headers: &mut Vec<Value>, mime: &str| {
        if !has_header(headers, "content-type") {
            headers.push(header_row("Content-Type", mime, false));
        }
    };
    let file = Regex::new(r"^@file\((.*?)\)").unwrap();
    let kind = Regex::new(r"\s*@contentType\([^)]*\)\s*$").unwrap();
    let body = match mode {
        "json" | "text" | "xml" | "sparql" => {
            typed(&mut headers, match mode { "json" => "application/json", "xml" => "application/xml", "sparql" => "application/sparql-query", _ => "text/plain" });
            json!({ "mode": "raw", "raw": text(&b[mode]) })
        }
        "formUrlEncoded" => json!({ "mode": "urlencoded", "urlencoded": bruno_rows(&b["formUrlEncoded"]).iter().map(|(k, v, on)| header_row(k, v, !on)).collect::<Vec<_>>() }),
        "multipartForm" => {
            headers.retain(|h| !h["key"].as_str().is_some_and(|k| k.eq_ignore_ascii_case("content-type")));
            let rows: Vec<Value> = list(&b["multipartForm"])
                .iter()
                .filter(|f| !text(&f["name"]).is_empty())
                .map(|f| {
                    let v = if f["value"].is_array() { String::new() } else { text(&f["value"]) };
                    let is_file = f["type"] == "file" || file.is_match(&v);
                    json!({ "key": f["name"], "value": kind.replace(&v, ""), "type": if is_file { "file" } else { "text" }, "disabled": f["enabled"] == false })
                })
                .collect();
            json!({ "mode": "formdata", "formdata": rows })
        }
        "graphql" => json!({ "mode": "graphql", "graphql": { "query": b["graphql"]["query"], "variables": b["graphql"]["variables"] } }),
        "file" => {
            c.issue(&here, "file body: attach the file again", "");
            Value::Null
        }
        _ => Value::Null,
    };
    let mut req = json!({ "method": r["method"].as_str().unwrap_or("GET"), "url": url_with(&url, &query), "header": headers, "body": body });
    if let Some(a) = bruno_auth(&r["auth"]) {
        req["auth"] = a;
    }
    if r.to_string().contains("{{process.env.") {
        c.issue(&here, "process.env variables", "");
    }
    Some(json!({ "name": name, "request": req, "event": bruno_events(r, &here, c) }))
}

fn bruno_items(items: &Value, path: &str, c: &mut Converted) -> Vec<Value> {
    let mut sorted: Vec<&Value> = list(items).iter().collect();
    let is_folder = |v: &Value| v["type"] == "folder" || v["items"].is_array();
    // Folders first, then by place; the given order where there is none.
    sorted.sort_by(|a, b| is_folder(b).cmp(&is_folder(a)).then(a["seq"].as_f64().unwrap_or(f64::MAX).total_cmp(&b["seq"].as_f64().unwrap_or(f64::MAX))));
    sorted.into_iter().filter_map(|i| bruno_item(i, path, c)).collect()
}

fn bruno(v: &Value) -> Converted {
    let mut c = Converted::new("bruno");
    let root = &v["root"]["request"];
    let mut col = collection(v["name"].as_str().unwrap_or("Bruno"), bruno_items(&v["items"], "", &mut c));
    // Its variables become the collection's own.
    let mut own = root.clone();
    let vars = bruno_rows(&own["vars"]["req"]);
    if own["vars"].is_object() {
        own["vars"]["req"] = json!([]);
    }
    col["variable"] = vars.iter().filter(|(_, _, on)| *on).map(|(k, v, _)| json!({ "key": k, "value": v })).collect();
    col["event"] = json!(bruno_events(&own, "collection", &mut c));
    if let Some(a) = bruno_auth(&root["auth"]) {
        col["auth"] = a;
    }
    add_headers(&mut col, &bruno_rows(&root["headers"]));
    c.collections.push(col);
    for e in list(&v["environments"]) {
        let rows: Vec<(String, String, bool)> = list(&e["variables"]).iter().filter(|r| r["enabled"] != false && !text(&r["name"]).is_empty()).map(|r| (text(&r["name"]), text(&r["value"]), r["secret"] == true)).collect();
        let name = text(&e["name"]);
        for (k, _, _) in rows.iter().filter(|r| r.1.contains("{{process.env.")) {
            c.issue(&format!("environment {name}"), "a process.env variable", k.clone());
        }
        // `{{x}}` in a value is `${x}` in hitop.
        let rows: Vec<_> = rows.into_iter().map(|(k, v, s)| (k, placeholders(&v), s)).collect();
        c.environments.push(environment(&name, &rows));
    }
    c
}

fn placeholders(s: &str) -> String {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{\s*([\w.\-]+)\s*\}\}").unwrap()).replace_all(s, "$${$1}").into_owned()
}

// ---- HAR ----------------------------------------------------------------------

/// Headers the HTTP client writes itself.
const CONNECTION_HEADERS: [&str; 9] = ["host", "content-length", "connection", "accept-encoding", "transfer-encoding", "te", "upgrade", "keep-alive", "proxy-connection"];
/// Headers that sign a person in: a recording's are left out, so they don't land in files.
const CREDENTIALS: [&str; 7] = ["authorization", "cookie", "proxy-authorization", "x-api-key", "x-auth-token", "x-csrf-token", "x-xsrf-token"];

fn har(v: &Value) -> Result<Converted> {
    let mut c = Converted::new("har");
    let mut hosts: Vec<(String, Vec<Value>)> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    let mut repeated = 0;
    for e in list(&v["log"]["entries"]) {
        let r = &e["request"];
        let url = text(&r["url"]);
        let method = text(&r["method"]).to_uppercase();
        let Some((_, rest)) = url.split_once("://").filter(|(s, _)| *s == "http" || *s == "https") else {
            c.issue(&format!("{method} {}", url.chars().take(80).collect::<String>()), "not an http(s) request", "");
            continue;
        };
        // Nobody's password belongs in a name.
        let host = rest.split(['/', '?', '#']).next().unwrap_or("").rsplit('@').next().unwrap_or("").to_string();
        let path = rest.find('/').map_or("/", |i| rest[i..].split(['?', '#']).next().unwrap_or("/"));
        let name = format!("{method} {path}");
        let location = format!("{host} / {name}");
        let mut headers = vec![];
        for h in list(&r["headers"]) {
            let key = text(&h["name"]);
            let k = key.to_lowercase();
            if k.starts_with(':') || k.starts_with("sec-") || CONNECTION_HEADERS.contains(&k.as_str()) {
                continue;
            }
            if CREDENTIALS.contains(&k.as_str()) {
                c.issue(&location, format!("the {key} header, left out: set the request's auth"), "");
                continue;
            }
            headers.push(header_row(&key, &text(&h["value"]), false));
        }
        let p = &r["postData"];
        let mime = text(&p["mimeType"]);
        let params = list(&p["params"]);
        let body = if mime.starts_with("application/x-www-form-urlencoded") && !params.is_empty() {
            json!({ "mode": "urlencoded", "urlencoded": params.iter().map(|x| header_row(&decoded(&text(&x["name"])), &decoded(&text(&x["value"])), false)).collect::<Vec<_>>() })
        } else if mime.starts_with("multipart/form-data") && !params.is_empty() {
            headers.retain(|h| !h["key"].as_str().is_some_and(|k| k.eq_ignore_ascii_case("content-type")));
            json!({ "mode": "formdata", "formdata": params.iter().map(|x| json!({ "key": x["name"], "value": text(&x["value"]), "type": if x["fileName"].is_string() { "file" } else { "text" } })).collect::<Vec<_>>() })
        } else if p.is_object() {
            json!({ "mode": "raw", "raw": text(&p["text"]) })
        } else {
            Value::Null
        };
        let request = json!({ "method": method, "url": { "raw": url.split('#').next().unwrap_or("") }, "header": headers, "body": body });
        if !seen.insert(request.to_string()) {
            repeated += 1;
            continue;
        }
        let item = json!({ "name": name, "request": request });
        match hosts.iter_mut().find(|(h, _)| *h == host) {
            Some((_, items)) => items.push(item),
            None => hosts.push((host, vec![item])),
        }
    }
    if hosts.is_empty() {
        bail!("this HAR file has no http(s) requests");
    }
    if repeated > 0 {
        c.issue("collection", "the same request again, once each", repeated.to_string());
    }
    let title = list(&v["log"]["pages"]).first().map(|p| text(&p["title"])).filter(|t| !t.is_empty() && !t.contains("://"));
    let name = title.unwrap_or_else(|| hosts[0].0.clone());
    let items = if hosts.len() == 1 { hosts.remove(0).1 } else { hosts.into_iter().map(|(h, items)| json!({ "name": h, "item": items })).collect() };
    c.collections.push(collection(&name, items));
    Ok(c)
}

/// Form values are recorded as sent by some browsers, plain by others: only valid escapes decode.
fn decoded(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => match b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(x) => {
                    out.push(x);
                    i += 2;
                }
                None => return s.to_string(),
            },
            b'+' => out.push(b' '),
            x => out.push(x),
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}
