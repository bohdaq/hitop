use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Emitter;

use hitop_core::{
    http,
    interpolation::interpolate,
    model::{Collection, HistoryItem},
    runner,
    scripting::{run_post_request_script, run_pre_request_script, PreRequestCtx},
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
}

#[derive(Serialize)]
pub struct PreScriptResult {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub context_updates: HashMap<String, Value>,
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
    };
    run_pre_request_script(&args.script, ctx)
        .map(|r| PreScriptResult {
            url: r.url,
            headers: r.headers,
            body: r.body,
            context_updates: r.context_updates,
        })
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
pub struct PostScriptArgs {
    pub script: String,
    pub response_body: String,
    pub response_headers: HashMap<String, String>,
    pub status_code: u16,
    pub variables: HashMap<String, String>,
    pub context: HashMap<String, Value>,
}

#[derive(Serialize)]
pub struct PostScriptResult {
    pub context_updates: HashMap<String, Value>,
}

#[tauri::command]
pub fn run_post_script(args: PostScriptArgs) -> Result<PostScriptResult, String> {
    run_post_request_script(
        &args.script,
        &args.response_body,
        &args.response_headers,
        args.status_code,
        &args.variables,
        &args.context,
    )
    .map(|r| PostScriptResult { context_updates: r.context_updates })
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
}

#[tauri::command]
pub fn run_collection_cmd(
    collection: Collection,
    stop_on_fail: bool,
    window: tauri::Window,
) -> Result<Vec<StepResult>, String> {
    let results = runner::run_collection(&collection, stop_on_fail, &mut |step| {
        let sr = match &step.result {
            Ok(r) => StepResult {
                request_name: step.request_name.clone(),
                method: step.method.clone(),
                url: step.url.clone(),
                status: Some(r.status),
                duration_ms: Some(r.duration_ms),
                error: None,
            },
            Err(e) => StepResult {
                request_name: step.request_name.clone(),
                method: step.method.clone(),
                url: step.url.clone(),
                status: None,
                duration_ms: None,
                error: Some(e.to_string()),
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
            },
            Err(e) => StepResult {
                request_name: step.request_name,
                method: step.method,
                url: step.url,
                status: None,
                duration_ms: None,
                error: Some(e.to_string()),
            },
        })
        .collect())
}
