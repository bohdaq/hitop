//! `send`: one saved request as `hitop run` sends it (hitop-core `runner::run_step`): folder and
//! collection auth and headers, OAuth 2.0 tokens, the pre-request script, form bodies, SigV4, the
//! request with the cookie jar and redirects, Digest's retry, the post-request script and its
//! tests. The app keeps what scripts set between sends (`scriptVariables`, `scriptGlobals`,
//! `context`) and the jar, and passes them back in. Everything returned is masked.

use std::collections::HashMap;
use std::time::Duration;

use hitop_core::cookies::Jar;
use hitop_core::model::{Collection, Environment};
use hitop_core::runner::{run_step, RunOptions, RunState, RunStepResult};
use hitop_core::scripting::VariableUpdates;
use hitop_core::secrets::{Masker, Origin, Secrets};
use serde_json::{json, Map, Value};

use crate::Args;

pub(crate) fn secrets_of(a: &Args, key: &str) -> Result<Secrets, String> {
    let values: HashMap<String, String> = a.opt_json(key)?.unwrap_or_default();
    let mut s = Secrets::default();
    for (k, v) in values {
        // Stored on the device (Android Keystore) and handed over: the keychain's origin.
        s.insert(&k, v, Origin::Keychain);
    }
    Ok(s)
}

/// The run's inputs from the app's arguments: the collection (as JSON text: exact 64-bit ids), the
/// environment, secret values, globals.
pub(crate) struct Inputs {
    pub collection: Collection,
    pub environment: Option<Environment>,
    pub secrets: Secrets,
    pub options: RunOptions,
}

pub(crate) fn inputs(a: &Args) -> Result<Inputs, String> {
    let collection: Collection = a.text_json("collection")?;
    let environment: Option<Environment> = a.opt_json("environment")?;
    let secrets = secrets_of(a, "secrets")?;
    let global_secrets = secrets_of(a, "globalSecrets")?;
    let mut globals: HashMap<String, String> = a.opt_json("globals")?.unwrap_or_default();
    global_secrets.apply(&mut globals);
    let timeout = a.0.get("timeoutMs").and_then(Value::as_u64).filter(|ms| *ms > 0).map(Duration::from_millis);
    let options = RunOptions { timeout, globals, global_secrets, ..Default::default() };
    Ok(Inputs { collection, environment, secrets, options })
}

pub(crate) fn masker(i: &Inputs) -> Masker {
    i.secrets.masker_with(&i.options.global_secrets)
}

/// A step as JSON (already masked by the caller).
pub(crate) fn step_json(step: &RunStepResult) -> Value {
    let mut out = json!({
        "name": step.request_name,
        "method": step.method,
        "url": step.url,
        "durationMs": step.duration_ms,
        "passed": step.passed(),
        "tests": step.tests.iter().map(|t| json!({ "name": t.name, "pass": t.pass, "detail": t.detail })).collect::<Vec<_>>(),
    });
    match &step.result {
        Ok(r) => {
            let mut headers: Vec<(&String, &String)> = r.headers.iter().collect();
            headers.sort();
            out["response"] = json!({
                "status": r.status,
                "headers": headers.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
                "body": r.body,
                "durationMs": r.duration_ms,
                "timings": r.timings,
                "redirects": r.redirects,
            });
        }
        Err(e) => out["error"] = format!("{e:#}").into(),
    }
    out
}

fn updates_json(u: &VariableUpdates) -> Value {
    Value::Object(u.iter().map(|(k, v)| (k.clone(), v.clone().map_or(Value::Null, Value::String))).collect::<Map<_, _>>())
}

