//! A local WebSocket echo server for tests/ws.rs and tests/mcp.rs.
#![allow(dead_code, clippy::result_large_err)] // tungstenite's handshake callback type

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{
    handshake::server::{Request, Response},
    http::HeaderValue,
    protocol::{frame::coding::CloseCode, CloseFrame},
    Message,
};

pub type Seen = Arc<Mutex<Vec<Vec<(String, String)>>>>;

/// An echo server in its own thread: echoes text and binary, answers the
/// text `bin` with bytes 0x00 0xff, closes with 4001 "bye" on `close`,
/// stays silent on `quiet`; records each handshake's headers and picks the
/// first subprotocol offered.
pub fn echo_server() -> (SocketAddr, Seen) {
    let seen: Seen = Arc::default();
    let log = seen.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(listener.local_addr().unwrap()).unwrap();
            while let Ok((tcp, _)) = listener.accept().await {
                let log = log.clone();
                tokio::spawn(async move {
                    let callback = |req: &Request, mut resp: Response| {
                        log.lock().unwrap().push(req.headers().iter().map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string())).collect());
                        if let Some(p) = req.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok()) {
                            let first = p.split(',').next().unwrap().trim().to_string();
                            resp.headers_mut().insert("sec-websocket-protocol", HeaderValue::from_str(&first).unwrap());
                        }
                        Ok(resp)
                    };
                    let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else { return };
                    while let Some(Ok(msg)) = ws.next().await {
                        let reply = match &msg {
                            Message::Text(t) if t.as_str() == "close" => {
                                let _ = ws.close(Some(CloseFrame { code: CloseCode::from(4001), reason: "bye".into() })).await;
                                continue;
                            }
                            Message::Text(t) if t.as_str() == "quiet" => continue,
                            Message::Text(t) if t.as_str() == "bin" => Message::binary(vec![0u8, 255]),
                            Message::Text(_) | Message::Binary(_) => msg,
                            _ => continue,
                        };
                        let _ = ws.send(reply).await;
                    }
                });
            }
        });
    });
    (rx.recv().unwrap(), seen)
}

pub fn header(seen: &Seen, i: usize, name: &str) -> Option<String> {
    seen.lock().unwrap()[i].iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}
