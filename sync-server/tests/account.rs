//! Deleting an account (`DELETE /v1/me`, account.rs): the confirmation, a solo account with
//! everything it owns gone (every table scanned), shared workspaces and orgs (refused for an only
//! owner, membership removed otherwise, a co-owned workspace moved out of the personal org), and
//! sessions and tokens dead afterwards. Apple's revoke is in tests/apple.rs, Stripe's cancel in
//! tests/billing.rs.

mod common;

use common::*;
use hitop_sync_server::db::{Db, Value as DbValue};
use serde_json::json;

const WS: &str = "0123456789abcdef0123456789abcdef";

fn delete(s: &Server, token: &str, confirm: Option<&str>) -> Reply {
    let body = confirm.map(|c| json!({ "confirm": c }));
    s.call("DELETE", "/v1/me", Some(token), body)
}

fn org_create(s: &Server, d: &Device, name: &str) -> String {
    let r = s.call(
        "POST",
        "/v1/orgs",
        Some(&d.session),
        Some(json!({ "name": name })),
    );
    assert_eq!(r.status, 201, "{}", r.json);
    r.json["id"].as_str().unwrap().to_string()
}

fn add_to_workspace(
    s: &Server,
    owner: &Device,
    ws: &str,
    key: &hitop_sync_crypto::WorkspaceKey,
    who: &Device,
    email: &str,
    role: &str,
) {
    let w = wraps_for(
        &[(who.key_id.clone(), who.keys.public_key().to_vec())],
        ws,
        1,
        key,
    );
    let r = s.call(
        "POST",
        &format!("/v1/workspaces/{ws}/members"),
        Some(&owner.session),
        Some(json!({ "email": email, "role": role, "wraps": w })),
    );
    assert_eq!(r.status, 201, "{}", r.json);
}

/// Every row of every table holding `needle` in a text column, as "table: row", except the
/// tables that keep it on purpose (the audit log and its chain heads).
fn rows_with(s: &Server, needle: &str) -> Vec<String> {
    let mut out = Vec::new();
    for table in Db::tables() {
        if table == "audit_log" || table == "audit_chain" {
            continue;
        }
        let rows = s
            .app
            .state()
            .db
            .tx(|tx| tx.query(&format!("SELECT * FROM {table}"), &[]))
            .unwrap();
        for r in rows {
            let hit = r.values().iter().any(|v| match v {
                DbValue::Text(t) => t.contains(needle),
                DbValue::Bytes(b) => String::from_utf8_lossy(b).contains(needle),
                _ => false,
            });
            if hit {
                out.push(format!("{table}: {:?}", r.values()));
            }
        }
    }
    out
}

#[test]
fn the_confirmation_and_who_may() {
    let s = Server::new("account-confirm");
    let ada = Device::new(&s, "ada@example.com");
    // No body, a wrong address, an empty one.
    assert_eq!(delete(&s, &ada.session, None).status, 400);
    let wrong = delete(&s, &ada.session, Some("bob@example.com"));
    assert_eq!(
        (wrong.status, wrong.json["error"].as_str()),
        (400, Some("confirm_mismatch"))
    );
    assert_eq!(delete(&s, &ada.session, Some("")).status, 400);
    // Not signed in.
    assert_eq!(
        s.call(
            "DELETE",
            "/v1/me",
            None,
            Some(json!({ "confirm": "ada@example.com" }))
        )
        .status,
        401
    );
    // An access token, even with write scope, can't.
    let t = s.call(
        "POST",
        "/v1/me/tokens",
        Some(&ada.session),
        Some(json!({ "name": "ci", "scopes": ["read", "write"] })),
    );
    assert_eq!(t.status, 201, "{}", t.json);
    let token = t.json["token"].as_str().unwrap().to_string();
    assert_eq!(delete(&s, &token, Some("ada@example.com")).status, 403);
    // A runner credential can't either.
    let key = create_workspace(&s, &ada, WS);
    let kp = hitop_sync_crypto::DeviceKeyPair::generate();
    let wrapped = hitop_sync_crypto::wrap_key(&kp.public_key(), WS, 1, &key).unwrap();
    let r = s.call(
        "POST",
        &format!("/v1/workspaces/{WS}/runners"),
        Some(&ada.session),
        Some(json!({ "name": "ci", "publicKey": b64(&kp.public_key()), "wrapped": b64(&wrapped) })),
    );
    assert_eq!(r.status, 201, "{}", r.json);
    let runner = r.json["token"].as_str().unwrap().to_string();
    assert_eq!(delete(&s, &runner, Some("ada@example.com")).status, 403);
    // Nothing went.
    assert_eq!(
        s.call("GET", "/v1/me", Some(&ada.session), None).status,
        200
    );
    assert_eq!(s.call("GET", "/v1/me", Some(&token), None).status, 200);
    // Case and spaces don't matter.
    let ok = delete(&s, &ada.session, Some("  Ada@Example.com "));
    assert_eq!(ok.status, 200, "{}", ok.json);
}

