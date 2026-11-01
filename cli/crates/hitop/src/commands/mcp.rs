//! `hitop mcp [folder]`: an MCP server over stdio (the official `rmcp` SDK).
//! The workspace is one collection folder, or global storage without one.
//!
//! Tool names, parameters and descriptions are a contract: clients and the
//! models behind them rely on them, so change them only additively.
//!
//! Secrets: values are resolved as in `hitop run` (HITOP_SECRET_* env vars,
//! then the OS keychain; no prompt, stdin is the protocol), and every value
//! seen in the session is masked as •••• in every tool output and error.
//! Requests run through `runner::run_collection_jar`, so variable precedence
//! and the cookie jar are the ones of `hitop run`.
//!
//! stdout is the protocol: nothing reached from here prints to it (notes
//! from secret resolution go to stderr).
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::Args;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use hitop_core::{
    model::{Auth, Collection, Environment, Header, Request},
    report::{ReportBuilder, RequestReport},
    runner::{self, RunOptions, RunStepResult},
    secrets::{Masker, Secrets},
    storage,
};

use super::cookies::{self, CookieFlags};
use super::secret::{self, SecretFlags};
use super::source::Source;

/// Response bodies longer than this come back cut, with `bodyTruncated`.
const MAX_BODY: usize = 64 * 1024;
/// Time limit for each request unless the call gives `timeout_ms`.
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

const INSTRUCTIONS: &str = "hitop: an HTTP API client. This server works on one workspace: a hitop \
collection folder (hitop.yaml, requests/, environments/) or, without one, hitop's global storage. \
Requests are found by name (or by their file path in a folder, such as orders/create-order). \
Reference variables as ${name}; never put secret values in requests: secret variables are resolved \
when a request runs (HITOP_SECRET_<ENV>_<NAME> env vars, the OS keychain, or ${secret:vault|aws|azure|op:...} \
references already in the collection files) and every secret value \
is shown as •••• in results. Variables, later ones win: globals, collection, environment, the call's \
`variables`, variables set by scripts, secrets. Cookies that responses set are kept and sent like in \
`hitop run`. There is no delete tool.";

#[derive(Args)]
pub struct McpArgs {
    /// Collection folder to serve (omit to serve global storage)
    folder: Option<PathBuf>,

    /// Don't resolve ${secret:vault|aws|azure|op:...} references (also HITOP_SECRET_MANAGERS=off)
    #[arg(long)]
    no_secret_managers: bool,
}

/// What the session remembers between calls.
struct State {
    /// The collection folder, or None for global storage.
    folder: Option<PathBuf>,
    /// The environment requests run with (`set_environment`).
    environment: Option<String>,
    /// Every secret value resolved in this session, masked in all output.
    known: Vec<String>,
    /// `--no-secret-managers`.
    no_secret_managers: bool,
}

#[derive(Clone)]
pub struct Hitop {
    state: Arc<Mutex<State>>,
    tool_router: ToolRouter<Self>,
}

pub fn run(args: McpArgs) -> Result<()> {
    let mut state = State::new(args.folder)?;
    state.no_secret_managers = args.no_secret_managers;
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async move {
        let server = Hitop { state: Arc::new(Mutex::new(state)), tool_router: Hitop::tool_router() };
        let service = server.serve(rmcp::transport::stdio()).await.context("starting the MCP server")?;
        service.waiting().await.context("MCP server")?;
        anyhow::Ok(())
    })
}

impl State {
    fn new(folder: Option<PathBuf>) -> Result<State> {
        let environment = match &folder {
            Some(dir) => {
                if !hitop_core::format::is_folder(dir) {
                    bail!("{} is not a collection folder (no hitop.yaml; create one with `hitop init {}`)", dir.display(), dir.display());
                }
                None
            }
            // Global storage starts with the active environment, as `hitop run` does.
            None => storage::load_environments()?.pick(None)?.map(|e| e.name.clone()),
        };
        Ok(State { folder, environment, known: vec![], no_secret_managers: false })
    }

    fn open(&self, collection: Option<&str>) -> Result<Source> {
        match (&self.folder, collection) {
            (Some(dir), name) => {
                let source = Source::open(&dir.to_string_lossy())?;
                if let Some(n) = name.filter(|n| !n.is_empty()) {
                    if n != source.collection.name {
                        bail!("this server serves the folder {} with the collection '{}', not '{n}'", dir.display(), source.collection.name);
                    }
                }
                Ok(source)
            }
            (None, Some(n)) if !n.is_empty() => Source::global(n),
            (None, _) => {
                let names: Vec<String> = storage::load_collections()?.into_iter().map(|c| c.name).collect();
                bail!("give `collection` (collections: {})", if names.is_empty() { "none".into() } else { names.join(", ") })
            }
        }
    }

