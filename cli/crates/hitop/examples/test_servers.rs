//! Local servers for the HITOP iOS app's end-to-end tests (ios/HITOP/UnitTests/ProtocolEndToEndTests.swift,
//! CI job `core` in .github/workflows/ios.yml): the CLI's WebSocket echo server (tests/support/ws_echo.rs),
//! hitop-core's gRPC test server (Greeter, reflection v1, v1alpha and none) and a small HTTP server
//! with Server-Sent Events. Prints one JSON line with the addresses, then serves until killed.
//!
//!   cargo run -p hitop --example test_servers
//!
//! HTTP paths: `/events` (three events, then the stream ends), `/live` (an event every 200 ms until
//! the client goes away), `/handshakes` (the WebSocket handshakes' headers so far, as JSON).
#[path = "../tests/support/ws_echo.rs"]
mod ws_echo;
#[path = "../../hitop-core/tests/support/grpc_server.rs"]
mod grpc_server;

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

fn respond(mut tcp: TcpStream, seen: &ws_echo::Seen) {
    let mut line = String::new();
    let mut reader = BufReader::new(match tcp.try_clone() {
        Ok(t) => t,
        Err(_) => return,
    });
    if reader.read_line(&mut line).is_err() {
        return;
    }
    // The rest of the request head.
    loop {
        let mut h = String::new();
        match reader.read_line(&mut h) {
            Ok(0) | Err(_) => break,
            Ok(_) if h == "\r\n" || h == "\n" => break,
            Ok(_) => {}
        }
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let stream_head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    match path.as_str() {
        "/events" => {
            let _ = tcp.write_all(stream_head.as_bytes());
            let events = [
                ": a comment line\nevent: price\nid: 1\ndata: {\"sku\":\"TEE-0193\",\"price\":2190}\n\n",
                "data: line one\ndata: line two\n\n",
                "event: done\nid: 3\ndata: bye\n\n",
            ];
            for e in events {
                // Split across writes, so the client sees an event arrive in pieces.
                let (a, b) = e.split_at(e.len() / 2);
                let _ = tcp.write_all(a.as_bytes());
                let _ = tcp.flush();
                std::thread::sleep(Duration::from_millis(30));
                let _ = tcp.write_all(b.as_bytes());
                let _ = tcp.flush();
                std::thread::sleep(Duration::from_millis(30));
            }
        }
        "/live" => {
            let _ = tcp.write_all(stream_head.as_bytes());
            for i in 0.. {
                if tcp.write_all(format!("event: tick\nid: {i}\ndata: {i}\n\n").as_bytes()).is_err() || tcp.flush().is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        "/handshakes" => {
            let all: Vec<Vec<(String, String)>> = seen.lock().map(|g| g.clone()).unwrap_or_default();
            let body = serde_json::to_string(&all).unwrap_or_else(|_| "[]".into());
            let _ = tcp.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
        _ => {
            let _ = tcp.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    }
}

fn main() {
    let (ws, seen) = ws_echo::echo_server();
    let grpc_v1 = grpc_server::start(grpc_server::Reflect::V1);
    let grpc_v1alpha = grpc_server::start(grpc_server::Reflect::V1Alpha);
    let grpc_none = grpc_server::start(grpc_server::Reflect::None);
    let http = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let addr = http.local_addr().expect("its address");
    println!(
        "{}",
        serde_json::json!({ "ws": ws.to_string(), "grpc": grpc_v1.to_string(), "grpcV1alpha": grpc_v1alpha.to_string(), "grpcNone": grpc_none.to_string(), "http": addr.to_string() })
    );
    let _ = std::io::stdout().flush();
    for tcp in http.incoming().flatten() {
        let seen = seen.clone();
        std::thread::spawn(move || respond(tcp, &seen));
    }
}
