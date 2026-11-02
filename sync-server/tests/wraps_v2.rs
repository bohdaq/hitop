//! Format 2 (authenticated) wrapped keys, hitop 1.2: the server stores them next to format 1
//! wraps, checks only their shape and that they name one of the caller's own device keys as
//! sender, lets a format 2 wrap replace a format 1 wrap (an owner upgrading a workspace) but
//! never the reverse, and reports which wraps are still format 1.

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use serde_json::{json, Value};

const WS: &str = "0a1b2c3d4e5f60718293a4b5c6d7e8f9";

fn v2(sender: &Device, to: &(String, Vec<u8>), epoch: u32, head: u32, key: &crypto::WorkspaceKey) -> Value {
    let w = crypto::seal_wrap(&crypto::Device { key_id: &sender.key_id, keys: &sender.keys }, &crypto::Recipient { key_id: &to.0, public_key: &to.1 }, WS, epoch, head, key).unwrap();
    json!({ "keyId": to.0, "wrapped": b64(&w) })
}

fn me(d: &Device) -> (String, Vec<u8>) {
    (d.key_id.clone(), d.keys.public_key().to_vec())
}

fn legacy_epochs(s: &Server, session: &str, key_id: &str) -> Value {
    let info = s.call("GET", &format!("/v1/workspaces/{WS}"), Some(session), None);
    info.json["members"].as_array().unwrap().iter().flat_map(|m| m["keys"].as_array().unwrap().clone()).find(|k| k["id"] == key_id).unwrap()["legacyEpochs"].clone()
}

fn stored(s: &Server, key_id: &str, epoch: i64) -> Vec<u8> {
    let rows = s.app.state().db.tx(|tx| tx.query("SELECT wrapped FROM wraps WHERE workspace_id = $1 AND key_id = $2 AND epoch = $3", &[WS.into(), key_id.into(), epoch.into()])).unwrap();
    rows[0].bytes(0)
}

#[test]
fn format_2_wraps_are_stored_and_upgrade_format_1_wraps() {
    let s = Server::new("wraps-v2");
    let ada = Device::new(&s, "ada@example.com");
    let key = crypto::WorkspaceKey::generate();
    // A workspace made with a format 2 wrap.
    let r = s.call("POST", "/v1/workspaces", Some(&ada.session), Some(json!({ "id": WS, "wraps": [v2(&ada, &me(&ada), 1, 1, &key)] })));
    assert_eq!(r.status, 201, "{}", r.json);
    assert_eq!(legacy_epochs(&s, &ada.session, &ada.key_id), json!([]));
    let got = s.call("GET", &format!("/v1/workspaces/{WS}/wraps?keyId={}", ada.key_id), Some(&ada.session), None);
    let wrapped = unb64(got.json["wraps"][0]["wrapped"].as_str().unwrap());
    let opened = crypto::open_wrap(&ada.keys, &ada.key_id, &ada.keys.public_key(), WS, 1, &wrapped).unwrap();
    assert_eq!(opened.key, key);

    // Bob is added by a hitop 1.1 client (format 1 wrap): reported as legacy.
    let bob = Device::new(&s, "bob@example.com");
    let w1 = wraps_for(&[me(&bob)], WS, 1, &key);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "editor", "wraps": w1 }))).status, 201);
    assert_eq!(legacy_epochs(&s, &ada.session, &bob.key_id), json!([1]));
    assert_eq!(stored(&s, &bob.key_id, 1).len(), 80);

    // The owner's 1.2 client replaces it with a format 2 wrap.
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&ada.session), Some(json!({ "epoch": 1, "wraps": [v2(&ada, &me(&bob), 1, 1, &key)] })));
    assert_eq!((r.status, r.json["stored"].as_u64()), (200, Some(1)), "{}", r.json);
    assert_eq!(legacy_epochs(&s, &ada.session, &bob.key_id), json!([]));
    let upgraded = stored(&s, &bob.key_id, 1);
    assert_eq!(upgraded[0], 2);
    assert_eq!(crypto::open_wrap(&bob.keys, &bob.key_id, &ada.keys.public_key(), WS, 1, &upgraded).unwrap().key, key);

    // Never back to format 1, and a format 2 wrap isn't replaced by another one.
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&ada.session), Some(json!({ "epoch": 1, "wraps": wraps_for(&[me(&bob)], WS, 1, &key) })));
    assert_eq!((r.status, r.json["stored"].as_u64()), (200, Some(0)));
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&ada.session), Some(json!({ "epoch": 1, "wraps": [v2(&ada, &me(&bob), 1, 1, &crypto::WorkspaceKey::generate())] })));
    assert_eq!((r.status, r.json["stored"].as_u64()), (200, Some(0)));
    assert_eq!(stored(&s, &bob.key_id, 1), upgraded);

    // Rotation with format 2 wraps for everyone.
    let k2 = crypto::WorkspaceKey::generate();
    let wraps = json!([v2(&ada, &me(&ada), 2, 2, &k2), v2(&ada, &me(&bob), 2, 2, &k2)]);
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/rotate"), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": wraps })));
    assert_eq!(r.status, 200, "{}", r.json);

    // A runner made with a format 2 wrap.
    let kp = crypto::DeviceKeyPair::generate();
    let runner = crypto::seal_wrap(&crypto::Device { key_id: &ada.key_id, keys: &ada.keys }, &crypto::Recipient { key_id: "pending", public_key: &kp.public_key() }, WS, 2, 2, &k2).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&ada.session), Some(json!({ "publicKey": b64(&kp.public_key()), "wrapped": b64(&runner) })));
    assert_eq!(r.status, 201, "{}", r.json);
}

