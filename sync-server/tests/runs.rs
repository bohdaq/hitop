//! HITOP Cloud C4: run history from runners (sealed reports + plaintext status rows), alerts, the
//! retention purge and plan limits.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use common::*;
use hitop_sync_crypto as crypto;
use serde_json::{json, Value};

const WS: &str = "5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c5c";
const REPORT: &[u8] = br#"{"reportVersion":1,"collection":"Shop API","requests":[{"name":"login","response":{"body":"the-response-body-secret"}}]}"#;

fn limited(name: &str) -> Server {
    Server::with(name, |c| {
        c.plan_limits = true;
        c.runs.insecure_local_webhooks = true;
    })
}

fn open_limits(name: &str) -> Server {
    Server::with(name, |c| c.runs.insecure_local_webhooks = true)
}

/// A runner credential for `ws` made by `owner`: its session token.
fn runner(s: &Server, owner: &Device, ws: &str, key: &crypto::WorkspaceKey) -> String {
    let keys = crypto::DeviceKeyPair::generate();
    let wrapped = crypto::wrap_key(&keys.public_key(), ws, 1, key).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{ws}/runners"), Some(&owner.session), Some(json!({ "name": "ci", "publicKey": b64(&keys.public_key()), "wrapped": b64(&wrapped) })));
    assert_eq!(r.status, 201, "{}", r.json);
    r.json["token"].as_str().unwrap().to_string()
}

fn add(s: &Server, owner: &Device, d: &Device, email: &str, role: &str, key: &crypto::WorkspaceKey) {
    let wraps = wraps_for(&[(d.key_id.clone(), d.keys.public_key().to_vec())], WS, 1, key);
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&owner.session), Some(json!({ "email": email, "role": role, "wraps": wraps })));
    assert_eq!(r.status, 201, "{}", r.json);
}

fn upload(s: &Server, token: &str, id: &str, status: &str, detail: Option<&[u8]>, key: &crypto::WorkspaceKey) -> Reply {
    let mut body = json!({
        "id": id, "startedAt": 1_800_000_000, "collection": "Shop API", "environment": "prod", "status": status,
        "durationMs": 1234, "requests": 3, "failed": if status == "failed" { 1 } else { 0 }, "failedNames": ["login"],
    });
    if let Some(d) = detail {
        body["detail"] = json!(b64(&crypto::seal_run(key, WS, id, 1, d).unwrap()));
    }
    s.call("POST", &format!("/v1/workspaces/{WS}/runs"), Some(token), Some(body))
}

#[test]
fn a_runner_uploads_a_sealed_report_that_only_members_open() {
    let s = open_limits("runs-sealed");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let bob = Device::new(&s, "bob@example.com");
    add(&s, &ada, &bob, "bob@example.com", "editor", &key);
    let ci = runner(&s, &ada, WS, &key);

    let r = upload(&s, &ci, "run-1", "passed", Some(REPORT), &key);
    assert_eq!(r.status, 201, "{}", r.json);
    assert_eq!(r.json["stored"], "status+report");
    assert_eq!(upload(&s, &ci, "run-1", "passed", None, &key).status, 409, "an id once");

    // Another member reads the history and opens the report on their own device.
    let list = s.call("GET", &format!("/v1/workspaces/{WS}/runs"), Some(&bob.session), None);
    assert_eq!(list.status, 200, "{}", list.json);
    let row = &list.json["runs"][0];
    assert_eq!((row["status"].as_str(), row["collection"].as_str(), row["requests"].as_i64(), row["hasReport"].as_bool(), row["runner"].as_bool()), (Some("passed"), Some("Shop API"), Some(3), Some(true), Some(true)));
    assert!(row.get("detail").is_none(), "lists carry no reports");
    assert_eq!(row["failedNames"], json!([]), "failed names only when the owners allow them");
    let one = s.call("GET", &format!("/v1/workspaces/{WS}/runs/run-1"), Some(&bob.session), None);
    let sealed = unb64(one.json["detail"].as_str().unwrap());
    let bob_key = bob.workspace_key(&s, WS, 1);
    assert_eq!(crypto::open_run(&bob_key, WS, "run-1", &sealed).unwrap(), REPORT);
    // The server keeps ciphertext only.
    let disk = s.disk_bytes();
    assert!(!disk.windows(b"the-response-body-secret".len()).any(|w| w == b"the-response-body-secret"));
    // The report's bytes count as the workspace's.
    let info = s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None);
    assert_eq!(info.json["bytes"].as_i64().unwrap(), sealed.len() as i64);
    assert_eq!(list.json["series"][0]["status"], "passed");
}

