//! hitop-core's pre/post request scripts for the Firefox extension (HITOP Next).
//!
//! Firefox has no sandboxed extension pages, and an extension page's CSP forbids `eval`, so the
//! Chrome build's sandboxed runner can't work there. This runs the scripts in boa (a JavaScript
//! interpreter in Rust) compiled to WebAssembly instead: the same engine, pm.* shim and results
//! as the desktop app and the CLI, and the script sees nothing of the page (no DOM, no extension
//! APIs, no storage) because it runs inside the interpreter, not in the browser's JS engine.
//!
//! The calls take and return JSON text in the desktop's `run_pre_script` / `run_post_script`
//! shapes (desktop/src-tauri/src/commands.rs), so the page maps them the same way.

use std::collections::HashMap;

use hitop_core::model::TestResult;
use hitop_core::scripting::{run_post_request_script_with, run_pre_request_script, PostRequestCtx, PreRequestCtx, SentRequest, VariableUpdates};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
pub struct PreScriptArgs {
    pub script: String,
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub variables: HashMap<String, String>,
    pub context: HashMap<String, Value>,
    #[serde(default)]
    pub globals: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct PreScriptResult {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub context_updates: HashMap<String, Value>,
    pub variables: VariableUpdates,
    pub globals: VariableUpdates,
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
    #[serde(default)]
    pub request: Option<SentRequestArgs>,
    #[serde(default)]
    pub globals: Option<HashMap<String, String>>,
}

#[derive(Serialize)]
pub struct PostScriptResult {
    pub context_updates: HashMap<String, Value>,
    pub tests: Vec<TestResult>,
    pub variables: VariableUpdates,
    pub globals: VariableUpdates,
}

/// The pre-request script: `args` and the answer are JSON (see the module docs).
pub fn pre(args: &str) -> Result<String, String> {
    let a: PreScriptArgs = serde_json::from_str(args).map_err(|e| format!("bad arguments: {e}"))?;
    let ctx = PreRequestCtx { url: a.url, method: a.method, headers: a.headers, body: a.body, variables: a.variables, context: a.context, globals: a.globals };
    let r = run_pre_request_script(&a.script, ctx).map_err(|e| e.to_string())?;
    let out = PreScriptResult { url: r.url, headers: r.headers, body: r.body, context_updates: r.context_updates, variables: r.variables, globals: r.globals };
    serde_json::to_string(&out).map_err(|e| e.to_string())
}

/// The post-request script: `args` and the answer are JSON (see the module docs).
pub fn post(args: &str) -> Result<String, String> {
    let a: PostScriptArgs = serde_json::from_str(args).map_err(|e| format!("bad arguments: {e}"))?;
    let sent = a.request.map(|r| SentRequest { url: r.url, method: r.method, headers: r.headers, body: r.body });
    let ctx = PostRequestCtx {
        response_body: &a.response_body,
        response_headers: &a.response_headers,
        status_code: a.status_code,
        duration_ms: a.duration_ms,
        variables: &a.variables,
        context: &a.context,
        request: sent.as_ref(),
        globals: a.globals.as_ref(),
    };
    let r = run_post_request_script_with(&a.script, &ctx).map_err(|e| e.to_string())?;
    let out = PostScriptResult { context_updates: r.context_updates, tests: r.tests, variables: r.variables, globals: r.globals };
    serde_json::to_string(&out).map_err(|e| e.to_string())
}

#[wasm_bindgen(js_name = runPreScript)]
pub fn run_pre_script(args: &str) -> Result<String, JsError> {
    pre(args).map_err(|e| JsError::new(&e))
}

#[wasm_bindgen(js_name = runPostScript)]
pub fn run_post_script(args: &str) -> Result<String, JsError> {
    post(args).map_err(|e| JsError::new(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_sets_headers_and_variables_through_the_pm_shim() {
        let args = serde_json::json!({
            "script": "pm.request.headers.add({key: 'X-Id', value: pm.environment.get('id')}); pm.environment.set('seen', 'yes');",
            "url": "https://api.example/x", "method": "GET", "headers": [], "body": "",
            "variables": {"id": "42"}, "context": {}
        });
        let out: Value = serde_json::from_str(&pre(&args.to_string()).unwrap()).unwrap();
        assert_eq!(out["headers"], serde_json::json!([["X-Id", "42"]]), "{out}");
        assert_eq!(out["variables"]["seen"], "yes");
    }

    #[test]
    fn post_runs_tests_and_reports_failures() {
        let args = serde_json::json!({
            "script": "pm.test('status 200', () => pm.response.to.have.status(200)); pm.test('has id', () => pm.expect(pm.response.json().id).to.eql(7));",
            "response_body": "{\"id\": 8}", "response_headers": {"content-type": "application/json"}, "status_code": 200,
            "variables": {}, "context": {}
        });
        let out: Value = serde_json::from_str(&post(&args.to_string()).unwrap()).unwrap();
        let tests = out["tests"].as_array().unwrap();
        assert_eq!(tests.len(), 2, "{out}");
        assert_eq!(tests[0]["pass"], true);
        assert_eq!(tests[1]["pass"], false);
    }

    #[test]
    fn a_script_error_is_an_error() {
        let args = serde_json::json!({"script": "throw new Error('nope')", "url": "u", "method": "GET", "headers": [], "body": "", "variables": {}, "context": {}});
        assert!(pre(&args.to_string()).unwrap_err().contains("nope"));
    }
}
