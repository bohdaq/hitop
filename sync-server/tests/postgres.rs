//! The same flows on Postgres (feature `postgres`). Runs when HITOP_SYNC_TEST_POSTGRES points at
//! a server (CI starts one), each test in a schema of its own; skipped otherwise.
#![cfg(feature = "postgres")]

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use serde_json::json;

const WS: &str = "00112233445566778899aabbccddeeff";

fn database(name: &str) -> Option<String> {
    let url = std::env::var("HITOP_SYNC_TEST_POSTGRES").ok()?;
    let schema = format!("t_{name}_{}", std::process::id());
    let mut admin = postgres::Client::connect(&url, postgres::NoTls).expect("HITOP_SYNC_TEST_POSTGRES");
    admin.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}")).unwrap();
    let sep = if url.contains('?') { '&' } else { '?' };
    Some(format!("{url}{sep}options=-c%20search_path%3D{schema}"))
}

#[test]
fn postgres_sync_flow() {
    let Some(url) = database("flow") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::with("pg-flow", |c| c.database = url.clone());
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let k1 = create_workspace(&s, &ada, WS);
    let wraps = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &k1);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "editor", "wraps": wraps }))).status, 201);
    let put = |d: &Device, version: u64, vv: serde_json::Value, epoch: u32, key: &crypto::WorkspaceKey, pt: &[u8]| {
        s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/a"), Some(&d.session), Some(json!({ "version": version, "vv": vv, "epoch": epoch, "content": seal(key, WS, "a", version, epoch, pt) })))
    };
    assert_eq!(put(&ada, 1, json!({ "a": 1 }), 1, &k1, b"one").status, 200);
    assert_eq!(put(&bob, 1, json!({ "b": 1 }), 1, &k1, b"other").status, 409);
    assert_eq!(put(&bob, 2, json!({ "a": 1, "b": 1 }), 1, &k1, b"two").status, 200);
    let feed = s.call("GET", &format!("/v1/workspaces/{WS}/changes?since=0"), Some(&ada.session), None);
    assert_eq!(feed.json["changes"][0]["version"], 2);
    assert!(feed.json["changes"][0]["size"].as_i64().unwrap() > 40);

    let k2 = crypto::WorkspaceKey::generate();
    let remaining = [(ada.key_id.clone(), ada.keys.public_key().to_vec())];
    let r = s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": wraps_for(&remaining, WS, 2, &k2) })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(put(&ada, 3, json!({ "a": 2, "b": 1 }), 1, &k1, b"stale").json["error"], "stale_epoch");
    assert_eq!(put(&ada, 3, json!({ "a": 2, "b": 1 }), 2, &k2, b"three").status, 200);
    let got = s.call("GET", &format!("/v1/workspaces/{WS}/blobs/a"), Some(&ada.session), None);
    assert_eq!(open(&ada.workspace_key(&s, WS, 2), WS, "a", 3, 2, got.json["content"].as_str().unwrap()), b"three");
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&bob.session), None).status, 404);
    assert_eq!(s.call("POST", "/v1/auth/logout", Some(&bob.session), None).status, 204);
}

#[test]
fn postgres_teams_flow() {
    let Some(url) = database("teams") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::with("pg-teams", |c| c.database = url.clone());
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let inv = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "viewer" })));
    assert_eq!(inv.status, 201, "{}", inv.json);
    let bob = Device::new(&s, "bob@example.com");
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&bob.session), Some(json!({ "token": inv.json["token"] }))).status, 200);
    let list = s.call("GET", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), None);
    assert_eq!(list.json["invites"][0]["accepted"]["email"], "bob@example.com");
    let w = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &key);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "viewer", "wraps": w }))).status, 201);
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), Some(json!({ "role": "editor" }))).status, 200);
    let kp = crypto::DeviceKeyPair::generate();
    let wrapped = crypto::wrap_key(&kp.public_key(), WS, 1, &key).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&ada.session), Some(json!({ "publicKey": b64(&kp.public_key()), "wrapped": b64(&wrapped) })));
    assert_eq!(r.status, 201, "{}", r.json);
    let token = r.json["token"].as_str().unwrap().to_string();
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&token), None).json["role"], "viewer");
    assert_eq!(s.call("POST", "/v1/workspaces", Some(&token), Some(json!({ "id": WS, "wraps": [] }))).status, 403);
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).status, 204);
    assert_eq!(s.call("GET", "/v1/me", Some(&token), None).status, 401);
}
