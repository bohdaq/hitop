//! OpenAPI 3.0 and 3.1 import (YAML or JSON) into a collection, and sync of
//! a collection imported before with a newer spec (`sync.rs`).
//!
//! Tags become folders, operations requests (`${baseUrl}` from the first
//! server, path parameters as `${name}` variables, query and header
//! parameters, a body from the spec's example or one built from the
//! schema), security schemes collection or request auth, and response
//! examples saved examples. Each request records the operation it came from
//! under the `openapi` key (spec/collection-format.md, "OpenAPI source"), and the
//! collection the spec under the same key in `hitop.yaml`; sync matches on
//! it. What doesn't map is listed in the [`OpenApiReport`].
//!
//! Pure Rust without I/O: the web app runs it through hitop-wasm. JSON maps
//! here are sorted, so requests come in path order and generated example
//! bodies list their keys alphabetically, the same on every surface.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::format::names;
use crate::model::{ApiKeyLocation, Auth, Collection, FormField, Header, Param, Request};
use crate::postman::Issue;

pub mod example;
pub mod refs;
mod sync;
#[cfg(test)]
mod tests;

pub use refs::parse;
pub use sync::{sync, CollectionChange, OpChange, RowChange, SyncDiff, Synced};

use example::Side;
use refs::{RefError, Resolver};

/// The key of the record on requests and on the collection.
pub const RECORD: &str = "openapi";
/// The collection variable that holds the server URL.
pub const BASE_URL: &str = "baseUrl";
/// Saved examples from the spec carry `source: openapi`; others are yours.
pub const EXAMPLE_SOURCE: &str = "openapi";

/// How to import.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// Every request at the top level instead of a folder per tag.
    pub flat: bool,
    /// Where the spec came from (a URL, or a file name): recorded in the
    /// collection, and relative server URLs resolve against a URL.
    pub source: Option<String>,
    /// The web app has no collection auth: copy it into each request that
    /// has none of its own (and compare auth that way on sync).
    pub inline_auth: bool,
}

/// What the import mapped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapped {
    /// The spec's `openapi` version.
    pub openapi: String,
    pub title: String,
    /// Operations in the spec.
    pub operations: usize,
    /// Requests made from them.
    pub requests: usize,
    pub folders: usize,
    pub examples: usize,
    /// Collection variables set (`baseUrl`, path parameters).
    pub variables: Vec<String>,
    /// Security schemes turned into auth (`api_key → apikey header`).
    pub auth: Vec<String>,
}

/// What an import mapped, skipped and wants checked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiReport {
    pub mapped: Mapped,
    /// Parts of the spec not imported (cookie parameters, webhooks,
    /// external `$ref`s, OAuth 2 schemes, body types).
    pub skipped: Vec<Issue>,
    /// Imported, but worth a look (relative server URL, secrets to set).
    pub warnings: Vec<Issue>,
}

impl OpenApiReport {
    fn skip(&mut self, location: impl Into<String>, what: impl Into<String>) {
        push(&mut self.skipped, location.into(), what.into());
    }
    fn warn(&mut self, location: impl Into<String>, what: impl Into<String>) {
        push(&mut self.warnings, location.into(), what.into());
    }
}

fn push(list: &mut Vec<Issue>, location: String, what: String) {
    if !list.iter().any(|i| i.location == location && i.what == what) {
        list.push(Issue { location, line: None, what, text: String::new() });
    }
}

/// The record on a request: the operation it was made from and a hash of
/// each spec-owned field as imported (sync's base).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpRecord {
    /// `METHOD /path`.
    pub operation: String,
    #[serde(rename = "operationId", default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub synced: BTreeMap<String, String>,
    /// Header and query parameter rows as imported, one hash per row
    /// (`headers: {x-trace: <hash>}`), so sync merges them one by one.
    /// Records written before 1.1 have none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rows: BTreeMap<String, BTreeMap<String, String>>,
}

/// The record on the collection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The spec's `info.version`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flat: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub synced: BTreeMap<String, String>,
}

