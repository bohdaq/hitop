//! Server-Sent Events: a request whose answer is `text/event-stream`, read as it arrives. The
//! request is prepared as a send prepares it (hitop-core `prepare_in`: variables, auth, scripts'
//! values, form bodies) and sent with reqwest over rustls (proxy from the environment, as hitop-
//! core's client), on its own thread:
//!
//! - `sseStart {collection, request, ...send's arguments}` → `{id}`; events `head {status,
//!   headers, url}`, `chunk {text}` (as it arrives, any split), `end`, `error {message}`;
//! - `sseNext {id, waitMs}` → `{events, done}`; `sseCancel {id}` stops after the next chunk.
//!
//! The app parses events (web app `parseEvents`). Secret values are masked.

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use hitop_core::prepare::{prepare_in, Draft};
use hitop_core::runner::RunState;
use serde_json::{json, Value};

use crate::send::{inputs, masker};
use crate::Args;

struct Stream {
    // Its own lock: waiting for one stream's events never holds up another's, or a cancel.
    events: Arc<Mutex<Receiver<Value>>>,
    cancel: Arc<AtomicBool>,
}

fn streams() -> &'static Mutex<HashMap<u64, Stream>> {
    static S: OnceLock<Mutex<HashMap<u64, Stream>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT: AtomicU64 = AtomicU64::new(1);

pub(crate) fn start(a: &Args) -> Result<Value, String> {
    let i = inputs(a)?;
    let index = a.u64("request")? as usize;
    let saved = i.collection.requests.get(index).ok_or("request: no such request")?;
    let request = i.collection.effective(saved);
    let mut state = RunState::new(&i.collection, i.environment.as_ref(), &i.secrets, &i.options);
    let session: hitop_core::scripting::VariableUpdates = a.opt_json("scriptVariables")?.unwrap_or_default();
    state.scopes.apply(&session, &Default::default());
    i.secrets.apply(&mut state.scopes.upper);
    let m = masker(&i);
    let draft = Draft::of(&request, i.collection.auth.as_ref());
    let pre = prepare_in(&draft, &state.scopes, &HashMap::new()).map_err(|e| m.mask(&format!("{e:#}")))?;
    let vars = state.scopes.merged();
    let (headers, body) = hitop_core::form::outgoing(&request, &pre.headers, &pre.body, &vars).map_err(|e| m.mask(&format!("{e:#}")))?;
    let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_| "bad method".to_string())?;
    let url = pre.url.clone();
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    streams().lock().unwrap_or_else(|p| p.into_inner()).insert(id, Stream { events: Arc::new(Mutex::new(rx)), cancel: cancel.clone() });
    std::thread::spawn(move || {
        let fail = |e: String| json!({ "type": "error", "message": m.mask(&e) });
        let mut builder = reqwest::blocking::Client::builder().use_rustls_tls().timeout(None::<Duration>).connect_timeout(Duration::from_secs(15));
        if let Some(n) = hitop_core::http::network() {
            builder = match n.apply(builder) {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(fail(format!("{e:#}")));
                    return;
                }
            };
        }
        let client = match builder.build() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(fail(e.to_string()));
                return;
            }
        };
        let mut req = client.request(method, &url).body(body);
        for (k, v) in &headers {
            req = req.header(k, v);
        }
        let mut resp = match req.send() {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(fail(format!("{e:#}")));
                return;
            }
        };
        let hs: Vec<Value> = resp.headers().iter().map(|(k, v)| json!([k.as_str(), m.mask(&String::from_utf8_lossy(v.as_bytes()))])).collect();
        let _ = tx.send(json!({ "type": "head", "status": resp.status().as_u16(), "headers": hs, "url": m.mask(&url) }));
        let mut buf = [0u8; 8192];
        let mut pending: Vec<u8> = vec![];
        loop {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            match resp.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    // Hand over whole UTF-8 characters; a split one waits for the rest.
                    let cut = match std::str::from_utf8(&pending) {
                        Ok(_) => pending.len(),
                        Err(e) => e.valid_up_to(),
                    };
                    if cut > 0 {
                        let text = String::from_utf8_lossy(&pending[..cut]).into_owned();
                        pending.drain(..cut);
                        if tx.send(json!({ "type": "chunk", "text": m.mask(&text) })).is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(fail(e.to_string()));
                    return;
                }
            }
        }
        let _ = tx.send(json!({ "type": "end", "cancelled": cancel.load(Ordering::Relaxed) }));
    });
    Ok(json!({ "id": id }))
}

