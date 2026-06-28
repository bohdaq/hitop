use std::collections::HashMap;
use anyhow::{anyhow, Result};
use boa_engine::{Context, Source};
use serde_json::Value;

/// Inputs the pre-request script can mutate.
#[derive(Debug, Clone)]
pub struct PreRequestCtx {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub variables: HashMap<String, String>,
    pub context: HashMap<String, Value>,
}

/// Outputs after running a pre-request script.
#[derive(Debug, Clone)]
pub struct PreRequestResult {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub context_updates: HashMap<String, Value>,
}

/// Outputs after running a post-request script.
#[derive(Debug, Clone)]
pub struct PostRequestResult {
    pub context_updates: HashMap<String, Value>,
}

pub fn run_pre_request_script(script: &str, ctx: PreRequestCtx) -> Result<PreRequestResult> {
    if script.trim().is_empty() {
        return Ok(PreRequestResult {
            url: ctx.url,
            headers: ctx.headers,
            body: ctx.body,
            context_updates: HashMap::new(),
        });
    }

    let mut engine = Context::default();

    let variables_json = serde_json::to_string(&ctx.variables)?;
    let context_json = serde_json::to_string(&ctx.context)?;

    // Build the wrapper script that exposes the API surface
    let preamble = format!(
        r#"
const __vars = {};
const __ctx  = {};
const __ctxUpdates = {{}};
let __url    = {};
let __body   = {};
let __headers = [];

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
"#,
        variables_json,
        context_json,
        serde_json::to_string(&ctx.url)?,
        serde_json::to_string(&ctx.body)?,
    );

    let postamble = r#"
JSON.stringify({ url: __url, body: __body, headers: __headers, ctx: __ctxUpdates })
"#;

    let full_script = format!("{}\n{}\n{}", preamble, script, postamble);

    let result = engine
        .eval(Source::from_bytes(full_script.as_bytes()))
        .map_err(|e| anyhow!("pre-request script error: {}", e.to_string()))?;

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
                        pair.get(0)?.as_str()?.to_string(),
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
    })
}

pub fn run_post_request_script(
    script: &str,
    response_body: &str,
    response_headers: &HashMap<String, String>,
    status_code: u16,
    variables: &HashMap<String, String>,
    context: &HashMap<String, Value>,
) -> Result<PostRequestResult> {
    if script.trim().is_empty() {
        return Ok(PostRequestResult {
            context_updates: HashMap::new(),
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
"#,
        response_json,
        response_text_json,
        headers_json,
        status_code,
        variables_json,
        context_json,
    );

    let postamble = "JSON.stringify(__ctxUpdates)";
    let full_script = format!("{}\n{}\n{}", preamble, script, postamble);

    let result = engine
        .eval(Source::from_bytes(full_script.as_bytes()))
        .map_err(|e| anyhow!("post-request script error: {}", e.to_string()))?;

    let json_str = result
        .as_string()
        .map(|s| s.to_std_string().unwrap_or_default())
        .unwrap_or_default();

    let ctx_updates: HashMap<String, Value> = serde_json::from_str(&json_str).unwrap_or_default();

    Ok(PostRequestResult {
        context_updates: ctx_updates,
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
    fn empty_post_script_returns_no_updates() {
        let result = run_post_request_script(
            "", r#"{"id":1}"#, &HashMap::new(), 200, &HashMap::new(), &HashMap::new(),
        ).unwrap();
        assert!(result.context_updates.is_empty());
    }

    #[test]
    fn set_context_in_post_script() {
        let result = run_post_request_script(
            "setContext('done', true)",
            "{}",
            &HashMap::new(),
            200,
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
            200,
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
            201,
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
            200,
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
            200,
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
            200,
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
            200,
            &HashMap::new(),
            &HashMap::new(),
        ).unwrap();
        assert_eq!(result.context_updates["isNull"], serde_json::json!(true));
    }

    #[test]
    fn post_script_syntax_error_returns_error() {
        let err = run_post_request_script(
            "!!!invalid", "{}", &HashMap::new(), 200, &HashMap::new(), &HashMap::new(),
        ).unwrap_err();
        assert!(err.to_string().contains("post-request script error"));
    }
}