pub(crate) fn send(a: &Args) -> Result<Value, String> {
    let i = inputs(a)?;
    let _files = crate::files::Scope::of(a)?;
    let _oauth = crate::oauth::Scope::new(a.0.get("sendId").and_then(Value::as_u64));
    let index = a.u64("request")? as usize;
    let saved = i.collection.requests.get(index).ok_or("request: no such request")?;
    if saved.is_websocket() || saved.is_grpc() {
        return Err("a WebSocket or gRPC request isn't sent with send".into());
    }
    let mut state = RunState::new(&i.collection, i.environment.as_ref(), &i.secrets, &i.options);
    // What scripts set in earlier sends of this collection, kept by the app for the session.
    let session_vars: VariableUpdates = a.opt_json("scriptVariables")?.unwrap_or_default();
    let session_globals: VariableUpdates = a.opt_json("scriptGlobals")?.unwrap_or_default();
    state.scopes.apply(&session_vars, &session_globals);
    // Secrets stay over script-set values, as in a run.
    i.secrets.apply(&mut state.scopes.upper);
    state.context = a.opt_json("context")?.unwrap_or_default();
    let mut jar: Option<Jar> = match a.0.get("jar") {
        None | Some(Value::Null) => None,
        Some(v) => Some(serde_json::from_value(v.clone()).map_err(|e| format!("jar: {e}"))?),
    };
    let mut step = run_step(&i.collection, saved, &mut state, &i.options, jar.as_mut());
    let m = masker(&i);
    step.mask(&m);
    let mut out = step_json(&step);
    if let Some(sent) = &state.last_sent {
        out["sent"] = json!({
            "url": m.mask(&sent.url),
            "method": sent.method,
            "headers": sent.headers.iter().map(|(k, v)| json!([k, m.mask(v)])).collect::<Vec<_>>(),
            "body": m.mask(&sent.body),
        });
    }
    out["scriptVariables"] = updates_json(&state.script_variables);
    out["scriptGlobals"] = updates_json(&state.script_globals);
    out["context"] = serde_json::to_value(&state.context).unwrap_or_default();
    if let Some(j) = jar {
        out["jar"] = serde_json::to_value(j).unwrap_or_default();
    }
    Ok(out)
}

/// `snippet`: the request as code (hitop-core `snippet::generate`, the CLI's and the web app's
/// Code). Secret values and variables with no value stay `{{NAME}}`.
pub(crate) fn snippet(a: &Args) -> Result<Value, String> {
    let i = inputs(a)?;
    let lang = hitop_core::snippet::Lang::parse(a.str("lang")?).ok_or("lang: curl, fetch, python, go, java or rust")?;
    let index = a.u64("request")? as usize;
    let saved = i.collection.requests.get(index).ok_or("request: no such request")?;
    let request = i.collection.effective(saved);
    let mut names: Vec<String> = i.secrets.names().map(str::to_string).collect();
    names.extend(i.options.global_secrets.names().map(str::to_string));
    if let Some(e) = &i.environment {
        names.extend(e.secrets.iter().cloned());
    }
    // Variables without the secrets' values: those become placeholders.
    let state = RunState::new(&i.collection, i.environment.as_ref(), &Secrets::default(), &RunOptions { globals: a.opt_json("globals")?.unwrap_or_default(), ..Default::default() });
    let mut vars = state.scopes.merged();
    let session: VariableUpdates = a.opt_json("scriptVariables")?.unwrap_or_default();
    hitop_core::prepare::apply_updates(&mut vars, &session);
    for n in &names {
        vars.remove(n);
    }
    let out = hitop_core::snippet::generate(lang, &request, i.collection.auth.as_ref(), &vars, &names).map_err(|e| format!("{e:#}"))?;
    Ok(json!({ "code": out.code, "placeholders": out.placeholders }))
}

