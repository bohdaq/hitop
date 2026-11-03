//! Recovery blobs (HITOP Cloud C2): the server stores, returns and deletes one opaque blob per
//! account, only for a signed-in device, audits each step, and lets a device delete its own unused
//! key. The client side (phrase, sealing, the lost-device flow) is in cli/tests/sync.rs and
//! webapp/tests/sync.cjs.

mod common;

use common::{b64, create_workspace, unb64, Device, Server};
use hitop_sync_crypto as crypto;
use serde_json::json;

const WS: &str = "cc00000000000000000000000000000c";

#[test]
fn a_blob_is_kept_per_account_for_signed_in_devices_only() {
    let s = Server::new("recovery-blob");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    assert_eq!(s.call("GET", "/v1/me/recovery", Some(&ada.session), None).status, 404);
    assert!(s.call("GET", "/v1/me", Some(&ada.session), None).json["recovery"].is_null());

    let phrase = crypto::RecoveryPhrase::generate();
    let sealed = crypto::seal_recovery(&phrase, &ada.user_id, b"{\"v\":1}").unwrap();
    let r = s.call("PUT", "/v1/me/recovery", Some(&ada.session), Some(json!({ "blob": b64(&sealed) })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert!(s.call("GET", "/v1/me", Some(&ada.session), None).json["recovery"]["updatedAt"].as_i64().is_some());

    let got = s.call("GET", "/v1/me/recovery", Some(&ada.session), None);
    assert_eq!(got.status, 200);
    assert_eq!(crypto::open_recovery(&phrase, &ada.user_id, &unb64(got.json["blob"].as_str().unwrap())).unwrap(), b"{\"v\":1}");
    assert_eq!(s.call("GET", "/v1/me/recovery", Some(&bob.session), None).status, 404, "Bob has none, and never Ada's");
    assert!(!s.disk_bytes().windows(7).any(|w| w == b"{\"v\":1}"), "the server holds ciphertext only");

    // Not for an access token, nor a runner credential, nor without a session.
    let t = s.call("POST", "/v1/me/tokens", Some(&ada.session), Some(json!({ "name": "ci", "scopes": ["read", "write"] })));
    let token = t.json["token"].as_str().unwrap();
    for (m, body) in [("GET", None), ("PUT", Some(json!({ "blob": b64(&sealed) }))), ("DELETE", None)] {
        assert_eq!(s.call(m, "/v1/me/recovery", Some(token), body.clone()).status, 403, "{m} with a token");
        assert_eq!(s.call(m, "/v1/me/recovery", None, body).status, 401, "{m} signed out");
    }
    let key = create_workspace(&s, &ada, WS);
    let runner_keys = crypto::DeviceKeyPair::generate();
    let wrapped = crypto::wrap_key(&runner_keys.public_key(), WS, 1, &key).unwrap();
    let runner = s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&ada.session), Some(json!({ "name": "ci", "publicKey": b64(&runner_keys.public_key()), "wrapped": b64(&wrapped) })));
    assert_eq!(runner.status, 201, "{}", runner.json);
    assert_eq!(s.call("GET", "/v1/me/recovery", Some(runner.json["token"].as_str().unwrap()), None).status, 403);

    // Size: empty and oversized blobs refused.
    assert_eq!(s.call("PUT", "/v1/me/recovery", Some(&ada.session), Some(json!({ "blob": "" }))).status, 413);
    // Unlock methods: `phrase` (the default) only, until the password one exists.
    assert_eq!(s.call("GET", "/v1/me/recovery?method=phrase", Some(&ada.session), None).status, 200);
    assert_eq!(s.call("PUT", "/v1/me/recovery?method=password", Some(&ada.session), Some(json!({ "blob": b64(&sealed) }))).status, 400);
    let huge = b64(&vec![1u8; 256 * 1024 + 1]);
    assert_eq!(s.call("PUT", "/v1/me/recovery", Some(&ada.session), Some(json!({ "blob": huge }))).status, 413);

    // Replace, delete; every step in the account's audit log.
    assert_eq!(s.call("PUT", "/v1/me/recovery", Some(&ada.session), Some(json!({ "blob": b64(&sealed) }))).status, 200);
    assert_eq!(s.call("DELETE", "/v1/me/recovery", Some(&ada.session), None).status, 204);
    assert_eq!(s.call("DELETE", "/v1/me/recovery", Some(&ada.session), None).status, 404);
    let log = s.call("GET", "/v1/me/audit", Some(&ada.session), None);
    let events: Vec<String> = log.json["events"].as_array().unwrap().iter().map(|e| e["event"].as_str().unwrap().to_string()).collect();
    for e in ["recovery_set", "recovery_fetched", "recovery_removed"] {
        assert!(events.iter().any(|x| x == e), "{e} in {events:?}");
    }
    let o = std::process::Command::new(env!("CARGO_BIN_EXE_hitop-sync-server")).arg("audit-verify").env("HITOP_SYNC_DATABASE", format!("sqlite:{}", s.dir.join("sync.db").display())).output().unwrap();
    assert!(String::from_utf8_lossy(&o.stdout).contains(" 0 broken"), "{}", String::from_utf8_lossy(&o.stdout));
}

#[test]
fn a_device_deletes_only_its_own_unused_keys() {
    let s = Server::new("recovery-keys");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    create_workspace(&s, &ada, WS);
    // The key a sign-in on a new computer registered, before the old device is recovered there.
    let fresh = Device::another(&s, "ada@example.com", &ada.session);
    let r = s.call("DELETE", &format!("/v1/me/keys/{}", ada.key_id), Some(&ada.session), None);
    assert_eq!((r.status, r.json["error"].as_str()), (409, Some("key_in_use")), "a key holding workspace keys stays");
    assert_eq!(s.call("DELETE", &format!("/v1/me/keys/{}", fresh.key_id), Some(&bob.session), None).status, 404, "not someone else's");
    assert_eq!(s.call("DELETE", &format!("/v1/me/keys/{}", fresh.key_id), Some(&ada.session), None).status, 204);
    let keys = s.call("GET", "/v1/me", Some(&ada.session), None).json["keys"].as_array().unwrap().len();
    assert_eq!(keys, 1);
    assert_eq!(s.call("DELETE", &format!("/v1/me/keys/{}", fresh.key_id), Some(&ada.session), None).status, 404);
}

#[test]
fn an_account_delete_takes_its_blob() {
    let s = Server::new("recovery-cascade");
    let ada = Device::new(&s, "ada@example.com");
    let sealed = crypto::seal_recovery(&crypto::RecoveryPhrase::generate(), &ada.user_id, b"x").unwrap();
    assert_eq!(s.call("PUT", "/v1/me/recovery", Some(&ada.session), Some(json!({ "blob": b64(&sealed) }))).status, 200);
    s.app.state().db.tx(|tx| tx.execute("DELETE FROM users WHERE id = $1", &[(&ada.user_id).into()])).unwrap();
    let n = s.app.state().db.tx(|tx| tx.query_one("SELECT COUNT(*) FROM recovery_blobs", &[])).unwrap().unwrap().int(0);
    assert_eq!(n, 0);
}
