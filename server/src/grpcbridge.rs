//! `POST /api/grpc/{list,describe,call}`: gRPC from the web app (a browser can't speak gRPC), the
//! same API as the desktop's `grpc_*` commands, with server reflection only (the browser has no
//! .proto files to give). Checked as the HTTP proxy checks a request: the web app's Origin, and
//! the address resolved here, all public, then connected to directly (hitop-core grpc
//! `Target::connect_to`). `call` streams one JSON event per line (`headers`, `message`, then
//! `status`) as the server sends them; closing the response cancels it. At most `MAX_CALLS` at
//! once, each for `CALL_FOR`; nothing is logged.

use std::io::Read;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use hitop_core::grpc::{self, Event, Reflection, Source, Target};
use rust_web_server::header::Header;
use rust_web_server::request::Request;
use rust_web_server::response::{Response, STATUS_CODE_REASON_PHRASE};
use serde::Deserialize;
use serde_json::{json, Value};

pub const MAX_CALLS: usize = 16;
pub const CALL_FOR: Duration = Duration::from_secs(5 * 60);
const CONNECT_FOR: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Args {
    address: String,
    #[serde(default)]
    plaintext: bool,
    #[serde(default)]
    metadata: Vec<(String, String)>,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    service: String,
    #[serde(default)]
    method: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    max_time_ms: Option<u64>,
}

fn error(status: &'static rust_web_server::response::StatusCodeReasonPhrase, message: &str) -> Response {
    Response::json(status, json!({ "error": message }).to_string().into_bytes())
}

#[derive(Clone)]
pub struct GrpcBridge {
    origins: Arc<Vec<String>>,
    deny: Arc<Vec<IpAddr>>,
    calls: Arc<AtomicUsize>,
    allow_private: bool,
}

/// A call's place among `MAX_CALLS`, given back when the response is done.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The call's events as lines, read by the response as they arrive.
struct Lines {
    rx: mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
    _slot: Slot,
}

impl Read for Lines {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.buf.is_empty() {
            match self.rx.recv() {
                Ok(b) => self.buf = b,
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len());
        out[..n].copy_from_slice(&self.buf[..n]);
        self.buf.drain(..n);
        Ok(n)
    }
}

impl GrpcBridge {
    pub fn new(origins: Vec<String>, deny: Vec<IpAddr>, allow_private: bool) -> Self {
        GrpcBridge { origins: Arc::new(origins), deny: Arc::new(deny), calls: Arc::new(AtomicUsize::new(0)), allow_private }
    }

    pub fn from_env() -> Self {
        let list = |name: &str, default: &str| -> Vec<String> { std::env::var(name).unwrap_or_else(|_| default.into()).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect() };
        let deny = list("HITOP_DENY_IPS", "").iter().filter_map(|s| s.parse().ok()).collect();
        GrpcBridge::new(list("HITOP_ALLOWED_ORIGINS", "https://hitop.rws8.tech"), deny, std::env::var("HITOP_BRIDGE_ALLOW_PRIVATE").as_deref() == Ok("1"))
    }

    /// The request's arguments and a target that connects only to a checked address.
    fn accept(&self, request: &Request) -> Result<(Args, Target), Box<Response>> {
        let s = STATUS_CODE_REASON_PHRASE;
        let origin = request.header("origin").map(|h| h.value.to_string()).unwrap_or_default();
        if !self.origins.contains(&origin) {
            return Err(Box::new(error(s.n403_forbidden, "requests must come from the HITOP web app")));
        }
        if request.body.len() > 1024 * 1024 {
            return Err(Box::new(error(s.n413_payload_too_large, "the request is over 1 MB")));
        }
        let args: Args = serde_json::from_slice(&request.body).map_err(|e| Box::new(error(s.n400_bad_request, &format!("invalid request JSON: {e}"))))?;
        let (uri, tls) = grpc::endpoint(&args.address, args.plaintext).map_err(|e| Box::new(error(s.n400_bad_request, &format!("{e:#}"))))?;
        let u = url::Url::parse(&uri).map_err(|e| Box::new(error(s.n400_bad_request, &format!("invalid address: {e}"))))?;
        let host = u.host_str().ok_or_else(|| Box::new(error(s.n400_bad_request, "the address has no host")))?.to_string();
        let port = u.port().unwrap_or(if tls { 443 } else { 80 });
        let addr = crate::guard::checked_addr(&host, port, &self.deny, self.allow_private).map_err(|e| Box::new(error(s.n403_forbidden, &e)))?;
        let metadata = args.metadata.iter().filter(|(k, _)| !k.trim().is_empty()).cloned().collect();
        let target = Target { address: args.address.clone(), plaintext: args.plaintext, metadata, service: args.service.clone(), method: args.method.clone(), message: args.message.clone(), connect_to: Some(addr), ..Default::default() };
        Ok((args, target))
    }

