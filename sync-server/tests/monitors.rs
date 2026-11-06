//! Monitors (monitors.rs): a library collection run by the worker in a child process (the real
//! binary), its runs recorded, a mail when it starts failing and when it passes again; secret
//! values masked; private addresses refused unless the test hook allows them; the plan gate.

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use common::*;
use hitop_sync_server::hostkey::HostKey;
use serde_json::{json, Value};

/// A tiny API on 127.0.0.1: `/orders` answers 200 or 500 (`broken`).
fn api(broken: Arc<AtomicBool>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let (code, body) = if broken.load(Ordering::SeqCst) { ("500 Internal Server Error", "{\"error\":true}") } else { ("200 OK", "{\"orders\":[]}") };
            let _ = write!(s, "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    format!("http://{addr}")
}

fn server(name: &str, allow_private: bool, plans: bool) -> Server {
    Server::with(name, |c| {
        c.host_key = Some(HostKey::from_bytes([9; 32]));
        c.monitor_bin = Some(env!("CARGO_BIN_EXE_hitop-sync-server").into());
        c.monitor_allow_private = allow_private;
        c.plan_limits = plans;
    })
}

fn library(s: &Server, token: &str, base: &str) {
    let col = json!({ "id": 1, "name": "Shop API", "requests": [
        { "id": 11, "name": "List orders", "method": "GET", "url": "${base}/orders", "headers": [{ "name": "Authorization", "value": "Bearer ${token}" }], "body": "", "preRequestScript": "",
          "postRequestScript": "pm.test('status is 200 for ' + pm.environment.get('token'), () => pm.response.to.have.status(200));" }
    ] });
    assert_eq!(s.call("PUT", "/v1/library/collection/1", Some(token), Some(json!({ "data": col, "baseVersion": 0 }))).status, 200);
    let envs = json!({ "environments": [{ "id": "e1", "name": "staging", "variables": { "base": base, "token": "" }, "secrets": ["token"] }], "activeEnvironmentId": null });
    let r = s.call("PUT", "/v1/library/environments/all", Some(token), Some(json!({ "data": envs, "secrets": { "e1": { "token": "sekret-tok-42" } }, "baseVersion": 0 })));
    assert_eq!(r.status, 200, "{}", r.json);
}

fn run_due(s: &Server) -> usize {
    hitop_sync_server::monitors::run_due(s.app.state())
}

fn monitor(s: &Server, token: &str) -> Value {
    s.call("GET", "/v1/library/collection/1/monitor", Some(token), None).json
}

#[test]
fn a_monitor_runs_records_and_mails_on_changes() {
    let s = server("monitors", true, false);
    let ada = s.sign_in("ada@example.com");
    let broken = Arc::new(AtomicBool::new(false));
    library(&s, &ada, &api(broken.clone()));
    assert_eq!(s.call("PUT", "/v1/library/collection/1/monitor", Some(&ada), Some(json!({ "environment": "staging", "everyMinutes": 7 }))).status, 400, "only the listed intervals");
    let on = s.call("PUT", "/v1/library/collection/1/monitor", Some(&ada), Some(json!({ "environment": "staging", "everyMinutes": 5 })));
    assert_eq!((on.status, on.json["emails"].clone()), (200, json!(["ada@example.com"])), "{}", on.json);

    let mails = |s: &Server| s.outbox.lock().unwrap().iter().filter(|m| m.subject.starts_with("HITOP monitor")).count();
    assert_eq!(run_due(&s), 1);
    let m = monitor(&s, &ada);
    assert_eq!((m["lastStatus"].as_str(), m["runs"][0]["passed"].as_bool(), m["runs"][0]["requests"].as_i64()), (Some("passed"), Some(true), Some(1)), "{m}");
    assert_eq!(mails(&s), 0, "no mail while it passes");
    assert_eq!(run_due(&s), 0, "not due again for 5 minutes");

    // It breaks: one mail, the secret masked in what's kept and sent.
    broken.store(true, Ordering::SeqCst);
    s.advance(301);
    assert_eq!(run_due(&s), 1);
    let m = monitor(&s, &ada);
    assert_eq!(m["lastStatus"], "failed", "{m}");
    let failure = m["runs"][0]["failures"][0].clone();
    assert_eq!(failure["status"], 500, "{failure}");
    assert!(!m.to_string().contains("sekret-tok-42") && failure["tests"][0].as_str().unwrap().contains("••••"), "{failure}");
    assert_eq!(mails(&s), 1);
    let mail = s.outbox.lock().unwrap().iter().rev().find(|m| m.subject.starts_with("HITOP monitor")).map(|m| (m.subject.clone(), m.body.clone())).unwrap();
    assert!(mail.0.contains("Shop API is failing") && mail.1.contains("status 500") && !mail.1.contains("sekret-tok-42"), "{mail:?}");
    s.advance(301);
    run_due(&s);
    assert_eq!(mails(&s), 1, "no second mail while it keeps failing");

    // Fixed: a mail that it passes again. Run now works once a minute.
    broken.store(false, Ordering::SeqCst);
    s.advance(61);
    assert_eq!(s.call("POST", "/v1/library/collection/1/monitor/run", Some(&ada), None).status, 202);
    assert_eq!(s.call("POST", "/v1/library/collection/1/monitor/run", Some(&ada), None).status, 202, "queued again is fine before a run");
    assert_eq!(run_due(&s), 1);
    assert_eq!(s.call("POST", "/v1/library/collection/1/monitor/run", Some(&ada), None).status, 429);
    assert_eq!(mails(&s), 2);
    assert_eq!(monitor(&s, &ada)["runs"].as_array().unwrap().len(), 4);

    assert_eq!(s.call("DELETE", "/v1/library/collection/1/monitor", Some(&ada), None).status, 200);
    assert_eq!(s.call("GET", "/v1/library/collection/1/monitor", Some(&ada), None).status, 404);
}

#[test]
fn a_monitor_never_reaches_private_addresses() {
    let s = server("monitors-guard", false, false);
    let ada = s.sign_in("ada@example.com");
    library(&s, &ada, &api(Arc::new(AtomicBool::new(false))));
    s.call("PUT", "/v1/library/collection/1/monitor", Some(&ada), Some(json!({ "environment": "staging", "everyMinutes": 5 })));
    assert_eq!(run_due(&s), 1);
    let m = monitor(&s, &ada);
    let error = m["runs"][0]["failures"][0]["error"].as_str().unwrap_or_default().to_string();
    assert!(m["lastStatus"] == "failed" && error.contains("private or reserved"), "{m}");
}

#[test]
fn monitors_need_a_paid_plan_when_plans_are_on() {
    let s = server("monitors-plans", true, true);
    let ada = s.sign_in("ada@example.com");
    library(&s, &ada, "http://127.0.0.1:9");
    let r = s.call("PUT", "/v1/library/collection/1/monitor", Some(&ada), Some(json!({ "everyMinutes": 60 })));
    assert_eq!((r.status, r.json["error"].as_str()), (402, Some("limit")), "{}", r.json);
}