#[test]
fn a_solo_account_goes_with_everything_it_owns() {
    let s = Server::new("account-solo");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let second = Device::another(&s, "ada@example.com", &s.sign_in("ada@example.com"));
    // A blob, a runner, a library item, a recovery bundle, an access token, a team org of one.
    let put = s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/r-1"), Some(&ada.session), Some(json!({ "version": 1, "vv": { "laptop": 1 }, "epoch": 1, "content": seal(&key, WS, "r-1", 1, 1, b"v1") })));
    assert!(put.status < 300, "{} {}", put.status, put.json);
    let kp = hitop_sync_crypto::DeviceKeyPair::generate();
    let wrapped = hitop_sync_crypto::wrap_key(&kp.public_key(), WS, 1, &key).unwrap();
    let runner = s.call(
        "POST",
        &format!("/v1/workspaces/{WS}/runners"),
        Some(&ada.session),
        Some(json!({ "name": "ci", "publicKey": b64(&kp.public_key()), "wrapped": b64(&wrapped) })),
    );
    assert_eq!(runner.status, 201, "{}", runner.json);
    let runner_session = runner.json["token"].as_str().unwrap().to_string();
    let lib = s.call(
        "PUT",
        "/v1/library/collection/7",
        Some(&ada.session),
        Some(json!({ "data": { "id": 7, "name": "Shop", "requests": [] }, "baseVersion": 0 })),
    );
    assert_eq!(lib.status, 200, "{}", lib.json);
    assert_eq!(s.call("PUT", "/v1/me/recovery", Some(&ada.session), Some(json!({ "blob": b64(&hitop_sync_crypto::seal_recovery(&hitop_sync_crypto::RecoveryPhrase::generate(), &ada.user_id, b"x").unwrap()) }))).status / 100, 2);
    let token = s
        .call(
            "POST",
            "/v1/me/tokens",
            Some(&ada.session),
            Some(json!({ "name": "ci", "scopes": ["read"] })),
        )
        .json["token"]
        .as_str()
        .unwrap()
        .to_string();
    let team = org_create(&s, &ada, "Solo team");
    // An invite to her address from someone else's org stays out of her account after deletion.
    let bob = Device::new(&s, "bob@example.com");
    let other = org_create(&s, &bob, "Bob's");
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{other}/members"),
            Some(&bob.session),
            Some(json!({ "email": "ada+later@example.com", "role": "member" }))
        )
        .status,
        201
    );

    let r = delete(&s, &ada.session, Some("ada@example.com"));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(
        (
            r.json["workspacesDeleted"].as_i64(),
            r.json["workspacesLeft"].as_i64(),
            r.json["orgsDeleted"].as_i64(),
            r.json["orgsLeft"].as_i64()
        ),
        (Some(1), Some(0), Some(2), Some(0))
    );
    // Every credential is dead: both sessions, the access token, the runner.
    for t in [&ada.session, &second.session, &token, &runner_session] {
        assert_eq!(s.call("GET", "/v1/me", Some(t), None).status, 401, "{t}");
    }
    assert_eq!(s.call("GET", "/v1/library", Some(&token), None).status, 401);
    // Nothing anywhere holds her id, her address or her orgs' ids (the audit log keeps its rows).
    for needle in [ada.user_id.as_str(), "ada@example.com", team.as_str(), WS] {
        assert_eq!(rows_with(&s, needle), Vec::<String>::new(), "{needle}");
    }
    // Bob is untouched.
    assert_eq!(
        s.call(
            "GET",
            &format!("/v1/orgs/{other}"),
            Some(&bob.session),
            None
        )
        .status,
        200
    );
    // The deletion is in the audit log, with what it did.
    let account = audit_rows(&s, "");
    let row = account
        .iter()
        .rev()
        .find(|r| r["event"] == "account_deleted")
        .expect("an account_deleted row");
    assert_eq!(row["target"], ada.user_id.as_str());
    assert_eq!(row["detail"]["orgsDeleted"], 2);
    // A short mail says so.
    let outbox = s.outbox.lock().unwrap();
    let mail = outbox
        .iter()
        .rev()
        .find(|m| m.to == "ada@example.com")
        .unwrap();
    assert!(mail.subject.contains("deleted"), "{}", mail.subject);
    drop(outbox);
    // Signing in again is a new, empty account.
    let again = Device::new(&s, "ada@example.com");
    assert_ne!(again.user_id, ada.user_id);
    assert_eq!(
        s.call("GET", "/v1/library", Some(&again.session), None)
            .json["items"]
            .as_array()
            .map_or(0, Vec::len),
        0
    );
    assert_eq!(
        s.call("GET", "/v1/workspaces", Some(&again.session), None)
            .json["workspaces"],
        json!([])
    );
}

