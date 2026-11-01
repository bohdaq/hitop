//! `hitop ws` against a local WebSocket echo server: messages from flags and
//! from stdin, --count, --wait, --json, a saved request from a collection
//! folder (headers, auth, subprotocols, saved messages, secrets masked), and
//! `hitop run` / `requests send` leaving WebSocket requests alone.
#![allow(clippy::result_large_err)] // tungstenite's handshake callback type

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use assert_cmd::Command;
use futures_util::{SinkExt, StreamExt};
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;
use tokio_tungstenite::tungstenite::{
    handshake::server::{Request, Response},
    http::HeaderValue,
    protocol::{frame::coding::CloseCode, CloseFrame},
    Message,
};

fn hitop(home: &TempDir, cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path()).env("XDG_CONFIG_HOME", home.path().join("config")).env("HITOP_KEYCHAIN", "off").current_dir(cwd);
    c
}

type Seen = Arc<Mutex<Vec<Vec<(String, String)>>>>;

/// An echo server in its own thread: echoes text and binary, answers the
/// text `bin` with bytes 0x00 0xff, closes with 4001 "bye" on `close`,
/// stays silent on `quiet`; records each handshake's headers and picks the
/// first subprotocol offered.
fn echo_server() -> (SocketAddr, Seen) {
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

fn header(seen: &Seen, i: usize, name: &str) -> Option<String> {
    seen.lock().unwrap()[i].iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

#[test]
fn messages_from_flags_until_count() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let (addr, seen) = echo_server();
    let out = hitop(&home, tmp.path())
        .args(["ws", &format!("ws://{addr}/echo"), "--send", "one", "-s", "x=${x}", "-s", "bin", "--count", "3", "--var", "x=7", "-H", "X-Trace: t1", "--protocol", "p2", "--protocol", "p1"])
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert_eq!(stdout, "one\nx=7\n[binary, 2 bytes] 00ff\n");
    let stderr = String::from_utf8(out.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("connected to ws://") && stderr.contains("subprotocol p2") && stderr.contains("> x=7"), "{stderr}");
    assert_eq!(header(&seen, 0, "x-trace").as_deref(), Some("t1"));
    assert_eq!(header(&seen, 0, "sec-websocket-protocol").as_deref(), Some("p2, p1"));
}

#[test]
fn stdin_lines_and_json_lines() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let (addr, _) = echo_server();
    let out = hitop(&home, tmp.path()).args(["ws", &format!("ws://{addr}"), "--json", "--wait", "500"]).write_stdin("hello\n\nworld\nbin\n").assert().success();
    let lines: Vec<Value> = String::from_utf8(out.get_output().stdout.clone()).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let events: Vec<&str> = lines.iter().map(|l| l["event"].as_str().unwrap()).collect();
    assert_eq!(events.first(), Some(&"open"));
    assert_eq!(events.last(), Some(&"closed"));
    let sent: Vec<&str> = lines.iter().filter(|l| l["event"] == "sent").map(|l| l["data"].as_str().unwrap()).collect();
    assert_eq!(sent, ["hello", "world", "bin"], "the empty line isn't sent");
    let got: Vec<String> = lines.iter().filter(|l| l["event"] == "received").map(|l| l["data"].as_str().or(l["binary"].as_str()).unwrap().to_string()).collect();
    assert_eq!(got, ["hello", "world", "AP8="]);
    assert_eq!(lines[0]["status"], 101);
    assert!(lines.iter().all(|l| l["time"].as_str().is_some_and(|t| t.len() == 24 && t.ends_with('Z'))), "{lines:?}");
}

#[test]
fn count_not_reached_and_server_close_and_refused() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let (addr, _) = echo_server();
    let url = format!("ws://{addr}");
    // One message back where two were asked for: exit 1 once --wait runs out.
    hitop(&home, tmp.path()).args(["ws", &url, "-s", "a", "-s", "quiet", "-n", "2", "--wait", "300"]).assert().code(1).stdout("a\n").stderr(contains("1 of 2 messages"));
    // The server closes: code and reason shown, exit 0 without --count.
    hitop(&home, tmp.path()).args(["ws", &url, "-s", "close"]).assert().success().stderr(contains("closed 4001 bye"));
    // Nothing listens on port 9.
    hitop(&home, tmp.path()).args(["ws", "ws://127.0.0.1:9", "-s", "x"]).assert().code(1).stderr(contains("couldn't connect"));
    hitop(&home, tmp.path()).args(["ws", "ftp://x", "-s", "x"]).assert().code(1).stderr(contains("not a WebSocket URL"));
    hitop(&home, tmp.path()).args(["ws", "Feed"]).assert().code(1).stderr(contains("not a ws:// or wss:// URL"));
}