    fn runtime() -> Result<tokio::runtime::Runtime, Box<Response>> {
        tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| Box::new(error(STATUS_CODE_REASON_PHRASE.n500_internal_server_error, &e.to_string())))
    }

    pub fn list(&self, request: &Request) -> Response {
        let (_, target) = match self.accept(request) {
            Ok(x) => x,
            Err(r) => return *r,
        };
        let rt = match Self::runtime() {
            Ok(r) => r,
            Err(r) => return *r,
        };
        let out = rt.block_on(async {
            let channel = grpc::connect(&target, Some(CONNECT_FOR)).await?;
            grpc::schema(&target, &channel, Some(CONNECT_FOR)).await
        });
        match out {
            Ok((pool, source)) => Response::json(STATUS_CODE_REASON_PHRASE.n200_ok, json!({ "source": source_name(&source), "services": grpc::services(&pool) }).to_string().into_bytes()),
            Err(e) => error(STATUS_CODE_REASON_PHRASE.n502_bad_gateway, &format!("{e:#}")),
        }
    }

    pub fn describe(&self, request: &Request) -> Response {
        let (args, target) = match self.accept(request) {
            Ok(x) => x,
            Err(r) => return *r,
        };
        let rt = match Self::runtime() {
            Ok(r) => r,
            Err(r) => return *r,
        };
        let out = rt.block_on(async {
            let channel = grpc::connect(&target, Some(CONNECT_FOR)).await?;
            let (pool, _) = grpc::schema(&target, &channel, Some(CONNECT_FOR)).await?;
            let description = grpc::describe(&pool, &args.symbol)?;
            let template = pool
                .get_message_by_name(args.symbol.trim_start_matches('.'))
                .or_else(|| grpc::split_method(&args.symbol).and_then(|(s, m)| grpc::find_method(&pool, &s, &m).ok()).map(|m| m.input()))
                .map(|m| grpc::template(&m));
            anyhow::Ok(json!({ "description": description, "template": template }))
        });
        match out {
            Ok(v) => Response::json(STATUS_CODE_REASON_PHRASE.n200_ok, v.to_string().into_bytes()),
            Err(e) => error(STATUS_CODE_REASON_PHRASE.n502_bad_gateway, &format!("{e:#}")),
        }
    }

    pub fn call(&self, request: &Request) -> Response {
        let s = STATUS_CODE_REASON_PHRASE;
        let (args, target) = match self.accept(request) {
            Ok(x) => x,
            Err(r) => return *r,
        };
        if self.calls.fetch_add(1, Ordering::SeqCst) >= MAX_CALLS {
            self.calls.fetch_sub(1, Ordering::SeqCst);
            return error(s.n503_service_unavailable, "too many gRPC calls are open. Try again in a minute.");
        }
        let slot = Slot(self.calls.clone());
        let rt = match Self::runtime() {
            Ok(r) => r,
            Err(r) => return *r,
        };
        let max = args.max_time_ms.map(Duration::from_millis).unwrap_or(CALL_FOR).min(CALL_FOR);
        // Problems before sending (no server, no such method, bad JSON) are errors; the rest streams.
        let events = rt.block_on(async {
            let channel = grpc::connect(&target, Some(CONNECT_FOR)).await?;
            let (pool, _) = grpc::schema(&target, &channel, Some(CONNECT_FOR)).await?;
            grpc::call(channel, &pool, &target, Some(max)).await
        });
        let mut events = match events {
            Ok(e) => e,
            Err(e) => return error(s.n502_bad_gateway, &format!("{e:#}")),
        };
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            rt.block_on(async move {
                let _ = tokio::time::timeout(CALL_FOR, async {
                    while let Some(e) = events.recv().await {
                        let line: Value = match e {
                            Event::Headers(metadata) => json!({ "type": "headers", "metadata": metadata }),
                            Event::Message(data) => json!({ "type": "message", "data": data }),
                            Event::Status(st) => json!({ "type": "status", "code": st.code, "name": st.name, "message": st.message, "trailers": st.trailers }),
                        };
                        let mut bytes = line.to_string().into_bytes();
                        bytes.push(b'\n');
                        // The page went away: stop.
                        if tx.send(bytes).is_err() {
                            return;
                        }
                    }
                })
                .await;
            });
        });
        let mut response = Response::get_response(s.n200_ok, None, None);
        response.headers.push(Header::new(Header::_CONTENT_TYPE, "application/x-ndjson"));
        response.headers.push(Header::new(Header::_CACHE_CONTROL, "no-store"));
        response.stream_pipe = Some(Box::new(Lines { rx, buf: Vec::new(), _slot: slot }));
        response
    }
}