#[test]
fn an_only_owner_of_shared_things_is_refused_until_they_hand_over() {
    let s = Server::new("account-shared");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    // Ada's workspace (in her personal org), shared with Bob; Ada's team org with Bob in it.
    let key = create_workspace(&s, &ada, WS);
    add_to_workspace(&s, &ada, WS, &key, &bob, "bob@example.com", "editor");
    let team = org_create(&s, &ada, "Acme");
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{team}/members"),
            Some(&ada.session),
            Some(json!({ "email": "bob@example.com", "role": "member" }))
        )
        .status,
        201
    );

    let r = delete(&s, &ada.session, Some("ada@example.com"));
    assert_eq!(
        (r.status, r.json["error"].as_str()),
        (409, Some("transfer_ownership_first")),
        "{}",
        r.json
    );
    let blockers: Vec<(String, String)> = r.json["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["kind"].as_str().unwrap().into(),
                b["id"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        blockers,
        vec![
            ("workspace".to_string(), WS.to_string()),
            ("org".to_string(), team.clone())
        ]
    );
    let message = r.json["message"].as_str().unwrap();
    assert!(message.contains("the organization Acme and 1 shared collection"), "{message}");
    assert!(!message.to_lowercase().contains("plan"));
    // Nothing changed.
    assert_eq!(
        s.call("GET", "/v1/me", Some(&ada.session), None).status,
        200
    );
    assert_eq!(
        s.call(
            "GET",
            &format!("/v1/workspaces/{WS}"),
            Some(&ada.session),
            None
        )
        .status,
        200
    );

    // Bob becomes an owner of both: now Ada can go, and Bob keeps both.
    assert_eq!(
        s.call(
            "PUT",
            &format!("/v1/workspaces/{WS}/members/{}", bob.user_id),
            Some(&ada.session),
            Some(json!({ "role": "owner" }))
        )
        .status,
        200
    );
    assert_eq!(
        s.call(
            "PUT",
            &format!("/v1/orgs/{team}/members/{}", bob.user_id),
            Some(&ada.session),
            Some(json!({ "role": "owner" }))
        )
        .status,
        200
    );
    let r = delete(&s, &ada.session, Some("ada@example.com"));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(
        (
            r.json["workspacesLeft"].as_i64(),
            r.json["workspacesMoved"].as_i64(),
            r.json["orgsLeft"].as_i64(),
            r.json["orgsDeleted"].as_i64()
        ),
        (Some(1), Some(1), Some(1), Some(1))
    );
    // The workspace: Bob alone, its blobs kept, a rotation asked of his client, moved out of Ada's
    // (deleted) personal org into Bob's.
    let w = s.call(
        "GET",
        &format!("/v1/workspaces/{WS}"),
        Some(&bob.session),
        None,
    );
    assert_eq!(w.status, 200, "{}", w.json);
    let members: Vec<&str> = w.json["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["userId"].as_str().unwrap())
        .collect();
    assert_eq!(members, vec![bob.user_id.as_str()]);
    assert_eq!(w.json["rotationRequested"]["by"], "account deleted");
    let org_of = s
        .app
        .state()
        .db
        .tx(|tx| tx.query_one("SELECT org_id FROM workspaces WHERE id = $1", &[WS.into()]))
        .unwrap()
        .unwrap()
        .text(0);
    assert_eq!(org_of, format!("personal-{}", bob.user_id));
    // Ada's wraps are gone; Bob's stay.
    let wraps = s
        .app
        .state()
        .db
        .tx(|tx| {
            tx.query(
                "SELECT key_id FROM wraps WHERE workspace_id = $1",
                &[WS.into()],
            )
        })
        .unwrap();
    assert_eq!(
        wraps.iter().map(|r| r.text(0)).collect::<Vec<_>>(),
        vec![bob.key_id.clone()]
    );
    // The org: Bob alone, as owner.
    let m = s.call(
        "GET",
        &format!("/v1/orgs/{team}/members"),
        Some(&bob.session),
        None,
    );
    assert_eq!(m.json["members"].as_array().unwrap().len(), 1);
    assert_eq!(m.json["members"][0]["userId"], bob.user_id.as_str());
    // Nothing else holds Ada (the workspace's member list, wraps, the org's members).
    assert_eq!(rows_with(&s, &ada.user_id), Vec::<String>::new());
    assert_eq!(rows_with(&s, "ada@example.com"), Vec::<String>::new());
}

#[test]
fn a_member_who_leaves_by_deleting_keeps_the_others_whole() {
    let s = Server::new("account-member");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let key = create_workspace(&s, &ada, WS);
    add_to_workspace(&s, &ada, WS, &key, &bob, "bob@example.com", "viewer");
    let team = org_create(&s, &ada, "Acme");
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{team}/members"),
            Some(&ada.session),
            Some(json!({ "email": "bob@example.com", "role": "admin" }))
        )
        .status,
        201
    );
    let r = delete(&s, &bob.session, Some("bob@example.com"));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(
        (
            r.json["workspacesLeft"].as_i64(),
            r.json["orgsLeft"].as_i64(),
            r.json["orgsDeleted"].as_i64()
        ),
        (Some(1), Some(1), Some(1)),
        "only Bob's personal org goes"
    );
    assert_eq!(
        s.call("GET", "/v1/me", Some(&bob.session), None).status,
        401
    );
    let w = s.call(
        "GET",
        &format!("/v1/workspaces/{WS}"),
        Some(&ada.session),
        None,
    );
    assert_eq!(w.json["members"].as_array().unwrap().len(), 1);
    assert!(w.json["rotationRequested"].is_object(), "{}", w.json);
    let m = s.call(
        "GET",
        &format!("/v1/orgs/{team}/members"),
        Some(&ada.session),
        None,
    );
    assert_eq!(m.json["members"].as_array().unwrap().len(), 1);
    // Ada's audit log says Bob left because his account was deleted.
    let rows = audit_rows(&s, WS);
    let removed = rows
        .iter()
        .find(|r| r["event"] == "member_removed")
        .unwrap();
    assert_eq!(removed["detail"]["reason"], "account_deleted");
}
