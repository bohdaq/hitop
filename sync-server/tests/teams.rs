//! Invites (mailed and by link), role changes and runner credentials.

mod common;

use common::*;
use serde_json::json;

const WS: &str = "00112233445566778899aabbccddeeff";

#[test]
fn invite_by_mail_then_accept_then_the_owner_adds_the_device() {
    let s = Server::new("invite-mail");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "email": "Bob@Example.com", "role": "editor" })));
    assert_eq!(r.status, 201, "{}", r.json);
    assert_eq!(r.json["mailed"], true);
    let token = r.json["token"].as_str().unwrap().to_string();
    assert!(r.json["link"].as_str().unwrap().ends_with(&format!("/invite#{token}")));
    {
        let outbox = s.outbox.lock().unwrap();
        let mail = outbox.iter().find(|m| m.to == "bob@example.com").expect("an invite mail");
        assert!(mail.body.contains("ada@example.com invited you") && mail.body.contains(&format!("hitop sync accept {token}")));
    }
    // The database keeps the token's hash only.
    assert!(!String::from_utf8_lossy(&s.disk_bytes()).contains(&token));

    // Someone else can't take it; Bob can, once he has a device key.
    let eve = Device::new(&s, "eve@example.com");
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&eve.session), Some(json!({ "token": token }))).status, 403);
    let bob_session = s.sign_in("bob@example.com");
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&bob_session), Some(json!({ "token": token }))).status, 400, "needs a device key");
    let bob = Device::another(&s, "bob@example.com", &bob_session);
    let r = s.call("POST", "/v1/invites/accept", Some(&bob.session), Some(json!({ "token": token })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json, json!({ "workspaceId": WS, "role": "editor" }));
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&bob.session), Some(json!({ "token": "nope" }))).status, 404);
    // Accepting isn't membership: the key isn't wrapped for Bob yet.
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&bob.session), None).status, 404);

    // The owner sees the accepted invite with Bob's device keys and adds him.
    let list = s.call("GET", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), None);
    let inv = &list.json["invites"][0];
    assert_eq!(inv["accepted"]["email"], "bob@example.com");
    assert_eq!(inv["accepted"]["keys"][0]["id"], bob.key_id.as_str());
    let pk = unb64(inv["accepted"]["keys"][0]["publicKey"].as_str().unwrap());
    let w = wraps_for(&[(bob.key_id.clone(), pk)], WS, 1, &key);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": inv["role"], "wraps": w }))).status, 201);
    assert_eq!(bob.workspace_key(&s, WS, 1), key);
    // The used invite is gone.
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), None).json["invites"], json!([]));
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&bob.session), Some(json!({ "token": token }))).status, 404);
}

#[test]
fn link_invites_expire_and_can_be_revoked() {
    let s = Server::new("invite-link");
    let ada = Device::new(&s, "ada@example.com");
    create_workspace(&s, &ada, WS);
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "role": "viewer" })));
    assert_eq!(r.status, 201);
    assert_eq!(r.json["mailed"], false);
    assert!(!s.outbox.lock().unwrap().iter().any(|m| m.subject.contains("invited")));
    let token = r.json["token"].as_str().unwrap().to_string();
    let cy = Device::new(&s, "cy@example.com");
    let dee = Device::new(&s, "dee@example.com");
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&cy.session), Some(json!({ "token": token }))).status, 200);
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&dee.session), Some(json!({ "token": token }))).json["error"], "already_accepted");
    // Revoked.
    let id = r.json["id"].as_str().unwrap();
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}/invites/{id}"), Some(&ada.session), None).status, 204);
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}/invites/{id}"), Some(&ada.session), None).status, 404);
    // Expired after 7 days.
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "role": "viewer" })));
    s.advance(7 * 24 * 3600 + 1);
    let cy = Device::new(&s, "cy2@example.com");
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&cy.session), Some(json!({ "token": r.json["token"] }))).status, 404);
    // Bad input.
    for body in [json!({ "role": "admin" }), json!({ "email": "not an address", "role": "viewer" }), json!({ "email": "x@runner.invalid", "role": "viewer" })] {
        assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(body)).status, 400);
    }
}