/// `api/`: a WebSocket request using a secret token and variables, plus an
/// HTTP request; environment `prod` has the secret.
fn folder(tmp: &TempDir, addr: SocketAddr) -> PathBuf {
    let dir = tmp.path().join("api");
    fs::create_dir_all(dir.join("requests")).unwrap();
    fs::create_dir_all(dir.join("environments")).unwrap();
    fs::write(dir.join("hitop.yaml"), format!("format: 1\nname: Sockets\nvariables:\n  wsBase: ws://{addr}\n  room: lobby\n")).unwrap();
    fs::write(
        dir.join("requests/feed.yaml"),
        "name: Feed\nkind: websocket\nurl: ${wsBase}/feed\nparams:\n- name: room\n  value: ${room}\nheaders:\n- name: X-Room\n  value: ${room}\nauth:\n  type: bearer\n  token: ${token}\nwebsocket:\n  subprotocols:\n  - feed.v2\n  messages:\n  - name: join\n    content: '{\"join\": \"${room}\", \"token\": \"${token}\"}'\n  - name: leave\n    content: leave ${room}\n",
    )
    .unwrap();
    fs::write(dir.join("requests/health.yaml"), "name: Health\nurl: http://127.0.0.1:9/health\n").unwrap();
    fs::write(dir.join("environments/prod.yaml"), "name: prod\nvariables:\n  token:\n    secret: true\n").unwrap();
    dir
}

#[test]
fn a_saved_request_with_auth_messages_and_a_masked_secret() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let (addr, seen) = echo_server();
    let dir = folder(&tmp, addr);
    let out = hitop(&home, tmp.path())
        .env("HITOP_SECRET_PROD_TOKEN", "s3cr3t-tok")
        .args(["ws", "Feed", "-c", dir.to_str().unwrap(), "--env", "prod", "-m", "join", "-m", "leave", "-s", "${room}!", "-n", "3", "--json"])
        .assert()
        .success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(!stdout.contains("s3cr3t") && !String::from_utf8_lossy(&out.get_output().stderr).contains("s3cr3t"), "{stdout}");
    let got: Vec<String> = stdout.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).filter(|l| l["event"] == "received").map(|l| l["data"].as_str().unwrap().to_string()).collect();
    assert_eq!(got, ["{\"join\": \"lobby\", \"token\": \"••••\"}", "leave lobby", "lobby!"]);
    assert!(stdout.contains("\"protocol\":\"feed.v2\""), "{stdout}");
    // What the server got: the real values.
    assert_eq!(header(&seen, 0, "authorization").as_deref(), Some("Bearer s3cr3t-tok"));
    assert_eq!(header(&seen, 0, "x-room").as_deref(), Some("lobby"));

    // An unknown message name, and an HTTP request, are errors.
    hitop(&home, tmp.path()).env("HITOP_SECRET_PROD_TOKEN", "t").args(["ws", "Feed", "-c", dir.to_str().unwrap(), "--env", "prod", "-m", "nope"]).assert().code(1).stderr(contains("saved: join, leave"));
    hitop(&home, tmp.path()).args(["ws", "Health", "-c", dir.to_str().unwrap()]).assert().code(1).stderr(contains("hitop requests send"));
}

#[test]
fn run_skips_and_requests_send_refuses_websocket_requests() {
    let (home, tmp) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let (addr, seen) = echo_server();
    let dir = folder(&tmp, addr);
    fs::remove_file(dir.join("requests/health.yaml")).unwrap();
    hitop(&home, tmp.path()).env("HITOP_SECRET_PROD_TOKEN", "t").args(["run", dir.to_str().unwrap(), "--env", "prod"]).assert().success();
    assert!(seen.lock().unwrap().is_empty(), "a run opens no connection");
    hitop(&home, tmp.path()).args(["requests", "send", dir.to_str().unwrap(), "Feed"]).assert().failure().stderr(contains("hitop ws"));
    hitop(&home, tmp.path()).args(["requests", "list", dir.to_str().unwrap()]).assert().success().stdout(contains("WS"));
}
