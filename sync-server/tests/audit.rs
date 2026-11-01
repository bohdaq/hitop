//! The audit log: every event type is written, only owners read a workspace's log, the hash
//! chain catches edited and deleted rows, retention purges keep the chain verifiable, and the
//! log holds no secrets.

mod common;

use std::collections::BTreeSet;

use common::idp::{self, MockIdp};
use common::*;
use hitop_sync_server::audit;
use serde_json::json;
use url::Url;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_rs::prelude::CreationChallengeResponse;

const WS: &str = "00112233445566778899aabbccddeeff";

/// An owner, an editor added by invite, a runner; then most of what an owner can do.
struct Team {
    ada: Device,
    bob: Device,
    key: hitop_sync_crypto::WorkspaceKey,
    secrets: Vec<String>,
}

fn team(s: &Server) -> Team {
    let ada = Device::new(s, "ada@example.com");
    let key = create_workspace(s, &ada, WS);
    let mut secrets = vec![ada.session.clone()];
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "editor" })));
    assert_eq!(r.status, 201, "{}", r.json);
    secrets.push(r.json["token"].as_str().unwrap().into());
    let bob = Device::new(s, "bob@example.com");
    secrets.push(bob.session.clone());
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&bob.session), Some(json!({ "token": secrets[1] }))).status, 200);
    let w = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &key);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "editor", "wraps": w }))).status, 201);
    Team { ada, bob, key, secrets }
}

fn events(s: &Server, ws: &str) -> Vec<String> {
    audit_rows(s, ws).iter().map(|r| r["event"].as_str().unwrap().to_string()).collect()
}

fn verify(s: &Server, ws: &str) -> audit::Verification {
    s.app.state().db.tx(|tx| audit::verify(tx, ws)).unwrap()
}

fn sql(s: &Server, statement: &str) {
    s.app.state().db.tx(|tx| tx.execute(statement, &[])).unwrap();
}