pub(crate) fn scalar(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// True when the text looks like an OpenAPI (or Swagger) document.
pub fn detect(text: &str) -> bool {
    match parse(text) {
        Ok(v) => v.get("openapi").is_some() || v.get("swagger").is_some(),
        Err(_) => false,
    }
}

/// Imports a spec as a collection.
pub fn import(text: &str, opts: &Options) -> Result<(Collection, OpenApiReport)> {
    let spec = parse(text)?;
    let built = build(&spec, opts)?;
    let mut used = HashSet::new();
    let mut requests = vec![];
    let mut folders = BTreeSet::new();
    for op in built.ops {
        let file = unique_file(&op.file, &mut used);
        if let Some((dir, _)) = file.rsplit_once('/') {
            folders.insert(dir.to_string());
        }
        let mut r = op.request;
        r.id = names::request_id(&file);
        r.file = Some(file);
        r.extra.insert(RECORD.into(), serde_json::to_value(&op.record)?);
        requests.push(r);
    }
    let mut report = built.report;
    report.mapped.requests = requests.len();
    report.mapped.folders = folders.len();
    let mut collection = Collection {
        id: names::collection_id(&built.title),
        name: built.title.clone(),
        requests,
        variables: built.variables.into_iter().collect(),
        auth: built.auth,
        ..Default::default()
    };
    collection.extra.insert(RECORD.into(), serde_json::to_value(&built.record)?);
    Ok((collection, report))
}

/// `base`, or `base-2`, `base-3`... not in `used` (compared without case).
fn unique_file(base: &str, used: &mut HashSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut n = 2;
    while used.contains(&candidate.to_lowercase()) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    used.insert(candidate.to_lowercase());
    candidate
}

/// One operation as a request, before it gets its file and id.
struct BuiltOp {
    record: OpRecord,
    request: Request,
    /// Suggested file: `tag/slug-of-name`.
    file: String,
}

struct Built {
    title: String,
    variables: BTreeMap<String, String>,
    auth: Option<Auth>,
    record: SpecRecord,
    ops: Vec<BuiltOp>,
    report: OpenApiReport,
}

const METHODS: [&str; 8] = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];

