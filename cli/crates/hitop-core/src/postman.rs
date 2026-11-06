//! Postman import: collections (v2.0 and v2.1), environments and globals.
//!
//! What hitop can't carry over is listed in an [`ImportReport`]: script lines
//! with `pm.*` calls the shim doesn't provide, auth types and body modes
//! hitop doesn't have, and Postman's `{{$dynamic}}` variables.
use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::format::names;
use crate::model::{Auth, Collection, Environment, FormField, Header, Param, Request};

/// One thing the import couldn't carry over as it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// Where: `Folder / Request (post-request script)`, `collection auth`, ...
    pub location: String,
    /// Script line, 1-based, for script issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// What isn't supported: `pm.sendRequest`, `oauth2 auth`, `{{$guid}}`.
    pub what: String,
    /// The script line or value it was found in.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.location)?;
        if let Some(line) = self.line {
            write!(f, " line {line}")?;
        }
        write!(f, ": {}", self.what)?;
        if !self.text.is_empty() {
            write!(f, " in `{}`", self.text)?;
        }
        Ok(())
    }
}

/// What an import couldn't carry over.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    pub issues: Vec<Issue>,
}

impl ImportReport {
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    /// Issues from scripts (unsupported `pm.*` calls and libraries).
    pub fn script_issues(&self) -> impl Iterator<Item = &Issue> {
        self.issues.iter().filter(|i| i.line.is_some())
    }

    fn push(&mut self, location: impl Into<String>, what: impl Into<String>, text: impl Into<String>) {
        self.issues.push(Issue { location: location.into(), line: None, what: what.into(), text: text.into() });
    }
}

/// What a Postman JSON file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Collection,
    Environment,
    Globals,
}

/// The kind of Postman file `v` is, or None for something else (a HITOP
/// export, say).
pub fn detect(v: &Value) -> Option<Kind> {
    if v["info"].is_object() && v["item"].is_array() {
        return Some(Kind::Collection);
    }
    if v["values"].is_array() {
        return Some(if v["_postman_variable_scope"] == "globals" { Kind::Globals } else { Kind::Environment });
    }
    None
}

/// An environment as imported. Secret values are kept apart so they go to
/// the keychain (CLI) or the web app's storage, never into a file.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedEnvironment {
    pub environment: Environment,
    /// Values of the variables listed in `environment.secrets`.
    pub secret_values: Vec<(String, String)>,
    pub kind: Kind,
}