#[test]
fn every_event_type_is_written_with_who_where_and_never_a_secret() {
    let idp = MockIdp::start();
    let s = Server::with("audit-all", |c| c.oidc = Some(idp.config()));
    let t = team(&s);
    let mut secrets = t.secrets.clone();
    // A second device of Bob's, wrapped by the owner.
    let bob2 = Device::another(&s, "bob@example.com", &t.bob.session);
    let w = wraps_for(&[(bob2.key_id.clone(), bob2.keys.public_key().to_vec())], WS, 1, &t.key);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&t.ada.session), Some(json!({ "epoch": 1, "wraps": w }))).status, 200);
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/members/{}", t.bob.user_id), Some(&t.ada.session), Some(json!({ "role": "viewer" }))).status, 200);
    let inv = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&t.ada.session), Some(json!({ "role": "viewer" })));
    secrets.push(inv.json["token"].as_str().unwrap().into());
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}/invites/{}", inv.json["id"].as_str().unwrap()), Some(&t.ada.session), None).status, 204);
    // A runner credential, then its removal (rotates the key, ends its session).
    let kp = hitop_sync_crypto::DeviceKeyPair::generate();
    let wrapped = hitop_sync_crypto::wrap_key(&kp.public_key(), WS, 1, &t.key).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&t.ada.session), Some(json!({ "name": "ci", "publicKey": b64(&kp.public_key()), "wrapped": b64(&wrapped) })));
    assert_eq!(r.status, 201);
    secrets.push(r.json["token"].as_str().unwrap().into());
    let runner = r.json["userId"].as_str().unwrap().to_string();
    let key2 = hitop_sync_crypto::WorkspaceKey::generate();
    let keys: Vec<_> = member_keys(&s, WS, &t.ada.session).into_iter().filter(|(id, _)| *id != r.json["keyId"].as_str().unwrap()).collect();
    let rm = s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{runner}"), Some(&t.ada.session), Some(json!({ "epoch": 2, "wraps": wraps_for(&keys, WS, 2, &key2) })));
    assert_eq!(rm.status, 200, "{}", rm.json);
    // A routine rotation.
    let key3 = hitop_sync_crypto::WorkspaceKey::generate();
    let keys = member_keys(&s, WS, &t.ada.session);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/rotate"), Some(&t.ada.session), Some(json!({ "epoch": 3, "wraps": wraps_for(&keys, WS, 3, &key3) }))).status, 200);
    // Account events: a failed sign-in, a passkey, a sign-out, SSO (linking, a config error).
    assert_eq!(s.call("POST", "/v1/auth/email/verify", None, Some(json!({ "token": "wrong" }))).status, 401);
    let start = s.call("POST", "/v1/auth/passkey/register/start", Some(&t.ada.session), None);
    let options: CreationChallengeResponse = serde_json::from_value(start.json["options"].clone()).unwrap();
    let credential = SoftPasskey::new(true).do_registration(Url::parse(ORIGIN).unwrap(), options).unwrap();
    assert_eq!(s.call("POST", "/v1/auth/passkey/register/finish", Some(&t.ada.session), Some(json!({ "challengeId": start.json["challengeId"], "credential": credential }))).status, 201);
    let flow = sso_start(&s, None);
    idp.next(idp.sign(&idp.claims("sub-cy", "cy@example.com", &flow.nonce)), &flow.pkce);
    let page = sso_callback(&s, &format!("code={}&state={}", idp::GOOD_CODE, flow.state));
    let code = sso_code(&page).unwrap();
    secrets.push(code.clone());
    secrets.push(sso_finish(&s, &code, &flow.verifier).json["session"].as_str().unwrap().into());
    idp.inner.lock().unwrap().claimed_issuer = Some("https://other.example".into());
    // The discovery cache is an hour old.
    s.advance(3601);
    assert_eq!(s.call("POST", "/v1/auth/sso/start", None, Some(json!({ "challenge": b64(&[5; 32]) }))).status, 503);
    // Bob signs out; Ada deletes the workspace.
    assert_eq!(s.call("POST", "/v1/auth/logout", Some(&t.bob.session), None).status, 204);
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}"), Some(&t.ada.session), None).status, 204);

    let mut seen = BTreeSet::new();
    for ws in ["", WS] {
        seen.extend(events(&s, ws));
    }
    let all: BTreeSet<String> = audit::EVENTS.iter().map(|e| e.to_string()).collect();
    assert_eq!(seen, all, "every event type, and no other");

    // Who, what, where.
    let rows = audit_rows(&s, WS);
    let added = rows.iter().find(|r| r["event"] == "member_added").unwrap();
    assert_eq!((added["actor"].as_str(), added["target"].as_str(), added["detail"]["role"].as_str()), (Some("ada@example.com"), Some(t.bob.user_id.as_str()), Some("editor")));
    let changed = rows.iter().find(|r| r["event"] == "role_changed").unwrap();
    assert_eq!(changed["detail"], json!({ "from": "editor", "to": "viewer" }));
    let rotated: Vec<_> = rows.iter().filter(|r| r["event"] == "key_rotated").map(|r| r["detail"]["reason"].as_str().unwrap()).collect();
    assert_eq!(rotated, ["member_removed", "rotate"]);
    assert_eq!(rows.last().unwrap()["event"], "workspace_deleted");
    assert_eq!(rows[0]["ip"], "127.0.0.0", "truncated");
    // The account chain: sign-ins with their method.
    let account = audit_rows(&s, "");
    let methods: BTreeSet<_> = account.iter().filter(|r| r["event"] == "signin").map(|r| r["detail"]["method"].as_str().unwrap().to_string()).collect();
    assert_eq!(methods, ["email", "sso"].iter().map(|m| m.to_string()).collect());
    // The log of a deleted workspace still verifies.
    assert!(verify(&s, WS).problem.is_none());
    assert!(verify(&s, "").problem.is_none());

    // No token, code or key in any row.
    let dump = s.app.state().db.tx(|tx| tx.query("SELECT detail, target, actor_email, user_agent FROM audit_log", &[])).unwrap();
    let text: String = dump.iter().map(|r| format!("{} {} {} {}\n", r.text(0), r.text(1), r.text(2), r.text(3))).collect();
    for secret in &secrets {
        assert!(!text.contains(secret.as_str()), "a secret in the audit log");
    }
    assert!(!text.contains(&b64(t.key.as_bytes())));
}