#[test]
fn a_runner_writes_runs_and_nothing_else() {
    let s = open_limits("runs-runner");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let ci = runner(&s, &ada, WS, &key);
    let blob = s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/b1"), Some(&ci), Some(json!({ "version": 1, "vv": { "r": 1 }, "epoch": 1, "content": seal(&key, WS, "b1", 1, 1, b"{}") })));
    assert_eq!(blob.status, 403, "no file writes: {}", blob.json);
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/run-policy"), Some(&ci), None).status, 200);
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/runs"), Some(&ci), None).status, 403, "runners don't read history");
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/alerts"), Some(&ci), None).status, 403);
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ci), Some(json!({}))).status, 403);
    let share = s.call("PUT", &format!("/v1/workspaces/{WS}/env-shares/{}", "a".repeat(32)), Some(&ci), Some(json!({ "version": 1, "sealed": b64(&[1u8; 60]), "wraps": [] })));
    assert!(share.status == 403 || share.status == 400, "{}", share.json);
    assert_eq!(upload(&s, &ci, "r1", "passed", None, &key).status, 201);
    // A viewer (a person) reads but doesn't upload.
    let vic = Device::new(&s, "vic@example.com");
    add(&s, &ada, &vic, "vic@example.com", "viewer", &key);
    assert_eq!(upload(&s, &vic.session, "r2", "passed", None, &key).status, 403);
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/runs"), Some(&vic.session), None).status, 200);
    // Someone outside the workspace sees nothing.
    let eve = Device::new(&s, "eve@example.com");
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/runs"), Some(&eve.session), None).status, 404);
    assert_eq!(upload(&s, &eve.session, "r3", "passed", None, &key).status, 404);
}

#[test]
fn uploads_are_checked() {
    let s = open_limits("runs-checks");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let post = |body: Value| s.call("POST", &format!("/v1/workspaces/{WS}/runs"), Some(&ada.session), Some(body));
    assert_eq!(post(json!({ "id": "runs/x", "startedAt": 1, "status": "passed" })).status, 400);
    assert_eq!(post(json!({ "id": "a", "startedAt": 1, "status": "maybe" })).status, 400);
    assert_eq!(post(json!({ "id": "a", "startedAt": 1 })).status, 400, "nothing to store");
    assert_eq!(post(json!({ "id": "a", "startedAt": 1, "status": "failed", "requests": 1, "failed": 2 })).status, 400);
    assert_eq!(post(json!({ "id": "a", "startedAt": 1, "detail": b64(b"not sealed") })).status, 400);
    // A report sealed for a future epoch.
    let future = crypto::seal_run(&key, WS, "b", 2, REPORT).unwrap();
    assert_eq!(post(json!({ "id": "b", "startedAt": 1, "detail": b64(&future) })).status, 409);
    let big = vec![1u8; (1 << 20) + 1];
    assert_eq!(post(json!({ "id": "c", "startedAt": 1, "detail": b64(&big) })).status, 413);
}

/// A local webhook receiver: every request body it got.
fn hook() -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/hook", listener.local_addr().unwrap().port());
    let got = Arc::new(Mutex::new(Vec::new()));
    let g = got.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut len = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            g.lock().unwrap().push(serde_json::from_slice(&body).unwrap_or(Value::Null));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok");
        }
    });
    (url, got)
}

