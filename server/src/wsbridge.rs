//! `GET /api/ws` (WebSocket): the web app's WebSocket requests with handshake headers, which a
//! browser can't send. The page opens this socket and sends one JSON text frame,
//! `{"url","headers":[[name,value]...],"protocols":[...]}`; the server checks it as the HTTP proxy
//! checks a request (allowed Origin, ws/wss, port, public address), resolves the host, connects to
//! a checked address itself (so DNS can't point it elsewhere afterwards), and answers
//! `{"type":"open","protocol"}` or `{"type":"error","message"}`. Then frames go both ways as they
//! are; either side's close ends both. At most `MAX_BRIDGES` at once, each for `BRIDGE_FOR`.
//! Nothing is logged.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use hitop_core::websocket::{self, Data, Event, Target};
use rust_web_server::websocket::{Message, WebSocketConnection};
use serde::Deserialize;
use serde_json::json;

use crate::guard::{ip_allowed, port_allowed};

pub const MAX_BRIDGES: usize = 16;
pub const BRIDGE_FOR: Duration = Duration::from_secs(10 * 60);
const MAX_MESSAGE: usize = 1024 * 1024;

#[derive(Deserialize)]
struct Open {
    url: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    protocols: Vec<String>,
}

/// Connection management is the bridge's business, not the user's.
const DROPPED: &[&str] = &["host", "connection", "upgrade", "sec-websocket-key", "sec-websocket-version", "sec-websocket-extensions", "content-length", "transfer-encoding", "x-hitop-proxy"];

#[derive(Clone)]
pub struct Bridge {
    origins: Arc<Vec<String>>,
    deny: Arc<Vec<IpAddr>>,
    open: Arc<AtomicUsize>,
    /// Tests only: private addresses allowed.
    allow_private: bool,
}

impl Bridge {
    pub fn new(origins: Vec<String>, deny: Vec<IpAddr>, allow_private: bool) -> Self {
        Bridge { origins: Arc::new(origins), deny: Arc::new(deny), open: Arc::new(AtomicUsize::new(0)), allow_private }
    }

    pub fn from_env() -> Self {
        let list = |name: &str, default: &str| -> Vec<String> { std::env::var(name).unwrap_or_else(|_| default.into()).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect() };
        let deny = list("HITOP_DENY_IPS", "").iter().filter_map(|s| s.parse().ok()).collect();
        Bridge::new(list("HITOP_ALLOWED_ORIGINS", "https://hitop.rws8.tech"), deny, std::env::var("HITOP_BRIDGE_ALLOW_PRIVATE").as_deref() == Ok("1"))
    }

    /// `GET /api/ws`: the upgrade, for the web app's Origin only.
    pub fn handle(&self, request: &rust_web_server::request::Request) -> rust_web_server::response::Response {
        let origin = request.header("origin").map(|h| h.value.to_string()).unwrap_or_default();
        if !self.origins.iter().any(|o| *o == origin) {
            return rust_web_server::response::Response::text(rust_web_server::response::STATUS_CODE_REASON_PHRASE.n403_forbidden, "connections must come from the HITOP web app");
        }
        let bridge = self.clone();
        rust_web_server::websocket::WebSocket::upgrade(request, move |ws| bridge.serve(ws))
    }

    /// The checked address to connect to for `url`, or why not.
    fn address(&self, url: &str) -> Result<(String, SocketAddr), String> {
        let ws = websocket::ws_url(url).map_err(|e| format!("{e:#}"))?;
        let u = url::Url::parse(&ws).map_err(|e| format!("invalid URL: {e}"))?;
        if !u.username().is_empty() || u.password().is_some() {
            return Err("put credentials in a header, not the URL".into());
        }
        let port = u.port_or_known_default().unwrap_or(0);
        if !self.allow_private && !port_allowed(port) {
            return Err(format!("port {port} is blocked"));
        }
        let host = u.host_str().ok_or("the URL has no host")?.trim_matches(['[', ']']).to_string();
        let addrs: Vec<SocketAddr> = (host.as_str(), port).to_socket_addrs().map_err(|e| format!("couldn't resolve {host}: {e}"))?.collect();
        let addr = addrs.iter().find(|a| self.allow_private || ip_allowed(a.ip(), &self.deny)).copied();
        match addr {
            Some(a) if self.allow_private || addrs.iter().all(|a| ip_allowed(a.ip(), &self.deny)) => Ok((ws, a)),
            _ => Err(format!("{host} is a private or reserved address")),
        }
    }