    /// The environment to use: the call's, else the session's.
    fn environment(&self, source: &Source, name: Option<&str>) -> Result<Option<Environment>> {
        match name.filter(|n| !n.is_empty()).or(self.environment.as_deref()) {
            Some(n) => source.environment(Some(n)),
            None => Ok(None),
        }
    }

    fn environments(&self) -> Result<Vec<Environment>> {
        Ok(match &self.folder {
            Some(dir) => hitop_core::format::read_folder(dir)?.environments,
            None => storage::load_environments()?.environments,
        })
    }

    fn globals(&self) -> Result<Environment> {
        Ok(match &self.folder {
            Some(dir) => hitop_core::format::read_globals(dir)?.unwrap_or_else(Environment::globals),
            None => storage::load_globals()?,
        })
    }

    /// Resolves the environment's and the globals' secrets, remembering the
    /// values for masking.
    /// `refs`: the `${secret:...}` references to resolve (each tool call
    /// resolves them afresh, so a rotated value is picked up).
    fn resolve(&mut self, source: &Source, env: Option<&Environment>, refs: &std::collections::BTreeSet<String>) -> Result<(Secrets, HashMap<String, String>, Secrets)> {
        let ws = source.workspace();
        let flags = SecretFlags { no_secret_managers: self.no_secret_managers, ..Default::default() };
        let mut secrets = secret::resolve(&ws, env, &flags).map_err(no_flag_hint)?;
        secret::add_refs(&mut secrets, refs, &flags)?;
        self.remember(&secrets);
        let (globals, global_secrets) = secret::resolve_globals(&ws, &source.globals()?, &flags).map_err(no_flag_hint)?;
        self.remember(&global_secrets);
        Ok((secrets, globals, global_secrets))
    }

    /// [`resolve`] for masking only: a secret without a value is fine here.
    fn learn(&mut self, source: &Source) {
        if let Ok(env) = self.environment(source, None) {
            let _ = self.resolve(source, env.as_ref(), &Default::default());
        }
    }

    fn remember(&mut self, secrets: &Secrets) {
        for name in secrets.names() {
            if let Some(v) = secrets.get(name) {
                if !v.is_empty() && !self.known.iter().any(|k| k == v) {
                    self.known.push(v.to_string());
                }
            }
        }
    }

    fn masker(&self) -> Masker {
        Masker::new(self.known.iter().map(String::as_str))
    }

    /// Refuses input that holds a secret value: it would land in a file.
    fn refuse_secrets(&self, inputs: &[&str]) -> Result<()> {
        let m = self.masker();
        if inputs.iter().any(|s| m.mask(s) != *s) {
            bail!("the request holds the value of a secret variable; reference the variable as ${{name}} instead, so the value stays out of the collection files");
        }
        Ok(())
    }
}

/// The `${secret:...}` references in the collection's variables and auth,
/// the environment and the globals. Not the call's `variables`: a client
/// may only use references the workspace files already hold.
fn refs_around(source: &Source, env: Option<&Environment>, refs: &mut std::collections::BTreeSet<String>) -> Result<()> {
    hitop_core::secret_refs::find_in_variables(&source.collection.variables, refs);
    if let Some(auth) = &source.collection.auth {
        hitop_core::secret_refs::find_in_json(&serde_json::to_value(auth)?, refs);
    }
    if let Some(e) = env {
        hitop_core::secret_refs::find_in_environment(e, refs);
    }
    hitop_core::secret_refs::find_in_environment(&source.globals()?, refs);
    Ok(())
}

/// A missing secret's error without the `--secret` hint: there are no flags
/// here; the env var (in the client's server config) or the keychain give it.
fn no_flag_hint(e: anyhow::Error) -> anyhow::Error {
    let mut text = format!("{e:#}");
    while let Some(start) = text.find(", pass --secret ") {
        match text[start..].find("=...").map(|end| start + end + 4) {
            Some(end) => text.replace_range(start..end, ""),
            None => break,
        }
    }
    anyhow::anyhow!(text)
}