fn alert_mails(s: &Server) -> Vec<String> {
    s.outbox.lock().unwrap().iter().filter(|m| m.subject.starts_with("HITOP: A scheduled run")).map(|m| m.body.clone()).collect()
}

#[test]
fn one_alert_per_status_change_with_no_bodies_or_values() {
    let s = open_limits("runs-alerts");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let ci = runner(&s, &ada, WS, &key);
    let (url, got) = hook();
    let (slack, slack_got) = hook();
    // Only members' addresses; https (or the test's local http) only.
    let put = |body: Value| s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(body));
    assert_eq!(put(json!({ "email": ["stranger@example.com"] })).status, 400, "not an open relay");
    assert_eq!(put(json!({ "webhooks": [{ "kind": "webhook", "url": "http://example.com/x" }] })).status, 400);
    assert_eq!(put(json!({ "webhooks": [{ "kind": "webhook", "url": "https://10.0.0.1/x" }] })).status, 400, "no private addresses");
    let r = put(json!({ "email": ["Ada@example.com"], "webhooks": [{ "kind": "webhook", "url": url }, { "kind": "slack", "url": slack }] }));
    assert_eq!(r.status, 200, "{}", r.json);
    let audit = audit_rows(&s, WS);
    let changed = audit.iter().find(|a| a["event"] == "alerts_changed").unwrap();
    assert!(!changed.to_string().contains("127.0.0.1"), "no webhook URL in the audit log");

    for (i, status) in ["passed", "failed", "failed", "passed", "passed"].iter().enumerate() {
        assert_eq!(upload(&s, &ci, &format!("r{i}"), status, Some(REPORT), &key).status, 201);
    }
    hitop_sync_server::alerts::deliver_pending(s.app.state()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let bodies = got.lock().unwrap().clone();
    let events: Vec<&str> = bodies.iter().map(|b| b["event"].as_str().unwrap()).collect();
    assert_eq!(events, ["failed", "recovered"], "one alert per change");
    assert_eq!(bodies[0]["collection"], "Shop API");
    assert_eq!(bodies[0]["environment"], "prod");
    assert_eq!(bodies[0]["failed"], 1);
    assert!(bodies[0]["link"].as_str().unwrap().contains("/runs/r1"));
    assert!(bodies[0].get("failedRequests").is_none(), "names only when allowed");
    for b in &bodies {
        assert!(!b.to_string().contains("the-response-body-secret"));
    }
    assert_eq!(slack_got.lock().unwrap().len(), 2);
    assert!(slack_got.lock().unwrap()[0]["text"].as_str().unwrap().contains("Shop API (prod) failed"));
    let mails = alert_mails(&s);
    assert_eq!(mails.len(), 2, "{mails:?}");
    let log = s.call("GET", &format!("/v1/workspaces/{WS}/alert-deliveries"), Some(&ada.session), None);
    assert_eq!(log.json["deliveries"].as_array().unwrap().len(), 6);
    assert!(!log.json.to_string().contains("127.0.0.1"), "the log names webhooks by position");

    // Owners allow failed request names: they go in the row and the webhook.
    let r = put(json!({ "email": [], "webhooks": [{ "kind": "webhook", "url": url }], "failedNames": true }));
    assert_eq!(r.status, 200);
    assert_eq!(upload(&s, &ci, "r9", "failed", None, &key).status, 201);
    hitop_sync_server::alerts::deliver_pending(s.app.state()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(got.lock().unwrap().last().unwrap()["failedRequests"], json!(["login"]));
    let one = s.call("GET", &format!("/v1/workspaces/{WS}/runs/r9"), Some(&ada.session), None);
    assert_eq!(one.json["failedNames"], json!(["login"]));
}

#[test]
fn a_missed_run_alerts_once_and_status_rows_can_be_off() {
    let s = open_limits("runs-missed");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let ci = runner(&s, &ada, WS, &key);
    let r = s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "email": ["ada@example.com"], "missedAfterMinutes": 60 })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(upload(&s, &ci, "r1", "passed", None, &key).status, 201);
    let tick = || hitop_sync_server::alerts::tick(s.app.state()).unwrap();
    assert_eq!(tick(), 0, "not late yet");
    s.advance(2 * 3600);
    assert_eq!(tick(), 1);
    assert_eq!(tick(), 0, "once per gap");
    s.advance(3600);
    assert_eq!(tick(), 0);
    hitop_sync_server::alerts::deliver_pending(s.app.state()).unwrap();
    assert_eq!(alert_mails(&s).len(), 1);
    assert!(alert_mails(&s)[0].contains("missed its scheduled run"));
    // A new run ends the gap; the next one alerts again.
    assert_eq!(upload(&s, &ci, "r2", "passed", None, &key).status, 201);
    s.advance(2 * 3600);
    assert_eq!(tick(), 1);

    // "History only, no alerts": the status is dropped, the report kept.
    let r = s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "statusRows": false })));
    assert_eq!(r.status, 200);
    let policy = s.call("GET", &format!("/v1/workspaces/{WS}/run-policy"), Some(&ci), None);
    assert_eq!(policy.json["statusRows"], false);
    let r = upload(&s, &ci, "r3", "failed", Some(REPORT), &key);
    assert_eq!((r.status, r.json["stored"].as_str()), (201, Some("report")));
    let row = s.call("GET", &format!("/v1/workspaces/{WS}/runs/r3"), Some(&ada.session), None);
    assert_eq!((row.json["status"].as_str(), row.json["requests"].as_i64()), (Some("sealed"), Some(0)));
}

