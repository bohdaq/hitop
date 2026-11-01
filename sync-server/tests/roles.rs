//! Every role against every workspace operation: owner, editor, viewer, a runner credential, a
//! signed-in outsider and a removed member. The server answers 404 to anyone who isn't a member
//! (workspace ids can't be probed) and 403 to a member whose role is too low.

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use std::cell::{Cell, RefCell};

use serde_json::{json, Value};

const WS: &str = "aaaabbbbccccddddeeeeffff00001111";

struct Team {
    s: Server,
    owner: Device,
    editor: Device,
    viewer: Device,
    runner: Device,
    outsider: Device,
    removed: Device,
    /// Current epoch and its key.
    epoch: Cell<u32>,
    key: RefCell<crypto::WorkspaceKey>,
}

fn add(s: &Server, owner: &Device, d: &Device, email: &str, role: &str, epoch: u32, key: &crypto::WorkspaceKey) {
    let w = wraps_for(&[(d.key_id.clone(), d.keys.public_key().to_vec())], WS, epoch, key);
    let r = s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&owner.session), Some(json!({ "email": email, "role": role, "wraps": w })));
    assert_eq!(r.status, 201, "{}", r.json);
}

/// A runner credential made the way the clients make one: a fresh key pair, the current key
/// wrapped for it.
pub fn runner(s: &Server, owner: &Device, ws: &str, epoch: u32, key: &crypto::WorkspaceKey) -> Device {
    let keys = crypto::DeviceKeyPair::generate();
    let wrapped = crypto::wrap_key(&keys.public_key(), ws, epoch, key).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{ws}/runners"), Some(&owner.session), Some(json!({ "name": "ci", "publicKey": b64(&keys.public_key()), "wrapped": b64(&wrapped) })));
    assert_eq!(r.status, 201, "{}", r.json);
    Device { session: r.json["token"].as_str().unwrap().into(), key_id: r.json["keyId"].as_str().unwrap().into(), keys, user_id: r.json["userId"].as_str().unwrap().into() }
}

impl Team {
    fn new() -> Team {
        let s = Server::new("roles-matrix");
        let owner = Device::new(&s, "owner@example.com");
        let editor = Device::new(&s, "editor@example.com");
        let viewer = Device::new(&s, "viewer@example.com");
        let outsider = Device::new(&s, "outsider@example.com");
        let removed = Device::new(&s, "removed@example.com");
        let k1 = create_workspace(&s, &owner, WS);
        add(&s, &owner, &editor, "editor@example.com", "editor", 1, &k1);
        add(&s, &owner, &viewer, "viewer@example.com", "viewer", 1, &k1);
        add(&s, &owner, &removed, "removed@example.com", "editor", 1, &k1);
        let runner = runner(&s, &owner, WS, 1, &k1);
        // One blob so reads have something to read.
        let r = s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/b1"), Some(&owner.session), Some(json!({ "version": 1, "vv": { "o": 1 }, "epoch": 1, "content": seal(&k1, WS, "b1", 1, 1, b"one") })));
        assert_eq!(r.status, 200);
        let t = Team { s, owner, editor, viewer, runner, outsider, removed, epoch: Cell::new(1), key: RefCell::new(k1) };
        let removed_id = t.removed.user_id.clone();
        assert_eq!(t.remove(&removed_id, None).status, 200);
        t
    }

    fn actors(&self) -> [(&'static str, &Device); 6] {
        [("viewer", &self.viewer), ("runner", &self.runner), ("editor", &self.editor), ("outsider", &self.outsider), ("removed", &self.removed), ("owner", &self.owner)]
    }

    fn keys_except(&self, gone: &str) -> Vec<(String, Vec<u8>)> {
        let r = self.s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&self.owner.session), None);
        r.json["members"].as_array().unwrap().iter().filter(|m| m["userId"] != gone).flat_map(|m| m["keys"].as_array().unwrap().iter().map(|k| (k["id"].as_str().unwrap().to_string(), unb64(k["publicKey"].as_str().unwrap())))).collect()
    }

    /// Removes `user` as `by` (the owner when None) with a correct rotation.
    fn remove(&self, user: &str, by: Option<&Device>) -> Reply {
        let next = self.epoch.get() + 1;
        let key = crypto::WorkspaceKey::generate();
        let body = json!({ "epoch": next, "wraps": wraps_for(&self.keys_except(user), WS, next, &key) });
        let by = by.unwrap_or(&self.owner);
        let r = self.s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{user}"), Some(&by.session), Some(body));
        if r.status == 200 {
            self.epoch.set(next);
            *self.key.borrow_mut() = key;
        }
        r
    }

    fn rotate_body(&self) -> (crypto::WorkspaceKey, Value) {
        let key = crypto::WorkspaceKey::generate();
        let next = self.epoch.get() + 1;
        (key.clone(), json!({ "epoch": next, "wraps": wraps_for(&self.keys_except(""), WS, next, &key) }))
    }
}

