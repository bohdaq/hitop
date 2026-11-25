//! The HITOP Android app's Rust (android/GOAL.md): one JNI function, `Native.call(op, args)`,
//! JSON in and JSON out like SyncFFI's `hitop_sync_call`. Operations this crate doesn't know go
//! to SyncFFI (`hitop_sync_ffi::call_json`): sync plan, folder format, hitop-sync-crypto, gRPC.

use serde::Deserialize;
use serde_json::{json, Value};

/// `hitop run --data`'s parser, the CLI's own file.
#[path = "../../../cli/crates/hitop/src/commands/data.rs"]
#[allow(dead_code)]
mod data;
mod files;
mod import;
mod network;
mod oauth;
mod run;
mod send;
mod sse;
mod ws;

type R = Result<Value, String>;

/// An operation's arguments (a JSON object).
pub(crate) struct Args<'a>(pub &'a Value);

impl Args<'_> {
    pub fn str(&self, k: &str) -> Result<&str, String> {
        self.0.get(k).and_then(Value::as_str).ok_or_else(|| format!("argument {k}: a string"))
    }
    pub fn u64(&self, k: &str) -> Result<u64, String> {
        self.0.get(k).and_then(Value::as_u64).ok_or_else(|| format!("argument {k}: a number"))
    }
    /// An optional argument; null or missing = None.
    pub fn opt_json<T: for<'de> Deserialize<'de>>(&self, k: &str) -> Result<Option<T>, String> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|e| format!("argument {k}: {e}")),
        }
    }
    /// A value that crosses as JSON text (exact 64-bit ids).
    pub fn text_json<T: for<'de> Deserialize<'de>>(&self, k: &str) -> Result<T, String> {
        serde_json::from_str(self.str(k)?).map_err(|e| format!("argument {k}: {e}"))
    }
}

fn call(op: &str, a: &Value) -> Option<R> {
    Some(match op {
        "version" => Ok(json!({ "ffi": env!("CARGO_PKG_VERSION") })),
        "echo" => Ok(a.get("text").cloned().unwrap_or(Value::Null)),
        "send" => send::send(&Args(a)),
        "import" => import::import(&Args(a)),
        "importFiles" => import::import_files(&Args(a)),
        "openapiSync" => import::openapi_sync(&Args(a)),
        "export" => import::export(&Args(a)),
        "setNetwork" => network::set(&Args(a)),
        "oauthPending" => oauth::pending(&Args(a)),
        "oauthCancel" => oauth::cancel(&Args(a)),
        "oauthForget" => oauth::forget(),
        "snippet" => send::snippet(&Args(a)),
        "runStart" => run::start(&Args(a)),
        "dataRows" => run::data_rows(&Args(a)),
        "runNext" => run::next(&Args(a)),
        "runCancel" => run::cancel(&Args(a)),
        "wsConnect" => ws::connect_op(&Args(a)),
        "wsSend" => ws::send(&Args(a)),
        "wsSendSaved" => ws::send_saved(&Args(a)),
        "wsNext" => ws::next(&Args(a)),
        "wsClose" => ws::close(&Args(a)),
        "sseStart" => sse::start(&Args(a)),
        "sseNext" => sse::next(&Args(a)),
        "sseCancel" => sse::cancel(&Args(a)),
        "grpcTarget" => sse::grpc_target(&Args(a)),
        // UI tests and screenshots: every http:// request goes to the app's local stub server
        // (reqwest reads HTTP_PROXY when hitop-core builds its client, before the first request).
        "useHttpProxy" => Args(a).str("proxy").map(|proxy| {
            std::env::set_var("HTTP_PROXY", proxy);
            std::env::set_var("http_proxy", proxy);
            std::env::remove_var("NO_PROXY");
            std::env::remove_var("no_proxy");
            Value::Bool(true)
        }),
        "evalScript" => {
            let script = a.get("script").and_then(Value::as_str).unwrap_or("");
            let ctx = hitop_core::scripting::PreRequestCtx {
                url: "https://example.com/".into(),
                method: "GET".into(),
                headers: vec![],
                body: String::new(),
                variables: Default::default(),
                context: Default::default(),
                globals: Default::default(),
            };
            hitop_core::scripting::run_pre_request_script(script, ctx)
                .map(|r| json!({ "variables": r.variables, "url": r.url }))
                .map_err(|e| format!("{e:#}"))
        }
        _ => return None,
    })
}

/// `{"ok": value}` or `{"error": message}`; panics are caught.
pub fn call_json(op: &str, args: &str) -> String {
    let parsed: Result<Value, _> = serde_json::from_str(if args.is_empty() { "{}" } else { args });
    let v = match parsed {
        Err(e) => return json!({ "error": format!("arguments: {e}") }).to_string(),
        Ok(v) => v,
    };
    match std::panic::catch_unwind(|| call(op, &v)) {
        Ok(Some(Ok(v))) => json!({ "ok": v }).to_string(),
        Ok(Some(Err(e))) => json!({ "error": e }).to_string(),
        Ok(None) => hitop_sync_ffi::call_json(op, args),
        Err(_) => json!({ "error": format!("{op}: internal error") }).to_string(),
    }
}

mod jni_entry;

#[cfg(test)]
mod tests {
    use super::call_json;
    use serde_json::Value;

    fn answer(op: &str, args: &str) -> Value {
        serde_json::from_str(&call_json(op, args)).unwrap()
    }

    #[test]
    fn a_script_runs_and_sync_ops_fall_through() {
        let out = answer("evalScript", r#"{"script": "pm.environment.set('x', 'y')"}"#);
        assert_eq!(out["ok"]["variables"]["x"], "y");
        assert_eq!(answer("randomId", "{}")["ok"].as_str().unwrap().len(), 32);
        assert!(answer("nope", "{}")["error"].as_str().unwrap().contains("nope"));
        assert!(answer("echo", "not json")["error"].as_str().unwrap().starts_with("arguments"));
    }
}