pub(crate) fn next(a: &Args) -> Result<Value, String> {
    let id = a.u64("id")?;
    let wait = Duration::from_millis(a.0.get("waitMs").and_then(Value::as_u64).unwrap_or(300));
    let rx = streams().lock().unwrap_or_else(|p| p.into_inner()).get(&id).map(|s| s.events.clone()).ok_or("no such stream")?;
    let rx = rx.lock().unwrap_or_else(|p| p.into_inner());
    let mut events = vec![];
    if let Ok(e) = rx.recv_timeout(wait) {
        events.push(e);
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
    }
    let done = events.iter().any(|e| e["type"] == "end" || e["type"] == "error");
    if done {
        streams().lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    }
    Ok(json!({ "events": events, "done": done }))
}

pub(crate) fn cancel(a: &Args) -> Result<Value, String> {
    let id = a.u64("id")?;
    if let Some(s) = streams().lock().unwrap_or_else(|p| p.into_inner()).get(&id) {
        s.cancel.store(true, Ordering::Relaxed);
    }
    Ok(Value::Bool(true))
}

/// `grpcTarget {collection, request, ...}`: a gRPC request's address, TLS and metadata with its
/// variables filled in and auth applied (hitop-core `grpc::target`), for SyncFFI's `grpc*` ops.
pub(crate) fn grpc_target(a: &Args) -> Result<Value, String> {
    let i = inputs(a)?;
    let index = a.u64("request")? as usize;
    let saved = i.collection.requests.get(index).ok_or("request: no such request")?;
    let request = i.collection.effective(saved);
    let state = RunState::new(&i.collection, i.environment.as_ref(), &i.secrets, &i.options);
    let mut vars = state.scopes.merged();
    let session: hitop_core::scripting::VariableUpdates = a.opt_json("scriptVariables")?.unwrap_or_default();
    hitop_core::prepare::apply_updates(&mut vars, &session);
    i.secrets.apply(&mut vars);
    let m = masker(&i);
    let t = hitop_core::grpc::target(&request, i.collection.auth.as_ref(), &vars, None).map_err(|e| m.mask(&format!("{e:#}")))?;
    let g = request.grpc.clone().unwrap_or_default();
    let message = hitop_core::interpolation::interpolate(&g.message, &vars).map_err(|e| m.mask(&format!("{e:#}")))?;
    Ok(json!({ "address": t.address, "plaintext": t.plaintext, "metadata": t.metadata, "service": g.service, "method": g.method, "message": message }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_for_one_stream_holds_up_neither_a_cancel_nor_another_stream() {
        let add = |id: u64| {
            let (tx, rx) = channel();
            let cancel = Arc::new(AtomicBool::new(false));
            streams().lock().unwrap().insert(id, Stream { events: Arc::new(Mutex::new(rx)), cancel: cancel.clone() });
            (tx, cancel)
        };
        // Ids no stream started by `start` reaches.
        let (a, b) = (u64::MAX - 1, u64::MAX);
        let (tx_a, cancel_a) = add(a);
        let (tx_b, _) = add(b);
        let waiting = std::thread::spawn(move || next(&Args(&json!({ "id": a, "waitMs": 60_000 }))));
        tx_b.send(json!({ "type": "end" })).unwrap();
        // Were the lock held, these would wait for the other stream's 60 s.
        let started = std::time::Instant::now();
        // The waiting thread has taken its receiver once a try_lock on it fails.
        while streams().lock().unwrap()[&a].events.try_lock().is_ok() {
            std::thread::yield_now();
        }
        assert_eq!(next(&Args(&json!({ "id": b, "waitMs": 0 }))).unwrap()["done"], true);
        cancel(&Args(&json!({ "id": a }))).unwrap();
        assert!(cancel_a.load(Ordering::Relaxed));
        assert!(started.elapsed() < Duration::from_secs(30));
        tx_a.send(json!({ "type": "end" })).unwrap();
        assert_eq!(waiting.join().unwrap().unwrap()["done"], true);
    }
}
