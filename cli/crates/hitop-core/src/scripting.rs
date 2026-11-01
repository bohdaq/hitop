#![cfg(feature = "native")]
use std::collections::HashMap;
use anyhow::{anyhow, Result};
use boa_engine::{Context, Source};
use serde_json::Value;

/// Inputs the pre-request script can mutate.
#[derive(Debug, Clone, Default)]
pub struct PreRequestCtx {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// Every scope merged (globals lowest).
    pub variables: HashMap<String, String>,
    pub context: HashMap<String, Value>,
    /// The globals scope alone, for `pm.globals`.
    pub globals: HashMap<String, String>,
}

/// Outputs after running a pre-request script.
#[derive(Debug, Clone)]
pub struct PreRequestResult {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub context_updates: HashMap<String, Value>,
    /// Variables the script set (`pm.environment.set`, ...) for the rest of
    /// the run; `None` unsets one.
    pub variables: VariableUpdates,
    /// Globals the script set (`pm.globals.set`, ...), as above.
    pub globals: VariableUpdates,
}

/// Outputs after running a post-request script.
#[derive(Debug, Clone)]
pub struct PostRequestResult {
    pub context_updates: HashMap<String, Value>,
    /// One entry per `assert.*` call (and `pm.test`, `tests[...]`), in order.
    pub tests: Vec<crate::model::TestResult>,
    /// Variables the script set, as in [`PreRequestResult::variables`].
    pub variables: VariableUpdates,
    /// Globals the script set.
    pub globals: VariableUpdates,
}

/// Variables set (`Some`) or unset (`None`) by a script.
pub type VariableUpdates = HashMap<String, Option<String>>;

/// The request a post-request script sees as `pm.request`.
#[derive(Debug, Clone, Default)]
pub struct SentRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

/// Everything a post-request script sees.
#[derive(Debug, Clone, Copy)]
pub struct PostRequestCtx<'a> {
    pub response_body: &'a str,
    pub response_headers: &'a HashMap<String, String>,
    pub status_code: u16,
    pub duration_ms: u64,
    pub variables: &'a HashMap<String, String>,
    pub context: &'a HashMap<String, Value>,
    pub request: Option<&'a SentRequest>,
    /// The globals scope alone, for `pm.globals` (`None`: pm.globals shares
    /// the merged variables).
    pub globals: Option<&'a HashMap<String, String>>,
}

/// The Postman `pm.*` shim (`shared/pm-shim.js`; this crate keeps a copy so
/// it is packaged, and a test checks the two are the same).
pub const PM_SHIM: &str = include_str!("../pm-shim.js");

/// True when a script may use Postman's API, so the shim is loaded. Scripts
/// that don't run exactly as before (no shim, no wrapper function).
fn uses_postman_api(script: &str) -> bool {
    static WORDS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    WORDS
        .get_or_init(|| regex::Regex::new(r"\b(pm|postman|tests|responseBody|responseCode|responseTime|environment|globals)\b").unwrap())
        .is_match(script)
}

/// JS that loads the shim as `__p` (given the host `api` object literal)
/// and runs `script` in a function that receives `pm` and the legacy names.
fn with_shim(api: &str, script: &str) -> String {
    format!(
        "{PM_SHIM}\nconst __p = __hitopPm({api});\n(function (pm, tests, postman, environment, globals, responseBody, responseCode, responseTime) {{\n{script}\n}}).call(undefined, __p.pm, __p.tests, __p.postman, __p.environment, __p.globals, __p.responseBody, __p.responseCode, __p.responseTime);\n__p.finish();\n"
    )
}

const SET_VAR_JS: &str = "function (k, v) { __varUpdates[k] = v; if (v === null) delete __vars[k]; else __vars[k] = v; }";
const SET_GLOBAL_JS: &str = "function (k, v) { __globalUpdates[k] = v; }";

fn variable_updates(v: &Value) -> VariableUpdates {
    v.as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_string))).collect())
        .unwrap_or_default()
}

