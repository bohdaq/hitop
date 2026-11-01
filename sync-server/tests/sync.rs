//! Workspaces, members, key rotation, blob versioning and conflicts.

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use serde_json::json;

const WS: &str = "0123456789abcdef0123456789abcdef";

fn put(s: &Server, d: &Device, blob: &str, body: serde_json::Value) -> Reply {
    s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/{blob}"), Some(&d.session), Some(body))
}

#[test]
fn workspace_needs_wraps_for_every_device() {
    let s = Server::new("ws-create");
    let ada = Device::new(&s, "ada@example.com");
    let ada_phone = Device::another(&s, "ada@example.com", &ada.session);
    let key = crypto::WorkspaceKey::generate();
    let only_laptop = wraps_for(&[(ada.key_id.clone(), ada.keys.public_key().to_vec())], WS, 1, &key);
    let r = s.call("POST", "/v1/workspaces", Some(&ada.session), Some(json!({ "id": WS, "wraps": only_laptop })));
    assert_eq!(r.status, 400);
    assert_eq!(r.json["error"], "wraps_mismatch");
    assert_eq!(r.json["missing"], json!([ada_phone.key_id]));

    let both = wraps_for(&[(ada.key_id.clone(), ada.keys.public_key().to_vec()), (ada_phone.key_id.clone(), ada_phone.keys.public_key().to_vec())], WS, 1, &key);
    assert_eq!(s.call("POST", "/v1/workspaces", Some(&ada.session), Some(json!({ "id": "not-hex", "wraps": both }))).status, 400);
    assert_eq!(s.call("POST", "/v1/workspaces", Some(&ada.session), Some(json!({ "id": WS, "wraps": both }))).status, 201);
    assert_eq!(s.call("POST", "/v1/workspaces", Some(&ada.session), Some(json!({ "id": WS, "wraps": both }))).status, 409);
    // Both devices unwrap the same key.
    assert_eq!(ada.workspace_key(&s, WS, 1), key);
    assert_eq!(ada_phone.workspace_key(&s, WS, 1), key);
    // A device can't read another device's wraps.
    let r = s.call("GET", &format!("/v1/workspaces/{WS}/wraps?keyId={}", ada.key_id), Some(&Device::new(&s, "mal@example.com").session), None);
    assert_eq!(r.status, 404);
    let list = s.call("GET", "/v1/workspaces", Some(&ada.session), None);
    assert_eq!(list.json["workspaces"][0]["role"], "owner");
}

#[test]
fn versions_and_conflicts() {
    let s = Server::new("versions");
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let blob = "r-7f3a";

    // Version 1 of a new blob.
    let r = put(&s, &ada, blob, json!({ "version": 1, "vv": { "laptop": 1 }, "epoch": 1, "content": seal(&key, WS, blob, 1, 1, b"v1") }));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json, json!({ "version": 1, "seq": 1 }));
    // Writing version 1 again (a client that hadn't seen it) is a conflict with the current state.
    let r = put(&s, &ada, blob, json!({ "version": 1, "vv": { "phone": 1 }, "epoch": 1, "content": seal(&key, WS, blob, 1, 1, b"other v1") }));
    assert_eq!(r.status, 409);
    assert_eq!(r.json["error"], "conflict");
    assert_eq!(r.json["current"]["version"], 1);
    assert_eq!(r.json["current"]["vv"], json!({ "laptop": 1 }));
    // Skipping a version is a conflict too.
    assert_eq!(put(&s, &ada, blob, json!({ "version": 3, "vv": { "laptop": 3 }, "epoch": 1, "content": seal(&key, WS, blob, 3, 1, b"v3") })).status, 409);

    // Two devices edit concurrently from version 1: the first to write wins version 2 ...
    let laptop = put(&s, &ada, blob, json!({ "version": 2, "vv": { "laptop": 2 }, "epoch": 1, "content": seal(&key, WS, blob, 2, 1, b"laptop edit") }));
    assert_eq!(laptop.status, 200);
    let phone = put(&s, &ada, blob, json!({ "version": 2, "vv": { "laptop": 1, "phone": 1 }, "epoch": 1, "content": seal(&key, WS, blob, 2, 1, b"phone edit") }));
    assert_eq!(phone.status, 409);
    // ... the second merges and writes version 3 with a vector that descends from both.
    let concurrent = put(&s, &ada, blob, json!({ "version": 3, "vv": { "laptop": 1, "phone": 1 }, "epoch": 1, "content": seal(&key, WS, blob, 3, 1, b"x") }));
    assert_eq!(concurrent.status, 409, "a right version with a vector that doesn't descend is refused");
    assert!(concurrent.json["message"].as_str().unwrap().contains("vector"));
    let merged = put(&s, &ada, blob, json!({ "version": 3, "vv": { "laptop": 2, "phone": 1 }, "epoch": 1, "content": seal(&key, WS, blob, 3, 1, b"merged") }));
    assert_eq!(merged.status, 200, "{}", merged.json);

    let got = s.call("GET", &format!("/v1/workspaces/{WS}/blobs/{blob}"), Some(&ada.session), None);
    assert_eq!(got.json["version"], 3);
    assert_eq!(open(&key, WS, blob, 3, 1, got.json["content"].as_str().unwrap()), b"merged");
    // The server can't pass an old envelope off as the current version: the version is bound.
    let old = seal(&key, WS, blob, 2, 1, b"laptop edit");
    assert!(crypto::decrypt_blob(&key, &crypto::BlobContext { workspace_id: WS, blob_id: blob, version: 3, epoch: 1 }, &unb64(&old)).is_err());

    // Tombstone, then the change feed.
    let r = put(&s, &ada, blob, json!({ "version": 4, "vv": { "laptop": 3, "phone": 1 }, "epoch": 1, "deleted": true }));
    assert_eq!(r.status, 200, "{}", r.json);
    put(&s, &ada, "r-other", json!({ "version": 1, "vv": { "laptop": 1 }, "epoch": 1, "content": seal(&key, WS, "r-other", 1, 1, b"o") }));
    let feed = s.call("GET", &format!("/v1/workspaces/{WS}/changes?since=0"), Some(&ada.session), None);
    assert_eq!(feed.json["seq"], 5);
    let changes = feed.json["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["id"], blob);
    assert_eq!(changes[0]["deleted"], true);
    assert_eq!(changes[1]["id"], "r-other");
    let since = s.call("GET", &format!("/v1/workspaces/{WS}/changes?since=4"), Some(&ada.session), None);
    assert_eq!(since.json["changes"].as_array().unwrap().len(), 1);
}

