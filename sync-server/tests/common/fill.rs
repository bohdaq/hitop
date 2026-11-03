//! A server with something in every kind of table, written through the API as clients would:
//! accounts, devices, a team workspace with blobs, an invite, a removal (key rotation), an org, an
//! access token, so the audit chains are real. For the migrate-db tests.

use serde_json::json;

use super::*;

pub const WS: &str = "0f0e0d0c0b0a09080706050403020100";

/// Fills `s` and returns how many audit chains it has (at least two: accounts and `WS`).
pub fn fill(s: &Server) -> usize {
    let ada = Device::new(s, "ada@example.com");
    let bob = Device::new(s, "bob@example.com");
    let cy = Device::new(s, "cy@example.com");
    let k1 = create_workspace(s, &ada, WS);
    for (d, email) in [(&bob, "bob@example.com"), (&cy, "cy@example.com")] {
        let wraps = wraps_for(&[(d.key_id.clone(), d.keys.public_key().to_vec())], WS, 1, &k1);
        let r = s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": email, "role": "editor", "wraps": wraps })));
        assert_eq!(r.status, 201, "{}", r.json);
    }
    for (i, blob) in ["a", "b", "c"].iter().enumerate() {
        let r = s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/{blob}"), Some(&ada.session), Some(json!({ "version": 1, "vv": { "a": i + 1 }, "epoch": 1, "content": seal(&k1, WS, blob, 1, 1, b"{\"v\":1}") })));
        assert_eq!(r.status, 200, "{}", r.json);
    }
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "role": "viewer" })));
    assert_eq!(r.status, 201, "{}", r.json);
    let k2 = crypto::WorkspaceKey::generate();
    let remaining = [(ada.key_id.clone(), ada.keys.public_key().to_vec()), (bob.key_id.clone(), bob.keys.public_key().to_vec())];
    let r = s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", cy.user_id), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": wraps_for(&remaining, WS, 2, &k2) })));
    assert_eq!(r.status, 200, "{}", r.json);
    let r = s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/a"), Some(&bob.session), Some(json!({ "version": 2, "vv": { "a": 1, "b": 1 }, "epoch": 2, "content": seal(&k2, WS, "a", 2, 2, b"{\"v\":2}") })));
    assert_eq!(r.status, 200, "{}", r.json);
    let r = s.call("POST", "/v1/orgs", Some(&ada.session), Some(json!({ "name": "Acme" })));
    assert_eq!(r.status, 201, "{}", r.json);
    let r = s.call("POST", "/v1/me/tokens", Some(&ada.session), Some(json!({ "name": "ci", "scopes": ["read"] })));
    assert_eq!(r.status, 201, "{}", r.json);
    // C1b: a password account with its key bundle; made-up parameters (a server secret).
    let (dee, bundle_key) = super::pw::sign_up(s, "dee@example.com", "dee's long password");
    let dee_id = s.call("GET", "/v1/me", Some(&dee), None).json["id"].as_str().unwrap().to_string();
    let bundle = crypto::seal_key_bundle(&bundle_key, &dee_id, b"{\"v\":1}").unwrap();
    assert_eq!(s.call("PUT", "/v1/me/recovery?method=password", Some(&dee), Some(json!({ "blob": b64(&bundle) }))).status, 200);
    assert_eq!(s.call("POST", "/v1/auth/password/params", None, Some(json!({ "email": "nobody@example.com" }))).status, 200);
    // C3: a webhook delivery and a trial. Written as the server writes them (billing needs a
    // configured store, which this server has not).
    s.app
        .state()
        .db
        .tx(|tx| {
            tx.execute("INSERT INTO billing_events (id, event_name, org_id, subscription_id, resource_updated_at, test_mode, outcome, received_at) VALUES ('e1', 'subscription_created', '', '1', 1, 1, 'unmatched', 1)", &[])?;
            tx.execute("INSERT INTO trials (org_id, user_id, email_domain, started_at, ends_at) VALUES ('o1', 'u1', 'acme.io', 1, 2)", &[])?;
            tx.execute("UPDATE entitlements SET ls_item_id = '7', ls_order_id = '9', ls_updated_at = 5 WHERE org_id = (SELECT MIN(org_id) FROM entitlements)", &[])
        })
        .unwrap();
    // C4: run history (a sealed report, status rows, a queued alert) and a shared environment.
    let run = |id: &str, status: &str| {
        let detail = crypto::seal_run(&k2, WS, id, 2, b"{\"reportVersion\":1}").unwrap();
        let r = s.call("POST", &format!("/v1/workspaces/{WS}/runs"), Some(&ada.session), Some(json!({ "id": id, "startedAt": 1, "collection": "c", "environment": "e", "status": status, "requests": 1, "failed": u8::from(status == "failed"), "detail": b64(&detail) })));
        assert_eq!(r.status, 201, "{}", r.json);
    };
    run("run-1", "failed");
    let r = s.call("PUT", &format!("/v1/workspaces/{WS}/alerts"), Some(&ada.session), Some(json!({ "email": ["ada@example.com"], "missedAfterMinutes": 60 })));
    assert_eq!(r.status, 200, "{}", r.json);
    run("run-2", "passed");
    let share_key = crypto::WorkspaceKey::generate();
    let share = "00112233445566778899aabbccddeeff";
    let pk = ada.keys.public_key();
    let wrap = crypto::seal_share_wrap(&crypto::Device { key_id: &ada.key_id, keys: &ada.keys }, &crypto::Recipient { key_id: &ada.key_id, public_key: &pk }, WS, share, 1, &share_key).unwrap();
    let sealed = crypto::seal_share(&share_key, WS, share, 1, b"{}").unwrap();
    let r = s.call("PUT", &format!("/v1/workspaces/{WS}/env-shares/{share}"), Some(&ada.session), Some(json!({ "version": 1, "sealed": b64(&sealed), "wraps": [{ "keyId": ada.key_id, "wrapped": b64(&wrap) }] })));
    assert_eq!(r.status, 200, "{}", r.json);
    s.app.state().db.tx(|tx| tx.query_one("SELECT COUNT(*) FROM audit_chain", &[])).unwrap().unwrap().int(0) as usize
}
