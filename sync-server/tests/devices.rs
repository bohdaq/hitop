//! DD3: a person's devices: last use from `X-Hitop-Device`, renaming, removing one that holds
//! workspace keys (force: excluded, wraps gone, rotation requested, its sessions ended), and never
//! the device the request comes from.

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use serde_json::json;

const WS: &str = "d0d1d2d3d4d5d6d7d8d9dadbdcdddedf";

/// Ada with two devices, each with its own session; a workspace whose key both hold.
fn ada_with_two(s: &Server) -> (Device, Device) {
    let one = Device::new(s, "ada@example.com");
    let session2 = s.sign_in("ada@example.com");
    let two = Device::another(s, "ada@example.com", &session2);
    let key = crypto::WorkspaceKey::generate();
    let wraps = wraps_for(&[(one.key_id.clone(), one.keys.public_key().to_vec()), (two.key_id.clone(), two.keys.public_key().to_vec())], WS, 1, &key);
    let r = s.call("POST", "/v1/workspaces", Some(&one.session), Some(json!({ "id": WS, "wraps": wraps })));
    assert_eq!(r.status, 201, "{}", r.json);
    (one, two)
}

fn as_device(s: &Server, method: &str, path: &str, d: &Device, body: Option<serde_json::Value>) -> Reply {
    s.raw(method, path, Some(&d.session), body.map(|b| b.to_string().into_bytes()).unwrap_or_default(), &[("X-Hitop-Device", &d.key_id)])
}

