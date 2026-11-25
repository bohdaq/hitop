//! gRPC for the HITOP iOS app (I4): hitop-core's client (`hitop_core::grpc`, the code `hitop grpc`
//! and the desktop app run: tonic over rustls, prost-reflect dynamic messages, server reflection
//! v1 → v1alpha) behind the same C call as sync. Operations:
//!
//! - `grpcList {server}` → `{source, services}` (the desktop's `grpc_list`);
//! - `grpcDescribe {server, symbol}` → `{description, template}` (`grpc_describe`; `template` is
//!   JSON *text*, pretty, so the field order survives Swift);
//! - `grpcStart {server, service, method, message, maxTimeMs}` → `{id}`: the call runs on its own
//!   thread and runtime; everything after the start arrives as events, problems found before
//!   sending as one `error` event;
//! - `grpcNext {id, waitMs}` → `{events, done}`: the events so far (waits up to `waitMs` for one),
//!   `done` once the call ended and every event was taken;
//! - `grpcCancel {id}`: stops the call (its runtime is dropped, so a server stream that never
//!   ends stops too).
//!
//! `server` = `{address, plaintext, metadata: [[name, value]], timeoutMs}` (variables already
//! filled in by the app, as the desktop's page does). Events: `{type: headers, metadata}`,
//! `{type: message, data}` (JSON text), `{type: status, code, name, message, trailers}`,
//! `{type: error, message}`.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use hitop_core::grpc::{self, Event, Reflection, Source, Target};
use serde_json::{json, Value};

use crate::{Args, R};

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!("no runtime: {e}"))
}

fn target(a: &Args) -> Result<Target, String> {
    let s = a.0.get("server").ok_or("argument server: an object")?;
    let address = s.get("address").and_then(Value::as_str).unwrap_or_default().to_string();
    let metadata: Vec<(String, String)> = match s.get("metadata") {
        None | Some(Value::Null) => vec![],
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("argument metadata: {e}"))?,
    };
    Ok(Target { address, plaintext: s.get("plaintext").and_then(Value::as_bool).unwrap_or(false), metadata, ..Default::default() })
}

fn timeout(a: &Args) -> Option<Duration> {
    let ms = a.0.get("server").and_then(|s| s.get("timeoutMs")).and_then(Value::as_u64).unwrap_or(10_000);
    Some(Duration::from_millis(ms))
}

fn source_name(s: &Source) -> &'static str {
    match s {
        Source::Protos => "protos",
        Source::Reflection(Reflection::V1) => "reflection v1",
        Source::Reflection(Reflection::V1Alpha) => "reflection v1alpha",
    }
}

async fn open(t: &Target, timeout: Option<Duration>) -> Result<(grpc::Channel, grpc::DescriptorPool, Source), String> {
    let channel = grpc::connect(t, timeout).await.map_err(|e| format!("{e:#}"))?;
    let (pool, source) = grpc::schema(t, &channel, timeout).await.map_err(|e| format!("{e:#}"))?;
    Ok((channel, pool, source))
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string())
}

/// A call in flight: its events until taken, whether it ended, how to stop it.
struct Call {
    events: Mutex<(VecDeque<Value>, bool)>,
    ready: Condvar,
    cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl Call {
    fn push(&self, e: Value) {
        let mut g = self.events.lock().unwrap_or_else(|p| p.into_inner());
        g.0.push_back(e);
        self.ready.notify_all();
    }
    fn finish(&self) {
        let mut g = self.events.lock().unwrap_or_else(|p| p.into_inner());
        g.1 = true;
        self.ready.notify_all();
    }
}

fn calls() -> &'static Mutex<HashMap<u64, Arc<Call>>> {
    static CALLS: OnceLock<Mutex<HashMap<u64, Arc<Call>>>> = OnceLock::new();
    CALLS.get_or_init(Default::default)
}

static NEXT: AtomicU64 = AtomicU64::new(1);