/// Masks every string in a JSON value, keys included.
fn mask_value(m: &Masker, v: Value) -> Value {
    match v {
        Value::String(s) => Value::String(m.mask(&s)),
        Value::Array(a) => Value::Array(a.into_iter().map(|x| mask_value(m, x)).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| (m.mask(&k), mask_value(m, x))).collect()),
        other => other,
    }
}

fn find<'a>(col: &'a Collection, request: &str) -> Result<&'a Request> {
    col.requests
        .iter()
        .find(|r| r.name == request)
        .or_else(|| col.requests.iter().find(|r| r.file.as_deref() == Some(request)))
        .ok_or_else(|| {
            let names: Vec<&str> = col.requests.iter().map(|r| r.name.as_str()).collect();
            anyhow::anyhow!("request '{request}' not found in '{}' (requests: {})", col.name, if names.is_empty() { "none".to_string() } else { names.join(", ") })
        })
}

fn find_mut<'a>(col: &'a mut Collection, request: &str) -> Result<&'a mut Request> {
    let name = find(col, request)?.name.clone();
    let file = find(col, request)?.file.clone();
    Ok(col.requests.iter_mut().find(|r| r.name == name && r.file == file).expect("found above"))
}

fn request_view(r: &Request) -> Value {
    let mut v = json!({
        "name": r.name,
        "file": r.file,
        "method": r.method,
        "url": r.url,
        "headers": r.headers.iter().filter(|h| !h.name.is_empty()).map(|h| json!({"name": h.name, "value": h.value, "enabled": h.enabled})).collect::<Vec<_>>(),
        "body": r.body,
        "auth": r.auth,
        "preRequestScript": r.pre_request_script,
        "postRequestScript": r.post_request_script,
    });
    if let Some(o) = v.as_object_mut() {
        if let Some(t) = &r.body_type {
            o.insert("bodyType".into(), json!(t));
        }
        if let Some(g) = &r.graphql {
            o.insert("graphql".into(), json!({"variables": g.variables, "operationName": g.operation_name}));
        }
        if let Some(k) = &r.kind {
            o.insert("kind".into(), json!(k));
        }
        if let Some(w) = &r.websocket {
            o.insert("websocket".into(), json!({"subprotocols": w.subprotocols, "messages": w.messages.iter().map(|m| json!({"name": m.name, "content": m.content})).collect::<Vec<_>>()}));
        }
        if let Some(g) = &r.grpc {
            o.insert(
                "grpc".into(),
                json!({"service": g.service, "method": g.method, "message": g.message, "protos": g.protos, "importPaths": g.import_paths, "plaintext": g.plaintext}),
            );
        }
        if let Some(mode) = &r.body_mode {
            o.insert("bodyMode".into(), json!(mode));
            o.insert("formFields".into(), json!(r.form_fields.iter().map(|f| json!({"key": f.key, "value": f.value, "type": f.kind, "enabled": f.enabled})).collect::<Vec<_>>()));
        }
    }
    v
}

fn step_view(step: &RunStepResult, environment: Option<&str>) -> Value {
    let mut v = serde_json::to_value(RequestReport::from_step(step)).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.insert("environment".into(), json!(environment));
        if let Ok(res) = &step.result {
            let headers: BTreeMap<&String, &String> = res.headers.iter().collect();
            o.insert("headers".into(), json!(headers));
            let (body, truncated) = cut(&res.body, MAX_BODY);
            o.insert("body".into(), json!(body));
            o.insert("bodyTruncated".into(), json!(truncated));
        }
    }
    v
}

fn cut(s: &str, max: usize) -> (&str, bool) {
    if s.len() <= max {
        return (s, false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (&s[..end], true)
}

fn timestamp_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

fn parse_auth(v: Value) -> Result<Auth> {
    serde_json::from_value(v).context(
        "auth: {\"type\":\"none\"}, {\"type\":\"bearer\",\"token\":\"${token}\"}, {\"type\":\"basic\",\"username\":\"...\",\"password\":\"${password}\"} or {\"type\":\"apikey\",\"key\":\"X-Api-Key\",\"value\":\"${key}\",\"in\":\"header\"|\"query\"}",
    )
}

fn auth_strings(a: &Auth) -> Vec<String> {
    match a {
        Auth::None => vec![],
        Auth::Bearer { token } => vec![token.clone()],
        Auth::Basic { username, password } => vec![username.clone(), password.clone()],
        Auth::ApiKey { key, value, .. } => vec![key.clone(), value.clone()],
    }
}

fn headers_of(input: Vec<HeaderInput>) -> Vec<Header> {
    input
        .into_iter()
        .map(|h| Header { enabled: h.enabled.unwrap_or(true), ..Header::new(h.name, h.value) })
        .collect()
}

fn options(variables: Option<BTreeMap<String, String>>, timeout_ms: Option<u64>, globals: HashMap<String, String>, global_secrets: Secrets) -> RunOptions {
    RunOptions {
        stop_on_fail: false,
        timeout: Some(Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS).max(1))),
        overrides: variables.unwrap_or_default().into_iter().collect(),
        globals,
        global_secrets,
    }
}