/// A Postman environment or globals export: `{name, values: [{key, value,
/// type, enabled}]}`. `type: "secret"` makes a secret variable; disabled
/// values are left out.
pub fn import_environment(text: &str) -> Result<ImportedEnvironment> {
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    let Some(kind @ (Kind::Environment | Kind::Globals)) = detect(&v) else {
        bail!("not a Postman environment or globals export (no `values` list)");
    };
    let fallback = if kind == Kind::Globals { "Globals" } else { "Imported" };
    let name = v["name"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or(fallback).to_string();
    let mut environment = Environment { id: names::slug(&name), name, ..Default::default() };
    let mut secret_values = vec![];
    for row in v["values"].as_array().into_iter().flatten() {
        let Some(key) = row["key"].as_str().filter(|k| !k.is_empty()) else { continue };
        if row["enabled"] == false {
            continue;
        }
        let value = scalar(&row["value"]);
        if row["type"] == "secret" {
            if !environment.secrets.iter().any(|s| s == key) {
                environment.secrets.push(key.to_string());
            }
            environment.variables.remove(key);
            secret_values.retain(|(k, _): &(String, String)| k != key);
            if !value.is_empty() {
                secret_values.push((key.to_string(), value));
            }
        } else {
            environment.variables.insert(key.to_string(), value);
        }
    }
    environment.secrets.sort();
    Ok(ImportedEnvironment { environment, secret_values, kind })
}

/// Puts imported Postman globals into hitop's globals scope: the imported
/// values win over what `globals` had (a name changes between plain and
/// secret as the import says). Returns the imported secret values, for the
/// keychain.
pub fn merge_into_globals(globals: &mut Environment, imported: &ImportedEnvironment) -> Vec<(String, String)> {
    for (k, v) in &imported.environment.variables {
        globals.secrets.retain(|s| s != k);
        globals.variables.insert(k.clone(), v.clone());
    }
    for name in &imported.environment.secrets {
        globals.variables.remove(name);
        if !globals.secrets.contains(name) {
            globals.secrets.push(name.clone());
        }
    }
    globals.secrets.sort();
    imported.secret_values.clone()
}

/// A Postman collection (v2.0 or v2.1). Folders become request paths
/// (`file`), collection auth the collection's default auth, and collection
/// and folder scripts are put in front of each request's own, in the order
/// Postman runs them. `{{name}}` becomes `${name}`.
pub fn import_collection(text: &str) -> Result<(Collection, ImportReport)> {
    import_collection_with(text, &script_calls)
}

/// Finds a script's unsupported calls as (line, call, line text).
pub type ScriptScan<'a> = &'a dyn Fn(&str) -> Vec<(usize, String, String)>;

/// [`import_collection`] with the script check given: the web app runs the
/// shim's own check in the page and passes the results in.
pub fn import_collection_with(text: &str, scan: ScriptScan) -> Result<(Collection, ImportReport)> {
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))?;
    if detect(&v) != Some(Kind::Collection) {
        bail!("not a Postman collection (no `info` and `item`)");
    }
    let name = v["info"]["name"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or("Imported").to_string();
    let mut im = Importer { report: ImportReport::default(), requests: vec![], used: HashSet::new(), scan };
    let auth = im.auth(&v["auth"], "collection auth").filter(|a| *a != Auth::None);
    let scripts = Scripts::default().with(&v["event"], "collection", &mut im.report, scan);
    im.items(&v["item"], &[], &scripts, None);
    let mut variables = HashMap::new();
    for row in v["variable"].as_array().into_iter().flatten() {
        if let Some(key) = row["key"].as_str().filter(|k| !k.is_empty()) {
            if row["disabled"] != true {
                variables.insert(key.to_string(), im.convert(&scalar(&row["value"]), "collection variables"));
            }
        }
    }
    let collection = Collection { id: names::collection_id(&name), name, requests: im.requests, variables, auth, ..Default::default() };
    Ok((collection, im.report))
}

/// Scripts inherited from the collection and folders above a request.
#[derive(Debug, Clone, Default)]
struct Scripts {
    pre: Vec<(String, String)>,
    post: Vec<(String, String)>,
}

impl Scripts {
    /// These scripts plus the ones in `events`, checked for unsupported
    /// calls once, where they are written.
    fn with(&self, events: &Value, location: &str, report: &mut ImportReport, scan: ScriptScan) -> Scripts {
        let mut out = self.clone();
        let (pre, post) = event_scripts(events);
        for (code, kind, list) in [(pre, "pre-request script", &mut out.pre), (post, "post-request script", &mut out.post)] {
            if code.trim().is_empty() {
                continue;
            }
            for (line, what, text) in scan(&code) {
                report.issues.push(Issue { location: format!("{location} ({kind})"), line: Some(line), what, text });
            }
            list.push((location.to_string(), code));
        }
        out
    }
}

/// Unsupported calls in a script as (line, call, text). The check runs in the
/// shim, so it needs the JS engine; a build without it (wasm) reports none.
#[cfg(feature = "native")]
fn script_calls(code: &str) -> Vec<(usize, String, String)> {
    crate::scripting::unsupported_calls(code).into_iter().map(|c| (c.line, c.call, c.text)).collect()
}

#[cfg(not(feature = "native"))]
fn script_calls(_: &str) -> Vec<(usize, String, String)> {
    vec![]
}