#[test]
fn rejects_malformed_writes() {
    let s = Server::with("malformed", |c| {
        c.max_blob_bytes = 1024;
        c.max_workspace_bytes = 1500;
    });
    let ada = Device::new(&s, "ada@example.com");
    let key = create_workspace(&s, &ada, WS);
    let ok = seal(&key, WS, "a", 1, 1, b"x");
    let cases = [
        ("bad/id", json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1, "content": ok })),
        ("a", json!({ "version": 1, "vv": {}, "epoch": 1, "content": ok })),
        ("a", json!({ "version": 1, "vv": { "d": 0 }, "epoch": 1, "content": ok })),
        ("a", json!({ "version": 1, "vv": { "bad key!": 1 }, "epoch": 1, "content": ok })),
        ("a", json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1 })),
        ("a", json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1, "content": ok, "deleted": true })),
        ("a", json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1, "content": b64(b"plain text, not an envelope at all") })),
        ("a", json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1, "content": seal(&key, WS, "a", 1, 2, b"x") })),
    ];
    for (blob, body) in cases {
        let r = put(&s, &ada, blob, body.clone());
        assert!(r.status == 400 || r.status == 404, "{body}: {} {}", r.status, r.json);
    }
    let big = seal(&key, WS, "a", 1, 1, &[7u8; 2000]);
    assert_eq!(put(&s, &ada, "a", json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1, "content": big })).status, 413);
    let near = |id: &str| seal(&key, WS, id, 1, 1, &[7u8; 900]);
    assert_eq!(put(&s, &ada, "a", json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1, "content": near("a") })).status, 200);
    let over = put(&s, &ada, "b", json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1, "content": near("b") }));
    assert_eq!(over.status, 413);
    assert_eq!(over.json["error"], "quota");
}

#[test]
fn roles_are_enforced() {
    let s = Server::new("roles");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let cy = Device::new(&s, "cy@example.com");
    let key = create_workspace(&s, &ada, WS);
    let wraps = |d: &Device| wraps_for(&[(d.key_id.clone(), d.keys.public_key().to_vec())], WS, 1, &key);

    // Strangers see nothing.
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&bob.session), None).status, 404);
    assert_eq!(put(&s, &bob, "a", json!({ "version": 1, "vv": { "b": 1 }, "epoch": 1, "content": seal(&key, WS, "a", 1, 1, b"x") })).status, 404);

    let add = |who: &str, role: &str, w| s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": who, "role": role, "wraps": w })));
    assert_eq!(add("bob@example.com", "editor", wraps(&cy)).status, 400, "wraps must be for the new member's keys");
    assert_eq!(add("nobody@example.com", "editor", json!([])).status, 404);
    assert_eq!(add("bob@example.com", "admin", wraps(&bob)).status, 400);
    assert_eq!(add("bob@example.com", "editor", wraps(&bob)).status, 201);
    assert_eq!(add("bob@example.com", "editor", wraps(&bob)).status, 409);
    assert_eq!(add("cy@example.com", "viewer", wraps(&cy)).status, 201);

    // Editors write, viewers read.
    assert_eq!(bob.workspace_key(&s, WS, 1), key);
    assert_eq!(put(&s, &bob, "a", json!({ "version": 1, "vv": { "b": 1 }, "epoch": 1, "content": seal(&key, WS, "a", 1, 1, b"by bob") })).status, 200);
    let r = put(&s, &cy, "a", json!({ "version": 2, "vv": { "b": 1, "c": 1 }, "epoch": 1, "content": seal(&key, WS, "a", 2, 1, b"by cy") }));
    assert_eq!(r.status, 403);
    let got = s.call("GET", &format!("/v1/workspaces/{WS}/blobs/a"), Some(&cy.session), None);
    assert_eq!(open(&cy.workspace_key(&s, WS, 1), WS, "a", 1, 1, got.json["content"].as_str().unwrap()), b"by bob");

    // Only owners manage members, rotate or delete.
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&bob.session), Some(json!({ "email": "dee@example.com", "role": "viewer", "wraps": [] })));
    assert_eq!(r.status, 403);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/rotate"), Some(&bob.session), Some(json!({ "epoch": 2, "wraps": [] }))).status, 403);
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}"), Some(&bob.session), None).status, 403);
    // A viewer may wrap the key for their own new device only.
    let cy_phone = Device::another(&s, "cy@example.com", &cy.session);
    let for_phone = wraps_for(&[(cy_phone.key_id.clone(), cy_phone.keys.public_key().to_vec())], WS, 1, &key);
    let bob_tablet = Device::another(&s, "bob@example.com", &bob.session);
    let for_tablet = wraps_for(&[(bob_tablet.key_id.clone(), bob_tablet.keys.public_key().to_vec())], WS, 1, &key);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&cy.session), Some(json!({ "epoch": 1, "wraps": for_tablet }))).status, 403);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&cy.session), Some(json!({ "epoch": 1, "wraps": for_phone }))).status, 200);
    assert_eq!(cy_phone.workspace_key(&s, WS, 1), key);
    // Deleting the workspace (owner).
    assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).status, 204);
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&ada.session), None).status, 404);
}

