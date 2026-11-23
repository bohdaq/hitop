//! The HITOP Android app's Rust (android/GOAL.md): one JNI function, `Native.call(op, args)`,
//! JSON in and JSON out like SyncFFI's `hitop_sync_call`. Operations this crate doesn't know go
//! to SyncFFI (`hitop_sync_ffi::call_json`): sync plan, folder format, hitop-sync-crypto, gRPC.

use serde_json::{json, Value};

type R = Result<Value, String>;

fn call(op: &str, a: &Value) -> Option<R> {
    Some(match op {
        "version" => Ok(json!({ "ffi": env!("CARGO_PKG_VERSION") })),
        "echo" => Ok(a.get("text").cloned().unwrap_or(Value::Null)),
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