#[test]
fn alerts_have_an_hourly_ceiling() {
    let s = Server::with("runs-ceiling", |c| c.runs.alerts_per_hour = 2);
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "email": ["ada@example.com"] }))).status, 200);
    for i in 0..6 {
        let status = if i % 2 == 0 { "failed" } else { "passed" };
        assert_eq!(upload(&s, &ada.session, &format!("r{i}"), status, None, &key).status, 201);
    }
    hitop_sync_server::alerts::deliver_pending(s.app.state()).unwrap();
    assert_eq!(alert_mails(&s).len(), 2, "a flapping series can't flood");
    let log = s.call("GET", &format!("/v1/workspaces/{WS}/alert-deliveries"), Some(&ada.session), None);
    assert!(log.json["deliveries"].as_array().unwrap().iter().any(|d| d["outcome"] == "suppressed"));
    // A removed member's address gets nothing even if queued before.
    let bob = Device::new(&s, "bob@example.com");
    add(&s, &ada, &bob, "bob@example.com", "editor", &key);
    s.advance(7200);
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "email": ["bob@example.com"] }))).status, 200);
    assert_eq!(upload(&s, &ada.session, "r10", "failed", None, &key).status, 201);
    let k2 = crypto::WorkspaceKey::generate();
    let remaining = [(ada.key_id.clone(), ada.keys.public_key().to_vec())];
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": wraps_for(&remaining, WS, 2, &k2) }))).status, 200);
    hitop_sync_server::alerts::deliver_pending(s.app.state()).unwrap();
    assert!(!s.outbox.lock().unwrap().iter().any(|m| m.to == "bob@example.com" && m.subject.starts_with("HITOP: A scheduled")));
}