/// The `prerequest` and `test` scripts of an `event` list: `exec` lines
/// joined with `\n`, several events of one kind joined with `\n`, disabled
/// ones left out. (The web app's `postmanScripts` must match this.)
fn event_scripts(events: &Value) -> (String, String) {
    let script = |listen: &str| -> String {
        events
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["listen"] == listen && e["disabled"] != true)
            .map(|e| match &e["script"]["exec"] {
                Value::Array(lines) => lines.iter().filter_map(|l| l.as_str()).collect::<Vec<_>>().join("\n"),
                Value::String(s) => s.clone(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    (script("prerequest"), script("test"))
}

/// One script from the inherited ones and the request's own. Each part runs
/// in its own block, as Postman runs them separately.
fn join_scripts(parts: &[(String, String)]) -> String {
    let parts: Vec<&(String, String)> = parts.iter().filter(|(_, c)| !c.trim().is_empty()).collect();
    match parts.as_slice() {
        [] => String::new(),
        [(_, only)] => only.trim_end().to_string(),
        many => many
            .iter()
            .map(|(from, code)| format!("// From {from}\n{{\n{}\n}}", code.trim_end()))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

struct Importer<'a> {
    report: ImportReport,
    requests: Vec<Request>,
    used: HashSet<String>,
    scan: ScriptScan<'a>,
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn placeholder_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").unwrap())
}

impl Importer<'_> {
    /// `{{name}}` → `${name}`. Postman's dynamic variables (`{{$guid}}`)
    /// have no hitop equivalent: they stay as written and are reported.
    fn convert(&mut self, text: &str, location: &str) -> String {
        let mut dynamic = vec![];
        let out = placeholder_re().replace_all(text, |c: &regex::Captures| {
            let name = &c[1];
            if name.starts_with('$') {
                dynamic.push(c[0].to_string());
                c[0].to_string()
            } else {
                format!("${{{name}}}")
            }
        });
        for d in dynamic {
            if !self.report.issues.iter().any(|i| i.location == location && i.what == d) {
                self.report.push(location, format!("dynamic variable {d}"), "");
            }
        }
        out.into_owned()
    }

    fn auth(&mut self, auth: &Value, location: &str) -> Option<Auth> {
        let kind = auth["type"].as_str()?;
        if kind == "noauth" {
            return Some(Auth::None);
        }
        match crate::auth::from_postman(auth) {
            Some(a) => Some(self.convert_auth(a, location)),
            None => {
                self.report.push(location, format!("{kind} auth"), "");
                Some(Auth::None)
            }
        }
    }

    fn convert_auth(&mut self, auth: Auth, location: &str) -> Auth {
        match auth {
            Auth::Bearer { token } => Auth::Bearer { token: self.convert(&token, location) },
            Auth::Basic { username, password } => Auth::Basic { username: self.convert(&username, location), password: self.convert(&password, location) },
            Auth::ApiKey { key, value, location: at } => Auth::ApiKey { key: self.convert(&key, location), value: self.convert(&value, location), location: at },
            Auth::OAuth2(o) => {
                let c = |s: &String, me: &mut Self| me.convert(s, location);
                let mut o = *o;
                o.token_url = c(&o.token_url, self);
                o.client_id = c(&o.client_id, self);
                o.client_secret = c(&o.client_secret, self);
                o.scope = c(&o.scope, self);
                o.username = c(&o.username, self);
                o.password = c(&o.password, self);
                Auth::OAuth2(Box::new(o))
            }
            Auth::Digest { username, password } => Auth::Digest { username: self.convert(&username, location), password: self.convert(&password, location) },
            Auth::AwsV4(a) => {
                let mut a = *a;
                for f in [&mut a.access_key, &mut a.secret_key, &mut a.session_token, &mut a.region, &mut a.service] {
                    *f = self.convert(f, location);
                }
                Auth::AwsV4(Box::new(a))
            }
            Auth::None => Auth::None,
        }
    }

    fn items(&mut self, items: &Value, path: &[String], scripts: &Scripts, folder_auth: Option<&Auth>) {
        for item in items.as_array().into_iter().flatten() {
            let name = item["name"].as_str().filter(|n| !n.trim().is_empty()).unwrap_or("Untitled").to_string();
            let mut here = path.to_vec();
            here.push(name.clone());
            let location = here.join(" / ");
            if item["item"].is_array() {
                // A folder.
                let auth = self.auth(&item["auth"], &format!("{location} (auth)"));
                let inner = scripts.with(&item["event"], &format!("folder {location}"), &mut self.report, self.scan);
                self.items(&item["item"], &here, &inner, auth.as_ref().or(folder_auth));
            } else if item["request"].is_object() || item["request"].is_string() {
                let own = scripts.with(&item["event"], &location, &mut self.report, self.scan);
                let request = self.request(item, &name, path, &location, &own, folder_auth);
                self.requests.push(request);
            }
        }
    }

    /// The request's file: the folder path and the name's slug, numbered
    /// when taken.
    fn file(&mut self, path: &[String], name: &str) -> String {
        let dir: Vec<String> = path.iter().map(|p| names::slug(p)).collect();
        let prefix = if dir.is_empty() { String::new() } else { format!("{}/", dir.join("/")) };
        let base = format!("{prefix}{}", names::slug(name));
        let mut candidate = base.clone();
        let mut n = 2;
        while self.used.contains(&candidate.to_lowercase()) {
            candidate = format!("{base}-{n}");
            n += 1;
        }
        self.used.insert(candidate.to_lowercase());
        candidate
    }

    fn request(&mut self, item: &Value, name: &str, path: &[String], location: &str, scripts: &Scripts, folder_auth: Option<&Auth>) -> Request {
        let req = &item["request"];
        let method = req["method"].as_str().unwrap_or("GET").to_uppercase();
        // `request` may be just the URL.
        let (raw_url, disabled) = url(if req.is_string() { req } else { &req["url"] });
        let url = self.convert(&raw_url, location);
        let disabled_params = disabled
            .into_iter()
            .map(|(k, v)| Param { key: self.convert(&k, location), value: self.convert(&v, location), ..Default::default() })
            .collect();
        let mut headers: Vec<Header> = vec![];
        for h in req["header"].as_array().into_iter().flatten() {
            let Some(key) = h["key"].as_str() else { continue };
            let mut header = Header::new(self.convert(key, location), self.convert(&scalar(&h["value"]), location));
            header.enabled = h["disabled"] != true;
            headers.push(header);
        }
        let mut request = Request {
            name: name.to_string(),
            method,
            url,
            disabled_params,
            pre_request_script: join_scripts(&scripts.pre),
            post_request_script: join_scripts(&scripts.post),
            ..Default::default()
        };
        self.body(&req["body"], &mut request, location);
        request.headers = headers;
        request.auth = match self.auth(&req["auth"], &format!("{location} (auth)")) {
            Some(a) => Some(a),
            None => folder_auth.cloned(),
        };
        request.examples = item["response"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                let mut e = serde_json::Map::new();
                e.insert("name".into(), json!(r["name"].as_str().unwrap_or("Example")));
                if let Some(code) = r["code"].as_u64() {
                    e.insert("status".into(), json!(code));
                }
                if let Some(body) = r["body"].as_str().filter(|b| !b.is_empty()) {
                    e.insert("body".into(), json!(body));
                }
                Value::Object(e)
            })
            .collect();
        let file = self.file(path, name);
        request.id = names::request_id(&file);
        request.file = Some(file);
        request
    }

    fn body(&mut self, body: &Value, request: &mut Request, location: &str) {
        if body["disabled"] == true {
            return;
        }
        let rows = |key: &str| body[key].as_array().cloned().unwrap_or_default();
        match body["mode"].as_str() {
            Some("raw") | None => request.body = self.convert(&scalar(&body["raw"]), location),
            Some(mode @ ("urlencoded" | "formdata")) => {
                request.body_mode = Some(if mode == "urlencoded" { "urlencoded" } else { "multipart" }.into());
                for row in rows(mode) {
                    let is_file = row["type"] == "file";
                    if is_file {
                        self.report.push(location, "file field in form body: attach the file again", scalar(&row["key"]));
                    }
                    request.form_fields.push(FormField {
                        key: self.convert(&scalar(&row["key"]), location),
                        value: if is_file { String::new() } else { self.convert(&scalar(&row["value"]), location) },
                        kind: if is_file { "file" } else { "text" }.into(),
                        enabled: row["disabled"] != true,
                        ..Default::default()
                    });
                }
            }
            Some("graphql") => {
                // Query and variables kept apart: sent as GraphQL (crate::graphql).
                let g = &body["graphql"];
                request.body = self.convert(&scalar(&g["query"]), location);
                request.body_type = Some(crate::graphql::BODY_TYPE.into());
                let variables = match &g["variables"] {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                };
                let variables = self.convert(&variables, location);
                if !variables.trim().is_empty() {
                    request.graphql = Some(crate::model::Graphql { variables, ..Default::default() });
                }
            }
            Some(other) => self.report.push(location, format!("{other} body"), ""),
        }
    }
}

/// A Postman URL (string, or `{raw, host, path, query}`) as text, plus the
/// disabled query parameters.
fn url(u: &Value) -> (String, Vec<(String, String)>) {
    let disabled = u["query"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|q| q["disabled"] == true)
        .map(|q| (scalar(&q["key"]), scalar(&q["value"])))
        .collect();
    if let Some(s) = u.as_str() {
        return (s.to_string(), disabled);
    }
    if let Some(raw) = u["raw"].as_str() {
        return (raw.to_string(), disabled);
    }
    let part = |v: &Value, sep: &str| -> String {
        match v {
            Value::Array(a) => a.iter().map(scalar).collect::<Vec<_>>().join(sep),
            other => scalar(other),
        }
    };
    let mut out = String::new();
    if let Some(p) = u["protocol"].as_str() {
        out.push_str(&format!("{p}://"));
    }
    out.push_str(&part(&u["host"], "."));
    if let Some(port) = u["port"].as_str() {
        out.push_str(&format!(":{port}"));
    }
    let path = part(&u["path"], "/");
    if !path.is_empty() {
        out.push('/');
        out.push_str(&path);
    }
    let query: Vec<String> = u["query"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|q| q["disabled"] != true)
        .map(|q| if q["value"].is_null() { scalar(&q["key"]) } else { format!("{}={}", scalar(&q["key"]), scalar(&q["value"])) })
        .collect();
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query.join("&"));
    }
    (out, disabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApiKeyLocation;

    const COLLECTION: &str = r#"{
      "info": {"name": "Shop", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
      "auth": {"type": "apikey", "apikey": [{"key": "key", "value": "X-API-Key"}, {"key": "value", "value": "{{apiKey}}"}, {"key": "in", "value": "header"}]},
      "event": [{"listen": "test", "script": {"exec": ["pm.test('ok', () => pm.response.to.be.ok);"]}}],
      "variable": [{"key": "base", "value": "http://localhost"}, {"key": "n", "value": 3}],
      "item": [
        {"name": "Orders", "auth": {"type": "bearer", "bearer": [{"key": "token", "value": "{{token}}"}]},
         "event": [{"listen": "prerequest", "script": {"exec": ["pm.sendRequest('x', () => {});"]}}],
         "item": [
           {"name": "Create order", "request": {"method": "post", "url": {"raw": "{{base}}/orders?debug=1", "query": [{"key": "debug", "value": "1"}, {"key": "trace", "value": "on", "disabled": true}]},
             "header": [{"key": "X-Id", "value": "{{$guid}}"}, {"key": "X-Off", "value": "1", "disabled": true}],
             "body": {"mode": "urlencoded", "urlencoded": [{"key": "q", "value": "{{n}}"}]}},
            "event": [{"listen": "test", "script": {"exec": ["pm.environment.set('id', pm.response.json().id);"]}}],
            "response": [{"name": "created", "code": 201, "body": "{\"id\": 1}"}]},
           {"name": "Create order", "request": {"method": "GET", "url": "{{base}}/orders", "auth": {"type": "noauth"}}},
           {"name": "Legacy", "request": {"url": "{{base}}/x", "method": "GET", "auth": {"type": "oauth2", "oauth2": []}}}
         ]},
        {"name": "Health", "request": {"method": "GET", "url": {"protocol": "http", "host": ["localhost"], "port": "8080", "path": ["health"]}}}
      ]
    }"#;

    #[test]
    fn collections_import_with_folders_auth_and_scripts() {
        let (c, report) = import_collection(COLLECTION).unwrap();
        assert_eq!(c.name, "Shop");
        assert_eq!(c.auth, Some(Auth::ApiKey { key: "X-API-Key".into(), value: "${apiKey}".into(), location: ApiKeyLocation::Header }));
        assert_eq!(c.variables["n"], "3");
        let files: Vec<&str> = c.requests.iter().map(|r| r.file.as_deref().unwrap()).collect();
        assert_eq!(files, ["orders/create-order", "orders/create-order-2", "orders/legacy", "health"]);
        let r = &c.requests[0];
        assert_eq!(r.method, "POST");
        assert_eq!(r.url, "${base}/orders?debug=1");
        assert_eq!(r.disabled_params[0].key, "trace");
        assert_eq!(r.headers[0].value, "{{$guid}}");
        assert!(!r.headers[1].enabled);
        assert_eq!(r.body_mode.as_deref(), Some("urlencoded"));
        assert_eq!(r.form_fields[0].value, "${n}");
        assert_eq!(r.auth, Some(Auth::Bearer { token: "${token}".into() }), "folder auth is inherited");
        assert_eq!(r.examples[0]["status"], 201);
        assert!(r.pre_request_script.contains("pm.sendRequest"));
        assert_eq!(
            r.post_request_script,
            "// From collection\n{\npm.test('ok', () => pm.response.to.be.ok);\n}\n// From Orders / Create order\n{\npm.environment.set('id', pm.response.json().id);\n}"
        );
        assert_eq!(c.requests[1].auth, Some(Auth::None));
        assert_eq!(c.requests[2].auth, Some(Auth::None));
        assert_eq!(c.requests[3].url, "http://localhost:8080/health");
        assert_eq!(c.requests[3].auth, None, "collection auth applies at run time");
        assert_eq!(r.id, names::request_id("orders/create-order"));

        let what: Vec<String> = report.issues.iter().map(|i| format!("{} | {} | {:?}", i.location, i.what, i.line)).collect();
        assert_eq!(
            what,
            [
                "folder Orders (pre-request script) | pm.sendRequest | Some(1)",
                "Orders / Create order | dynamic variable {{$guid}} | None",
                "Orders / Legacy (auth) | oauth2 auth | None",
            ]
        );
        assert_eq!(report.script_issues().count(), 1);
    }

    #[test]
    fn the_folder_format_writes_an_import() {
        let (c, _) = import_collection(COLLECTION).unwrap();
        let dir = tempfile::tempdir().unwrap();
        crate::format::write_collection(dir.path(), &c).unwrap();
        assert!(dir.path().join("requests/orders/create-order-2.yaml").is_file());
        let back = crate::format::read_folder(dir.path()).unwrap().collection;
        assert_eq!(back.requests.len(), 4);
        assert_eq!(back.requests[0].post_request_script, c.requests[0].post_request_script);
        let text = std::fs::read_to_string(dir.path().join("requests/orders/create-order.yaml")).unwrap();
        assert!(!text.contains("\nid:"), "ids are the derived ones: {text}");
    }

    #[test]
    fn graphql_bodies_stay_graphql() {
        let text = r#"{"info": {"name": "G", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},
            "item": [{"name": "Me", "request": {"method": "POST", "url": "{{base}}/graphql",
              "body": {"mode": "graphql", "graphql": {"query": "query Me($id: ID) { me(id: $id) { name } }", "variables": "{\"id\": \"{{uid}}\"}"}}}},
              {"name": "Plain", "request": {"method": "POST", "url": "{{base}}/graphql", "body": {"mode": "graphql", "graphql": {"query": "{ ping }"}}}}]}"#;
        let (c, report) = import_collection(text).unwrap();
        assert!(report.is_empty(), "{report:?}");
        let me = &c.requests[0];
        assert_eq!(me.body, "query Me($id: ID) { me(id: $id) { name } }");
        assert_eq!(me.body_type.as_deref(), Some("graphql"));
        assert_eq!(me.graphql.as_ref().unwrap().variables, r#"{"id": "${uid}"}"#);
        assert!(me.headers.is_empty(), "the Content-Type comes with the GraphQL body when sent");
        assert!(c.requests[1].is_graphql() && c.requests[1].graphql.is_none());
        // Back out to Postman as a GraphQL body.
        let out: Value = serde_json::from_str(&crate::export::to_postman(&c).unwrap()).unwrap();
        assert_eq!(out["item"][0]["request"]["body"], json!({"mode": "graphql", "graphql": {"query": me.body, "variables": r#"{"id": "${uid}"}"#}}));
    }

    #[test]
    fn environments_keep_secret_values_apart() {
        let env = import_environment(
            r#"{"name": "Prod", "values": [{"key": "base", "value": "https://x", "enabled": true},
               {"key": "token", "value": "s3cr3t", "type": "secret", "enabled": true},
               {"key": "empty", "value": "", "type": "secret"},
               {"key": "off", "value": "1", "enabled": false}], "_postman_variable_scope": "environment"}"#,
        )
        .unwrap();
        assert_eq!(env.kind, Kind::Environment);
        assert_eq!(env.environment.name, "Prod");
        assert_eq!(env.environment.variables, [("base".to_string(), "https://x".to_string())].into());
        assert_eq!(env.environment.secrets, ["empty", "token"]);
        assert_eq!(env.secret_values, [("token".to_string(), "s3cr3t".to_string())]);
        assert!(!serde_json::to_string(&env.environment).unwrap().contains("s3cr3t"));
    }

    #[test]
    fn globals_go_to_the_globals_scope() {
        let imported = import_environment(
            r#"{"name": "My Workspace Globals", "_postman_variable_scope": "globals", "values": [{"key": "base", "value": "g"}, {"key": "g", "value": "1"}, {"key": "gs", "value": "v", "type": "secret"}]}"#,
        )
        .unwrap();
        assert_eq!(imported.kind, Kind::Globals);
        let mut globals = Environment::globals();
        globals.variables.insert("gs".into(), "old plain".into());
        globals.variables.insert("keep".into(), "k".into());
        globals.secrets.push("base".into());
        let secret_values = merge_into_globals(&mut globals, &imported);
        assert_eq!(globals.variables, [("base".to_string(), "g".to_string()), ("g".to_string(), "1".to_string()), ("keep".to_string(), "k".to_string())].into());
        assert_eq!(globals.secrets, ["gs"]);
        assert_eq!(secret_values, [("gs".to_string(), "v".to_string())]);
        assert!(import_environment(r#"{"info": {}, "item": []}"#).is_err());
    }
}