#[test]
fn a_format_2_wrap_must_name_one_of_the_callers_own_keys_and_be_well_formed() {
    let s = Server::new("wraps-v2-sender");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let key = create_workspace(&s, &ada, WS);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "editor", "wraps": wraps_for(&[me(&bob)], WS, 1, &key) }))).status, 201);
    let ada2 = Device::another(&s, "ada@example.com", &ada.session);
    // Bob (an editor) can't hand Ada's new device a wrap claiming to come from Ada's key.
    let pretend = Device { session: bob.session.clone(), key_id: ada.key_id.clone(), keys: crypto::DeviceKeyPair::generate(), user_id: bob.user_id.clone() };
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&bob.session), Some(json!({ "epoch": 1, "wraps": [v2(&pretend, &me(&ada2), 1, 1, &key)] })));
    assert_eq!(r.status, 400, "{}", r.json);
    assert!(r.json["message"].as_str().unwrap().contains("not one of your device keys"), "{}", r.json);
    // His own key as sender is fine (Ada's client verifies it against her pin of Bob's key).
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&bob.session), Some(json!({ "epoch": 1, "wraps": [v2(&bob, &me(&ada2), 1, 1, &key)] })));
    assert_eq!(r.status, 200, "{}", r.json);
    // Malformed shapes: wrong format byte, length prefix that doesn't match, empty sender.
    let good = crypto::seal_wrap(&crypto::Device { key_id: &ada.key_id, keys: &ada.keys }, &crypto::Recipient { key_id: &bob.key_id, public_key: &bob.keys.public_key() }, WS, 1, 1, &key).unwrap();
    let mut bad_format = good.clone();
    bad_format[0] = 3;
    let mut bad_len = good.clone();
    bad_len[6] += 1;
    let empty = [&[2u8, 0, 0, 0, 1, 0, 0][..], &good[good.len() - 80..]].concat();
    for (name, bytes) in [("format", bad_format), ("length", bad_len), ("empty sender", empty), ("short", good[..60].to_vec())] as [(&str, Vec<u8>); 4] {
        let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&ada.session), Some(json!({ "epoch": 1, "wraps": [{ "keyId": ada2.key_id, "wrapped": b64(&bytes) }] })));
        assert_eq!(r.status, 400, "{name}: {}", r.json);
    }
    // The same rules on create, add and rotate (exact_wraps).
    let k2 = crypto::WorkspaceKey::generate();
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/rotate"), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": [v2(&bob, &me(&ada), 2, 2, &k2), v2(&ada, &me(&ada2), 2, 2, &k2), v2(&ada, &me(&bob), 2, 2, &k2)] })));
    assert_eq!(r.status, 400, "{}", r.json);
}

#[test]
fn an_invite_code_with_the_inviters_fingerprint_is_accepted() {
    let s = Server::new("wraps-v2-invite");
    let ada = Device::new(&s, "ada@example.com");
    create_workspace(&s, &ada, WS);
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&ada.session), Some(json!({ "role": "viewer" })));
    let token = r.json["token"].as_str().unwrap();
    let bob = Device::new(&s, "bob@example.com");
    let code = format!("{token}.{}", crypto::fingerprint(&ada.keys.public_key()).replace(' ', ""));
    let r = s.call("POST", "/v1/invites/accept", Some(&bob.session), Some(json!({ "token": code })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["workspaceId"], WS);
}