fn event(e: Event) -> Value {
    match e {
        Event::Headers(metadata) => json!({ "type": "headers", "metadata": metadata }),
        Event::Message(data) => json!({ "type": "message", "data": pretty(&data) }),
        Event::Status(s) => json!({ "type": "status", "code": s.code, "name": s.name, "message": s.message, "trailers": s.trailers }),
    }
}

fn start(a: &Args) -> R {
    let mut t = target(a)?;
    t.service = a.0.get("service").and_then(Value::as_str).unwrap_or_default().to_string();
    t.method = a.0.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
    t.message = a.0.get("message").and_then(Value::as_str).unwrap_or_default().to_string();
    let connect_timeout = timeout(a);
    let max_time = a.0.get("maxTimeMs").and_then(Value::as_u64).map(Duration::from_millis);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let call = Arc::new(Call { events: Mutex::new((VecDeque::new(), false)), ready: Condvar::new(), cancel: Mutex::new(Some(tx)) });
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    calls().lock().map_err(|e| e.to_string())?.insert(id, call.clone());
    let rt = runtime()?;
    std::thread::Builder::new()
        .name(format!("hitop-grpc-{id}"))
        .spawn(move || {
            let c = call.clone();
            let work = async move {
                let (channel, pool, _) = match open(&t, connect_timeout).await {
                    Ok(x) => x,
                    Err(e) => return c.push(json!({ "type": "error", "message": e })),
                };
                let mut events = match grpc::call(channel, &pool, &t, max_time).await {
                    Ok(rx) => rx,
                    Err(e) => return c.push(json!({ "type": "error", "message": format!("{e:#}") })),
                };
                while let Some(e) = events.recv().await {
                    c.push(event(e));
                }
            };
            rt.block_on(async move {
                tokio::select! {
                    _ = rx => {}
                    _ = work => {}
                }
            });
            // Dropping the runtime stops the call's own task (a stream that never ends).
            // A name lookup that hangs isn't waited for.
            rt.shutdown_background();
            call.finish();
        })
        .map_err(|e| format!("no thread: {e}"))?;
    Ok(json!({ "id": id }))
}

fn next(a: &Args) -> R {
    let id = a.u64("id")?;
    let wait = Duration::from_millis(a.0.get("waitMs").and_then(Value::as_u64).unwrap_or(0));
    let Some(call) = calls().lock().map_err(|e| e.to_string())?.get(&id).cloned() else {
        return Ok(json!({ "events": [], "done": true }));
    };
    let mut g = call.events.lock().unwrap_or_else(|p| p.into_inner());
    if g.0.is_empty() && !g.1 && !wait.is_zero() {
        g = call.ready.wait_timeout_while(g, wait, |(q, done)| q.is_empty() && !*done).unwrap_or_else(|p| p.into_inner()).0;
    }
    let events: Vec<Value> = g.0.drain(..).collect();
    let done = g.1;
    drop(g);
    if done {
        calls().lock().map_err(|e| e.to_string())?.remove(&id);
    }
    Ok(json!({ "events": events, "done": done }))
}

fn cancel(a: &Args) -> R {
    let id = a.u64("id")?;
    if let Some(call) = calls().lock().map_err(|e| e.to_string())?.remove(&id) {
        if let Some(tx) = call.cancel.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = tx.send(());
        }
    }
    Ok(Value::Null)
}

pub fn call(op: &str, a: &Args) -> R {
    match op {
        "grpcList" => {
            let t = target(a)?;
            let to = timeout(a);
            runtime()?.block_on(async move {
                let (_, pool, source) = open(&t, to).await?;
                Ok(json!({ "source": source_name(&source), "services": grpc::services(&pool) }))
            })
        }
        "grpcDescribe" => {
            let t = target(a)?;
            let to = timeout(a);
            let symbol = a.str("symbol")?.to_string();
            runtime()?.block_on(async move {
                let (_, pool, _) = open(&t, to).await?;
                let description = grpc::describe(&pool, &symbol).map_err(|e| format!("{e:#}"))?;
                let template = pool
                    .get_message_by_name(symbol.trim_start_matches('.'))
                    .or_else(|| grpc::split_method(&symbol).and_then(|(s, m)| grpc::find_method(&pool, &s, &m).ok()).map(|m| m.input()))
                    .map(|m| pretty(&grpc::template(&m)));
                Ok(json!({ "description": description, "template": template }))
            })
        }
        "grpcStart" => start(a),
        "grpcNext" => next(a),
        "grpcCancel" => cancel(a),
        other => Err(format!("unknown operation {other}")),
    }
}