// ---- Tool parameters -------------------------------------------------------

const COLLECTION: &str = "Collection name. Required with global storage; with a folder workspace it can be left out.";

#[derive(Deserialize, JsonSchema)]
pub struct CollectionParam {
    #[schemars(description = COLLECTION)]
    pub collection: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RequestParam {
    #[schemars(description = COLLECTION)]
    pub collection: Option<String>,
    /// Request name (or its file path in a folder, such as orders/create-order).
    pub request: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct RunRequestParams {
    #[schemars(description = COLLECTION)]
    pub collection: Option<String>,
    /// Request name (or its file path in a folder, such as orders/create-order).
    pub request: String,
    /// Environment for this call only; the session's (set_environment) by default.
    pub environment: Option<String>,
    /// Variables for this call, like `hitop run --var`: they override globals, collection and environment variables, but not secrets.
    pub variables: Option<BTreeMap<String, String>>,
    /// Time limit for the request in milliseconds (default 30000).
    pub timeout_ms: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RunCollectionParams {
    #[schemars(description = COLLECTION)]
    pub collection: Option<String>,
    /// Environment for this call only; the session's (set_environment) by default.
    pub environment: Option<String>,
    /// Variables for this run, like `hitop run --var`: they override globals, collection and environment variables, but not secrets.
    pub variables: Option<BTreeMap<String, String>>,
    /// Stop at the first request that fails (default false: run every request).
    pub bail: Option<bool>,
    /// Time limit for each request in milliseconds (default 30000).
    pub timeout_ms: Option<u64>,
    /// Run the collection this many times (default 1, at most 100).
    pub iterations: Option<u64>,
}

#[derive(Deserialize, JsonSchema)]
pub struct HeaderInput {
    pub name: String,
    pub value: String,
    /// False keeps the header in the request without sending it (default true).
    pub enabled: Option<bool>,
}

const AUTH: &str = "Auth preset: {\"type\":\"none\"}, {\"type\":\"bearer\",\"token\":\"${token}\"}, {\"type\":\"basic\",\"username\":\"u\",\"password\":\"${password}\"} or {\"type\":\"apikey\",\"key\":\"X-Api-Key\",\"value\":\"${apiKey}\",\"in\":\"header\" or \"query\"}. Without auth the collection's default applies.";

#[derive(Deserialize, JsonSchema)]
pub struct CreateRequestParams {
    #[schemars(description = COLLECTION)]
    pub collection: Option<String>,
    /// Name of the new request (unique in the collection).
    pub name: String,
    /// HTTP method (default GET).
    pub method: Option<String>,
    /// URL; may contain ${variables}.
    pub url: String,
    /// Headers, in order.
    pub headers: Option<Vec<HeaderInput>>,
    /// Raw body (JSON, text, XML, ...); may contain ${variables}.
    pub body: Option<String>,
    #[schemars(description = AUTH)]
    pub auth: Option<Value>,
    /// JavaScript run before the request (pm.* and hitop's script API).
    pub pre_request_script: Option<String>,
    /// JavaScript run after the response, for example pm.test(...) or assert.status(200).
    pub post_request_script: Option<String>,
    /// Folder workspaces: file path under requests/ without .yaml (such as orders/create-order); default from the name.
    pub file: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateRequestParams {
    #[schemars(description = COLLECTION)]
    pub collection: Option<String>,
    /// The request to change: its name (or file path in a folder).
    pub request: String,
    /// New name.
    pub name: Option<String>,
    /// New HTTP method.
    pub method: Option<String>,
    /// New URL.
    pub url: Option<String>,
    /// New headers: replace all of them.
    pub headers: Option<Vec<HeaderInput>>,
    /// New raw body.
    pub body: Option<String>,
    #[schemars(description = AUTH)]
    pub auth: Option<Value>,
    /// New pre-request script.
    pub pre_request_script: Option<String>,
    /// New post-request script.
    pub post_request_script: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SetEnvironmentParams {
    /// Environment name; leave out (or null) to run without an environment.
    pub name: Option<String>,
}

// ---- Tools -----------------------------------------------------------------

impl Hitop {
    /// Runs `f` off the async runtime (hitop-core's HTTP client blocks), one
    /// call at a time, and masks the result or error.
    async fn call<F>(&self, f: F) -> CallToolResult
    where
        F: FnOnce(&mut State) -> Result<Value> + Send + 'static,
    {
        let state = self.state.clone();
        let out = tokio::task::spawn_blocking(move || {
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
            let result = f(&mut st);
            let m = st.masker();
            match result {
                Ok(v) => Ok(m.mask(&serde_json::to_string_pretty(&mask_value(&m, v)).unwrap_or_default())),
                Err(e) => Err(m.mask(&format!("{e:#}"))),
            }
        })
        .await;
        match out {
            Ok(Ok(text)) => CallToolResult::success(vec![ContentBlock::text(text)]),
            Ok(Err(e)) => CallToolResult::error(vec![ContentBlock::text(e)]),
            Err(_) => CallToolResult::error(vec![ContentBlock::text("internal error: the tool stopped unexpectedly")]),
        }
    }
}

#[tool_router]
impl Hitop {
    #[tool(
        description = "List the collections in this workspace: the folder's collection, or every collection in global storage, with how many requests each has.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_collections(&self) -> CallToolResult {
        self.call(|st| {
            let (workspace, cols) = match &st.folder {
                Some(dir) => (format!("folder {}", dir.display()), vec![Source::open(&dir.to_string_lossy())?.collection]),
                None => ("global storage".to_string(), storage::load_collections()?),
            };
            Ok(json!({
                "workspace": workspace,
                "collections": cols.iter().map(|c| json!({"name": c.name, "requests": c.requests.len()})).collect::<Vec<_>>(),
            }))
        })
        .await
    }

    #[tool(
        description = "List a collection's requests in run order: name, method, URL and (in a folder) file path.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_requests(&self, Parameters(p): Parameters<CollectionParam>) -> CallToolResult {
        self.call(move |st| {
            let source = st.open(p.collection.as_deref())?;
            st.learn(&source);
            let col = &source.collection;
            Ok(json!({
                "collection": col.name,
                "requests": col.requests.iter().map(|r| {
                    let mut v = json!({"name": r.name, "method": r.method, "url": r.url, "file": r.file});
                    if let Some(k) = &r.kind {
                        v["kind"] = json!(k);
                    }
                    v
                }).collect::<Vec<_>>(),
            }))
        })
        .await
    }

    #[tool(
        description = "Show one request as saved: method, URL, headers, body, auth, and pre- and post-request scripts. ${variables} are shown as written, not resolved; secret values are never shown.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_request(&self, Parameters(p): Parameters<RequestParam>) -> CallToolResult {
        self.call(move |st| {
            let source = st.open(p.collection.as_deref())?;
            st.learn(&source);
            let mut v = request_view(find(&source.collection, &p.request)?);
            v["collection"] = json!(source.collection.name);
            Ok(v)
        })
        .await
    }

    #[tool(
        description = "Send one saved request with the session's environment (or `environment`) and return the response: status, headers, body (cut at 64 KiB), timing, and the results of its post-request assertions. Variables and cookies work as in `hitop run`; secret values are masked as ••••.",
        annotations(read_only_hint = false, destructive_hint = false, open_world_hint = true)
    )]
    async fn run_request(&self, Parameters(p): Parameters<RunRequestParams>) -> CallToolResult {
        self.call(move |st| {
            let source = st.open(p.collection.as_deref())?;
            let env = st.environment(&source, p.environment.as_deref())?;
            let request = find(&source.collection, &p.request)?;
            let mut refs = std::collections::BTreeSet::new();
            hitop_core::secret_refs::find_in_request(request, &mut refs);
            refs_around(&source, env.as_ref(), &mut refs)?;
            let (secrets, globals, global_secrets) = st.resolve(&source, env.as_ref(), &refs)?;
            let mut col = source.collection.clone();
            if request.is_websocket() {
                bail!("'{}' is a WebSocket request: this server doesn't open WebSocket connections (use `hitop ws` in a terminal)", request.name);
            }
            if request.is_grpc() {
                bail!("'{}' is a gRPC request: this server doesn't make gRPC calls (use `hitop grpc` in a terminal)", request.name);
            }
            col.requests = vec![request.clone()];
            let options = options(p.variables, p.timeout_ms, globals, global_secrets);
            let ws = source.workspace();
            let mut jar = cookies::open(&ws, CookieFlags::default())?;
            let steps = runner::run_collection_jar(&col, env.as_ref(), &secrets, &options, jar.as_mut(), &mut |_| {});
            cookies::save(&ws, jar.as_ref())?;
            let step = steps.first().context("the request did not run")?;
            Ok(step_view(step, env.as_ref().map(|e| e.name.as_str())))
        })
        .await
    }