#[test]
fn roles_change_but_a_workspace_keeps_an_owner() {
    let s = Server::new("set-role");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let key = create_workspace(&s, &ada, WS);
    let w = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &key);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "viewer", "wraps": w }))).status, 201);
    let set = |by: &Device, user: &str, role: &str| s.call("PUT", &format!("/v1/workspaces/{WS}/members/{user}"), Some(&by.session), Some(json!({ "role": role })));
    assert_eq!(set(&ada, &ada.user_id, "editor").status, 400, "the only owner");
    assert_eq!(set(&ada, &bob.user_id, "owner").status, 200);
    assert_eq!(set(&ada, &ada.user_id, "viewer").status, 200, "now Bob owns it too");
    assert_eq!(set(&ada, &bob.user_id, "viewer").status, 403, "Ada is a viewer now");
    assert_eq!(set(&bob, "no-such-user", "viewer").status, 404);
    assert_eq!(set(&bob, &ada.user_id, "boss").status, 400);
    let ws = s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&bob.session), None);
    let roles: Vec<_> = ws.json["members"].as_array().unwrap().iter().map(|m| (m["email"].as_str().unwrap().to_string(), m["role"].as_str().unwrap().to_string(), m["runner"].as_bool().unwrap())).collect();
    assert_eq!(roles, [("ada@example.com".into(), "viewer".into(), false), ("bob@example.com".into(), "owner".into(), false)]);
}

#[test]
fn runner_credentials_read_one_workspace_and_go_away_with_it() {
    let s = Server::new("runners");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let kp = hitop_sync_crypto::DeviceKeyPair::generate();
    let wrapped = hitop_sync_crypto::wrap_key(&kp.public_key(), WS, 1, &key).unwrap();
    let make = |pk: &[u8], w: &[u8]| s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&ada.session), Some(json!({ "name": "nightly", "publicKey": b64(pk), "wrapped": b64(w) })));
    assert_eq!(make(&[1u8; 31], &wrapped).status, 400);
    assert_eq!(make(&kp.public_key(), &[0u8; 10]).status, 400);
    let r = make(&kp.public_key(), &wrapped);
    assert_eq!(r.status, 201, "{}", r.json);
    let token = r.json["token"].as_str().unwrap().to_string();
    // A year-long session, stored hashed.
    assert!(r.json["expiresAt"].as_i64().unwrap() - r.json["epoch"].as_i64().unwrap() > 300 * 24 * 3600);
    assert!(!String::from_utf8_lossy(&s.disk_bytes()).contains(&token));
    let runner = Device { session: token.clone(), key_id: r.json["keyId"].as_str().unwrap().into(), keys: kp, user_id: r.json["userId"].as_str().unwrap().into() };
    assert_eq!(runner.workspace_key(&s, WS, 1), key);
    let ws = s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None);
    let m = ws.json["members"].as_array().unwrap().iter().find(|m| m["userId"] == runner.user_id.as_str()).unwrap().clone();
    assert_eq!((m["role"].as_str(), m["runner"].as_bool()), (Some("viewer"), Some(true)));
    // Ten at most.
    for _ in 1..10 {
        assert_eq!(make(&hitop_sync_crypto::DeviceKeyPair::generate().public_key(), &wrapped).status, 201);
    }
    assert_eq!(make(&hitop_sync_crypto::DeviceKeyPair::generate().public_key(), &wrapped).status, 403);
    // Deleting the workspace deletes its runner accounts.
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).status, 204);
    assert_eq!(s.call("GET", "/v1/me", Some(&token), None).status, 401);
}

#[test]
fn the_invite_page_keeps_the_code_in_the_fragment() {
    let s = Server::new("invite-page");
    let r = s.call("GET", "/invite", None, None);
    assert_eq!(r.status, 200);
    let html = String::from_utf8_lossy(&r.raw);
    assert!(html.contains("location.hash"));
    assert!(r.header("content-security-policy").unwrap().contains("default-src 'none'"));
}