#[cfg(test)]
mod tests {
    use crate::call_json;
    use serde_json::Value;

    fn ok(op: &str, args: &str) -> Value {
        let v: Value = serde_json::from_str(&call_json(op, args)).unwrap();
        assert!(v.get("error").is_none(), "{op}: {v}");
        v["ok"].clone()
    }

    fn drain(id: u64) -> Vec<Value> {
        let mut all = vec![];
        loop {
            let v = ok("grpcNext", &format!(r#"{{"id":{id},"waitMs":5000}}"#));
            all.extend(v["events"].as_array().unwrap().clone());
            if v["done"] == true {
                return all;
            }
        }
    }

    /// Against hitop's test server (`cargo run -p hitop --example test_servers` in cli/, its
    /// `grpc` address in HITOP_TEST_GRPC); skipped without it. The Swift end-to-end test runs the
    /// same through the app's Core in CI.
    #[test]
    fn against_the_test_server() {
        let Ok(addr) = std::env::var("HITOP_TEST_GRPC") else { return };
        let server = format!(r#"{{"address":"{addr}","plaintext":true,"metadata":[["x-tenant","acme"]]}}"#);
        let listed = ok("grpcList", &format!(r#"{{"server":{server}}}"#));
        assert_eq!(listed["source"], "reflection v1");
        assert!(listed["services"].to_string().contains("hitop.test.Greeter"), "{listed}");
        let d = ok("grpcDescribe", &format!(r#"{{"server":{server},"symbol":"hitop.test.Greeter/SayHello"}}"#));
        assert!(d["template"].as_str().unwrap().contains("\"name\""), "{d}");
        let started = ok("grpcStart", &format!(r#"{{"server":{server},"service":"hitop.test.Greeter","method":"StreamHellos","message":"{{\"name\":\"Ada\",\"times\":3}}"}}"#));
        let events = drain(started["id"].as_u64().unwrap());
        let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["headers", "message", "message", "message", "status"], "{events:?}");
        assert_eq!(events[4]["name"], "OK");
        // Cancel stops a call at once.
        let started = ok("grpcStart", &format!(r#"{{"server":{server},"service":"hitop.test.Greeter","method":"StreamHellos","message":"{{\"times\":100000}}"}}"#));
        let id = started["id"].as_u64().unwrap();
        ok("grpcCancel", &format!(r#"{{"id":{id}}}"#));
        assert_eq!(ok("grpcNext", &format!(r#"{{"id":{id}}}"#))["done"], true);
    }

    #[test]
    fn a_closed_port_is_a_clean_error() {
        let v: Value = serde_json::from_str(&call_json("grpcList", r#"{"server":{"address":"127.0.0.1:1","plaintext":true,"timeoutMs":2000}}"#)).unwrap();
        assert!(v["error"].as_str().is_some(), "{v}");
        let v: Value = serde_json::from_str(&call_json("grpcStart", r#"{"server":{"address":"127.0.0.1:1","plaintext":true,"timeoutMs":2000},"service":"a.B","method":"C"}"#)).unwrap();
        let id = v["ok"]["id"].as_u64().unwrap();
        let mut all = vec![];
        loop {
            let v: Value = serde_json::from_str(&call_json("grpcNext", &format!(r#"{{"id":{id},"waitMs":5000}}"#))).unwrap();
            all.extend(v["ok"]["events"].as_array().unwrap().clone());
            if v["ok"]["done"] == true {
                break;
            }
        }
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!(all[0]["type"], "error");
    }
}
