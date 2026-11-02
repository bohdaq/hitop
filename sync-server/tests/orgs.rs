//! Organizations: every org role (owner, admin, member) and the outsiders (the owner of another
//! org, a runner credential, a workspace collaborator who isn't in the org) against every org
//! operation; personal orgs; org roles give no workspace access ("other org"); personal access
//! tokens. Anyone who isn't a member gets 404, a member whose role is too low 403.

mod common;

use common::*;
use serde_json::{json, Value};

struct T {
    s: Server,
    owner: Device,
    admin: Device,
    member: Device,
    /// Owner of another org.
    other: Device,
    org: String,
    other_org: String,
}

fn org_create(s: &Server, d: &Device, name: &str) -> String {
    let r = s.call("POST", "/v1/orgs", Some(&d.session), Some(json!({ "name": name })));
    assert_eq!(r.status, 201, "{}", r.json);
    r.json["id"].as_str().unwrap().to_string()
}

fn add(s: &Server, by: &Device, org: &str, email: &str, role: &str) -> Reply {
    s.call("POST", &format!("/v1/orgs/{org}/members"), Some(&by.session), Some(json!({ "email": email, "role": role })))
}

impl T {
    fn new(name: &str) -> T {
        let s = Server::new(name);
        let owner = Device::new(&s, "owner@example.com");
        let admin = Device::new(&s, "admin@example.com");
        let member = Device::new(&s, "member@example.com");
        let other = Device::new(&s, "other@example.com");
        let org = org_create(&s, &owner, "Acme");
        let other_org = org_create(&s, &other, "Globex");
        assert_eq!(add(&s, &owner, &org, "admin@example.com", "admin").status, 201);
        assert_eq!(add(&s, &owner, &org, "member@example.com", "member").status, 201);
        T { s, owner, admin, member, other, org, other_org }
    }