    /// Runs one bridged connection to its end.
    pub fn serve(&self, mut ws: WebSocketConnection) {
        if self.open.fetch_add(1, Ordering::SeqCst) >= MAX_BRIDGES {
            self.open.fetch_sub(1, Ordering::SeqCst);
            let _ = ws.send_text(&json!({ "type": "error", "message": "the WebSocket bridge is busy: try again in a minute" }).to_string());
            let _ = ws.close(1013, "busy");
            return;
        }
        self.run(&mut ws);
        self.open.fetch_sub(1, Ordering::SeqCst);
    }

    fn run(&self, ws: &mut WebSocketConnection) {
        let fail = |ws: &mut WebSocketConnection, message: &str| {
            let _ = ws.send_text(&json!({ "type": "error", "message": message }).to_string());
            let _ = ws.close(1008, "refused");
        };
        let open: Open = match ws.recv() {
            Ok(Some(Message::Text(t))) => match serde_json::from_str(&t) {
                Ok(o) => o,
                Err(e) => return fail(ws, &format!("the first message must say where to connect: {e}")),
            },
            _ => return,
        };
        let (url, addr) = match self.address(&open.url) {
            Ok(x) => x,
            Err(e) => return fail(ws, &e),
        };
        let headers = open.headers.into_iter().filter(|(k, _)| !k.trim().is_empty() && !DROPPED.contains(&k.trim().to_ascii_lowercase().as_str())).collect();
        let target = Target { url, headers, subprotocols: open.protocols, connect_to: Some(addr) };
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(r) => r,
            Err(e) => return fail(ws, &format!("internal error: {e}")),
        };
        let conn = match runtime.block_on(websocket::connect(&target, Some(Duration::from_secs(10)))) {
            Ok(c) => c,
            Err(e) => return fail(ws, &format!("{e:#}")),
        };
        if ws.send_text(&json!({ "type": "open", "protocol": conn.opened.protocol }).to_string()).is_err() {
            conn.sender.close();
            return;
        }
        // Upstream → page on a thread of its own, until upstream ends or the time is up.
        let to_page = ws.sender();
        let mut events = conn.events;
        let pump = std::thread::spawn(move || {
            runtime.block_on(async move {
                let ended = tokio::time::timeout(BRIDGE_FOR, async {
                    while let Some(e) = events.recv().await {
                        match e {
                            Event::Message(Data::Text(t)) => {
                                if to_page.send_text(&t).is_err() {
                                    return;
                                }
                            }
                            Event::Message(Data::Binary(b)) => {
                                if to_page.send_binary(&b).is_err() {
                                    return;
                                }
                            }
                            Event::Closed { code, reason } => {
                                let _ = to_page.close(code.unwrap_or(1000), &reason);
                                return;
                            }
                            Event::Error(m) => {
                                let _ = to_page.close(1011, &m.chars().take(120).collect::<String>());
                                return;
                            }
                        }
                    }
                })
                .await;
                if ended.is_err() {
                    let _ = to_page.close(1000, "time limit");
                }
            });
        });
        // Page → upstream on this thread.
        let upstream = conn.sender;
        while let Ok(Some(m)) = ws.recv() {
            let data = match m {
                Message::Text(t) if t.len() <= MAX_MESSAGE => Data::Text(t),
                Message::Binary(b) if b.len() <= MAX_MESSAGE => Data::Binary(b),
                _ => break,
            };
            if upstream.send(data).is_err() {
                break;
            }
        }
        upstream.close();
        let _ = pump.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_web_server::app::App;
    use rust_web_server::test_client::TestClient;
    use std::sync::Mutex;