    #[tool(
        description = "Run every request of a collection in order, like `hitop run`, and return the run report (hitop's report.json, reportVersion 1: per request status, timing, error and assertion results, and a summary). Script-set variables and cookies carry from one request to the next; secret values are masked as ••••.",
        annotations(read_only_hint = false, destructive_hint = false, open_world_hint = true)
    )]
    async fn run_collection(&self, Parameters(p): Parameters<RunCollectionParams>) -> CallToolResult {
        self.call(move |st| {
            let iterations = p.iterations.unwrap_or(1);
            if !(1..=100).contains(&iterations) {
                bail!("iterations must be between 1 and 100");
            }
            let source = st.open(p.collection.as_deref())?;
            let env = st.environment(&source, p.environment.as_deref())?;
            let mut refs = std::collections::BTreeSet::new();
            hitop_core::secret_refs::find_in_collection(&source.collection, &mut refs);
            refs_around(&source, env.as_ref(), &mut refs)?;
            let (secrets, globals, global_secrets) = st.resolve(&source, env.as_ref(), &refs)?;
            let bail_on_fail = p.bail.unwrap_or(false);
            let mut options = options(p.variables, p.timeout_ms, globals, global_secrets);
            options.stop_on_fail = bail_on_fail;
            let ws = source.workspace();
            let mut jar = cookies::open(&ws, CookieFlags::default())?;
            let col = &source.collection;
            let mut builder = ReportBuilder::new(env!("CARGO_PKG_VERSION"));
            builder.collection(&col.name, env.as_ref().map(|e| e.name.as_str()));
            let mut bailed = false;
            for _ in 0..iterations {
                builder.start_iteration();
                let steps = runner::run_collection_jar(col, env.as_ref(), &secrets, &options, jar.as_mut(), &mut |_| {});
                let failed = steps.iter().any(|s| !s.passed());
                builder.iteration(None, &steps);
                if failed && bail_on_fail {
                    bailed = true;
                    break;
                }
            }
            cookies::save(&ws, jar.as_ref())?;
            Ok(serde_json::to_value(builder.finish(bailed))?)
        })
        .await
    }

    #[tool(
        description = "Add a request to a collection and save it (a new YAML file in a folder workspace). Reference variables as ${name}; a request that contains a secret's value is refused. Returns the saved request.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_request(&self, Parameters(p): Parameters<CreateRequestParams>) -> CallToolResult {
        self.call(move |st| {
            let mut source = st.open(p.collection.as_deref())?;
            st.learn(&source);
            if p.name.trim().is_empty() {
                bail!("name must not be empty");
            }
            if source.collection.requests.iter().any(|r| r.name == p.name) {
                bail!("a request named '{}' already exists in '{}'; change it with update_request", p.name, source.collection.name);
            }
            let auth = p.auth.map(parse_auth).transpose()?;
            let req = Request {
                id: timestamp_id(),
                name: p.name,
                method: p.method.filter(|m| !m.is_empty()).unwrap_or_else(|| "GET".into()).to_uppercase(),
                url: p.url,
                headers: headers_of(p.headers.unwrap_or_default()),
                body: p.body.unwrap_or_default(),
                pre_request_script: p.pre_request_script.unwrap_or_default(),
                post_request_script: p.post_request_script.unwrap_or_default(),
                auth,
                file: p.file.filter(|f| !f.is_empty()),
                ..Default::default()
            };
            check(st, &req)?;
            if req.file.is_some() && source.folder().is_none() {
                bail!("`file` is for folder workspaces only");
            }
            source.collection.requests.push(req);
            source.save()?;
            let source = st.open(Some(&source.collection.name))?;
            let saved = source.collection.requests.last().context("not saved")?;
            let mut v = request_view(saved);
            v["collection"] = json!(source.collection.name);
            Ok(v)
        })
        .await
    }

    #[tool(
        description = "Change a saved request: give only the fields to change (headers replace all headers). Reference variables as ${name}; a change that contains a secret's value is refused. Returns the saved request.",
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn update_request(&self, Parameters(p): Parameters<UpdateRequestParams>) -> CallToolResult {
        self.call(move |st| {
            let mut source = st.open(p.collection.as_deref())?;
            st.learn(&source);
            if let Some(n) = &p.name {
                if n.trim().is_empty() {
                    bail!("name must not be empty");
                }
                let current = find(&source.collection, &p.request)?.name.clone();
                if *n != current && source.collection.requests.iter().any(|r| r.name == *n) {
                    bail!("a request named '{n}' already exists in '{}'", source.collection.name);
                }
            }
            let auth = p.auth.map(parse_auth).transpose()?;
            let req = find_mut(&mut source.collection, &p.request)?;
            if let Some(v) = p.name {
                req.name = v;
            }
            if let Some(v) = p.method.filter(|m| !m.is_empty()) {
                req.method = v.to_uppercase();
            }
            if let Some(v) = p.url {
                req.url = v;
            }
            if let Some(v) = p.headers {
                req.headers = headers_of(v);
            }
            if let Some(v) = p.body {
                req.body = v;
            }
            if let Some(v) = auth {
                req.auth = Some(v);
            }
            if let Some(v) = p.pre_request_script {
                req.pre_request_script = v;
            }
            if let Some(v) = p.post_request_script {
                req.post_request_script = v;
            }
            let (name, file) = (req.name.clone(), req.file.clone());
            check(st, req)?;
            source.save()?;
            let source = st.open(Some(&source.collection.name))?;
            let saved = source.collection.requests.iter().find(|r| r.name == name && (file.is_none() || r.file == file)).context("not saved")?;
            let mut v = request_view(saved);
            v["collection"] = json!(source.collection.name);
            Ok(v)
        })
        .await
    }

    #[tool(
        description = "List the environments (the folder's environments/, or global storage's) and the globals, with their variables. Secret variables are listed by name only, never with a value. `active` is the environment requests run with in this session.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_environments(&self) -> CallToolResult {
        self.call(|st| {
            let view = |e: &Environment| {
                let vars: BTreeMap<&String, &String> = e.variables.iter().filter(|(k, _)| !e.secrets.contains(k)).collect();
                json!({"name": e.name, "variables": vars, "secrets": e.secrets})
            };
            let envs = st.environments()?;
            let globals = st.globals()?;
            let g = view(&globals);
            Ok(json!({
                "active": st.environment,
                "environments": envs.iter().map(|e| {
                    let mut v = view(e);
                    v["active"] = json!(st.environment.as_deref() == Some(e.name.as_str()));
                    v
                }).collect::<Vec<_>>(),
                "globals": {"variables": g["variables"], "secrets": g["secrets"]},
            }))
        })
        .await
    }

    #[tool(
        description = "Choose the environment that run_request and run_collection use from now on in this session (leave `name` out to run without one). Only this session changes; hitop's saved active environment stays as it is.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn set_environment(&self, Parameters(p): Parameters<SetEnvironmentParams>) -> CallToolResult {
        self.call(move |st| {
            let names: Vec<String> = st.environments()?.into_iter().map(|e| e.name).collect();
            match p.name.filter(|n| !n.is_empty()) {
                Some(n) if !names.contains(&n) => {
                    bail!("environment '{n}' not found (environments: {})", if names.is_empty() { "none".into() } else { names.join(", ") })
                }
                name => st.environment = name,
            }
            Ok(json!({"active": st.environment, "environments": names}))
        })
        .await
    }
}

/// A request about to be saved holds no secret value.
fn check(st: &State, r: &Request) -> Result<()> {
    let mut inputs: Vec<&str> = vec![&r.url, &r.body, &r.pre_request_script, &r.post_request_script, &r.name];
    for h in &r.headers {
        inputs.push(&h.name);
        inputs.push(&h.value);
    }
    let auth = r.auth.as_ref().map(auth_strings).unwrap_or_default();
    inputs.extend(auth.iter().map(String::as_str));
    st.refuse_secrets(&inputs)
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Hitop {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("hitop", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}