    fn actors(&self) -> [(&'static str, &Device); 4] {
        [("owner", &self.owner), ("admin", &self.admin), ("member", &self.member), ("other", &self.other)]
    }

    fn call(&self, d: &Device, method: &str, path: &str, body: Option<Value>) -> Reply {
        self.s.call(method, &path.replace("{org}", &self.org), Some(&d.session), body)
    }
}

/// Expected status per actor (owner, admin, member, other).
fn expect(t: &T, what: &str, mut op: impl FnMut(&T, &Device) -> Reply, want: [u16; 4]) {
    for ((who, d), status) in t.actors().into_iter().zip(want) {
        let r = op(t, d);
        assert_eq!(r.status as u16, status, "{what} as {who}: {}", r.json);
    }
}

#[test]
fn reads_by_role() {
    let t = T::new("orgs-reads");
    for path in ["/v1/orgs/{org}", "/v1/orgs/{org}/members", "/v1/orgs/{org}/usage", "/v1/orgs/{org}/limits"] {
        expect(&t, path, |t, d| t.call(d, "GET", path, None), [200, 200, 200, 404]);
    }
    let list = t.call(&t.member, "GET", "/v1/orgs", None);
    let ids: Vec<&str> = list.json["orgs"].as_array().unwrap().iter().map(|o| o["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [format!("personal-{}", t.member.user_id).as_str(), t.org.as_str()], "personal first");
    assert_eq!(list.json["orgs"][1]["role"], "member");
    assert!(!t.call(&t.other, "GET", "/v1/orgs", None).json.to_string().contains(&t.org), "another org's id isn't listed");
    let members = t.call(&t.member, "GET", "/v1/orgs/{org}/members", None);
    let roles: Vec<(String, String)> = members.json["members"].as_array().unwrap().iter().map(|m| (m["email"].as_str().unwrap().into(), m["role"].as_str().unwrap().into())).collect();
    assert_eq!(roles, [("admin@example.com".into(), "admin".into()), ("member@example.com".into(), "member".into()), ("owner@example.com".into(), "owner".into())]);
    // A runner credential has no orgs.
    let ws = "0123456789abcdef0123456789abcdef";
    let key = create_workspace(&t.s, &t.owner, ws);
    let runner = runner(&t.s, &t.owner, ws, &key);
    assert_eq!(t.s.call("GET", "/v1/orgs", Some(&runner), None).status, 403);
    assert_eq!(t.s.call("GET", &format!("/v1/orgs/{}", t.org), Some(&runner), None).status, 403);
}

fn runner(s: &Server, owner: &Device, ws: &str, key: &hitop_sync_crypto::WorkspaceKey) -> String {
    let keys = hitop_sync_crypto::DeviceKeyPair::generate();
    let wrapped = hitop_sync_crypto::wrap_key(&keys.public_key(), ws, 1, key).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{ws}/runners"), Some(&owner.session), Some(json!({ "name": "ci", "publicKey": b64(&keys.public_key()), "wrapped": b64(&wrapped) })));
    assert_eq!(r.status, 201, "{}", r.json);
    r.json["token"].as_str().unwrap().to_string()
}

#[test]
fn rename_and_delete_by_role() {
    let t = T::new("orgs-rename");
    expect(&t, "rename", |t, d| t.call(d, "PUT", "/v1/orgs/{org}", Some(json!({ "name": format!("Acme {}", d.user_id) }))), [200, 200, 403, 404]);
    assert_eq!(t.call(&t.owner, "PUT", "/v1/orgs/{org}", Some(json!({ "name": "  " }))).status, 400);
    assert_eq!(t.call(&t.owner, "PUT", "/v1/orgs/{org}", Some(json!({ "name": "x".repeat(65) }))).status, 400);
    // Not while it owns a workspace.
    let ws = "11111111111111111111111111111111";
    let key = hitop_sync_crypto::WorkspaceKey::generate();
    let wraps = wraps_for(&[(t.member.key_id.clone(), t.member.keys.public_key().to_vec())], ws, 1, &key);
    assert_eq!(t.call(&t.member, "POST", "/v1/orgs/{org}/workspaces", Some(json!({ "id": ws, "wraps": wraps }))).status, 201);
    expect(&t, "delete with a workspace", |t, d| t.call(d, "DELETE", "/v1/orgs/{org}", None), [409, 403, 403, 404]);
    assert_eq!(t.s.call("DELETE", &format!("/v1/workspaces/{ws}"), Some(&t.member.session), None).status, 204);
    expect(&t, "delete", |t, d| t.call(d, "DELETE", "/v1/orgs/{org}", None), [204, 404, 404, 404]);
    assert_eq!(t.call(&t.owner, "GET", "/v1/orgs/{org}", None).status, 404);
    let rows = audit_rows(&t.s, &format!("org:{}", t.org));
    let events: Vec<&str> = rows.iter().map(|r| r["event"].as_str().unwrap()).collect();
    assert_eq!(events, ["org_created", "org_member_added", "org_member_added", "org_renamed", "org_renamed", "org_deleted"]);
}

#[test]
fn members_by_role() {
    let t = T::new("orgs-members");
    for e in ["n1", "n2", "n3"] {
        Device::new(&t.s, &format!("{e}@example.com"));
    }
    let mut n = 0;
    expect(
        &t,
        "add a member",
        |t, d| {
            n += 1;
            add(&t.s, d, &t.org, &format!("n{n}@example.com"), "member")
        },
        [201, 201, 403, 404],
    );
    assert_eq!(add(&t.s, &t.owner, &t.org, "admin@example.com", "member").status, 409, "already a member");
    // Owners only by owners.
    assert_eq!(add(&t.s, &t.admin, &t.org, "n3@example.com", "owner").status, 403);
    assert_eq!(add(&t.s, &t.owner, &t.org, "n3@example.com", "owner").status, 201);
    // Someone without an account yet: a pending invite, which becomes a membership on sign-in.
    let r = add(&t.s, &t.admin, &t.org, "later@example.com", "member");
    assert_eq!((r.status, r.json["status"].as_str()), (201, Some("invited")), "{}", r.json);
    assert!(t.s.outbox.lock().unwrap().iter().any(|m| m.to == "later@example.com" && m.body.contains("Acme")));
    assert_eq!(add(&t.s, &t.owner, &t.org, "later@example.com", "member").status, 409);
    let later = Device::new(&t.s, "later@example.com");
    let orgs = t.s.call("GET", "/v1/orgs", Some(&later.session), None);
    assert_eq!(orgs.json["orgs"][1]["id"], t.org.as_str());
    assert!(t.call(&t.owner, "GET", "/v1/orgs/{org}/members", None).json["invites"].as_array().unwrap().is_empty());
    // Deleting an invite (admin).
    assert_eq!(add(&t.s, &t.owner, &t.org, "gone@example.com", "member").status, 201);
    assert_eq!(t.call(&t.member, "DELETE", "/v1/orgs/{org}/invites/gone%40example.com", None).status, 403);
    assert_eq!(t.call(&t.admin, "DELETE", "/v1/orgs/{org}/invites/gone%40example.com", None).status, 204);
    assert_eq!(t.call(&t.admin, "DELETE", "/v1/orgs/{org}/invites/gone%40example.com", None).status, 404);

    // Roles.
    let member_id = t.member.user_id.clone();
    let set = |by: &Device, user: &str, role: &str| t.s.call("PUT", &format!("/v1/orgs/{}/members/{user}", t.org), Some(&by.session), Some(json!({ "role": role })));
    assert_eq!(set(&t.member, &member_id, "admin").status, 403);
    assert_eq!(set(&t.other, &member_id, "admin").status, 404);
    assert_eq!(set(&t.admin, &member_id, "admin").status, 200);
    assert_eq!(set(&t.admin, &member_id, "member").status, 200);
    assert_eq!(set(&t.admin, &member_id, "owner").status, 403, "an admin can't make owners");
    assert_eq!(set(&t.admin, &t.owner.user_id, "member").status, 403, "nor demote one");
    assert_eq!(set(&t.owner, &member_id, "boss").status, 400);
    // The last owner stays.
    let n3 = t.call(&t.owner, "GET", "/v1/orgs/{org}/members", None).json["members"].as_array().unwrap().iter().find(|m| m["email"] == "n3@example.com").unwrap()["userId"].as_str().unwrap().to_string();
    assert_eq!(set(&t.owner, &n3, "member").status, 200);
    assert_eq!(set(&t.owner, &t.owner.user_id, "admin").status, 400);

    // Removing.
    let remove = |by: &Device, user: &str| t.s.call("DELETE", &format!("/v1/orgs/{}/members/{user}", t.org), Some(&by.session), None);
    assert_eq!(remove(&t.member, &n3).status, 403);
    assert_eq!(remove(&t.other, &n3).status, 404);
    assert_eq!(remove(&t.admin, &t.owner.user_id).status, 403, "an admin can't remove an owner");
    assert_eq!(remove(&t.owner, &t.owner.user_id).status, 400, "the last owner can't leave");
    assert_eq!(remove(&t.admin, &n3).status, 204);
    assert_eq!(remove(&t.member, &member_id).status, 204, "a member may leave");
    assert_eq!(t.call(&t.member, "GET", "/v1/orgs/{org}", None).status, 404, "and is out");
    assert_eq!(remove(&t.owner, "no-such-user").status, 404);
    let events: Vec<String> = audit_rows(&t.s, &format!("org:{}", t.org)).iter().map(|r| r["event"].as_str().unwrap().to_string()).collect();
    for e in ["org_member_added", "org_invite_created", "org_invite_accepted", "org_invite_deleted", "org_role_changed", "org_member_removed"] {
        assert!(events.iter().any(|x| x == e), "{e} in {events:?}");
    }
}

#[test]
fn workspaces_in_orgs() {
    let t = T::new("orgs-workspaces");
    let key = hitop_sync_crypto::WorkspaceKey::generate();
    let ids = ["a0000000000000000000000000000000", "a1000000000000000000000000000000", "a2000000000000000000000000000000", "a3000000000000000000000000000000"];
    let mut i = 0;
    expect(
        &t,
        "create a workspace in the org",
        |t, d| {
            let ws = ids[i];
            i += 1;
            let wraps = wraps_for(&[(d.key_id.clone(), d.keys.public_key().to_vec())], ws, 1, &key);
            t.call(d, "POST", "/v1/orgs/{org}/workspaces", Some(json!({ "id": ws, "wraps": wraps })))
        },
        [201, 201, 201, 404],
    );
    // POST /v1/workspaces with `org` does the same; an org the caller isn't in is 404.
    let ws = "b0000000000000000000000000000000";
    let wraps = wraps_for(&[(t.other.key_id.clone(), t.other.keys.public_key().to_vec())], ws, 1, &key);
    assert_eq!(t.s.call("POST", "/v1/workspaces", Some(&t.other.session), Some(json!({ "id": ws, "org": t.org, "wraps": wraps.clone() }))).status, 404);
    assert_eq!(t.s.call("POST", "/v1/workspaces", Some(&t.other.session), Some(json!({ "id": ws, "org": t.other_org, "wraps": wraps }))).status, 201);
    let list = t.s.call("GET", "/v1/workspaces", Some(&t.owner.session), None);
    assert_eq!(list.json["workspaces"][0]["org"], json!({ "id": t.org, "name": "Acme", "personal": false }));

    // Moving a personal workspace into the org: its owner, if they're in the org.
    let mine = "c0000000000000000000000000000000";
    let k2 = create_workspace(&t.s, &t.member, mine);
    let w = wraps_for(&[(t.admin.key_id.clone(), t.admin.keys.public_key().to_vec())], mine, 1, &k2);
    assert_eq!(t.s.call("POST", &format!("/v1/workspaces/{mine}/members"), Some(&t.member.session), Some(json!({ "email": "admin@example.com", "role": "editor", "wraps": w }))).status, 201);
    let mv = |d: &Device, org: &str| t.s.call("POST", &format!("/v1/orgs/{org}/workspaces"), Some(&d.session), Some(json!({ "workspace": mine })));
    assert_eq!(mv(&t.admin, &t.org).status, 403, "an editor of the workspace can't move it");
    assert_eq!(mv(&t.other, &t.org).status, 404);
    assert_eq!(mv(&t.member, &t.other_org).status, 404, "not into an org they aren't in");
    let r = mv(&t.member, &t.org);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(t.s.call("GET", "/v1/workspaces", Some(&t.member.session), None).json["workspaces"].as_array().unwrap().iter().find(|w| w["id"] == mine).unwrap()["org"]["id"], t.org.as_str());
    assert_eq!(audit_rows(&t.s, mine).last().unwrap()["event"], "workspace_moved");
    // Back to the personal org.
    assert_eq!(mv(&t.member, &format!("personal-{}", t.member.user_id)).status, 200);
    let usage = t.call(&t.owner, "GET", "/v1/orgs/{org}/usage", None);
    assert_eq!(usage.json["usage"]["workspaces"], 3);

    // Org roles give no workspace access: the org's owner and admin aren't members of the
    // member's workspace, and get 404 like anyone else.
    let member_ws = ids[2];
    for d in [&t.owner, &t.admin, &t.other] {
        for (m, p) in [("GET", format!("/v1/workspaces/{member_ws}")), ("GET", format!("/v1/workspaces/{member_ws}/changes")), ("DELETE", format!("/v1/workspaces/{member_ws}"))] {
            assert_eq!(t.s.call(m, &p, Some(&d.session), None).status, 404, "{m} {p}");
        }
    }
}

#[test]
fn personal_orgs() {
    let t = T::new("orgs-personal");
    let personal = format!("personal-{}", t.owner.user_id);
    let p = |path: &str| path.replace("{p}", &personal);
    assert_eq!(t.s.call("GET", &p("/v1/orgs/{p}"), Some(&t.owner.session), None).json["personal"], true);
    assert_eq!(t.s.call("PUT", &p("/v1/orgs/{p}"), Some(&t.owner.session), Some(json!({ "name": "Mine" }))).status, 400);
    assert_eq!(t.s.call("DELETE", &p("/v1/orgs/{p}"), Some(&t.owner.session), None).status, 400);
    assert_eq!(add(&t.s, &t.owner, &personal, "admin@example.com", "member").status, 400);
    assert_eq!(t.s.call("DELETE", &p(&format!("/v1/orgs/{{p}}/members/{}", t.owner.user_id)), Some(&t.owner.session), None).status, 400);
    for d in [&t.admin, &t.other] {
        for path in ["/v1/orgs/{p}", "/v1/orgs/{p}/usage", "/v1/orgs/{p}/limits", "/v1/orgs/{p}/members"] {
            assert_eq!(t.s.call("GET", &p(path), Some(&d.session), None).status, 404, "someone else's personal org: {path}");
        }
    }
    // Self-hosted (limits off): no limits shown.
    let l = t.s.call("GET", &p("/v1/orgs/{p}/limits"), Some(&t.owner.session), None);
    assert_eq!((l.json["enforced"].as_bool(), &l.json["limits"]["workspaces"]), (Some(false), &Value::Null));
    assert_eq!(t.s.call("POST", "/v1/orgs", Some(&t.owner.session), Some(json!({ "name": "" }))).status, 400);
    for i in 1..20 {
        org_create(&t.s, &t.owner, &format!("Org {i}"));
    }
    assert_eq!(t.s.call("POST", "/v1/orgs", Some(&t.owner.session), Some(json!({ "name": "one too many" }))).status, 403);
}

#[test]
fn access_tokens() {
    let t = T::new("orgs-tokens");
    let ws = "d0000000000000000000000000000000";
    let key = create_workspace(&t.s, &t.owner, ws);
    let make = |scopes: Value, days: i64| t.s.call("POST", "/v1/me/tokens", Some(&t.owner.session), Some(json!({ "name": "script", "scopes": scopes, "expiresInDays": days })));
    assert_eq!(make(json!(["write"]), 30).status, 400, "read is always in");
    assert_eq!(make(json!(["admin"]), 30).status, 400);
    assert_eq!(make(json!(["read"]), 0).status, 400);
    assert_eq!(make(json!(["read"]), 366).status, 400);
    let read = make(json!(["read"]), 30);
    assert_eq!(read.status, 201, "{}", read.json);
    let read_token = read.json["token"].as_str().unwrap().to_string();
    assert!(read_token.starts_with("hitop_pat_"));
    let write = make(json!(["read", "write"]), 1);
    let write_token = write.json["token"].as_str().unwrap().to_string();

    let put = |token: &str, version: u64| {
        t.s.call("PUT", &format!("/v1/workspaces/{ws}/blobs/x"), Some(token), Some(json!({ "version": version, "vv": { "t": version }, "epoch": 1, "content": seal(&key, ws, "x", version, 1, b"v") })))
    };
    assert_eq!(t.s.call("GET", "/v1/workspaces", Some(&read_token), None).json["workspaces"][0]["id"], ws);
    assert_eq!(t.s.call("GET", &format!("/v1/orgs/{}/usage", t.org), Some(&read_token), None).status, 200);
    let refused = put(&read_token, 1);
    assert_eq!((refused.status, refused.json["error"].as_str()), (403, Some("token_scope")));
    assert_eq!(put(&write_token, 1).status, 200);
    // Tokens can't manage tokens.
    assert_eq!(t.s.call("GET", "/v1/me/tokens", Some(&write_token), None).status, 403);
    assert_eq!(t.s.call("POST", "/v1/me/tokens", Some(&write_token), Some(json!({ "name": "x", "scopes": ["read"] }))).status, 403);
    // Listed (never the token), used, revocable; others' tokens aren't reachable.
    let list = t.s.call("GET", "/v1/me/tokens", Some(&t.owner.session), None);
    assert_eq!(list.json["tokens"].as_array().unwrap().len(), 2);
    assert!(!list.raw.windows(10).any(|w| w == b"hitop_pat_"));
    assert!(list.json["tokens"].as_array().unwrap().iter().all(|t| t["lastUsedAt"].is_i64()));
    let read_id = read.json["id"].as_str().unwrap();
    assert_eq!(t.s.call("DELETE", &format!("/v1/me/tokens/{read_id}"), Some(&t.other.session), None).status, 404);
    assert_eq!(t.s.call("DELETE", &format!("/v1/me/tokens/{read_id}"), Some(&t.owner.session), None).status, 204);
    assert_eq!(t.s.call("GET", "/v1/workspaces", Some(&read_token), None).status, 401, "revoked at once");
    // Expired.
    t.s.advance(86_401);
    assert_eq!(t.s.call("GET", "/v1/workspaces", Some(&write_token), None).status, 401);
    // The database holds neither token; the account log has both events.
    let disk = t.s.disk_bytes();
    for tok in [&read_token, &write_token] {
        let secret = tok.trim_start_matches("hitop_pat_");
        assert!(!disk.windows(secret.len()).any(|w| w == secret.as_bytes()));
    }
    let mine = t.s.call("GET", "/v1/me/audit?limit=500", Some(&t.owner.session), None);
    let events: Vec<&str> = mine.json["events"].as_array().unwrap().iter().map(|e| e["event"].as_str().unwrap()).collect();
    assert!(events.contains(&"token_created") && events.contains(&"token_revoked"), "{events:?}");
}