/// What each actor gets, in the order of [`Team::actors`].
fn expect(t: &Team, what: &str, got: impl Fn(&str, &Device) -> i16, want: [i16; 6]) {
    for ((name, d), want) in t.actors().into_iter().zip(want) {
        assert_eq!(got(name, d), want, "{what} as {name}");
    }
}

const READ: [i16; 6] = [200, 200, 200, 404, 404, 200];
const OWNER_ONLY: [i16; 6] = [403, 403, 403, 404, 404, 0];

#[test]
fn every_role_against_every_operation() {
    let t = Team::new();
    let s = &t.s;

    // Reads: every member, runner included.
    expect(&t, "workspace", |_, d| s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&d.session), None).status, READ);
    expect(&t, "changes", |_, d| s.call("GET", &format!("/v1/workspaces/{WS}/changes?since=0"), Some(&d.session), None).status, READ);
    expect(&t, "blob", |_, d| s.call("GET", &format!("/v1/workspaces/{WS}/blobs/b1"), Some(&d.session), None).status, READ);
    expect(&t, "own wraps", |_, d| s.call("GET", &format!("/v1/workspaces/{WS}/wraps?keyId={}", d.key_id), Some(&d.session), None).status, READ);
    let listed = |d: &Device| s.call("GET", "/v1/workspaces", Some(&d.session), None).json["workspaces"].as_array().unwrap().len();
    assert_eq!((listed(&t.viewer), listed(&t.runner), listed(&t.outsider), listed(&t.removed)), (1, 1, 0, 0));

    // Writes and deletes: owner and editor.
    let (key, epoch) = (t.key.borrow().clone(), t.epoch.get());
    let write = |name: &str, d: &Device| {
        let blob = format!("w-{name}");
        s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/{blob}"), Some(&d.session), Some(json!({ "version": 1, "vv": { name: 1 }, "epoch": epoch, "content": seal(&key, WS, &blob, 1, epoch, b"x") }))).status
    };
    expect(&t, "write", write, [403, 403, 200, 404, 404, 200]);
    let delete = |name: &str, d: &Device| {
        let current = s.call("GET", &format!("/v1/workspaces/{WS}/blobs/w-owner"), Some(&t.owner.session), None).json;
        let mut vv = current["vv"].clone();
        vv[name] = json!(vv[name].as_i64().unwrap_or(0) + 1);
        let version = current["version"].as_i64().unwrap() + 1;
        s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/w-owner"), Some(&d.session), Some(json!({ "version": version, "vv": vv, "deleted": true, "epoch": epoch }))).status
    };
    expect(&t, "delete", delete, [403, 403, 200, 404, 404, 200]);

    // Wrapping the key for another member's device: owner and editor; viewers only for their own.
    let other = &t.editor;
    let wrap_for = |d: &Device, target: &Device| {
        let w = wraps_for(&[(target.key_id.clone(), target.keys.public_key().to_vec())], WS, epoch, &key);
        s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&d.session), Some(json!({ "epoch": epoch, "wraps": w }))).status
    };
    expect(&t, "wrap for another device", |_, d| wrap_for(d, if std::ptr::eq(d, other) { &t.owner } else { other }), [403, 403, 200, 404, 404, 200]);
    assert_eq!(wrap_for(&t.viewer, &t.viewer), 200, "a viewer wraps for its own devices");

    // Owner only: members, roles, invites, runners, rotation.
    let fay = Device::new(s, "fay@example.com");
    let add_fay = |_: &str, d: &Device| {
        let w = wraps_for(&[(fay.key_id.clone(), fay.keys.public_key().to_vec())], WS, epoch, &key);
        s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&d.session), Some(json!({ "email": "fay@example.com", "role": "viewer", "wraps": w }))).status
    };
    expect(&t, "add member", add_fay, with_owner(OWNER_ONLY, 201));
    let role = |_: &str, d: &Device| s.call("PUT", &format!("/v1/workspaces/{WS}/members/{}", fay.user_id), Some(&d.session), Some(json!({ "role": "editor" }))).status;
    expect(&t, "change a role", role, with_owner(OWNER_ONLY, 200));
    let invite = |_: &str, d: &Device| s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&d.session), Some(json!({ "role": "viewer" }))).status;
    expect(&t, "invite", invite, with_owner(OWNER_ONLY, 201));
    let invites = |_: &str, d: &Device| s.call("GET", &format!("/v1/workspaces/{WS}/invites"), Some(&d.session), None).status;
    expect(&t, "list invites", invites, with_owner(OWNER_ONLY, 200));
    let invite_id = s.call("GET", &format!("/v1/workspaces/{WS}/invites"), Some(&t.owner.session), None).json["invites"][0]["id"].as_str().unwrap().to_string();
    let drop_invite = |_: &str, d: &Device| s.call("DELETE", &format!("/v1/workspaces/{WS}/invites/{invite_id}"), Some(&d.session), None).status;
    expect(&t, "delete invite", drop_invite, with_owner(OWNER_ONLY, 204));
    let new_runner = |_: &str, d: &Device| {
        let kp = crypto::DeviceKeyPair::generate();
        let wrapped = crypto::wrap_key(&kp.public_key(), WS, epoch, &key).unwrap();
        s.call("POST", &format!("/v1/workspaces/{WS}/runners"), Some(&d.session), Some(json!({ "publicKey": b64(&kp.public_key()), "wrapped": b64(&wrapped) }))).status
    };
    expect(&t, "create runner", new_runner, with_owner(OWNER_ONLY, 201));
    let (k, rotation) = t.rotate_body();
    let rotate = |_: &str, d: &Device| s.call("POST", &format!("/v1/workspaces/{WS}/rotate"), Some(&d.session), Some(rotation.clone())).status;
    expect(&t, "rotate", rotate, with_owner(OWNER_ONLY, 200));
    t.epoch.set(t.epoch.get() + 1);
    *t.key.borrow_mut() = k;

    // Removing a member (with rotation): owner only.
    let fay_id = fay.user_id.clone();
    for (name, d) in [("viewer", &t.viewer), ("runner", &t.runner), ("editor", &t.editor)] {
        let d = Device { session: d.session.clone(), key_id: d.key_id.clone(), keys: d.keys.clone(), user_id: d.user_id.clone() };
        assert_eq!(t.remove(&fay_id, Some(&d)).status, 403, "remove as {name}");
    }
    let outsider = Device { session: t.outsider.session.clone(), key_id: String::new(), keys: t.outsider.keys.clone(), user_id: String::new() };
    assert_eq!(t.remove(&fay_id, Some(&outsider)).status, 404);
    assert_eq!(t.remove(&fay_id, None).status, 200);

    // Runner credentials do nothing outside their workspace.
    let r = &t.runner;
    assert_eq!(s.call("POST", "/v1/workspaces", Some(&r.session), Some(json!({ "id": "11112222333344445555666677778888", "wraps": [] }))).status, 403);
    assert_eq!(s.call("POST", "/v1/me/keys", Some(&r.session), Some(json!({ "publicKey": b64(&[7u8; 32]) }))).status, 403);
    assert_eq!(s.call("GET", "/v1/users/keys?email=owner@example.com", Some(&r.session), None).status, 403);
    assert_eq!(s.call("POST", "/v1/invites/accept", Some(&r.session), Some(json!({ "token": "x" }))).status, 403);
    assert_eq!(s.call("POST", "/v1/auth/passkey/register/start", Some(&r.session), None).status, 403);
    let me = s.call("GET", "/v1/me", Some(&r.session), None);
    let runner_email = me.json["email"].as_str().unwrap().to_string();
    assert!(runner_email.ends_with("@runner.invalid"));
    assert_eq!(s.call("POST", "/v1/auth/email", None, Some(json!({ "email": runner_email }))).status, 400, "no sign-in as a runner");
    let k = t.key.borrow().clone();
    let w = wraps_for(&[(r.key_id.clone(), r.keys.public_key().to_vec())], WS, t.epoch.get(), &k);
    assert_eq!(s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&t.owner.session), Some(json!({ "email": runner_email, "role": "editor", "wraps": w }))).status, 400);
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/members/{}", r.user_id), Some(&t.owner.session), Some(json!({ "role": "editor" }))).status, 400, "runners stay viewers");

    // The runner reads with the current key (it was in every rotation).
    assert_eq!(r.workspace_key(s, WS, t.epoch.get()), k);
    // Removing the runner ends its session.
    let runner_id = t.runner.user_id.clone();
    let runner_session = t.runner.session.clone();
    assert_eq!(t.remove(&runner_id, None).status, 200);
    assert_eq!(t.s.call("GET", "/v1/me", Some(&runner_session), None).status, 401);

    // Deleting the workspace: owner only, last.
    expect(&t, "delete workspace", |_, d| t.s.call("DELETE", &format!("/v1/workspaces/{WS}"), Some(&d.session), None).status, [403, 401, 403, 404, 404, 204]);
}

