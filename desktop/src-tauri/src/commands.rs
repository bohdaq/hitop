use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Emitter;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hitop_core::cookies::Jar;
use hitop_core::secrets::GLOBAL_WORKSPACE;
use hitop_core::{
    format,
    http,
    model::Environment,
    secrets,
    interpolation::interpolate,
    model::{Collection, HistoryItem, TestResult},
    runner,
    scripting::{run_post_request_script_with, run_pre_request_script, PostRequestCtx, PreRequestCtx, SentRequest, VariableUpdates},
    storage,
};

// ── Storage ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn load_collections() -> Result<Vec<Collection>, String> {
    storage::load_collections().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_collections(collections: Vec<Collection>) -> Result<(), String> {
    storage::save_collections(&collections).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_history() -> Result<Vec<HistoryItem>, String> {
    storage::load_history().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_history(history: Vec<HistoryItem>) -> Result<(), String> {
    storage::save_history(&history).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_contexts() -> Result<Value, String> {
    storage::load_contexts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_contexts(contexts: Value) -> Result<(), String> {
    storage::save_contexts(&contexts).map_err(|e| e.to_string())
}

/// The globals of global storage (`globals.json`, shared with `hitop globals`);
/// secret values are in the keychain as `global/global/<name>`.
#[tauri::command]
pub fn load_globals() -> Result<Environment, String> {
    storage::load_globals().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_globals(globals: Environment) -> Result<(), String> {
    storage::save_globals(&globals).map_err(|e| e.to_string())
}

/// Environments, shared with the CLI (`environments.json`).
#[tauri::command]
pub fn load_environments() -> Result<storage::Environments, String> {
    storage::load_environments().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_environments(environments: storage::Environments) -> Result<(), String> {
    storage::save_environments(&environments).map_err(|e| e.to_string())
}

// ── HTTP ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ExecuteRequestArgs {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Serialize)]
pub struct ExecuteRequestResult {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub duration_ms: u64,
}

#[tauri::command]
pub fn execute_request(args: ExecuteRequestArgs) -> Result<ExecuteRequestResult, String> {
    http::execute(&args.method, &args.url, &args.headers, &args.body)
        .map(|r| ExecuteRequestResult {
            status: r.status,
            headers: r.headers,
            body: r.body,
            duration_ms: r.duration_ms,
        })
        .map_err(|e| e.to_string())
}

// ── Scripting ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PreScriptArgs {
    pub script: String,
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub variables: HashMap<String, String>,
    pub context: HashMap<String, Value>,
    /// The globals scope alone, for `pm.globals`.
    #[serde(default)]
    pub globals: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct PreScriptResult {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub context_updates: HashMap<String, Value>,
    /// Variables the script set (`pm.environment.set`, ...); `null` unsets one.
    pub variables: VariableUpdates,
    /// Globals the script set (`pm.globals.set`, ...).
    pub globals: VariableUpdates,
}

#[tauri::command]
pub fn run_pre_script(args: PreScriptArgs) -> Result<PreScriptResult, String> {
    let ctx = PreRequestCtx {
        url: args.url,
        method: args.method,
        headers: args.headers,
        body: args.body,
        variables: args.variables,
        context: args.context,
        globals: args.globals,
    };
    run_pre_request_script(&args.script, ctx)
        .map(|r| PreScriptResult {
            url: r.url,
            headers: r.headers,
            body: r.body,
            context_updates: r.context_updates,
            variables: r.variables,
            globals: r.globals,
        })
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct PostScriptArgs {
    pub script: String,
    pub response_body: String,
    pub response_headers: HashMap<String, String>,
    pub status_code: u16,
    #[serde(default)]
    pub duration_ms: u64,
    pub variables: HashMap<String, String>,
    pub context: HashMap<String, Value>,
    /// The request as sent, for `pm.request`.
    #[serde(default)]
    pub request: Option<SentRequestArgs>,
    /// The globals scope alone, for `pm.globals` (left out: pm.globals
    /// shares the variables).
    #[serde(default)]
    pub globals: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct SentRequestArgs {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: String,
}

#[derive(Serialize)]
pub struct PostScriptResult {
    pub context_updates: HashMap<String, Value>,
    pub tests: Vec<TestResult>,
    /// Variables the script set, as in [`PreScriptResult::variables`].
    pub variables: VariableUpdates,
    /// Globals the script set.
    pub globals: VariableUpdates,
}

#[tauri::command]
pub fn run_post_script(args: PostScriptArgs) -> Result<PostScriptResult, String> {
    let sent = args.request.map(|r| SentRequest { url: r.url, method: r.method, headers: r.headers, body: r.body });
    let ctx = PostRequestCtx {
        response_body: &args.response_body,
        response_headers: &args.response_headers,
        status_code: args.status_code,
        duration_ms: args.duration_ms,
        variables: &args.variables,
        context: &args.context,
        request: sent.as_ref(),
        globals: args.globals.as_ref(),
    };
    run_post_request_script_with(&args.script, &ctx)
        .map(|r| PostScriptResult { context_updates: r.context_updates, tests: r.tests, variables: r.variables, globals: r.globals })
        .map_err(|e| e.to_string())
}

// ── Variable interpolation ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct InterpolateArgs {
    pub template: String,
    pub variables: HashMap<String, String>,
}

#[tauri::command]
pub fn interpolate_template(args: InterpolateArgs) -> Result<String, String> {
    interpolate(&args.template, &args.variables).map_err(|e| e.to_string())
}

// ── Collection runner ─────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct StepResult {
    pub request_name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    pub tests: Vec<TestResult>,
}

#[tauri::command]
pub fn run_collection_cmd(
    collection: Collection,
    stop_on_fail: bool,
    window: tauri::Window,
) -> Result<Vec<StepResult>, String> {
    let envs = storage::load_environments().map_err(|e| e.to_string())?;
    let env = envs.pick(None).map_err(|e| e.to_string())?;
    let results = runner::run_collection(&collection, env, stop_on_fail, &mut |step| {
        let sr = match &step.result {
            Ok(r) => StepResult {
                request_name: step.request_name.clone(),
                method: step.method.clone(),
                url: step.url.clone(),
                status: Some(r.status),
                duration_ms: Some(r.duration_ms),
                error: None,
                tests: step.tests.clone(),
            },
            Err(e) => StepResult {
                request_name: step.request_name.clone(),
                method: step.method.clone(),
                url: step.url.clone(),
                status: None,
                duration_ms: None,
                error: Some(e.to_string()),
                tests: step.tests.clone(),
            },
        };
        // Emit live progress event to the frontend
        let _ = window.emit("run-step", &sr);
    });

    Ok(results
        .into_iter()
        .map(|step| match step.result {
            Ok(r) => StepResult {
                request_name: step.request_name,
                method: step.method,
                url: step.url,
                status: Some(r.status),
                duration_ms: Some(r.duration_ms),
                error: None,
                tests: step.tests.clone(),
            },
            Err(e) => StepResult {
                request_name: step.request_name,
                method: step.method,
                url: step.url,
                status: None,
                duration_ms: None,
                error: Some(e.to_string()),
                tests: step.tests.clone(),
            },
        })
        .collect())
}

// ── The Lit web app (webapp/, built with `npm run build:tauri`) ───────────────

/// A request as the web app's HITOP proxy takes it, so the app sends the
/// same envelope on the web and here.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendArgs {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<String>,
    /// `base64` when `body` holds bytes (multipart with files).
    #[serde(default)]
    pub body_encoding: Option<String>,
    /// Use the cookie jar (hitop-core's, workspace `global`, shared with
    /// `hitop send`): send its cookies, keep the ones the response sets.
    #[serde(default)]
    pub cookies: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub status: u16,
    pub status_text: String,
    pub url: String,
    /// Every header, repeats (Set-Cookie) apart.
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// `base64` when the body isn't UTF-8.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_encoding: Option<String>,
    pub time_ms: u64,
    /// The jar after the response, when the request used it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookies: Option<Jar>,
}

#[tauri::command(async)]
pub fn send_request(args: SendArgs) -> Result<SendResult, String> {
    let body = match (args.body, args.body_encoding.as_deref()) {
        (None, _) => vec![],
        (Some(b), Some("base64")) => B64.decode(b).map_err(|e| format!("invalid base64 body: {e}"))?,
        (Some(b), _) => b.into_bytes(),
    };
    let mut jar = if args.cookies { Some(Jar::load(GLOBAL_WORKSPACE).map_err(|e| format!("{e:#}"))?) } else { None };
    let r = http::send_with(&args.method, &args.url, &args.headers, body, http::Options { timeout: None, jar: jar.as_mut() }).map_err(|e| format!("{e:#}"))?;
    if let Some(j) = &jar {
        j.save(GLOBAL_WORKSPACE).map_err(|e| format!("{e:#}"))?;
    }
    let (body, body_encoding) = match String::from_utf8(r.body) {
        Ok(text) => (text, None),
        Err(e) => (B64.encode(e.into_bytes()), Some("base64".to_string())),
    };
    Ok(SendResult { status: r.status, status_text: r.status_text, url: r.url, headers: r.headers, body, body_encoding, time_ms: r.duration_ms, cookies: jar })
}

/// The cookie jar the web app shows on its Cookies page (workspace `global`).
#[tauri::command(async)]
pub fn cookies_load() -> Result<Jar, String> {
    Jar::load(GLOBAL_WORKSPACE).map_err(|e| format!("{e:#}"))
}

#[tauri::command(async)]
pub fn cookies_save(cookies: Jar) -> Result<(), String> {
    cookies.save(GLOBAL_WORKSPACE).map_err(|e| format!("{e:#}"))
}

#[derive(Serialize)]
pub struct FolderResult {
    pub collection: Collection,
    pub environments: Vec<Environment>,
    /// `globals.yaml`, when the folder has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub globals: Option<Environment>,
}

/// Reads a collection folder (hitop.yaml, requests/, environments/).
#[tauri::command(async)]
pub fn read_folder(app: tauri::AppHandle, path: String) -> Result<FolderResult, String> {
    crate::bookmarks::access(&app, &path);
    let f = format::read_folder(std::path::Path::new(&path)).map_err(|e| format!("{e:#}"))?;
    Ok(FolderResult { collection: f.collection, environments: f.environments, globals: f.globals })
}

/// Writes a collection folder: changed files only, stale request and
/// environment files removed. Secret values are never written.
#[tauri::command(async)]
pub fn write_folder(app: tauri::AppHandle, path: String, collection: Collection, environments: Vec<Environment>, globals: Option<Environment>) -> Result<(), String> {
    crate::bookmarks::access(&app, &path);
    // Without `globals` an existing globals.yaml is left as it is.
    format::write_folder(std::path::Path::new(&path), &format::Folder { collection, environments, globals }).map_err(|e| format!("{e:#}"))
}

/// Secret values in the OS keychain (service `hitop`, account
/// `<workspace>/<env>/<name>`, as the CLI uses them).
#[tauri::command(async)]
pub fn secret_get(account: String) -> Result<Option<String>, String> {
    secrets::os_store().get(&account).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn secret_set(account: String, value: String) -> Result<(), String> {
    secrets::os_store().set(&account, &value).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn secret_delete(account: String) -> Result<bool, String> {
    secrets::os_store().delete(&account).map_err(|e| e.to_string())
}

/// `${secret:vault|aws|azure|op:...}` references, by inner text
/// (`secret:vault:kv/app#token`), resolved as the CLI resolves them (VAULT_*,
/// AWS_*, AZURE_* env vars, `~/.vault-token`, `~/.aws`, the `az` and `op`
/// CLIs). The page keeps the values in memory for one send or run, masks
/// them, and never saves them. Every failure is named in the error.
#[tauri::command(async)]
pub fn secret_refs_resolve(refs: Vec<String>) -> Result<HashMap<String, String>, String> {
    use hitop_core::{secret_managers, secret_refs};
    let refs: std::collections::BTreeSet<String> = refs.into_iter().filter(|r| secret_refs::is_reference(r)).collect();
    if refs.is_empty() {
        return Ok(HashMap::new());
    }
    if !secret_managers::enabled() {
        return Err("secret manager references are turned off (HITOP_SECRET_MANAGERS=off)".into());
    }
    let (values, errors) = secret_managers::Resolver::from_env().resolve_all(&refs);
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(values.into_iter().collect())
}

/// Writes an export to the path the save dialog gave (`data` is base64).
#[tauri::command(async)]
pub fn write_file(path: String, data: String) -> Result<(), String> {
    let bytes = B64.decode(data).map_err(|e| format!("invalid base64: {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("writing {path}: {e}"))
}

// ── Usage ping (feature `telemetry`; absent from Mac App Store builds) ────────

/// The desktop's usage ping setting and exactly what a ping sends; an error
/// in builds without the ping, so the web app shows no setting there.
#[tauri::command]
pub fn telemetry_status(app: tauri::AppHandle) -> Result<Value, String> {
    #[cfg(feature = "telemetry")]
    {
        use hitop_core::telemetry::{endpoint, os, Surface, Telemetry};
        let t = Telemetry::new(Surface::Desktop).map_err(|e| format!("{e:#}"))?;
        let version = app.package_info().version.to_string();
        let state = t.state();
        let ping = match t.payload(&version) {
            Some(p) => serde_json::to_value(p).map_err(|e| e.to_string())?,
            None => serde_json::json!({ "id": "<a random id, made when you turn it on>", "surface": "desktop", "version": version, "os": os() }),
        };
        Ok(serde_json::json!({ "enabled": state.enabled, "asked": state.asked, "endpoint": endpoint(), "ping": ping, "lastPing": state.last_ping }))
    }
    #[cfg(not(feature = "telemetry"))]
    {
        let _ = app;
        Err("this build has no usage ping".into())
    }
}

/// Turns the ping on or off (and records that the question was answered).
/// Turned on, the first ping goes out now.
#[tauri::command]
pub fn telemetry_set(app: tauri::AppHandle, enabled: bool) -> Result<Value, String> {
    #[cfg(feature = "telemetry")]
    {
        use hitop_core::telemetry::{Surface, Telemetry};
        let t = Telemetry::new(Surface::Desktop).map_err(|e| format!("{e:#}"))?;
        t.set_enabled(enabled).map_err(|e| format!("{e:#}"))?;
        if enabled {
            ping_in_background(app.package_info().version.to_string());
        }
        telemetry_status(app)
    }
    #[cfg(not(feature = "telemetry"))]
    {
        let _ = (app, enabled);
        Err("this build has no usage ping".into())
    }
}

/// Sends the ping off the main thread when it is on and a week has passed.
#[cfg(feature = "telemetry")]
pub fn ping_in_background(version: String) {
    use hitop_core::telemetry::{now, Surface, Telemetry};
    std::thread::spawn(move || {
        if let Ok(t) = Telemetry::new(Surface::Desktop) {
            let _ = t.maybe_ping(&version, now());
        }
    });
}

// ── WebSocket ───────────────────────────────────────────────────────────────
// The web app's WebSocket requests on the desktop: hitop-core's client
// (tokio-tungstenite over rustls), so handshake headers and auth reach the
// server, which a browser's WebSocket can't do. `ws_connect` opens a
// connection and streams its events to the page on a channel; `ws_send` and
// `ws_close` act on it by id. Variables are filled in by the page.

/// Open connections, by id.
#[derive(Default)]
pub struct Sockets {
    next: std::sync::atomic::AtomicU64,
    open: std::sync::Mutex<HashMap<u64, hitop_core::websocket::Sender>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsConnectArgs {
    pub url: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub protocols: Vec<String>,
    /// Time limit for the handshake (default 30 s).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsOpened {
    pub id: u64,
    pub status: u16,
    pub protocol: Option<String>,
    pub headers: Vec<(String, String)>,
}

/// What the page gets on the channel; `closed` and `error` are the last.
#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsEvent {
    /// `data` is the text, or base64 when `binary`.
    Message { data: String, binary: bool },
    Closed { code: Option<u16>, reason: String },
    Error { message: String },
}

fn ws_event(e: hitop_core::websocket::Event) -> WsEvent {
    use hitop_core::websocket::{Data, Event};
    match e {
        Event::Message(Data::Text(data)) => WsEvent::Message { data, binary: false },
        Event::Message(Data::Binary(b)) => WsEvent::Message { data: B64.encode(b), binary: true },
        Event::Closed { code, reason } => WsEvent::Closed { code, reason },
        Event::Error(message) => WsEvent::Error { message },
    }
}

#[tauri::command]
pub async fn ws_connect(
    app: tauri::AppHandle,
    state: tauri::State<'_, Sockets>,
    args: WsConnectArgs,
    on_event: tauri::ipc::Channel<WsEvent>,
) -> Result<WsOpened, String> {
    use hitop_core::websocket::{self, Target};
    use tauri::Manager;
    let target = Target { url: websocket::ws_url(&args.url).map_err(|e| format!("{e:#}"))?, headers: args.headers, subprotocols: args.protocols };
    let timeout = std::time::Duration::from_millis(args.timeout_ms.unwrap_or(30_000));
    let conn = websocket::connect(&target, Some(timeout)).await.map_err(|e| format!("{e:#}"))?;
    let id = state.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    state.open.lock().map_err(|e| e.to_string())?.insert(id, conn.sender);
    let mut events = conn.events;
    tauri::async_runtime::spawn(async move {
        while let Some(e) = events.recv().await {
            let last = !matches!(e, websocket::Event::Message(_));
            let _ = on_event.send(ws_event(e));
            if last {
                break;
            }
        }
        if let Ok(mut open) = app.state::<Sockets>().open.lock() {
            open.remove(&id);
        }
    });
    Ok(WsOpened { id, status: conn.opened.status, protocol: conn.opened.protocol, headers: conn.opened.headers })
}

/// Sends text, or bytes given as base64 when `binary`.
#[tauri::command]
pub fn ws_send(state: tauri::State<'_, Sockets>, id: u64, data: String, binary: Option<bool>) -> Result<(), String> {
    use hitop_core::websocket::Data;
    let open = state.open.lock().map_err(|e| e.to_string())?;
    let sender = open.get(&id).ok_or("the connection is closed")?;
    let data = if binary.unwrap_or(false) { Data::Binary(B64.decode(data).map_err(|e| format!("invalid base64: {e}"))?) } else { Data::Text(data) };
    sender.send(data).map_err(|e| format!("{e:#}"))
}

/// Starts the closing handshake; the channel then gets `closed`.
#[tauri::command]
pub fn ws_close(state: tauri::State<'_, Sockets>, id: u64) -> Result<(), String> {
    if let Some(sender) = state.open.lock().map_err(|e| e.to_string())?.get(&id) {
        sender.close();
    }
    Ok(())
}

// ── gRPC ────────────────────────────────────────────────────────────────────
// The web app's gRPC requests on the desktop (a browser can't make gRPC
// calls): hitop-core's client (tonic over rustls, dynamic messages). The
// schema comes from server reflection, or from .proto files (relative paths
// taken from `baseDir`, the collection's folder). `grpc_list` and
// `grpc_describe` read the schema; `grpc_call` streams the response headers,
// messages and final status to the page on a channel; `grpc_cancel` stops a
// call. Variables are filled in by the page.

/// Calls in flight, by id.
#[derive(Default)]
pub struct GrpcCalls {
    next: std::sync::atomic::AtomicU64,
    running: std::sync::Mutex<HashMap<u64, tauri::async_runtime::JoinHandle<()>>>,
}

/// Where the server is and where its schema comes from.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcServerArgs {
    pub address: String,
    #[serde(default)]
    pub plaintext: bool,
    /// Metadata, in order (header auth already applied by the page).
    #[serde(default)]
    pub metadata: Vec<(String, String)>,
    /// .proto files; none means server reflection.
    #[serde(default)]
    pub protos: Vec<String>,
    #[serde(default)]
    pub import_paths: Vec<String>,
    /// The collection's folder, for relative paths.
    #[serde(default)]
    pub base_dir: Option<String>,
    /// Time limit for connecting and reading the schema (default 10 s).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcCallArgs {
    #[serde(flatten)]
    pub server: GrpcServerArgs,
    pub service: String,
    pub method: String,
    /// The request message, JSON text.
    #[serde(default)]
    pub message: String,
    /// Deadline for the call (none by default).
    #[serde(default)]
    pub max_time_ms: Option<u64>,
}

fn grpc_target(a: &GrpcServerArgs) -> hitop_core::grpc::Target {
    let path = |p: &String| {
        let p = std::path::PathBuf::from(p);
        match &a.base_dir {
            Some(b) if p.is_relative() && !b.is_empty() => std::path::Path::new(b).join(p),
            _ => p,
        }
    };
    hitop_core::grpc::Target {
        address: a.address.clone(),
        plaintext: a.plaintext,
        metadata: a.metadata.clone(),
        protos: a.protos.iter().map(path).collect(),
        import_paths: a.import_paths.iter().map(path).collect(),
        ..Default::default()
    }
}

async fn grpc_open(
    t: &hitop_core::grpc::Target,
    timeout_ms: Option<u64>,
) -> Result<(hitop_core::grpc::Channel, hitop_core::grpc::DescriptorPool, hitop_core::grpc::Source), String> {
    use hitop_core::grpc;
    let timeout = Some(std::time::Duration::from_millis(timeout_ms.unwrap_or(10_000)));
    let channel = grpc::connect(t, timeout).await.map_err(|e| format!("{e:#}"))?;
    let (pool, source) = grpc::schema(t, &channel, timeout).await.map_err(|e| format!("{e:#}"))?;
    Ok((channel, pool, source))
}

fn grpc_source(s: &hitop_core::grpc::Source) -> &'static str {
    use hitop_core::grpc::{Reflection, Source};
    match s {
        Source::Protos => "protos",
        Source::Reflection(Reflection::V1) => "reflection v1",
        Source::Reflection(Reflection::V1Alpha) => "reflection v1alpha",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcListed {
    /// `protos`, `reflection v1` or `reflection v1alpha`.
    pub source: String,
    pub services: Vec<hitop_core::grpc::ServiceInfo>,
}

/// The services and methods of the schema.
#[tauri::command]
pub async fn grpc_list(args: GrpcServerArgs) -> Result<GrpcListed, String> {
    let (_, pool, source) = grpc_open(&grpc_target(&args), args.timeout_ms).await?;
    Ok(GrpcListed { source: grpc_source(&source).into(), services: hitop_core::grpc::services(&pool) })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrpcDescribed {
    pub description: String,
    /// A JSON message to start from: a message's own, a method's request.
    pub template: Option<Value>,
}

/// A service, method (`package.Service/Method`), message or enum, as in a .proto file.
#[tauri::command]
pub async fn grpc_describe(args: GrpcServerArgs, symbol: String) -> Result<GrpcDescribed, String> {
    use hitop_core::grpc;
    let (_, pool, _) = grpc_open(&grpc_target(&args), args.timeout_ms).await?;
    let description = grpc::describe(&pool, &symbol).map_err(|e| format!("{e:#}"))?;
    let template = pool
        .get_message_by_name(symbol.trim_start_matches('.'))
        .or_else(|| grpc::split_method(&symbol).and_then(|(s, m)| grpc::find_method(&pool, &s, &m).ok()).map(|m| m.input()))
        .map(|m| grpc::template(&m));
    Ok(GrpcDescribed { description, template })
}

/// What the page gets on the channel; `status` is the last.
#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GrpcEvent {
    Headers { metadata: Vec<(String, String)> },
    Message { data: Value },
    Status { code: i32, name: String, message: String, trailers: Vec<(String, String)> },
}

#[derive(Serialize)]
pub struct GrpcStarted {
    pub id: u64,
}

/// Starts a unary or server-streaming call. Problems found before sending
/// (no server, no such method, bad JSON) are errors; the rest arrives on
/// the channel.
#[tauri::command]
pub async fn grpc_call(
    app: tauri::AppHandle,
    state: tauri::State<'_, GrpcCalls>,
    args: GrpcCallArgs,
    on_event: tauri::ipc::Channel<GrpcEvent>,
) -> Result<GrpcStarted, String> {
    use hitop_core::grpc::{self, Event};
    use tauri::Manager;
    let mut target = grpc_target(&args.server);
    target.service = args.service;
    target.method = args.method;
    target.message = args.message;
    let (channel, pool, _) = grpc_open(&target, args.server.timeout_ms).await?;
    let max_time = args.max_time_ms.map(std::time::Duration::from_millis);
    let mut events = grpc::call(channel, &pool, &target, max_time).await.map_err(|e| format!("{e:#}"))?;
    let id = state.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let handle = tauri::async_runtime::spawn(async move {
        while let Some(e) = events.recv().await {
            let _ = on_event.send(match e {
                Event::Headers(metadata) => GrpcEvent::Headers { metadata },
                Event::Message(data) => GrpcEvent::Message { data },
                Event::Status(s) => GrpcEvent::Status { code: s.code, name: s.name, message: s.message, trailers: s.trailers },
            });
        }
        if let Ok(mut running) = app.state::<GrpcCalls>().running.lock() {
            running.remove(&id);
        }
    });
    state.running.lock().map_err(|e| e.to_string())?.insert(id, handle);
    Ok(GrpcStarted { id })
}

/// Stops a call; the page gets no more events for it.
#[tauri::command]
pub fn grpc_cancel(state: tauri::State<'_, GrpcCalls>, id: u64) -> Result<(), String> {
    if let Some(h) = state.running.lock().map_err(|e| e.to_string())?.remove(&id) {
        h.abort();
    }
    Ok(())
}
