//! WebSocket requests (`kind: websocket`): hitop-core's `websocket::target` fills the URL, headers,
//! auth and subprotocols with the request's variables, `websocket::connect` opens it (tokio-
//! tungstenite over rustls, the CLI's and the desktop's client). Each connection runs on its own
//! thread and current-thread runtime:
//!
//! - `wsConnect {collection, request, environment?, secrets?, globals?, ..., timeoutMs?}` → `{id}`;
//!   the handshake's answer arrives as the first event (`opened {status, protocol, headers}`)
//!   or `error {message}`;
//! - `wsSend {id, text}` or `{id, base64}` (binary); `wsSendSaved {id, name}`: a saved message,
//!   variables filled in at send time;
//! - `wsNext {id, waitMs}` → `{events, done}`: `message {text}` / `message {base64}`, `closed {code,
//!   reason}`, `error {message}`; `wsClose {id}`.
//!
//! Secret values are masked in every event.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender as StdSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hitop_core::runner::RunState;
use hitop_core::secrets::Masker;
use hitop_core::websocket::{connect, Data, Event, Sender};
use serde_json::{json, Value};

use crate::send::{inputs, masker};
use crate::Args;

struct Socket {
    // Its own lock: waiting for events never holds up a send, a close or another connection.
    events: Arc<Mutex<Receiver<Value>>>,
    sender: Option<Sender>,
    request: hitop_core::model::Request,
    vars: HashMap<String, String>,
    masker: Masker,
}

fn sockets() -> &'static Mutex<HashMap<u64, Socket>> {
    static S: OnceLock<Mutex<HashMap<u64, Socket>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

static NEXT: AtomicU64 = AtomicU64::new(1);

pub(crate) fn connect_op(a: &Args) -> Result<Value, String> {
    let i = inputs(a)?;
    let index = a.u64("request")? as usize;
    let saved = i.collection.requests.get(index).ok_or("request: no such request")?.clone();
    let request = i.collection.effective(&saved);
    let state = RunState::new(&i.collection, i.environment.as_ref(), &i.secrets, &i.options);
    let mut vars = state.scopes.merged();
    let session: hitop_core::scripting::VariableUpdates = a.opt_json("scriptVariables")?.unwrap_or_default();
    hitop_core::prepare::apply_updates(&mut vars, &session);
    i.secrets.apply(&mut vars);
    let m = masker(&i);
    let target = hitop_core::websocket::target(&request, i.collection.auth.as_ref(), &vars).map_err(|e| m.mask(&format!("{e:#}")))?;
    let wait = Duration::from_millis(a.0.get("timeoutMs").and_then(Value::as_u64).unwrap_or(15_000));
    let timeout = Some(wait);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = channel::<Value>();
    let (sender_tx, sender_rx) = channel::<Sender>();
    let mask = m.clone();
    std::thread::spawn(move || run(target, timeout, tx, sender_tx, mask));
    // The sender arrives once the handshake succeeded (or never: the error is an event).
    let sender = sender_rx.recv_timeout(wait + Duration::from_secs(1)).ok();
    sockets().lock().unwrap_or_else(|p| p.into_inner()).insert(id, Socket { events: Arc::new(Mutex::new(rx)), sender, request, vars, masker: m });
    Ok(json!({ "id": id }))
}

fn run(target: hitop_core::websocket::Target, timeout: Option<Duration>, tx: StdSender<Value>, sender_tx: StdSender<Sender>, m: Masker) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            let _ = tx.send(json!({ "type": "error", "message": format!("no runtime: {e}") }));
            return;
        }
    };
    rt.block_on(async move {
        let mut c = match connect(&target, timeout).await {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(json!({ "type": "error", "message": m.mask(&format!("{e:#}")) }));
                return;
            }
        };
        let headers: Vec<Value> = c.opened.headers.iter().map(|(k, v)| json!([k, m.mask(v)])).collect();
        let _ = tx.send(json!({ "type": "opened", "status": c.opened.status, "protocol": c.opened.protocol, "headers": headers }));
        let _ = sender_tx.send(c.sender.clone());
        drop(c.sender);
        while let Some(e) = c.events.recv().await {
            let v = match e {
                Event::Message(Data::Text(t)) => json!({ "type": "message", "text": m.mask(&t) }),
                Event::Message(Data::Binary(b)) => json!({ "type": "message", "base64": STANDARD.encode(&b), "size": b.len() }),
                Event::Closed { code, reason } => json!({ "type": "closed", "code": code, "reason": m.mask(&reason) }),
                Event::Error(e) => json!({ "type": "error", "message": m.mask(&e) }),
            };
            let last = v["type"] != "message";
            if tx.send(v).is_err() || last {
                break;
            }
        }
    });
}

fn with<T>(id: u64, f: impl FnOnce(&mut Socket) -> Result<T, String>) -> Result<T, String> {
    let mut map = sockets().lock().unwrap_or_else(|p| p.into_inner());
    let s = map.get_mut(&id).ok_or("no such connection")?;
    f(s)
}

pub(crate) fn send(a: &Args) -> Result<Value, String> {
    let id = a.u64("id")?;
    with(id, |s| {
        let sender = s.sender.as_ref().ok_or("the connection isn't open")?;
        let data = match (a.0.get("text").and_then(Value::as_str), a.0.get("base64").and_then(Value::as_str)) {
            (Some(t), _) => Data::Text(t.to_string()),
            (None, Some(b)) => Data::Binary(STANDARD.decode(b).map_err(|_| "base64: not base64".to_string())?),
            _ => return Err("text or base64".into()),
        };
        sender.send(data).map_err(|e| e.to_string())?;
        Ok(Value::Bool(true))
    })
}

/// A saved message, variables filled in now; answers the text as sent (masked).
pub(crate) fn send_saved(a: &Args) -> Result<Value, String> {
    let id = a.u64("id")?;
    let name = a.str("name")?.to_string();
    with(id, |s| {
        let text = hitop_core::websocket::message(&s.request, &name, &s.vars).map_err(|e| s.masker.mask(&format!("{e:#}")))?;
        s.sender.as_ref().ok_or("the connection isn't open")?.send(Data::Text(text.clone())).map_err(|e| e.to_string())?;
        Ok(Value::String(s.masker.mask(&text)))
    })
}

pub(crate) fn next(a: &Args) -> Result<Value, String> {
    let id = a.u64("id")?;
    let wait = Duration::from_millis(a.0.get("waitMs").and_then(Value::as_u64).unwrap_or(300));
    let rx = with(id, |s| Ok(s.events.clone()))?;
    let rx = rx.lock().unwrap_or_else(|p| p.into_inner());
    let mut events = vec![];
    if let Ok(e) = rx.recv_timeout(wait) {
        events.push(e);
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
    }
    let done = events.iter().any(|e| e["type"] == "closed" || e["type"] == "error");
    if done {
        sockets().lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    }
    Ok(json!({ "events": events, "done": done }))
}

pub(crate) fn close(a: &Args) -> Result<Value, String> {
    let id = a.u64("id")?;
    if let Some(s) = sockets().lock().unwrap_or_else(|p| p.into_inner()).get_mut(&id) {
        if let Some(sender) = s.sender.take() {
            sender.close();
        }
    }
    Ok(Value::Bool(true))
}