#[test]
fn the_list_says_when_each_device_was_last_used() {
    let s = Server::new("devices-list");
    let (one, two) = ada_with_two(&s);
    let r = s.call("GET", "/v1/me/devices", Some(&one.session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    let d = r.json["devices"].as_array().unwrap();
    assert_eq!(d.len(), 2);
    assert!(d.iter().all(|x| x["lastUsedAt"].is_null() && x["thisDevice"] == false), "no header yet: never seen");
    assert!(d.iter().all(|x| x["workspaces"] == 1 && x["onlyHere"] == 0), "both hold the key");
    // A request that says which device it is marks it used, ties the session to it.
    let r = as_device(&s, "GET", "/v1/me/devices", &one, None);
    let d = r.json["devices"].as_array().unwrap();
    let me = d.iter().find(|x| x["id"] == one.key_id.as_str()).unwrap();
    assert_eq!(me["thisDevice"], true);
    assert!(me["lastUsedAt"].as_i64().unwrap() > 0);
    assert!(d.iter().find(|x| x["id"] == two.key_id.as_str()).unwrap()["lastUsedAt"].is_null());
    // Another person's key id in the header is ignored.
    let bob = Device::new(&s, "bob@example.com");
    let r = s.raw("GET", "/v1/me", Some(&bob.session), vec![], &[("X-Hitop-Device", &one.key_id)]);
    assert_eq!(r.status, 200);
    let row = s.app.state().db.tx(|tx| tx.query_one("SELECT device_key_id FROM sessions WHERE user_id = $1", &[bob.user_id.as_str().into()])).unwrap().unwrap();
    assert_eq!(row.text(0), "");
    // At most every 10 minutes.
    let at = |s: &Server| s.app.state().db.tx(|tx| tx.query_one("SELECT last_used_at FROM device_keys WHERE id = $1", &[one.key_id.as_str().into()])).unwrap().unwrap().int(0);
    let first = at(&s);
    s.advance(60);
    as_device(&s, "GET", "/v1/me", &one, None);
    assert_eq!(at(&s), first);
    s.advance(600);
    as_device(&s, "GET", "/v1/me", &one, None);
    assert!(at(&s) > first);
    // Not with an access token.
    assert_eq!(s.call("GET", "/v1/me/devices", None, None).status, 401);
}

#[test]
fn rename() {
    let s = Server::new("devices-rename");
    let (one, _) = ada_with_two(&s);
    let r = s.call("PUT", &format!("/v1/me/keys/{}", one.key_id), Some(&one.session), Some(json!({ "name": "  Work laptop " })));
    assert_eq!((r.status, r.json["name"].as_str()), (200, Some("Work laptop")));
    assert_eq!(s.call("PUT", &format!("/v1/me/keys/{}", one.key_id), Some(&one.session), Some(json!({ "name": " " }))).status, 400);
    assert_eq!(s.call("PUT", &format!("/v1/me/keys/{}", one.key_id), Some(&one.session), Some(json!({ "name": "x".repeat(81) }))).status, 400);
    let bob = Device::new(&s, "bob@example.com");
    assert_eq!(s.call("PUT", &format!("/v1/me/keys/{}", one.key_id), Some(&bob.session), Some(json!({ "name": "mine" }))).status, 404);
    let names: Vec<String> = s.call("GET", "/v1/me", Some(&one.session), None).json["keys"].as_array().unwrap().iter().map(|k| k["name"].as_str().unwrap().into()).collect();
    assert!(names.contains(&"Work laptop".to_string()));
    assert!(audit_rows(&s, "").iter().any(|r| r["event"] == "device_renamed"));
}

#[test]
fn removing_a_device_that_holds_keys_rotates_and_signs_it_out() {
    let s = Server::new("devices-remove");
    let (one, two) = ada_with_two(&s);
    // Device two says who it is, so its session is tied to it.
    assert_eq!(as_device(&s, "GET", "/v1/me", &two, None).status, 200);
    // Never the device the request comes from.
    let r = as_device(&s, "DELETE", &format!("/v1/me/keys/{}", one.key_id), &one, None);
    assert_eq!((r.status, r.json["error"].as_str()), (409, Some("this_device")));
    // Without force: says what it holds.
    let r = as_device(&s, "DELETE", &format!("/v1/me/keys/{}", two.key_id), &one, None);
    assert_eq!((r.status, r.json["error"].as_str()), (409, Some("key_in_use")), "{}", r.json);
    assert_eq!((r.json["workspaces"].as_i64(), r.json["onlyHere"].as_i64()), (Some(1), Some(0)));
    // With force: gone, its wraps gone, rotation asked for, its session ended.
    let r = as_device(&s, "DELETE", &format!("/v1/me/keys/{}?force=true", two.key_id), &one, None);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["rotationRequested"], json!([WS]));
    assert_eq!(r.json["sessionsEnded"], 1);
    let db = |sql: &str| s.app.state().db.tx(|tx| tx.query_one(sql, &[two.key_id.as_str().into()])).unwrap().map_or(0, |r| r.int(0));
    assert_eq!(db("SELECT COUNT(*) FROM wraps WHERE key_id = $1"), 0);
    assert_eq!(db("SELECT COUNT(*) FROM device_keys WHERE id = $1"), 0);
    assert_eq!(db("SELECT COUNT(*) FROM workspace_excluded_keys WHERE key_id = $1"), 0, "the key is gone, so nothing to exclude");
    let rotation = s.app.state().db.tx(|tx| tx.query_one("SELECT rotation_requested_at FROM workspaces WHERE id = $1", &[WS.into()])).unwrap().unwrap().int(0);
    assert!(rotation > 0);
    assert_eq!(s.call("GET", "/v1/me", Some(&two.session), None).status, 401, "the removed device is signed out");
    assert_eq!(s.call("GET", "/v1/me", Some(&one.session), None).status, 200, "this one isn't");
    let rows = audit_rows(&s, "");
    let removed = rows.iter().find(|r| r["event"] == "device_key_removed").unwrap();
    assert_eq!((removed["detail"]["workspaces"].as_i64(), removed["detail"]["sessionsEnded"].as_i64()), (Some(1), Some(1)));
    assert!(audit_rows(&s, WS).iter().any(|r| r["event"] == "device_removed"));
    // The only copy: onlyHere says so.
    let three = Device::another(&s, "ada@example.com", &one.session);
    let key = crypto::WorkspaceKey::generate();
    let ws2 = "e0e1e2e3e4e5e6e7e8e9eaebecedeeef";
    let wraps = wraps_for(&[(one.key_id.clone(), one.keys.public_key().to_vec()), (three.key_id.clone(), three.keys.public_key().to_vec())], ws2, 1, &key);
    let r = s.call("POST", "/v1/workspaces", Some(&one.session), Some(json!({ "id": ws2, "wraps": wraps })));
    assert_eq!(r.status, 201, "{}", r.json);
    // As if device one had never been given this workspace's key.
    s.app.state().db.tx(|tx| tx.execute("DELETE FROM wraps WHERE workspace_id = $1 AND key_id = $2", &[ws2.into(), one.key_id.as_str().into()])).unwrap();
    let r = as_device(&s, "DELETE", &format!("/v1/me/keys/{}", three.key_id), &one, None);
    assert_eq!(r.json["onlyHere"], 1, "{}", r.json);
}

#[test]
fn a_device_drops_its_own_fresh_key_and_untied_sessions_end_only_when_asked() {
    let s = Server::new("devices-fresh");
    let (one, two) = ada_with_two(&s);
    // A password unlock on a new computer: a fresh key (no wraps), dropped by the device itself.
    let fresh = Device::another(&s, "ada@example.com", &one.session);
    let r = as_device(&s, "DELETE", &format!("/v1/me/keys/{}", fresh.key_id), &fresh, None);
    assert!(r.status == 204 || r.status == 200, "{} {}", r.status, r.json);
    assert_eq!(s.call("GET", "/v1/me", Some(&one.session), None).status, 200, "the requester stays signed in");
    // An older client's session: tied to no device.
    let old = s.sign_in("ada@example.com");
    let three = Device::another(&s, "ada@example.com", &old);
    // Removing another device without `untied`: the untied session stays.
    let r = as_device(&s, "DELETE", &format!("/v1/me/keys/{}", three.key_id), &one, None);
    assert!(r.status == 204 || r.status == 200, "{}", r.json);
    assert_eq!(s.call("GET", "/v1/me", Some(&old), None).status, 200);
    // With `untied=true` (the Devices page): it ends, the requester's doesn't.
    let r = as_device(&s, "DELETE", &format!("/v1/me/keys/{}?force=true&untied=true", two.key_id), &one, None);
    assert_eq!(r.status, 200, "{}", r.json);
    assert!(r.json["untiedSessionsEnded"].as_u64().unwrap() >= 1, "{}", r.json);
    assert_eq!(s.call("GET", "/v1/me", Some(&old), None).status, 401);
    assert_eq!(s.call("GET", "/v1/me", Some(&one.session), None).status, 200);
}