#[test]
fn plans_limit_history_channels_and_uploads() {
    let s = limited("runs-plans");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let org = format!("personal-{}", ada.user_id);
    // Free: status rows only, email only, 100 runs a day, 7 days.
    let r = upload(&s, &ada.session, "f1", "passed", Some(REPORT), &key);
    assert_eq!((r.status, r.json["stored"].as_str()), (201, Some("status")));
    assert!(r.json["note"].as_str().unwrap().contains("status rows only"));
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).json["bytes"], 0);
    let hook = s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "webhooks": [{ "kind": "webhook", "url": "http://127.0.0.1:9/x" }] })));
    assert_eq!((hook.status, hook.json["error"].as_str(), hook.json["limit"].as_str()), (402, Some("limit"), Some("alertWebhooks")));
    let limits = s.call("GET", &format!("/v1/orgs/{org}/limits"), Some(&ada.session), None);
    assert_eq!(limits.json["runs"]["runsPerDay"], 100);
    assert_eq!(limits.json["runs"]["alertChannels"], json!(["email"]));
    for i in 2..=100 {
        assert_eq!(upload(&s, &ada.session, &format!("f{i}"), "passed", None, &key).status, 201);
    }
    let over = upload(&s, &ada.session, "f101", "passed", None, &key);
    assert_eq!((over.status, over.json["limit"].as_str(), over.json["max"].as_i64()), (402, Some("runsPerDay"), Some(100)));
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/usage"), Some(&ada.session), None).json["usage"]["runsToday"], 100);
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/runs?limit=500"), Some(&ada.session), None).json["runs"].as_array().unwrap().len(), 100, "reads never refused");
    s.advance(86_400);
    assert_eq!(upload(&s, &ada.session, "f101", "passed", None, &key).status, 201, "a new day");
    s.advance(7 * 86_400);
    assert_eq!(hitop_sync_server::runs::purge(s.app.state()).unwrap(), 100, "Free keeps 7 days");

    // Pro: the sealed report is kept (and counts as storage), webhooks yes, Slack no, 30 days.
    hitop_sync_server::limits::set_plan(s.app.state(), &org, "pro", 0).unwrap();
    let r = upload(&s, &ada.session, "p1", "failed", Some(REPORT), &key);
    assert_eq!(r.json["stored"], "status+report", "{}", r.json);
    assert!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).json["bytes"].as_i64().unwrap() > 0);
    let ok = s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "webhooks": [{ "kind": "webhook", "url": "http://127.0.0.1:9/x" }] })));
    assert_eq!(ok.status, 200, "{}", ok.json);
    let slack = s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "webhooks": [{ "kind": "slack", "url": "http://127.0.0.1:9/x" }] })));
    assert_eq!((slack.status, slack.json["limit"].as_str()), (402, Some("alertSlack")));
    s.advance(29 * 86_400);
    assert_eq!(hitop_sync_server::runs::purge(s.app.state()).unwrap(), 1, "f101 (Free's day) is past 30 days");
    s.advance(2 * 86_400);
    assert_eq!(hitop_sync_server::runs::purge(s.app.state()).unwrap(), 1);
    let bytes = s.app.state().db.tx(|tx| tx.query_one("SELECT bytes FROM workspaces WHERE id = $1", &[WS.into()])).unwrap().unwrap().int(0);
    assert_eq!(bytes, 0, "the purge frees the bytes");
    // (The session ran out with the clock: sign in again.)
    let ada = Device::new(&s, "ada@example.com");
    hitop_sync_server::limits::set_plan(s.app.state(), &org, "team", 2).unwrap();
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "webhooks": [{ "kind": "slack", "url": "http://127.0.0.1:9/x" }] }))).status, 200, "Team has Slack");
}

#[test]
fn self_hosted_servers_keep_history_for_the_configured_days() {
    let s = Server::with("runs-selfhost", |c| c.runs.retention_days = 3);
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let r = upload(&s, &ada.session, "a", "passed", Some(REPORT), &key);
    assert_eq!(r.json["stored"], "status+report", "no plans: everything");
    s.advance(2 * 86_400);
    assert_eq!(hitop_sync_server::runs::purge(s.app.state()).unwrap(), 0);
    s.advance(2 * 86_400);
    assert_eq!(hitop_sync_server::runs::purge(s.app.state()).unwrap(), 1);
}