    /// An upstream WebSocket server on 127.0.0.1 that echoes, after a first message naming the
    /// `x-client` header its handshake had.
    fn echo() -> (SocketAddr, Arc<Mutex<Option<String>>>) {
        let seen = Arc::new(Mutex::new(None));
        let std_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = std_listener.local_addr().unwrap();
        std_listener.set_nonblocking(true).unwrap();
        let record = seen.clone();
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
                use futures_util::{SinkExt, StreamExt};
                let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
                let (stream, _) = listener.accept().await.unwrap();
                let cb = |req: &tokio_tungstenite::tungstenite::handshake::server::Request, resp| {
                    *record.lock().unwrap() = req.headers().get("x-client").and_then(|v| v.to_str().ok()).map(String::from);
                    Ok(resp)
                };
                let mut ws = tokio_tungstenite::accept_hdr_async(stream, cb).await.unwrap();
                while let Some(Ok(m)) = ws.next().await {
                    if m.is_text() || m.is_binary() {
                        ws.send(m).await.unwrap();
                    } else if m.is_close() {
                        break;
                    }
                }
            });
        });
        (addr, seen)
    }

    fn app(bridge: Bridge) -> TestClient<rust_web_server::state::AppWithState<Bridge>> {
        TestClient::new(App::with_state(bridge).get("/api/ws", |request, _, _, b| b.handle(request)))
    }

    #[test]
    fn the_bridge_sends_headers_and_relays_both_ways() {
        let (addr, seen) = echo();
        let client = app(Bridge::new(vec!["https://hitop.test".into()], vec![], true));
        assert_eq!(client.websocket("/api/ws").connect().err().expect("refused").status(), 403, "another origin is refused");
        let mut ws = client.websocket("/api/ws").header("Origin", "https://hitop.test").connect().expect("a 101");
        ws.send_text(&json!({ "url": format!("ws://{addr}/feed"), "headers": [["X-Client", "hitop-ada"], ["Host", "evil"]] }).to_string());
        let opened: serde_json::Value = serde_json::from_str(&ws.recv_text()).unwrap();
        assert_eq!(opened["type"], "open", "{opened}");
        assert_eq!(seen.lock().unwrap().as_deref(), Some("hitop-ada"), "the handshake carried the header");
        ws.send_text("ping 1");
        assert_eq!(ws.recv_text(), "ping 1");
    }

    #[test]
    fn a_private_address_is_refused_before_connecting() {
        let client = app(Bridge::new(vec!["https://hitop.test".into()], vec![], false));
        let mut ws = client.websocket("/api/ws").header("Origin", "https://hitop.test").connect().expect("a 101");
        ws.send_text(r#"{"url":"ws://127.0.0.1:8080/"}"#);
        let answer: serde_json::Value = serde_json::from_str(&ws.recv_text()).unwrap();
        assert!(answer["type"] == "error" && answer["message"].as_str().unwrap().contains("private"), "{answer}");
    }

    #[test]
    fn addresses() {
        let b = Bridge::new(vec![], vec![], false);
        assert!(b.address("ws://127.0.0.1:9000/").unwrap_err().contains("private"));
        assert!(b.address("wss://localhost/").unwrap_err().contains("private"));
        assert!(b.address("ws://[::1]:8080/").unwrap_err().contains("private"));
        assert!(b.address("ws://example.com:25/").unwrap_err().contains("blocked"));
        assert!(b.address("ftp://example.com/").is_err());
        let open = Bridge::new(vec![], vec![], true);
        assert_eq!(open.address("ws://127.0.0.1:9000/x").unwrap().1, "127.0.0.1:9000".parse().unwrap());
    }
}
