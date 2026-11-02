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
    // A format 2 wrap replaces Bob's format 1 wrap (upsert with length() on bytea), not the reverse.
    let legacy = |s: &Server| s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).json["members"].as_array().unwrap().iter().find(|m| m["email"] == "bob@example.com").unwrap()["keys"][0]["legacyEpochs"].clone();
    assert_eq!(legacy(&s), json!([1]));
    let v2 = crypto::seal_wrap(&crypto::Device { key_id: &ada.key_id, keys: &ada.keys }, &crypto::Recipient { key_id: &bob.key_id, public_key: &bob.keys.public_key() }, WS, 1, 1, &key).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&ada.session), Some(json!({ "epoch": 1, "wraps": [{ "keyId": bob.key_id, "wrapped": b64(&v2) }] })));
    assert_eq!(r.json["stored"], 1, "{}", r.json);
    assert_eq!(legacy(&s), json!([]));
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&ada.session), Some(json!({ "epoch": 1, "wraps": w }))).json["stored"], 0);
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

#[test]
fn postgres_audit_and_sso() {
    use common::idp::{self, MockIdp};
    use hitop_sync_server::audit;
    let Some(url) = database("audit") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let idp = MockIdp::start();
    let s = Server::with("pg-audit", |c| {
        c.database = url.clone();
        c.oidc = Some(idp.config());
        c.audit_retention_days = 30;
    });
    let ada = Device::new(&s, "ada@example.com");
    create_workspace(&s, &ada, WS);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "role": "viewer" }))).status, 201);
    // SSO links the existing account (oidc_identities on Postgres).
    let flow = sso_start(&s, Some("http://127.0.0.1:4000/"));
    idp.next(idp.sign(&idp.claims("sub-ada", "ada@example.com", &flow.nonce)), &flow.pkce);
    let code = sso_code(&sso_callback(&s, &format!("code={}&state={}", idp::GOOD_CODE, flow.state))).unwrap();
    let session = sso_finish(&s, &code, &flow.verifier).json["session"].as_str().unwrap().to_string();
    assert_eq!(s.call("GET", "/v1/me", Some(&session), None).json["id"], ada.user_id.as_str());
    let log = s.call("GET", &format!("/v1/workspaces/{WS}/audit?limit=1"), Some(&ada.session), None);
    assert_eq!((log.json["events"][0]["event"].as_str(), log.json["next"].as_i64()), (Some("invite_created"), Some(2)));
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/audit/verify"), Some(&ada.session), None).json["ok"], true);
    s.app.state().db.tx(|tx| tx.execute("UPDATE audit_log SET actor_email = 'x' WHERE workspace_id = $1 AND seq = 1", &[WS.into()])).unwrap();
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/audit/verify"), Some(&ada.session), None).json["ok"], false);
    s.advance(31 * 86_400);
    assert!(audit::purge(s.app.state()).unwrap() >= 2);
    assert!(audit::verify_all(s.app.state()).unwrap().iter().all(|(_, v)| v.problem.is_none()), "purged chains verify from their anchors");
}

/// Migration 3 → 4 of a T25-shaped database on Postgres (common/t25.rs, as tests/migration.rs).
#[test]
fn postgres_t25_database_migrates_to_orgs() {
    let Some(url) = database("t25") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    common::t25::build(&url);
    let before = common::t25::counts(&hitop_sync_server::Db::open_at(&url, 3).unwrap());
    let s = Server::with("pg-t25", |c| {
        c.database = url.clone();
        c.plan_limits = true;
    });
    common::t25::check(&s, &before);
}

/// Orgs, members, usage, limits, a refused create and access tokens on Postgres.
#[test]
fn postgres_orgs_limits_tokens() {
    let Some(url) = database("orgs") else {
        eprintln!("HITOP_SYNC_TEST_POSTGRES not set: skipped");
        return;
    };
    let s = Server::with("pg-orgs", |c| {
        c.database = url.clone();
        c.plan_limits = true;
    });
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let org = s.call("POST", "/v1/orgs", Some(&ada.session), Some(json!({ "name": "Acme" })));
    assert_eq!(org.status, 201, "{}", org.json);
    let id = org.json["id"].as_str().unwrap().to_string();
    assert_eq!(s.call("POST", &format!("/v1/orgs/{id}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "admin" }))).status, 201);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{id}/members"), Some(&bob.session), Some(json!({ "email": "new@example.com", "role": "member" }))).json["status"], "invited");
    for i in 0..3 {
        let ws = format!("{i:032x}");
        let key = crypto::WorkspaceKey::generate();
        let wraps = wraps_for(&[(ada.key_id.clone(), ada.keys.public_key().to_vec())], &ws, 1, &key);
        let r = s.call("POST", &format!("/v1/orgs/{id}/workspaces"), Some(&ada.session), Some(json!({ "id": ws, "wraps": wraps })));
        assert_eq!(r.status, 201, "{}", r.json);
        let put = s.call("PUT", &format!("/v1/workspaces/{ws}/blobs/a"), Some(&ada.session), Some(json!({ "version": 1, "vv": { "a": 1 }, "epoch": 1, "content": seal(&key, &ws, "a", 1, 1, b"one") })));
        assert_eq!(put.status, 200, "{}", put.json);
    }
    let key = crypto::WorkspaceKey::generate();
    let ws = format!("{:032x}", 9);
    let r = s.call("POST", "/v1/workspaces", Some(&ada.session), Some(json!({ "id": ws, "org": id, "wraps": wraps_for(&[(ada.key_id.clone(), ada.keys.public_key().to_vec())], &ws, 1, &key) })));
    assert_eq!((r.status, r.json["limit"].as_str()), (402, Some("workspaces")), "{}", r.json);
    let u = s.call("GET", &format!("/v1/orgs/{id}/usage"), Some(&bob.session), None);
    assert_eq!(u.status, 200, "{}", u.json);
    assert_eq!((u.json["usage"]["workspaces"].as_i64(), u.json["usage"]["seats"].as_i64(), u.json["usage"]["writesToday"].as_i64()), (Some(3), Some(3), Some(3)));
    assert!(u.json["usage"]["storageBytes"].as_i64().unwrap() > 0);
    let over = s.app.state().db.tx(|tx| hitop_sync_server::limits::over_limits(s.app.state(), tx)).unwrap();
    assert!(over.is_empty(), "{over:?}");
    hitop_sync_server::limits::set_plan(s.app.state(), &id, "team", 5).unwrap();
    assert_eq!(s.call("GET", &format!("/v1/orgs/{id}/limits"), Some(&ada.session), None).json["limits"]["seats"], 5);
    let t = s.call("POST", "/v1/me/tokens", Some(&ada.session), Some(json!({ "name": "ci", "scopes": ["read"], "expiresInDays": 30 })));
    assert_eq!(t.status, 201, "{}", t.json);
    let token = t.json["token"].as_str().unwrap();
    assert_eq!(s.call("GET", "/v1/workspaces", Some(token), None).json["workspaces"].as_array().unwrap().len(), 3);
    assert_eq!(s.call("DELETE", &format!("/v1/me/tokens/{}", t.json["id"].as_str().unwrap()), Some(&ada.session), None).status, 204);
    assert_eq!(s.call("GET", "/v1/workspaces", Some(token), None).status, 401);
}