#[test]
fn only_owners_read_a_workspace_log_and_everyone_reads_their_own() {
    let s = Server::new("audit-read");
    let t = team(&s);
    let path = format!("/v1/workspaces/{WS}/audit");
    let r = s.call("GET", &path, Some(&t.ada.session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    let names: Vec<_> = r.json["events"].as_array().unwrap().iter().map(|e| e["event"].as_str().unwrap()).collect();
    assert_eq!(names, ["member_added", "invite_accepted", "invite_created", "workspace_created"], "newest first");
    assert_eq!(r.json["head"]["seq"], 4);
    assert_eq!(s.call("GET", &path, Some(&t.bob.session), None).status, 403, "an editor");
    assert_eq!(s.call("GET", &format!("{path}/verify"), Some(&t.bob.session), None).status, 403);
    let eve = Device::new(&s, "eve@example.com");
    assert_eq!(s.call("GET", &path, Some(&eve.session), None).status, 404, "an outsider");
    assert_eq!(s.call("GET", &path, None, None).status, 401);
    // Pages and filters.
    let p1 = s.call("GET", &format!("{path}?limit=3"), Some(&t.ada.session), None);
    assert_eq!((p1.json["events"].as_array().unwrap().len(), p1.json["next"].as_i64()), (3, Some(2)));
    let p2 = s.call("GET", &format!("{path}?limit=3&before=2"), Some(&t.ada.session), None);
    assert_eq!((p2.json["events"][0]["event"].as_str(), p2.json["next"].is_null()), (Some("workspace_created"), true));
    let f = s.call("GET", &format!("{path}?event=invite_accepted"), Some(&t.ada.session), None);
    assert_eq!(f.json["events"].as_array().unwrap().len(), 1);
    assert_eq!(f.json["events"][0]["actor"]["email"], "bob@example.com");
    let f = s.call("GET", &format!("{path}?actor=Bob@Example.com"), Some(&t.ada.session), None);
    assert_eq!(f.json["events"].as_array().unwrap().len(), 1);
    assert_eq!(s.call("GET", &format!("{path}?since=99999999999"), Some(&t.ada.session), None).json["events"], json!([]));
    assert_eq!(s.call("GET", &format!("{path}?limit=x"), Some(&t.ada.session), None).status, 400);
    // A runner credential isn't an owner and has no account log.
    let kp = hitop_sync_crypto::DeviceKeyPair::generate();
    let wrapped = hitop_sync_crypto::wrap_key(&kp.public_key(), WS, 1, &t.key).unwrap();
    let runner = s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&t.ada.session), Some(json!({ "publicKey": b64(&kp.public_key()), "wrapped": b64(&wrapped) })));
    let rt = runner.json["token"].as_str().unwrap();
    assert_eq!(s.call("GET", &path, Some(rt), None).status, 403);
    assert_eq!(s.call("GET", "/v1/me/audit", Some(rt), None).status, 403);
    // Own account log: Bob's sign-in and device key, nobody else's.
    let mine = s.call("GET", "/v1/me/audit", Some(&t.bob.session), None);
    assert_eq!(mine.status, 200);
    let rows = mine.json["events"].as_array().unwrap();
    assert!(!rows.is_empty() && rows.iter().all(|e| e["actor"]["email"] == "bob@example.com" || e["target"] == t.bob.user_id.as_str()), "{rows:?}");
    // The verify endpoint.
    let v = s.call("GET", &format!("{path}/verify"), Some(&t.ada.session), None);
    assert_eq!(v.json["ok"], true, "{}", v.json);
    assert_eq!(v.json["checked"], 5);
}

#[test]
fn the_chain_catches_an_edited_or_deleted_row() {
    let cases: &[(&str, &str, &str)] = &[
        ("edit", "UPDATE audit_log SET target = 'someone-else' WHERE workspace_id <> '' AND seq = 2", "row 2 was changed"),
        ("edit-detail", "UPDATE audit_log SET detail = '{\"role\":\"owner\"}' WHERE workspace_id <> '' AND seq = 4", "row 4 was changed"),
        ("delete-middle", "DELETE FROM audit_log WHERE workspace_id <> '' AND seq = 2", "row 2 is missing"),
        ("delete-first", "DELETE FROM audit_log WHERE workspace_id <> '' AND seq = 1", "row 1 is missing"),
        ("delete-last", "DELETE FROM audit_log WHERE workspace_id <> '' AND seq = 4", "rows 4 to 4 are missing"),
        ("reorder", "UPDATE audit_log SET at = at + 1 WHERE workspace_id <> '' AND seq = 3", "row 3 was changed"),
    ];
    for (name, statement, problem) in cases {
        let s = Server::new(&format!("audit-chain-{name}"));
        let t = team(&s);
        assert!(verify(&s, WS).problem.is_none());
        sql(&s, statement);
        let found = verify(&s, WS).problem.unwrap_or_default();
        assert!(found.starts_with(problem), "{name}: {found}");
        let v = s.call("GET", &format!("/v1/workspaces/{WS}/audit/verify"), Some(&t.ada.session), None);
        assert_eq!(v.json["ok"], false, "{name}");
        assert!(v.json["problem"].as_str().unwrap().starts_with(problem), "{name}");
        // Operators check every chain at once.
        let all = audit::verify_all(s.app.state()).unwrap();
        assert!(all.iter().any(|(ws, v)| ws == WS && v.problem.is_some()));
        assert!(all.iter().any(|(ws, v)| ws.is_empty() && v.problem.is_none()));
    }
    // Recomputing the edited row's hash only moves the break to the next row.
    let s = Server::new("audit-chain-rehash");
    team(&s);
    let row = s.app.state().db.tx(|tx| tx.query_one("SELECT prev_hash, at, event, actor_id, actor_email, detail, ip, user_agent FROM audit_log WHERE workspace_id = $1 AND seq = 2", &[WS.into()])).unwrap().unwrap();
    let forged = audit::row_hash(&row.text(0), WS, 2, row.int(1), &row.text(2), &row.text(3), &row.text(4), "forged", &row.text(5), &row.text(6), &row.text(7));
    s.app.state().db.tx(|tx| tx.execute("UPDATE audit_log SET target = 'forged', hash = $1 WHERE workspace_id = $2 AND seq = 2", &[forged.into(), WS.into()])).unwrap();
    assert_eq!(verify(&s, WS).problem.as_deref(), Some("row 3 doesn't follow the row before it (prev_hash differs)"));
}

