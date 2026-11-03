//! HITOP Cloud C4: shared environment values. The server stores a sealed share and wraps of its
//! key, checks versions and who may write, and forgets a removed member's wraps.

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use serde_json::{json, Value};

const WS: &str = "6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d6d";
const SHARE: &str = "0123456789abcdef0123456789abcdef";
const VALUES: &[u8] = br#"{"v":1,"environment":"prod","values":{"API_TOKEN":"tok-very-secret"}}"#;

fn add(s: &Server, owner: &Device, d: &Device, email: &str, role: &str, key: &crypto::WorkspaceKey) {
    let wraps = wraps_for(&[(d.key_id.clone(), d.keys.public_key().to_vec())], WS, 1, key);
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&owner.session), Some(json!({ "email": email, "role": role, "wraps": wraps })));
    assert_eq!(r.status, 201, "{}", r.json);
}

fn wrap(from: &Device, to: &Device, version: u32, key: &crypto::WorkspaceKey) -> Value {
    let pk = to.keys.public_key();
    let w = crypto::seal_share_wrap(&crypto::Device { key_id: &from.key_id, keys: &from.keys }, &crypto::Recipient { key_id: &to.key_id, public_key: &pk }, WS, SHARE, version, key).unwrap();
    json!({ "keyId": to.key_id, "wrapped": b64(&w) })
}

fn put(s: &Server, who: &Device, version: u32, key: &crypto::WorkspaceKey, wraps: Vec<Value>) -> Reply {
    let sealed = crypto::seal_share(key, WS, SHARE, version, VALUES).unwrap();
    s.call("PUT", &format!("/v1/workspaces/{WS}/env-shares/{SHARE}"), Some(&who.session), Some(json!({ "version": version, "sealed": b64(&sealed), "wraps": wraps })))
}

#[test]
fn a_share_is_sealed_versioned_and_wrapped_per_device() {
    let s = Server::new("shares-basic");
    let ada = Device::new(&s, "ada@example.com");
    let wkey = create_workspace(&s, &ada, WS);
    let bob = Device::new(&s, "bob@example.com");
    add(&s, &ada, &bob, "bob@example.com", "editor", &wkey);
    let vic = Device::new(&s, "vic@example.com");
    add(&s, &ada, &vic, "vic@example.com", "viewer", &wkey);
    let eve = Device::new(&s, "eve@example.com");

    let k1 = crypto::WorkspaceKey::generate();
    assert_eq!(put(&s, &vic, 1, &k1, vec![wrap(&vic, &vic, 1, &k1)]).status, 403, "viewers don't share");
    assert_eq!(put(&s, &ada, 1, &k1, vec![wrap(&ada, &eve, 1, &k1)]).status, 400, "only members' devices");
    assert_eq!(put(&s, &ada, 1, &k1, vec![wrap(&bob, &bob, 1, &k1)]).status, 400, "a wrap names the caller's own key as sender");
    assert_eq!(put(&s, &ada, 2, &k1, vec![wrap(&ada, &ada, 2, &k1)]).status, 409, "a new share starts at 1");
    let r = put(&s, &ada, 1, &k1, vec![wrap(&ada, &ada, 1, &k1), wrap(&ada, &bob, 1, &k1)]);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(put(&s, &ada, 1, &k1, vec![wrap(&ada, &ada, 1, &k1)]).status, 409, "no going back to a version");

    // Bob opens it; the viewer sees the share exists but has no wrap.
    let list = s.call("GET", &format!("/v1/workspaces/{WS}/env-shares"), Some(&bob.session), None);
    let share = &list.json["shares"][0];
    let mine = share["wraps"].as_array().unwrap().iter().find(|w| w["keyId"] == bob.key_id.as_str()).unwrap();
    let key = crypto::open_share_wrap(&bob.keys, &bob.key_id, &ada.keys.public_key(), WS, SHARE, 1, &unb64(mine["wrapped"].as_str().unwrap())).unwrap();
    assert_eq!(crypto::open_share(&key, WS, SHARE, 1, &unb64(share["sealed"].as_str().unwrap())).unwrap(), VALUES);
    let seen = s.call("GET", &format!("/v1/workspaces/{WS}/env-shares"), Some(&vic.session), None);
    assert!(!seen.json["shares"][0]["wraps"].as_array().unwrap().iter().any(|w| w["keyId"] == vic.key_id.as_str()));
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/env-shares"), Some(&eve.session), None).status, 404);
    assert!(!s.disk_bytes().windows(b"tok-very-secret".len()).any(|w| w == b"tok-very-secret"), "ciphertext only");

    // A new device of Bob's gets a wrap of the current version, from a member who can open it.
    let bob2 = Device::another(&s, "bob@example.com", &bob.session);
    let add_wrap = |who: &Device, to: &Device, version: u32| s.call("POST", &format!("/v1/workspaces/{WS}/env-shares/{SHARE}/wraps"), Some(&who.session), Some(json!({ "version": version, "wraps": [wrap(who, to, version, &k1)] })));
    assert_eq!(add_wrap(&bob, &bob2, 2).status, 409, "the current version only");
    assert_eq!(add_wrap(&vic, &bob2, 1).status, 403, "viewers wrap for their own devices only");
    let r = add_wrap(&bob, &bob2, 1);
    assert_eq!((r.status, r.json["stored"].as_i64()), (200, Some(1)));
    assert_eq!(add_wrap(&bob, &bob2, 1).json["stored"], 0, "an existing wrap is never replaced");

    // Removing Bob deletes his devices' wraps; Ada re-keys the share for whoever is left.
    let k2w = crypto::WorkspaceKey::generate();
    let remaining = [(ada.key_id.clone(), ada.keys.public_key().to_vec()), (vic.key_id.clone(), vic.keys.public_key().to_vec())];
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": wraps_for(&remaining, WS, 2, &k2w) }))).status, 200);
    let left = s.call("GET", &format!("/v1/workspaces/{WS}/env-shares"), Some(&ada.session), None);
    assert_eq!(left.json["shares"][0]["wraps"].as_array().unwrap().len(), 1, "only Ada's");
    let k2 = crypto::WorkspaceKey::generate();
    assert_eq!(put(&s, &ada, 2, &k2, vec![wrap(&ada, &ada, 2, &k2)]).status, 200);
    let rows = audit_rows(&s, WS);
    assert!(rows.iter().filter(|r| r["event"] == "env_share_updated").count() == 2);
    // Unsharing.
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}/env-shares/{SHARE}"), Some(&vic.session), None).status, 403);
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}/env-shares/{SHARE}"), Some(&ada.session), None).status, 204);
    assert!(s.call("GET", &format!("/v1/workspaces/{WS}/env-shares"), Some(&ada.session), None).json["shares"].as_array().unwrap().is_empty());
}