fn source_name(s: &Source) -> &'static str {
    match s {
        Source::Protos => "protos",
        Source::Reflection(Reflection::V1) => "reflection v1",
        Source::Reflection(Reflection::V1Alpha) => "reflection v1alpha",
    }
}

#[cfg(test)]
#[path = "../../cli/crates/hitop-core/tests/support/grpc_server.rs"]
mod grpc_server;

#[cfg(test)]
mod tests {
    use super::*;
    use rust_web_server::app::App;
    use rust_web_server::test_client::TestClient;

    fn client(allow_private: bool) -> TestClient<rust_web_server::state::AppWithState<GrpcBridge>> {
        let b = GrpcBridge::new(vec!["https://hitop.test".into()], vec![], allow_private);
        TestClient::new(
            App::with_state(b)
                .post("/api/grpc/list", |r, _, _, b| b.list(r))
                .post("/api/grpc/describe", |r, _, _, b| b.describe(r))
                .post("/api/grpc/call", |r, _, _, b| b.call(r)),
        )
    }

    #[test]
    fn list_describe_and_a_streamed_call() {
        let addr = grpc_server::start(grpc_server::Reflect::V1);
        let c = client(true);
        let body = |extra: serde_json::Value| {
            let mut v = json!({ "address": addr.to_string(), "plaintext": true, "metadata": [["x-tenant", "acme"]] });
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            v.to_string()
        };
        assert_eq!(c.post("/api/grpc/list").body_json(&body(json!({}))).send().status(), 403, "another origin is refused");
        let listed = c.post("/api/grpc/list").header("Origin", "https://hitop.test").body_json(&body(json!({}))).send();
        let listed: Value = serde_json::from_slice(listed.body_bytes()).unwrap();
        assert_eq!(listed["source"], "reflection v1", "{listed}");
        assert!(listed.to_string().contains("hitop.test.Greeter"), "{listed}");
        let described = c.post("/api/grpc/describe").header("Origin", "https://hitop.test").body_json(&body(json!({ "symbol": "hitop.test.Greeter/SayHello" }))).send();
        let described: Value = serde_json::from_slice(described.body_bytes()).unwrap();
        assert!(described["description"].as_str().unwrap().contains("SayHello") && described["template"].is_object(), "{described}");
        let mut call = c.post("/api/grpc/call").header("Origin", "https://hitop.test").body_json(&body(json!({ "service": "hitop.test.Greeter", "method": "SayHello", "message": "{\"name\":\"Ada\"}" }))).send();
        let mut text = String::new();
        call.take_stream().expect("a streamed body").read_to_string(&mut text).unwrap();
        let events: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["headers", "message", "status"], "{text}");
        assert_eq!((events[1]["data"]["message"].as_str(), events[1]["data"]["tenant"].as_str(), events[2]["name"].as_str()), (Some("Hello, Ada"), Some("acme"), Some("OK")), "{text}");
    }

    #[test]
    fn a_private_address_is_refused() {
        let c = client(false);
        let r = c.post("/api/grpc/list").header("Origin", "https://hitop.test").body_json(r#"{"address":"127.0.0.1:50051","plaintext":true}"#).send();
        assert_eq!(r.status(), 403);
        assert!(String::from_utf8_lossy(r.body_bytes()).contains("private"));
    }
}