fn with_owner(mut v: [i16; 6], owner: i16) -> [i16; 6] {
    v[5] = owner;
    v
}

#[test]
fn a_removed_member_cant_decrypt_anything_written_after_the_rotation() {
    let s = Server::new("removed-decrypt");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let k1 = create_workspace(&s, &ada, WS);
    add(&s, &ada, &bob, "bob@example.com", "editor", 1, &k1);
    // Bob keeps a copy of everything he could read: epoch 1's key and the ciphertexts.
    let bob_k1 = bob.workspace_key(&s, WS, 1);
    assert_eq!(s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/x"), Some(&ada.session), Some(json!({ "version": 1, "vv": { "a": 1 }, "epoch": 1, "content": seal(&k1, WS, "x", 1, 1, b"old secret") }))).status, 200);

    let k2 = crypto::WorkspaceKey::generate();
    let remaining = [(ada.key_id.clone(), ada.keys.public_key().to_vec())];
    let r = s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": wraps_for(&remaining, WS, 2, &k2) })));
    assert_eq!(r.status, 200);
    // The owner's client re-encrypts the live blob under epoch 2, and writes a new one.
    for (blob, version, text) in [("x", 2u64, &b"old secret, re-encrypted"[..]), ("y", 1, b"new secret")] {
        let vv = if version == 1 { json!({ "a": 1 }) } else { json!({ "a": 2 }) };
        let r = s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/{blob}"), Some(&ada.session), Some(json!({ "version": version, "vv": vv, "epoch": 2, "content": seal(&k2, WS, blob, version, 2, text) })));
        assert_eq!(r.status, 200, "{}", r.json);
    }
    // Bob's session sees nothing any more.
    for path in [format!("/v1/workspaces/{WS}"), format!("/v1/workspaces/{WS}/changes?since=0"), format!("/v1/workspaces/{WS}/blobs/y"), format!("/v1/workspaces/{WS}/wraps?keyId={}", bob.key_id)] {
        assert_eq!(s.call("GET", &path, Some(&bob.session), None).status, 404, "{path}");
    }
    // Even with the server's database (every byte on disk) and his old key, he can't open the
    // new writes: they are under epoch 2, whose key was never wrapped for him.
    let disk = s.disk_bytes();
    for (blob, version) in [("x", 2u64), ("y", 1)] {
        let env = unb64(s.call("GET", &format!("/v1/workspaces/{WS}/blobs/{blob}"), Some(&ada.session), None).json["content"].as_str().unwrap());
        assert_eq!(crypto::blob_epoch(&env).unwrap(), 2);
        for epoch in [1, 2] {
            assert!(crypto::decrypt_blob(&bob_k1, &crypto::BlobContext { workspace_id: WS, blob_id: blob, version, epoch }, &env).is_err());
        }
        assert!(!contains(&disk, b"new secret") && !contains(&disk, b"re-encrypted"));
    }
    // Nothing wrapped for Bob's key under epoch 2 exists anywhere.
    for wrap in s.call("GET", &format!("/v1/workspaces/{WS}/wraps?keyId={}", ada.key_id), Some(&ada.session), None).json["wraps"].as_array().unwrap() {
        if wrap["epoch"] == 2 {
            let wrapped = unb64(wrap["wrapped"].as_str().unwrap());
            assert!(crypto::unwrap_key(&bob.keys, WS, 2, &wrapped).is_err());
        }
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}