/// A script line that calls a `pm.*` (or `postman.*`, `require`, ...) API
/// the shim doesn't provide.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnsupportedCall {
    /// 1-based line in the script.
    pub line: usize,
    /// The call, as `pm.sendRequest`.
    pub call: String,
    /// The line's text, trimmed.
    pub text: String,
}

/// Every line of `script` that uses a Postman call the shim doesn't
/// provide (the same check the web app runs: it lives in the shim).
pub fn unsupported_calls(script: &str) -> Vec<UnsupportedCall> {
    if script.trim().is_empty() {
        return vec![];
    }
    let mut engine = Context::default();
    let Ok(source) = serde_json::to_string(script) else { return vec![] };
    let js = format!("{PM_SHIM}\nJSON.stringify(__hitopPmUnsupported({source}))");
    engine
        .eval(Source::from_bytes(js.as_bytes()))
        .ok()
        .and_then(|v| v.as_string().map(|s| s.to_std_string().unwrap_or_default()))
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn run_pre_request_script(script: &str, ctx: PreRequestCtx) -> Result<PreRequestResult> {
    if script.trim().is_empty() {
        return Ok(PreRequestResult {
            url: ctx.url,
            headers: ctx.headers,
            body: ctx.body,
            context_updates: HashMap::new(),
            variables: HashMap::new(),
            globals: HashMap::new(),
        });
    }

    let mut engine = Context::default();

    let variables_json = serde_json::to_string(&ctx.variables)?;
    let globals_json = serde_json::to_string(&ctx.globals)?;
    let context_json = serde_json::to_string(&ctx.context)?;

    // Build the wrapper script that exposes the API surface
    let preamble = format!(
        r#"
const __vars = {};
const __ctx  = {};
const __ctxUpdates = {{}};
const __varUpdates = {{}};
const __globals = {};
const __globalUpdates = {{}};
let __url    = {};
let __body   = {};
let __headers = {};
const __method = {};

function getVariable(k) {{ return __vars[k]; }}
function getContext(k)  {{ return __ctxUpdates[k] !== undefined ? __ctxUpdates[k] : __ctx[k]; }}
function setContext(k, v) {{ __ctxUpdates[k] = v; }}
function setUrl(u)  {{ __url = u; }}
function setBody(b) {{ __body = b; }}
function setHeader(name, value) {{
    let found = false;
    for (let h of __headers) {{ if (h[0] === name) {{ h[1] = value; found = true; break; }} }}
    if (!found) __headers.push([name, value]);
}}
function __removeHeader(name) {{
    const n = String(name).toLowerCase();
    __headers = __headers.filter(h => String(h[0]).toLowerCase() !== n);
}}
"#,
        variables_json,
        context_json,
        globals_json,
        serde_json::to_string(&ctx.url)?,
        serde_json::to_string(&ctx.body)?,
        // The script starts from the request's headers, as in the web app.
        serde_json::to_string(&ctx.headers)?,
        serde_json::to_string(&ctx.method)?,
    );

    let postamble = r#"
JSON.stringify({ url: __url, body: __body, headers: __headers, ctx: __ctxUpdates, vars: __varUpdates, globals: __globalUpdates })
"#;

    let body = if uses_postman_api(script) {
        let api = format!(
            "{{ vars: __vars, setVar: {SET_VAR_JS}, globals: __globals, setGlobal: {SET_GLOBAL_JS}, assert: {{ ok: function () {{}} }}, \
             request: {{ get url() {{ return __url; }}, method: __method, get headers() {{ return __headers; }}, get body() {{ return __body; }} }}, \
             setUrl: setUrl, setHeader: setHeader, removeHeader: __removeHeader, setBody: setBody, response: null }}"
        );
        with_shim(&api, script)
    } else {
        script.to_string()
    };
    let full_script = format!("{}\n{}\n{}", preamble, body, postamble);

    let result = engine
        .eval(Source::from_bytes(full_script.as_bytes()))
        .map_err(|e| anyhow!("pre-request script error: {}", e))?;

    let json_str = result
        .as_string()
        .map(|s| s.to_std_string().unwrap_or_default())
        .unwrap_or_default();

    let parsed: Value = serde_json::from_str(&json_str)
        .map_err(|_| anyhow!("pre-request script returned invalid JSON"))?;

    let new_url = parsed["url"].as_str().unwrap_or(&ctx.url).to_string();
    let new_body = parsed["body"].as_str().unwrap_or(&ctx.body).to_string();

    let new_headers: Vec<(String, String)> = parsed["headers"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|h| {
                    let pair = h.as_array()?;
                    Some((
                        pair.first()?.as_str()?.to_string(),
                        pair.get(1)?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_else(|| ctx.headers.clone());

    let ctx_updates: HashMap<String, Value> = parsed["ctx"]
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    Ok(PreRequestResult {
        url: new_url,
        headers: new_headers,
        body: new_body,
        context_updates: ctx_updates,
        variables: variable_updates(&parsed["vars"]),
        globals: variable_updates(&parsed["globals"]),
    })
}

pub fn run_post_request_script(
    script: &str,
    response_body: &str,
    response_headers: &HashMap<String, String>,
    status_code: u16,
    duration_ms: u64,
    variables: &HashMap<String, String>,
    context: &HashMap<String, Value>,
) -> Result<PostRequestResult> {
    run_post_request_script_with(
        script,
        &PostRequestCtx { response_body, response_headers, status_code, duration_ms, variables, context, request: None, globals: None },
    )
}

/// [`run_post_request_script`] that also gives the script the request as
/// sent (`pm.request`).
pub fn run_post_request_script_with(script: &str, ctx: &PostRequestCtx) -> Result<PostRequestResult> {
    let PostRequestCtx { response_body, response_headers, status_code, duration_ms, variables, context, request, globals } = *ctx;
    if script.trim().is_empty() {
        return Ok(PostRequestResult {
            context_updates: HashMap::new(),
            tests: vec![],
            variables: HashMap::new(),
            globals: HashMap::new(),
        });
    }

    let mut engine = Context::default();

    let parsed_response: Value = serde_json::from_str(response_body).unwrap_or(Value::Null);
    let response_json = serde_json::to_string(&parsed_response)?;
    let response_text_json = serde_json::to_string(response_body)?;
    let headers_json = serde_json::to_string(response_headers)?;
    let variables_json = serde_json::to_string(variables)?;
    let context_json = serde_json::to_string(context)?;

    let preamble = format!(
        r#"
const response        = {};
const responseText    = {};
const responseHeaders = {};
const statusCode      = {};
const __vars = {};
const __ctx  = {};
const __ctxUpdates = {{}};
const __varUpdates = {{}};
const __globals = {};
const __globalUpdates = {{}};

function getVariable(k) {{ return __vars[k]; }}
function getContext(k)  {{ return __ctxUpdates[k] !== undefined ? __ctxUpdates[k] : __ctx[k]; }}
function setContext(k, v) {{ __ctxUpdates[k] = v; }}
function getResponseValue(path) {{
    const keys = path.split('.');
    let val = response;
    for (const k of keys) {{ if (val && typeof val === 'object') val = val[k]; else return undefined; }}
    return val;
}}
function getResponseHeader(name) {{ return responseHeaders[name.toLowerCase()]; }}
const __tests = [];
function __record(name, pass, detail) {{ __tests.push({{ name: String(name), pass: !!pass, detail: pass ? "" : String(detail) }}); }}
const assert = {{
    status(e) {{ __record("status is " + e, statusCode === Number(e), "got " + statusCode); }},
    bodyContains(f) {{ __record("body contains \"" + f + "\"", String(responseText).includes(String(f)), "not found in the response body"); }},
    header(n, e) {{
        const a = responseHeaders[String(n).toLowerCase()];
        if (e === undefined) __record("has header " + n, a !== undefined, "header missing");
        else __record("header " + n + " is \"" + e + "\"", a !== undefined && a.includes(String(e)), a === undefined ? "header missing" : "got \"" + a + "\"");
    }},
    responseTime(ms) {{ __record("responds within " + ms + " ms", {duration} <= Number(ms), "took {duration} ms"); }},
    equal(a, e, name) {{ __record(name === undefined ? JSON.stringify(a) + " equals " + JSON.stringify(e) : name, JSON.stringify(a) === JSON.stringify(e), "got " + JSON.stringify(a)); }},
    ok(c, name, detail) {{ __record(name === undefined ? "condition is true" : name, c, detail === undefined || detail === "" ? "was false" : detail); }},
}};
"#,
        response_json,
        response_text_json,
        headers_json,
        status_code,
        variables_json,
        context_json,
        globals.map(serde_json::to_string).transpose()?.unwrap_or_else(|| "null".into()),
        duration = duration_ms,
    );

    let postamble = "JSON.stringify({ ctx: __ctxUpdates, tests: __tests, vars: __varUpdates, globals: __globalUpdates })";
    let body = if uses_postman_api(script) {
        let sent = request.cloned().unwrap_or_default();
        let api = format!(
            "{{ vars: __vars, setVar: {SET_VAR_JS}, globals: __globals || undefined, setGlobal: {SET_GLOBAL_JS}, assert: assert, \
             request: {{ url: {}, method: {}, headers: {}, body: {} }}, \
             response: {{ code: statusCode, text: responseText, headers: responseHeaders, time: {duration_ms} }} }}",
            serde_json::to_string(&sent.url)?,
            serde_json::to_string(&sent.method)?,
            serde_json::to_string(&sent.headers)?,
            serde_json::to_string(&sent.body)?,
        );
        with_shim(&api, script)
    } else {
        script.to_string()
    };
    let full_script = format!("{}\n{}\n{}", preamble, body, postamble);

    let result = engine
        .eval(Source::from_bytes(full_script.as_bytes()))
        .map_err(|e| anyhow!("post-request script error: {}", e))?;

    let json_str = result
        .as_string()
        .map(|s| s.to_std_string().unwrap_or_default())
        .unwrap_or_default();

    let out: Value = serde_json::from_str(&json_str).unwrap_or_default();
    let ctx_updates: HashMap<String, Value> = serde_json::from_value(out["ctx"].clone()).unwrap_or_default();
    let tests = serde_json::from_value(out["tests"].clone()).unwrap_or_default();

    Ok(PostRequestResult {
        context_updates: ctx_updates,
        tests,
        variables: variable_updates(&out["vars"]),
        globals: variable_updates(&out["globals"]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_pre_ctx(url: &str) -> PreRequestCtx {
        PreRequestCtx {
            url: url.to_string(),
            method: "GET".to_string(),
            headers: vec![],
            body: String::new(),
            variables: HashMap::new(),
            context: HashMap::new(),
            globals: HashMap::new(),
        }
    }

    // --- pre-request ---

    #[test]
    fn empty_pre_script_returns_inputs_unchanged() {
        let ctx = empty_pre_ctx("https://example.com");
        let result = run_pre_request_script("", ctx).unwrap();
        assert_eq!(result.url, "https://example.com");
        assert!(result.headers.is_empty());
        assert!(result.body.is_empty());
        assert!(result.context_updates.is_empty());
    }

    #[test]
    fn whitespace_only_pre_script_treated_as_empty() {
        let ctx = empty_pre_ctx("https://example.com");
        let result = run_pre_request_script("   \n  ", ctx).unwrap();
        assert_eq!(result.url, "https://example.com");
    }

    #[test]
    fn set_url_changes_url() {
        let ctx = empty_pre_ctx("https://original.com");
        let result = run_pre_request_script("setUrl('https://new.com/path')", ctx).unwrap();
        assert_eq!(result.url, "https://new.com/path");
    }

    #[test]
    fn set_body_changes_body() {
        let ctx = empty_pre_ctx("https://example.com");
        let result = run_pre_request_script(r#"setBody('{"key":"value"}')"#, ctx).unwrap();
        assert_eq!(result.body, r#"{"key":"value"}"#);
    }

    #[test]
    fn set_header_adds_new_header() {
        let ctx = empty_pre_ctx("https://example.com");
        let result = run_pre_request_script("setHeader('Authorization', 'Bearer token123')", ctx).unwrap();
        assert!(result.headers.iter().any(|(k, v)| k == "Authorization" && v == "Bearer token123"));
    }

    #[test]
    fn set_header_updates_existing_header() {
        let mut ctx = empty_pre_ctx("https://example.com");
        ctx.headers = vec![("Authorization".to_string(), "Bearer old".to_string())];
        let result = run_pre_request_script("setHeader('Authorization', 'Bearer new')", ctx).unwrap();
        let auth_values: Vec<_> = result.headers.iter()
            .filter(|(k, _)| k == "Authorization")
            .collect();
        assert_eq!(auth_values.len(), 1);
        assert_eq!(auth_values[0].1, "Bearer new");
    }

    #[test]
    fn set_context_stores_value() {
        let ctx = empty_pre_ctx("https://example.com");
        let result = run_pre_request_script("setContext('userId', 42)", ctx).unwrap();
        assert_eq!(result.context_updates["userId"], serde_json::json!(42));
    }

    #[test]
    fn get_variable_reads_collection_variable() {
        let mut ctx = empty_pre_ctx("https://example.com");
        ctx.variables.insert("host".to_string(), "https://api.example.com".to_string());
        let result = run_pre_request_script(
            "const h = getVariable('host'); setUrl(h + '/users')",
            ctx,
        ).unwrap();
        assert_eq!(result.url, "https://api.example.com/users");
    }

    #[test]
    fn get_context_reads_existing_context() {
        let mut ctx = empty_pre_ctx("https://example.com");
        ctx.context.insert("id".to_string(), serde_json::json!(7));
        let result = run_pre_request_script(
            "const id = getContext('id'); setUrl('https://example.com/todos/' + id)",
            ctx,
        ).unwrap();
        assert_eq!(result.url, "https://example.com/todos/7");
    }

    #[test]
    fn get_context_prefers_updates_over_existing() {
        let mut ctx = empty_pre_ctx("https://example.com");
        ctx.context.insert("val".to_string(), serde_json::json!("old"));
        let result = run_pre_request_script(
            "setContext('val', 'new'); const v = getContext('val'); setBody(v)",
            ctx,
        ).unwrap();
        assert_eq!(result.body, "new");
    }

    #[test]
    fn pre_script_syntax_error_returns_error() {
        let ctx = empty_pre_ctx("https://example.com");
        let err = run_pre_request_script("this is not valid JS @@@", ctx).unwrap_err();
        assert!(err.to_string().contains("pre-request script error"));
    }

    // --- post-request ---

    #[test]
    fn assertions_are_recorded_not_thrown() {
        let headers: HashMap<String, String> = [("content-type".to_string(), "application/json".to_string())].into();
        let result = run_post_request_script(
            "assert.status(200); assert.bodyContains('id'); assert.header('Content-Type', 'json'); assert.responseTime(10); assert.equal(response.id, 2, 'id is 2'); assert.ok(true)",
            r#"{"id":1}"#, &headers, 200, 50, &HashMap::new(), &HashMap::new(),
        ).unwrap();
        let summary: Vec<(String, bool)> = result.tests.iter().map(|t| (t.name.clone(), t.pass)).collect();
        assert_eq!(summary, vec![
            ("status is 200".to_string(), true),
            ("body contains \"id\"".to_string(), true),
            ("header Content-Type is \"json\"".to_string(), true),
            ("responds within 10 ms".to_string(), false),
            ("id is 2".to_string(), false),
            ("condition is true".to_string(), true),
        ]);
        assert_eq!(result.tests[3].detail, "took 50 ms");
        assert_eq!(result.tests[4].detail, "got 1");
    }

    #[test]
    fn empty_post_script_returns_no_updates() {
        let result = run_post_request_script(
            "", r#"{"id":1}"#, &HashMap::new(), 200, 50, &HashMap::new(), &HashMap::new(),
        ).unwrap();
        assert!(result.context_updates.is_empty());
    }

    #[test]
    fn set_context_in_post_script() {
        let result = run_post_request_script(
            "setContext('done', true)",
            "{}",
            &HashMap::new(),
            200, 50,
            &HashMap::new(),
            &HashMap::new(),
        ).unwrap();
        assert_eq!(result.context_updates["done"], serde_json::json!(true));
    }

    #[test]
    fn response_json_accessible_in_post_script() {
        let result = run_post_request_script(
            "setContext('userId', response.userId)",
            r#"{"userId": 5, "title": "test"}"#,
            &HashMap::new(),
            200, 50,
            &HashMap::new(),
            &HashMap::new(),
        ).unwrap();
        assert_eq!(result.context_updates["userId"], serde_json::json!(5));
    }

    #[test]
    fn status_code_accessible_in_post_script() {
        let result = run_post_request_script(
            "setContext('status', statusCode)",
            "{}",
            &HashMap::new(),
            201, 50,
            &HashMap::new(),
            &HashMap::new(),
        ).unwrap();
        assert_eq!(result.context_updates["status"], serde_json::json!(201));
    }

    #[test]
    fn get_response_value_dot_notation() {
        let result = run_post_request_script(
            "setContext('name', getResponseValue('user.name'))",
            r#"{"user":{"name":"Alice"}}"#,
            &HashMap::new(),
            200, 50,
            &HashMap::new(),
            &HashMap::new(),
        ).unwrap();
        assert_eq!(result.context_updates["name"], serde_json::json!("Alice"));
    }

    #[test]
    fn get_response_header_in_post_script() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        let result = run_post_request_script(
            "setContext('ct', getResponseHeader('Content-Type'))",
            "{}",
            &headers,
            200, 50,
            &HashMap::new(),
            &HashMap::new(),
        ).unwrap();
        assert_eq!(result.context_updates["ct"], serde_json::json!("application/json"));
    }

    #[test]
    fn post_script_can_read_collection_variable() {
        let mut vars = HashMap::new();
        vars.insert("env".to_string(), "production".to_string());
        let result = run_post_request_script(
            "setContext('env', getVariable('env'))",
            "{}",
            &HashMap::new(),
            200, 50,
            &vars,
            &HashMap::new(),
        ).unwrap();
        assert_eq!(result.context_updates["env"], serde_json::json!("production"));
    }

    #[test]
    fn post_script_non_json_response_sets_null() {
        let result = run_post_request_script(
            "setContext('isNull', response === null)",
            "not json at all",
            &HashMap::new(),
            200, 50,
            &HashMap::new(),
            &HashMap::new(),
        ).unwrap();
        assert_eq!(result.context_updates["isNull"], serde_json::json!(true));
    }

    #[test]
    fn post_script_syntax_error_returns_error() {
        let err = run_post_request_script(
            "!!!invalid", "{}", &HashMap::new(), 200, 50, &HashMap::new(), &HashMap::new(),
        ).unwrap_err();
        assert!(err.to_string().contains("post-request script error"));
    }

    // --- pm.* shim ---

    fn post(script: &str, body: &str, status: u16) -> PostRequestResult {
        let headers: HashMap<String, String> = [("content-type".to_string(), "application/json; charset=utf-8".to_string())].into();
        let vars: HashMap<String, String> = [("base".to_string(), "http://x".to_string())].into();
        let sent = SentRequest { url: "http://x/get".into(), method: "GET".into(), headers: vec![("Accept".into(), "*/*".into())], body: String::new() };
        run_post_request_script_with(
            script,
            &PostRequestCtx { response_body: body, response_headers: &headers, status_code: status, duration_ms: 12, variables: &vars, context: &HashMap::new(), request: Some(&sent), globals: None },
        )
        .unwrap()
    }

    #[test]
    fn globals_are_their_own_scope() {
        let ctx = PreRequestCtx {
            url: "http://x".into(),
            method: "GET".into(),
            variables: [("k".to_string(), "env".to_string()), ("g".to_string(), "G".to_string())].into(),
            globals: [("k".to_string(), "global".to_string()), ("g".to_string(), "G".to_string())].into(),
            ..Default::default()
        };
        let script = "setHeader('a', pm.globals.get('k') + '/' + pm.environment.get('k') + '/' + pm.variables.get('g')); \
                      pm.globals.set('k', 'g2'); pm.globals.set('n', 'new'); postman.clearGlobalVariable('g'); \
                      setHeader('b', pm.variables.get('k') + '/' + pm.variables.get('n') + '/' + pm.variables.get('g') + '/' + pm.globals.get('k') + '/' + globals.k);";
        let r = run_pre_request_script(script, ctx).unwrap();
        let h: HashMap<_, _> = r.headers.into_iter().collect();
        assert_eq!(h["a"], "global/env/G");
        // The environment still hides k; n and the unset g come from the
        // globals; the legacy `globals` object is the scope as the script began.
        assert_eq!(h["b"], "env/new/undefined/g2/global");
        assert!(r.variables.is_empty());
        assert_eq!(r.globals, [("k".to_string(), Some("g2".to_string())), ("n".to_string(), Some("new".to_string())), ("g".to_string(), None)].into());
        // Without a globals scope (old hosts) pm.globals is the shared set.
        let post = post("pm.globals.set('z', 1); pm.test('t', () => pm.expect(pm.variables.get('base')).to.eql(pm.globals.get('base')));", "{}", 200);
        assert_eq!(post.variables["z"].as_deref(), Some("1"));
        assert!(post.tests[0].pass);
    }

    fn summary(r: &PostRequestResult) -> Vec<(String, bool, String)> {
        r.tests.iter().map(|t| (t.name.clone(), t.pass, t.detail.clone())).collect()
    }

    #[test]
    fn shared_shim_is_the_packaged_copy() {
        let shared = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../shared/pm-shim.js");
        if let Ok(text) = std::fs::read_to_string(&shared) {
            assert!(text == PM_SHIM, "cli/crates/hitop-core/pm-shim.js differs from shared/pm-shim.js: copy it over");
        }
    }

    #[test]
    fn pm_test_and_chai_expect() {
        let r = post(
            r#"pm.test('expect response be 200', function () {
    pm.response.to.be.ok
})
pm.test('expect response json contain args', function () {
    pm.expect(pm.response.json().args).to.have.property('source')
      .and.equal('newman')
})
pm.test('fails with a message', function () { pm.expect(pm.response.code).to.equal(201); });
pm.test('not and deep', function () {
    pm.expect([1, 2]).to.be.an('array').that.is.not.empty;
    pm.expect({a: {b: 1}}).to.deep.equal({a: {b: 1}});
    pm.expect({a: 1}).to.eql({a: 1});
    pm.expect('hello').to.include('ell');
    pm.expect(5).to.be.above(3).and.below(10);
    pm.expect(null).to.be.null;
    pm.expect(pm.response.headers.get('Content-Type')).to.include('json');
    pm.response.to.have.status(200);
    pm.response.to.have.header('content-type');
    pm.response.to.not.be.error;
    pm.expect(pm.response.responseTime).to.be.below(1000);
    pm.expect(pm.request.url.toString()).to.equal('http://x/get');
    pm.expect(pm.request.headers.get('accept')).to.equal('*/*');
});
pm.test('a thrown error', function () { undefined.x; });
pm.expect(1).to.equal(2);"#,
            r#"{"args":{"source":"newman"}}"#,
            200,
        );
        let s = summary(&r);
        assert_eq!(s[0], ("expect response be 200".into(), true, "".into()));
        assert_eq!(s[1], ("expect response json contain args".into(), true, "".into()));
        assert_eq!(s[2], ("fails with a message".into(), false, "expected 200 to equal 201".into()));
        assert!(s[3].1, "{:?}", s[3]);
        assert!(!s[4].1 && s[4].2.starts_with("TypeError"), "{:?}", s[4]);
        assert_eq!(s[5], ("expected 1 to equal 2".into(), false, "expected 1 to equal 2".into()));
        assert_eq!(s.len(), 6);
    }

    #[test]
    fn pm_variables_are_returned_as_updates() {
        let r = post(
            "pm.environment.set('token', pm.response.json().token); pm.collectionVariables.set('n', 3); pm.globals.unset('base'); \
             pm.test('reads back', () => { pm.expect(pm.variables.get('token')).to.equal('t1'); pm.expect(pm.environment.has('base')).to.be.false; });",
            r#"{"token":"t1"}"#,
            200,
        );
        assert_eq!(r.variables["token"].as_deref(), Some("t1"));
        assert_eq!(r.variables["n"].as_deref(), Some("3"));
        assert_eq!(r.variables["base"], None);
        assert!(r.tests[0].pass, "{:?}", r.tests);
    }

    #[test]
    fn legacy_tests_object_and_globals() {
        let r = post(
            "var j = JSON.parse(responseBody); tests['has user'] = j.hasOwnProperty('user'); tests['code'] = responseCode.code === 201; \
             if (!(environment.isIntegrationTest)) {} postman.setEnvironmentVariable('slug', j.user);",
            r#"{"user":"u"}"#,
            200,
        );
        assert_eq!(summary(&r), vec![("has user".into(), true, "".into()), ("code".into(), false, "was false".into())]);
        assert_eq!(r.variables["slug"].as_deref(), Some("u"));
    }

    #[test]
    fn pre_request_pm_request_edits() {
        let mut ctx = empty_pre_ctx("http://x/a");
        ctx.headers = vec![("X-Old".into(), "1".into())];
        let r = run_pre_request_script(
            "pm.request.headers.add({key: 'X-New', value: 'v'}); pm.request.headers.remove('x-old'); pm.request.url = pm.request.url.toString() + '?q=1'; \
             pm.request.body.update('{\"a\":1}'); pm.variables.set('ts', 7);",
            ctx,
        )
        .unwrap();
        assert_eq!(r.url, "http://x/a?q=1");
        assert_eq!(r.headers, vec![("X-New".to_string(), "v".to_string())]);
        assert_eq!(r.body, r#"{"a":1}"#);
        assert_eq!(r.variables["ts"].as_deref(), Some("7"));
    }

    #[test]
    fn hitop_scripts_are_untouched_by_the_shim() {
        let r = post("const pmx = 1; assert.status(200); setContext('a', pmx)", "{}", 200);
        assert!(r.variables.is_empty());
        assert_eq!(r.tests.len(), 1);
    }

    #[test]
    fn unsupported_calls_are_listed_by_line() {
        let script = "pm.test('x', () => {});\n// pm.sendRequest in a comment\npm.sendRequest('http://x', () => {}); pm.info.requestName;\nconst _ = require('lodash');\npostman.setNextRequest('a'); pm.response.to.have.jsonSchema({});";
        let calls: Vec<(usize, String)> = unsupported_calls(script).into_iter().map(|c| (c.line, c.call)).collect();
        assert_eq!(
            calls,
            vec![
                (3, "pm.sendRequest".to_string()),
                (3, "pm.info.requestName".to_string()),
                (4, "require()".to_string()),
                (5, "postman.setNextRequest".to_string()),
                (5, "pm.response.to.have.jsonSchema".to_string()),
            ]
        );
        assert!(unsupported_calls("pm.environment.set('a', 1); pm.expect(x).to.equal(1); tests['a'] = true").is_empty());
    }
}