fn build(spec: &Value, opts: &Options) -> Result<Built> {
    let version = refs::check_version(spec)?;
    let r = Resolver::new(spec);
    let mut report = OpenApiReport::default();
    report.mapped.openapi = version.clone();
    let title = spec["info"]["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).unwrap_or("Imported API").to_string();
    report.mapped.title = title.clone();

    let mut variables = BTreeMap::new();
    variables.insert(BASE_URL.to_string(), base_url(&spec["servers"], opts.source.as_deref(), "servers", &mut report));

    let mut schemes = Schemes::new(&r, &spec["components"]["securitySchemes"]);
    let auth = schemes.requirement(&spec["security"], "security", &mut report).flatten();
    if let Some(a) = &auth {
        report.mapped.auth.push(format!("collection: {}", auth_label(a)));
    }

    // Folders follow the spec's tag list; other tags after them, by name.
    let tag_order: Vec<String> = spec["tags"].as_array().into_iter().flatten().filter_map(|t| t["name"].as_str().map(String::from)).collect();

    let mut ops: Vec<(usize, BuiltOp)> = vec![];
    for (path, item) in spec["paths"].as_object().into_iter().flatten() {
        let item = match r.get(item) {
            Ok(i) => i,
            Err(e) => {
                report.skip(path.clone(), e.to_string());
                continue;
            }
        };
        let path_params: &[Value] = item["parameters"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        for method in METHODS {
            let Some(op) = item.get(method).filter(|o| o.is_object()) else { continue };
            report.mapped.operations += 1;
            let operation = format!("{} {path}", method.to_uppercase());
            let servers = if op["servers"].is_array() { &op["servers"] } else { &item["servers"] };
            let base = if servers.as_array().is_some_and(|s| !s.is_empty()) {
                Some(base_url(servers, opts.source.as_deref(), &operation, &mut report))
            } else {
                None
            };
            let ctx = OpCtx { r: &r, path, method, operation: &operation, op, path_params, base };
            let built = ctx.build(&mut variables, &mut schemes, auth.as_ref(), opts, &mut report);
            let tag = op["tags"].as_array().and_then(|t| t.first()).and_then(Value::as_str).map(String::from);
            let rank = match &tag {
                None => 0,
                Some(t) => tag_order.iter().position(|x| x == t).map(|i| i + 1).unwrap_or(tag_order.len() + 1),
            };
            let file = match (&tag, opts.flat) {
                (Some(t), false) => format!("{}/{}", names::slug(t), names::slug(&built.request.name)),
                _ => names::slug(&built.request.name),
            };
            report.mapped.examples += built.request.examples.len();
            ops.push((rank, BuiltOp { file, ..built }));
        }
    }
    // Stable: path order inside each folder.
    ops.sort_by(|(ra, a), (rb, b)| ra.cmp(rb).then_with(|| folder_of(&a.file).cmp(folder_of(&b.file))));
    schemes.finish(&mut report);

    let webhooks = spec["webhooks"].as_object().map(|w| w.len()).unwrap_or(0);
    if webhooks > 0 {
        report.skip("webhooks", format!("{webhooks} webhook{} (requests the API sends, not ones you send)", if webhooks == 1 { "" } else { "s" }));
    }
    let mut auth = auth;
    let mut ops: Vec<BuiltOp> = ops.into_iter().map(|(_, o)| o).collect();
    // No `security` at the top, but every operation asks for the same one:
    // it becomes the collection's auth.
    if auth.is_none() && spec.get("security").is_none() {
        if let Some(Some(first)) = ops.first().map(|o| o.request.auth.clone()) {
            if first != Auth::None && ops.iter().all(|o| o.request.auth.as_ref() == Some(&first)) {
                for op in &mut ops {
                    op.request.auth = None;
                }
                report.mapped.auth.push(format!("collection: {} (every operation uses it)", auth_label(&first)));
                auth = Some(first);
            }
        }
    }
    if opts.inline_auth {
        for op in &mut ops {
            op.request.auth = match op.request.auth.take() {
                Some(Auth::None) => None,
                Some(a) => Some(a),
                None => auth.clone(),
            };
        }
        auth = None;
    }
    for op in &mut ops {
        op.record.synced = sync::request_hashes(&op.request);
        op.record.rows = sync::request_rows(&op.request);
    }
    report.mapped.variables = variables.keys().cloned().collect();
    let mut synced = BTreeMap::new();
    synced.insert(BASE_URL.to_string(), sync::hash(&json!(variables[BASE_URL])));
    if !opts.inline_auth {
        synced.insert("auth".into(), sync::hash(&serde_json::to_value(&auth)?));
    }
    let record = SpecRecord {
        source: opts.source.clone(),
        version: spec["info"]["version"].as_str().map(String::from).unwrap_or_else(|| scalar(&spec["info"]["version"])),
        flat: opts.flat,
        synced,
    };
    Ok(Built { title, variables, auth, record, ops, report })
}

fn folder_of(file: &str) -> &str {
    file.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

/// The first server's URL with its variables at their defaults, without a
/// trailing `/`. A relative URL resolves against the spec's own URL.
fn base_url(servers: &Value, source: Option<&str>, location: &str, report: &mut OpenApiReport) -> String {
    let list = servers.as_array().cloned().unwrap_or_default();
    let Some(first) = list.first() else {
        let url = source.and_then(origin).unwrap_or_default();
        if url.is_empty() {
            report.warn(location, "no servers: set the baseUrl variable to the API's address");
        }
        return url;
    };
    if list.len() > 1 {
        report.warn(location, format!("{} servers: baseUrl uses the first ({}); the others are {}", list.len(), scalar(&first["url"]), list[1..].iter().map(|s| scalar(&s["url"])).collect::<Vec<_>>().join(", ")));
    }
    let mut url = scalar(&first["url"]);
    for (name, var) in first["variables"].as_object().into_iter().flatten() {
        url = url.replace(&format!("{{{name}}}"), &scalar(&var["default"]));
    }
    if !url.contains("://") {
        match source.filter(|s| s.contains("://")) {
            Some(src) => url = join_url(src, &url),
            None => report.warn(location, format!("relative server URL {url}: set baseUrl to the full address")),
        }
    }
    url.trim_end_matches('/').to_string()
}

/// `https://host` of a URL.
fn origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    Some(format!("{scheme}://{host}"))
}

fn join_url(base: &str, rel: &str) -> String {
    if rel.starts_with("//") {
        return format!("{}:{rel}", base.split("://").next().unwrap_or("https"));
    }
    if rel.starts_with('/') {
        return format!("{}{rel}", origin(base).unwrap_or_default());
    }
    let dir = match base.split(['?', '#']).next().unwrap_or(base).rfind('/') {
        Some(i) if i > base.find("://").map(|p| p + 2).unwrap_or(0) => &base[..i + 1],
        _ => return format!("{}/{rel}", base.trim_end_matches('/')),
    };
    let rel = rel.trim_start_matches("./");
    if rel == "." {
        return dir.to_string();
    }
    format!("{dir}{rel}")
}

fn auth_label(a: &Auth) -> &'static str {
    match a {
        Auth::None => "none",
        Auth::Bearer { .. } => "bearer",
        Auth::Basic { .. } => "basic",
        Auth::ApiKey { location: ApiKeyLocation::Header, .. } => "apikey header",
        Auth::ApiKey { location: ApiKeyLocation::Query, .. } => "apikey query",
        Auth::OAuth2(_) => "oauth2",
        Auth::AwsV4(_) => "awsv4",
        Auth::Digest { .. } => "digest",
    }
}

/// A variable name from a scheme name: letters, digits and `_`.
fn var_name(name: &str) -> String {
    let out: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() { "token".into() } else { out }
}

/// The security schemes, mapped to auth once each; what isn't is counted
/// per scheme and reported once.
struct Schemes {
    mapped: HashMap<String, Result<Auth, String>>,
    uses: BTreeMap<String, usize>,
    reported: BTreeSet<String>,
}

impl Schemes {
    fn new(r: &Resolver, schemes: &Value) -> Schemes {
        let mut mapped = HashMap::new();
        for (name, s) in schemes.as_object().into_iter().flatten() {
            let s = match r.get(s) {
                Ok(s) => s,
                Err(e) => {
                    mapped.insert(name.clone(), Err(e.to_string()));
                    continue;
                }
            };
            let v = var_name(name);
            let kind = s["type"].as_str().unwrap_or("");
            let auth = match kind {
                "http" => match s["scheme"].as_str().unwrap_or("").to_ascii_lowercase().as_str() {
                    "bearer" => Ok(Auth::Bearer { token: format!("${{{v}}}") }),
                    "basic" => Ok(Auth::Basic { username: format!("${{{v}Username}}"), password: format!("${{{v}Password}}") }),
                    other => Err(format!("HTTP {other} auth")),
                },
                "apiKey" => match s["in"].as_str() {
                    Some("header") => Ok(Auth::ApiKey { key: scalar(&s["name"]), value: format!("${{{v}}}"), location: ApiKeyLocation::Header }),
                    Some("query") => Ok(Auth::ApiKey { key: scalar(&s["name"]), value: format!("${{{v}}}"), location: ApiKeyLocation::Query }),
                    _ => Err("API key in a cookie".to_string()),
                },
                "oauth2" => Err("OAuth 2: get a token yourself and set bearer auth".to_string()),
                "openIdConnect" => Err("OpenID Connect: get a token yourself and set bearer auth".to_string()),
                "mutualTLS" => Err("mutual TLS".to_string()),
                other => Err(format!("{other} security scheme")),
            };
            mapped.insert(name.clone(), auth);
        }
        Schemes { mapped, uses: BTreeMap::new(), reported: BTreeSet::new() }
    }

    /// The auth for a `security` list: `None` when the key isn't there,
    /// `Some(None)` for no auth, else the first alternative that maps.
    fn requirement(&mut self, security: &Value, location: &str, report: &mut OpenApiReport) -> Option<Option<Auth>> {
        let list = security.as_array()?;
        if list.is_empty() {
            return Some(None);
        }
        for alt in list {
            let Some(names) = alt.as_object() else { continue };
            if names.is_empty() {
                // `{}`: auth is optional.
                return Some(None);
            }
            if names.len() > 1 {
                // Several schemes at once: hitop sends one auth.
                let both = names.keys().cloned().collect::<Vec<_>>().join(" and ");
                if self.reported.insert(both.clone()) {
                    report.skip(location, format!("security schemes {both} together (hitop sends one auth; add the others as headers)"));
                }
                continue;
            }
            let name = names.keys().next().unwrap();
            if self.mapped.contains_key(name) {
                *self.uses.entry(name.clone()).or_default() += 1;
            }
            match self.mapped.get(name) {
                Some(Ok(a)) => return Some(Some(a.clone())),
                Some(Err(_)) => {}
                None => {
                    if self.reported.insert(name.clone()) {
                        report.skip(location, format!("security scheme {name} isn't defined in components"));
                    }
                }
            }
        }
        Some(None)
    }

    /// Reports each scheme the spec uses: what to set for the ones that
    /// map, why the others don't. Schemes no operation uses are left out.
    fn finish(&self, report: &mut OpenApiReport) {
        for (name, uses) in &self.uses {
            let n = format!("{uses} use{}", if *uses == 1 { "" } else { "s" });
            match &self.mapped[name] {
                Ok(a) => {
                    let v = var_name(name);
                    let names = match a {
                        Auth::Basic { .. } => format!("{v}Username and {v}Password"),
                        _ => v,
                    };
                    report.mapped.auth.push(format!("{name} → {} ({n})", auth_label(a)));
                    report.warn(format!("security scheme {name}"), format!("set {names} in an environment (as a secret)"));
                }
                Err(why) => report.skip(format!("security scheme {name}"), format!("{why} ({n})")),
            }
        }
        report.skipped.sort_by(|a, b| a.location.cmp(&b.location).then(a.what.cmp(&b.what)));
    }
}

struct OpCtx<'a, 'b> {
    r: &'b Resolver<'a>,
    path: &'a str,
    method: &'static str,
    operation: &'b str,
    op: &'a Value,
    path_params: &'a [Value],
    base: Option<String>,
}

/// A parameter or header value as text: arrays joined with `,`, objects as JSON.
fn param_text(v: &Value) -> String {
    match v {
        Value::Array(a) => a.iter().map(scalar).collect::<Vec<_>>().join(","),
        Value::Object(_) => v.to_string(),
        other => scalar(other),
    }
}

impl<'a> OpCtx<'a, '_> {
    fn resolve(&self, v: &'a Value, what: &str, report: &mut OpenApiReport) -> Option<&'a Value> {
        match self.r.get(v) {
            Ok(v) => Some(v),
            Err(e) => {
                report.skip(self.operation, format!("{what}: {e}"));
                None
            }
        }
    }

    /// A parameter's value: its `example`, the first of its `examples`, or
    /// the schema's own example, default or first enum value. Nothing made
    /// up (`string`, `0`) for parameters: those you fill in.
    fn explicit_example(&self, holder: &'a Value, report: &mut OpenApiReport) -> Option<Value> {
        if holder.get("example").is_some() || holder["examples"].is_object() {
            return self.example_of(holder, Side::Request, report);
        }
        let schema = self.r.get(holder.get("schema")?).ok()?;
        example::own_example(schema).or_else(|| {
            // An array of an enum: its first value.
            let items = self.r.get(schema.get("items")?).ok()?;
            example::own_example(items)
        })
    }

    /// An example for a parameter or media type: `example`, the first of
    /// `examples`, then one from the schema.
    fn example_of(&self, holder: &'a Value, side: Side, report: &mut OpenApiReport) -> Option<Value> {
        if let Some(v) = holder.get("example") {
            return Some(v.clone());
        }
        if let Some((_, first)) = holder["examples"].as_object().and_then(|m| m.iter().next()) {
            if let Some(ex) = self.resolve(first, "example", report) {
                if let Some(v) = ex.get("value") {
                    return Some(v.clone());
                }
                if ex.get("externalValue").is_some() {
                    report.skip(self.operation, "example with externalValue (a file outside the spec)");
                }
            }
        }
        let schema = holder.get("schema")?;
        if let Err(RefError::External(r)) = self.r.get(schema) {
            report.skip(self.operation, format!("schema: {}", RefError::External(r)));
            return None;
        }
        let v = example::example(self.r, schema, side);
        (!v.is_null()).then_some(v)
    }

    fn build(&self, variables: &mut BTreeMap<String, String>, schemes: &mut Schemes, collection_auth: Option<&Auth>, _opts: &Options, report: &mut OpenApiReport) -> BuiltOp {
        let op = self.op;
        let operation_id = op["operationId"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
        let summary = op["summary"].as_str().map(|s| s.lines().next().unwrap_or("").trim().to_string()).filter(|s| !s.is_empty());
        let name = summary.or_else(|| operation_id.clone()).unwrap_or_else(|| self.operation.to_string());

        // Path-level parameters, overridden by the operation's (same name and in).
        let mut params: Vec<&'a Value> = vec![];
        let own = op["parameters"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        for p in self.path_params.iter().chain(own) {
            let Some(p) = self.resolve(p, "parameter", report) else { continue };
            let key = (scalar(&p["name"]), scalar(&p["in"]));
            params.retain(|q| (scalar(&q["name"]), scalar(&q["in"])) != key);
            params.push(p);
        }

        let mut url_path = String::new();
        let mut rest = self.path;
        while let Some(start) = rest.find('{') {
            url_path.push_str(&rest[..start]);
            match rest[start..].find('}') {
                Some(end) => {
                    url_path.push_str(&format!("${{{}}}", &rest[start + 1..start + end]));
                    rest = &rest[start + end + 1..];
                }
                None => {
                    url_path.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        url_path.push_str(rest);
        let mut url = format!("{}{url_path}", self.base.clone().unwrap_or_else(|| format!("${{{BASE_URL}}}")));
        if self.base.is_some() {
            report.warn(self.operation, "has its own server: its URL is written into the request instead of ${baseUrl}");
        }

        let mut query = vec![];
        let mut disabled_params = vec![];
        let mut headers = vec![];
        for p in &params {
            let pname = scalar(&p["name"]);
            let required = p["required"] == true;
            let value = self.explicit_example(p, report).map(|v| param_text(&v)).unwrap_or_default();
            match p["in"].as_str() {
                Some("path") => {
                    variables.entry(pname.clone()).or_insert(value);
                }
                Some("query") => {
                    if required {
                        query.push((pname, value));
                    } else {
                        disabled_params.push(Param { key: pname, value, ..Default::default() });
                    }
                }
                Some("header") => {
                    if ["accept", "content-type", "authorization"].contains(&pname.to_ascii_lowercase().as_str()) {
                        continue;
                    }
                    let mut h = Header::new(pname, value);
                    h.enabled = required;
                    headers.push(h);
                }
                Some("cookie") => report.skip(self.operation, format!("cookie parameter {pname}")),
                other => report.skip(self.operation, format!("parameter {pname} in {}", other.unwrap_or("?"))),
            }
        }
        if !query.is_empty() {
            url = crate::params::with_query(&url, &query);
        }

        let mut request = Request { name, method: self.method.to_uppercase(), url, disabled_params, ..Default::default() };
        // An optional body on a method that rarely has one (DELETE) is left out.
        let wants_body = matches!(self.method, "post" | "put" | "patch");
        if let Some(body) = op.get("requestBody").filter(|b| wants_body || self.r.get(b).is_ok_and(|b| b["required"] == true)) {
            if let Some(body) = self.resolve(body, "request body", report) {
                self.body(body, &mut request, &mut headers, report);
            }
        }
        request.headers = headers;

        if op.get("security").is_some() {
            match schemes.requirement(&op["security"], self.operation, report) {
                Some(Some(a)) if Some(&a) == collection_auth => {}
                Some(Some(a)) => {
                    request.auth = Some(a);
                }
                // `security: []` (or `{}`, auth optional) turns the collection's off.
                Some(None) if collection_auth.is_some() && op["security"].as_array().is_some_and(|l| l.is_empty() || l.iter().any(|x| x.as_object().is_some_and(|o| o.is_empty()))) => {
                    request.auth = Some(Auth::None);
                }
                Some(None) => {}
                None => {}
            }
        }

        request.examples = self.examples(report);
        if op.get("callbacks").is_some() {
            report.skip(self.operation, "callbacks (requests the API sends back)");
        }
        BuiltOp { record: OpRecord { operation: self.operation.to_string(), operation_id, synced: BTreeMap::new(), rows: BTreeMap::new() }, request, file: String::new() }
    }

    fn body(&self, body: &'a Value, request: &mut Request, headers: &mut Vec<Header>, report: &mut OpenApiReport) {
        let Some(content) = body["content"].as_object() else { return };
        let pick = |pred: &dyn Fn(&str) -> bool| content.iter().find(|(k, _)| pred(&k.to_ascii_lowercase()));
        let is_json = |t: &str| t.starts_with("application/json") || (t.starts_with("application/") && t.split(';').next().unwrap_or("").ends_with("+json"));
        let chosen = pick(&is_json)
            .or_else(|| pick(&|t| t.starts_with("application/x-www-form-urlencoded")))
            .or_else(|| pick(&|t| t.starts_with("multipart/form-data")))
            .or_else(|| pick(&|t| t.contains("xml") || t.starts_with("text/")));
        // `*/*` with a schema (Kubernetes): JSON.
        let any = content.iter().find(|(k, v)| (k.as_str() == "*/*" || k.as_str() == "application/*") && v.get("schema").is_some());
        let mut chosen = chosen.map(|(k, v)| (k.clone(), v));
        if chosen.is_none() {
            chosen = any.map(|(_, v)| ("application/json".to_string(), v));
        }
        let Some((media_type, media)) = chosen else {
            let types = content.keys().cloned().collect::<Vec<_>>().join(", ");
            report.skip(self.operation, format!("request body {types}: attach it yourself"));
            return;
        };
        let media_type = &media_type;
        let lower = media_type.to_ascii_lowercase();
        if lower.starts_with("application/x-www-form-urlencoded") || lower.starts_with("multipart/form-data") {
            let multipart = lower.starts_with("multipart/");
            request.body_mode = Some(if multipart { "multipart" } else { "urlencoded" }.into());
            let example = self.example_of(media, Side::Request, report);
            let schema = media.get("schema").and_then(|s| self.r.get(s).ok());
            let mut props: Vec<(String, &Value)> = vec![];
            if let Some(schema) = schema {
                let mut parts = vec![schema];
                parts.extend(schema["allOf"].as_array().into_iter().flatten().filter_map(|p| self.r.get(p).ok()));
                for part in parts {
                    for (k, v) in part["properties"].as_object().into_iter().flatten() {
                        props.push((k.clone(), self.r.get(v).unwrap_or(v)));
                    }
                }
            }
            for (key, prop) in props {
                let format = prop["format"].as_str().unwrap_or("");
                let is_file = multipart && (format == "binary" || format == "base64" || prop.get("contentMediaType").is_some() || prop["items"]["format"] == "binary");
                let value = if is_file { String::new() } else { example.as_ref().and_then(|e| e.get(&key)).map(param_text).unwrap_or_default() };
                request.form_fields.push(FormField { key, value, kind: if is_file { "file" } else { "text" }.into(), ..Default::default() });
            }
            return;
        }
        headers.insert(0, Header::new("Content-Type", media_type.clone()));
        let example = self.example_of(media, Side::Request, report);
        request.body = match example {
            Some(Value::String(s)) => s,
            Some(v) if is_json(&lower) => serde_json::to_string_pretty(&v).unwrap_or_default(),
            Some(_) | None => {
                if !is_json(&lower) {
                    report.warn(self.operation, format!("{media_type} body has no example: write one"));
                }
                String::new()
            }
        };
    }

    /// Saved examples from the responses' `example` and `examples`.
    fn examples(&self, report: &mut OpenApiReport) -> Vec<Value> {
        let mut out = vec![];
        for (code, resp) in self.op["responses"].as_object().into_iter().flatten() {
            let Some(resp) = self.resolve(resp, &format!("response {code}"), report) else { continue };
            let Some(content) = resp["content"].as_object() else { continue };
            let media = content.iter().find(|(k, _)| k.contains("json")).or_else(|| content.iter().next());
            let Some((_, media)) = media else { continue };
            let status = code.parse::<u64>().ok();
            let description = resp["description"].as_str().and_then(|d| d.lines().next()).map(str::trim).unwrap_or("").chars().take(60).collect::<String>();
            let mut found: Vec<(String, Value)> = vec![];
            if let Some(v) = media.get("example") {
                found.push((String::new(), v.clone()));
            }
            for (key, ex) in media["examples"].as_object().into_iter().flatten() {
                let Some(ex) = self.resolve(ex, "example", report) else { continue };
                if let Some(v) = ex.get("value") {
                    let label = ex["summary"].as_str().map(String::from).unwrap_or_else(|| key.clone());
                    found.push((label, v.clone()));
                } else if ex.get("externalValue").is_some() {
                    report.skip(self.operation, format!("example {key} with externalValue (a file outside the spec)"));
                }
            }
            for (label, value) in found {
                let label = if !label.is_empty() { label } else { description.clone() };
                let name = if label.is_empty() { code.clone() } else { format!("{code} {label}") };
                let mut e = serde_json::Map::new();
                e.insert("name".into(), json!(name));
                if let Some(s) = status {
                    e.insert("status".into(), json!(s));
                }
                let body = match value {
                    Value::String(s) => s,
                    other => serde_json::to_string_pretty(&other).unwrap_or_default(),
                };
                e.insert("body".into(), json!(body));
                e.insert("source".into(), json!(EXAMPLE_SOURCE));
                out.push(Value::Object(e));
            }
        }
        out
    }
}