#[test]
fn removing_a_member_rotates_the_key() {
    let s = Server::new("rotate");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let k1 = create_workspace(&s, &ada, WS);
    let bob_wraps = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &k1);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "editor", "wraps": bob_wraps }))).status, 201);
    assert_eq!(put(&s, &bob, "a", json!({ "version": 1, "vv": { "b": 1 }, "epoch": 1, "content": seal(&k1, WS, "a", 1, 1, b"before") })).status, 200);

    let remove = |body| s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), body);
    // Without the rotated key, or with the key still wrapped for Bob, the removal is refused.
    assert_eq!(remove(None).status, 400);
    let k2 = crypto::WorkspaceKey::generate();
    let everyone = member_keys(&s, WS, &ada.session);
    assert_eq!(remove(Some(json!({ "epoch": 2, "wraps": wraps_for(&everyone, WS, 2, &k2) }))).json["error"], "wraps_mismatch");
    let remaining = [(ada.key_id.clone(), ada.keys.public_key().to_vec())];
    assert_eq!(remove(Some(json!({ "epoch": 3, "wraps": wraps_for(&remaining, WS, 3, &k2) }))).status, 409);
    let r = remove(Some(json!({ "epoch": 2, "wraps": wraps_for(&remaining, WS, 2, &k2) })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["epoch"], 2);

    // Bob is out: no workspace, no wraps, no writes.
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&bob.session), None).status, 404);
    assert_eq!(s.call("GET", &format!("/v1/workspaces/{WS}/wraps?keyId={}", bob.key_id), Some(&bob.session), None).status, 404);
    assert_eq!(put(&s, &bob, "a", json!({ "version": 2, "vv": { "b": 2 }, "epoch": 1, "content": seal(&k1, WS, "a", 2, 1, b"x") })).status, 404);

    // Ada now has both epochs; writes under the old epoch are refused.
    assert_eq!(ada.workspace_key(&s, WS, 2), k2);
    assert_eq!(ada.workspace_key(&s, WS, 1), k1);
    let stale = put(&s, &ada, "a", json!({ "version": 2, "vv": { "b": 1, "a": 1 }, "epoch": 1, "content": seal(&k1, WS, "a", 2, 1, b"old key") }));
    assert_eq!(stale.status, 409);
    assert_eq!(stale.json["error"], "stale_epoch");
    assert_eq!(put(&s, &ada, "a", json!({ "version": 2, "vv": { "b": 1, "a": 1 }, "epoch": 2, "content": seal(&k2, WS, "a", 2, 2, b"after") })).status, 200);
    // Bob's old key doesn't open what was written after his removal.
    let got = s.call("GET", &format!("/v1/workspaces/{WS}/blobs/a"), Some(&ada.session), None);
    let env = unb64(got.json["content"].as_str().unwrap());
    let ctx = crypto::BlobContext { workspace_id: WS, blob_id: "a", version: 2, epoch: 2 };
    assert!(crypto::decrypt_blob(&k1, &ctx, &env).is_err());
    assert_eq!(crypto::decrypt_blob(&k2, &ctx, &env).unwrap(), b"after");

    // The last owner can't be removed, and a plain rotation needs every remaining device.
    let k3 = crypto::WorkspaceKey::generate();
    let r = s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", ada.user_id), Some(&ada.session), Some(json!({ "epoch": 3, "wraps": wraps_for(&remaining, WS, 3, &k3) })));
    assert_eq!(r.status, 400);
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/rotate"), Some(&ada.session), Some(json!({ "epoch": 3, "wraps": wraps_for(&remaining, WS, 3, &k3) })));
    assert_eq!(r.status, 200);
}