#[test]
fn a_runner_sees_only_shares_wrapped_for_it() {
    let s = Server::new("shares-runner");
    let ada = Device::new(&s, "ada@example.com");
    let wkey = create_workspace(&s, &ada, WS);
    let rkeys = crypto::DeviceKeyPair::generate();
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&ada.session), Some(json!({ "name": "ci", "publicKey": b64(&rkeys.public_key()), "wrapped": b64(&crypto::wrap_key(&rkeys.public_key(), WS, 1, &wkey).unwrap()) })));
    let (token, rkey_id) = (r.json["token"].as_str().unwrap().to_string(), r.json["keyId"].as_str().unwrap().to_string());
    let k1 = crypto::WorkspaceKey::generate();
    assert_eq!(put(&s, &ada, 1, &k1, vec![wrap(&ada, &ada, 1, &k1)]).status, 200);
    assert!(s.call("GET", &format!("/v1/workspaces/{WS}/env-shares"), Some(&token), None).json["shares"].as_array().unwrap().is_empty());
    let pk = rkeys.public_key();
    let w = crypto::seal_share_wrap(&crypto::Device { key_id: &ada.key_id, keys: &ada.keys }, &crypto::Recipient { key_id: &rkey_id, public_key: &pk }, WS, SHARE, 2, &k1).unwrap();
    let sealed = crypto::seal_share(&k1, WS, SHARE, 2, VALUES).unwrap();
    let r = s.call("PUT", &format!("/v1/workspaces/{WS}/env-shares/{SHARE}"), Some(&ada.session), Some(json!({ "version": 2, "sealed": b64(&sealed), "wraps": [wrap(&ada, &ada, 2, &k1), { "keyId": rkey_id, "wrapped": b64(&w) }] })));
    assert_eq!(r.status, 200, "{}", r.json);
    let mine = s.call("GET", &format!("/v1/workspaces/{WS}/env-shares"), Some(&token), None);
    let share = &mine.json["shares"][0];
    let wr = share["wraps"].as_array().unwrap().iter().find(|x| x["keyId"] == rkey_id.as_str()).unwrap();
    let key = crypto::open_share_wrap(&rkeys, &rkey_id, &ada.keys.public_key(), WS, SHARE, 2, &unb64(wr["wrapped"].as_str().unwrap())).unwrap();
    assert_eq!(crypto::open_share(&key, WS, SHARE, 2, &unb64(share["sealed"].as_str().unwrap())).unwrap(), VALUES);
}

#[test]
fn shares_need_a_plan_that_has_them() {
    let s = Server::with("shares-plan", |c| c.plan_limits = true);
    let ada = Device::new(&s, "ada@example.com");
    create_workspace(&s, &ada, WS);
    let k1 = crypto::WorkspaceKey::generate();
    let r = put(&s, &ada, 1, &k1, vec![wrap(&ada, &ada, 1, &k1)]);
    assert_eq!((r.status, r.json["limit"].as_str()), (402, Some("sharedSecrets")), "{}", r.json);
    assert!(r.json["message"].as_str().unwrap().contains("has no shared environment values"));
    hitop_sync_server::limits::set_plan(s.app.state(), &format!("personal-{}", ada.user_id), "pro", 0).unwrap();
    assert_eq!(put(&s, &ada, 1, &k1, vec![wrap(&ada, &ada, 1, &k1)]).status, 200);
}