#[test]
fn retention_purges_old_rows_and_the_chain_still_verifies() {
    let s = Server::with("audit-purge", |c| c.audit_retention_days = 30);
    let t = team(&s);
    let before = verify(&s, WS);
    assert_eq!(before.checked, 4);
    assert_eq!(audit::purge(s.app.state()).unwrap(), 0, "nothing is old yet");
    s.advance(31 * 86_400);
    // (Sessions last 30 days.)
    let ada = s.sign_in("ada@example.com");
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/members/{}", t.bob.user_id), Some(&ada), Some(json!({ "role": "viewer" }))).status, 200);
    let purged = audit::purge(s.app.state()).unwrap();
    assert!(purged >= 4, "{purged}");
    assert_eq!(events(&s, WS), ["role_changed"]);
    let after = verify(&s, WS);
    assert_eq!((after.problem, after.anchor_seq, after.checked, after.head_seq), (None, 4, 1, 5));
    assert!(verify(&s, "").problem.is_none(), "the account chain too");
    assert_eq!(events(&s, ""), ["signin"], "only the new sign-in is left");
    // The anchor is the last purged row's hash: deleting the oldest remaining row still shows.
    sql(&s, "DELETE FROM audit_log WHERE seq = 5");
    assert!(verify(&s, WS).problem.is_some());
    // 0 keeps everything.
    let keep = Server::with("audit-keep", |c| c.audit_retention_days = 0);
    team(&keep);
    keep.advance(10 * 365 * 86_400);
    assert_eq!(audit::purge(keep.app.state()).unwrap(), 0);
}

#[test]
fn client_ips_are_truncated_and_proxies_must_be_trusted() {
    let s = Server::new("audit-ip");
    let call = |headers: &[(&str, &str)]| {
        s.raw("POST", "/v1/auth/email/verify", None, json!({ "token": "x" }).to_string().into_bytes(), headers);
        audit_rows(&s, "").pop().unwrap()
    };
    // The front proxy (on loopback) appends the client to X-Forwarded-For; the last entry counts.
    let r = call(&[("X-Forwarded-For", "10.9.9.9, 203.0.113.77"), ("User-Agent", "hitop/1.1 (linux)")]);
    assert_eq!((r["ip"].as_str(), r["userAgent"].as_str()), (Some("203.0.113.0"), Some("hitop/1.1 (linux)")));
    let r = call(&[("X-Forwarded-For", "2001:db8:aa:bb::1")]);
    assert_eq!(r["ip"], "2001:db8:aa::");
    let long = "x".repeat(500);
    assert_eq!(call(&[("User-Agent", &long)])["userAgent"].as_str().unwrap().len(), 200);
    // A peer that isn't a trusted proxy can't choose its address.
    let untrusted = Server::with("audit-ip-untrusted", |c| c.trusted_proxies = vec![]);
    untrusted.raw("POST", "/v1/auth/email/verify", None, json!({ "token": "x" }).to_string().into_bytes(), &[("X-Forwarded-For", "198.51.100.1")]);
    assert_eq!(audit_rows(&untrusted, "").pop().unwrap()["ip"], "127.0.0.0");
    // Or no IP at all.
    let off = Server::with("audit-ip-off", |c| c.audit_ip = audit::IpPolicy::Off);
    off.raw("POST", "/v1/auth/email/verify", None, json!({ "token": "x" }).to_string().into_bytes(), &[]);
    assert_eq!(audit_rows(&off, "").pop().unwrap()["ip"], "");
}