#[cfg(test)]
mod tests {
    use crate::call_json;
    use serde_json::{json, Value};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Answers every connection with `answer` and returns the first request's text.
    fn server(answer: &'static str) -> (u16, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut s = stream.unwrap();
                let mut buf = [0u8; 8192];
                let n = s.read(&mut buf).unwrap();
                let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
                s.write_all(answer.as_bytes()).unwrap();
            }
        });
        (port, rx)
    }

    #[test]
    fn a_saved_request_sends_with_scripts_secrets_and_cookies() {
        let body = r#"{"id":"ord_1","total":6480}"#;
        let answer: &'static str = Box::leak(
            format!("HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nSet-Cookie: session=s1; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_boxed_str(),
        );
        let (port, requests) = server(answer);
        let collection = json!({
            "id": 18446744073709551000u64,
            "name": "shop-api",
            "auth": { "type": "bearer", "token": "${token}" },
            "variables": { "baseUrl": format!("http://127.0.0.1:{port}") },
            "requests": [{
                "id": 1, "name": "Create order", "method": "POST",
                "url": "${baseUrl}/v2/orders?k=${token}",
                "headers": [{ "name": "Content-Type", "value": "application/json", "enabled": true }],
                "body": "{\"customerId\": ${customerId}}",
                "preRequestScript": "pm.environment.set('fromPre', 'yes')",
                "postRequestScript": "pm.test('Status is 201', () => pm.response.to.have.status(201)); pm.environment.set('orderId', pm.response.json().id)",
                "unknownKey": { "kept": true }
            }]
        });
        let args = json!({
            "collection": collection.to_string(),
            "request": 0,
            "environment": { "id": "env-staging", "name": "Staging", "variables": { "customerId": "42" }, "secrets": ["token"] },
            "secrets": { "token": "tok_s3cret" },
            "scriptVariables": { "earlier": "value" },
            "jar": [],
        });
        let out: Value = serde_json::from_str(&call_json("send", &args.to_string())).unwrap();
        let ok = &out["ok"];
        assert_eq!(ok["response"]["status"], 201, "{out}");
        assert_eq!(ok["tests"][0]["pass"], true, "{out}");
        assert_eq!(ok["scriptVariables"]["orderId"], "ord_1");
        assert_eq!(ok["scriptVariables"]["fromPre"], "yes");
        assert_eq!(ok["jar"][0]["name"], "session");
        assert!(ok["url"].as_str().unwrap().ends_with("/v2/orders?k=••••"), "{out}");
        let text = out.to_string();
        assert!(!text.contains("tok_s3cret"), "{text}");
        let wire = requests.recv().unwrap();
        assert!(wire.contains("authorization: Bearer tok_s3cret") || wire.contains("Authorization: Bearer tok_s3cret"), "{wire}");
        assert!(wire.ends_with("{\"customerId\": 42}"), "{wire}");
        assert_eq!(ok["sent"]["body"], "{\"customerId\": 42}");
    }

    #[test]
    fn a_snippet_keeps_secrets_as_placeholders() {
        let collection = json!({ "id": 1, "name": "c", "auth": { "type": "bearer", "token": "${token}" }, "requests": [{ "id": 2, "name": "r", "method": "GET", "url": "https://${host}/x", "headers": [], "body": "" }] });
        let args = json!({ "collection": collection.to_string(), "request": 0, "lang": "curl",
            "environment": { "id": "e", "name": "E", "variables": { "host": "api.example.com" }, "secrets": ["token"] }, "secrets": { "token": "s3" } });
        let out: Value = serde_json::from_str(&call_json("snippet", &args.to_string())).unwrap();
        let code = out["ok"]["code"].as_str().unwrap();
        assert!(code.contains("https://api.example.com/x") && code.contains("{{token}}") && !code.contains("s3"), "{code}");
    }

    #[test]
    fn a_missing_variable_is_an_error_naming_it() {
        let collection = json!({ "id": 1, "name": "c", "requests": [{ "id": 2, "name": "r", "method": "GET", "url": "${nowhere}/x", "headers": [], "body": "" }] });
        let out: Value = serde_json::from_str(&call_json("send", &json!({ "collection": collection.to_string(), "request": 0 }).to_string())).unwrap();
        assert!(out["ok"]["error"].as_str().unwrap().contains("nowhere"), "{out}");
    }
}
