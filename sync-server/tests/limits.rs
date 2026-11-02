//! Plan limits (HITOP_SYNC_LIMITS=plans): every limit at its boundary for each plan (the last
//! create that fits goes through, the next is a 402 naming the limit), reads and shrinking writes
//! never refused, a downgrade keeping everything readable, and a self-hosted server (limits off)
//! enforcing none of it.

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use hitop_sync_server::limits;
use serde_json::{json, Value};

const MB: i64 = 1 << 20;
const GB: i64 = 1 << 30;

fn server(name: &str) -> Server {
    Server::with(name, |c| c.plan_limits = true)
}

fn ws_id(i: usize) -> String {
    format!("{:032x}", 0xabc0000 + i)
}

fn org_create(s: &Server, d: &Device, name: &str) -> String {
    let r = s.call("POST", "/v1/orgs", Some(&d.session), Some(json!({ "name": name })));
    assert_eq!(r.status, 201, "{}", r.json);
    r.json["id"].as_str().unwrap().to_string()
}

/// Creates workspace `ws` in `org` (None: the personal org) owned by `d`.
fn new_ws(s: &Server, d: &Device, org: Option<&str>, ws: &str) -> (Reply, crypto::WorkspaceKey) {
    let key = crypto::WorkspaceKey::generate();
    let wraps = wraps_for(&[(d.key_id.clone(), d.keys.public_key().to_vec())], ws, 1, &key);
    let mut body = json!({ "id": ws, "wraps": wraps });
    if let Some(o) = org {
        body["org"] = json!(o);
    }
    (s.call("POST", "/v1/workspaces", Some(&d.session), Some(body)), key)
}

fn refused(r: &Reply, limit: &str, plan: &str, max: i64) {
    assert_eq!(r.status, 402, "{}", r.json);
    assert_eq!((r.json["error"].as_str(), r.json["limit"].as_str(), r.json["plan"].as_str(), r.json["max"].as_i64()), (Some("limit"), Some(limit), Some(plan), Some(max)), "{}", r.json);
    assert!(r.json["upgradeUrl"].as_str().unwrap().starts_with("https://cloud.hitop.rws8.tech/#/orgs/"));
    let m = r.json["message"].as_str().unwrap();
    assert!(m.contains(&format!("{plan} plan")) && m.contains(&max.to_string()) && m.contains("nothing was changed"), "{m}");
}

fn add_member(s: &Server, owner: &Device, ws: &str, key: &crypto::WorkspaceKey, d: &Device, email: &str, role: &str) -> Reply {
    let wraps = wraps_for(&[(d.key_id.clone(), d.keys.public_key().to_vec())], ws, 1, key);
    s.call("POST", &format!("/v1/workspaces/{ws}/members"), Some(&owner.session), Some(json!({ "email": email, "role": role, "wraps": wraps })))
}

fn usage(s: &Server, d: &Device, org: &str) -> Value {
    let r = s.call("GET", &format!("/v1/orgs/{org}/usage"), Some(&d.session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    r.json
}

#[test]
fn the_limits_of_each_plan() {
    let s = server("limits-table");
    let d = Device::new(&s, "ada@example.com");
    let org = org_create(&s, &d, "Acme");
    let expect = [
        ("free", 0, json!({ "seats": 3, "workspaces": 3, "storageBytes": 100 * MB, "devices": 3, "runners": 5, "invitesPerDay": 20, "guestViewers": 0 })),
        ("pro", 0, json!({ "seats": 3, "workspaces": 25, "storageBytes": 2 * GB, "devices": 10, "runners": 10, "invitesPerDay": 50, "guestViewers": 0 })),
        ("team", 4, json!({ "seats": 4, "workspaces": null, "storageBytes": 14 * GB, "devices": 10, "runners": 40, "invitesPerDay": 200, "guestViewers": 5 })),
        ("business", 10, json!({ "seats": 10, "workspaces": null, "storageBytes": 70 * GB, "devices": 10, "runners": 250, "invitesPerDay": 1000, "guestViewers": 0 })),
        ("enterprise", 0, json!({ "seats": null, "workspaces": null, "storageBytes": null, "devices": null, "runners": null, "invitesPerDay": null, "guestViewers": 0 })),
    ];
    for (plan, seats, limits) in expect {
        limits::set_plan(s.app.state(), &org, plan, seats).unwrap();
        let l = s.call("GET", &format!("/v1/orgs/{org}/limits"), Some(&d.session), None);
        assert_eq!((plan, &l.json["limits"]), (plan, &limits));
        assert_eq!((l.json["plan"].as_str(), l.json["enforced"].as_bool()), (Some(plan), Some(true)));
    }
    assert!(limits::set_plan(s.app.state(), &org, "gold", 1).is_err());
    let events: Vec<String> = audit_rows(&s, &format!("org:{org}")).iter().map(|r| r["event"].as_str().unwrap().to_string()).collect();
    assert_eq!(events.iter().filter(|e| *e == "plan_changed").count(), 5);
}

#[test]
fn workspaces_at_the_boundary() {
    let s = server("limits-workspaces");
    let d = Device::new(&s, "ada@example.com");
    // Free (the personal org): 3.
    for i in 0..3 {
        assert_eq!(new_ws(&s, &d, None, &ws_id(i)).0.status, 201);
    }
    refused(&new_ws(&s, &d, None, &ws_id(3)).0, "workspaces", "free", 3);
    // Pro: 25.
    let personal = format!("personal-{}", d.user_id);
    limits::set_plan(s.app.state(), &personal, "pro", 0).unwrap();
    for i in 3..25 {
        assert_eq!(new_ws(&s, &d, None, &ws_id(i)).0.status, 201, "{i}");
    }
    refused(&new_ws(&s, &d, None, &ws_id(25)).0, "workspaces", "pro", 25);
    // Team and Business: none (the server-wide limit of 50 owned workspaces stays).
    for (plan, n) in [("team", 1), ("business", 1)] {
        let org = org_create(&s, &d, plan);
        limits::set_plan(s.app.state(), &org, plan, n).unwrap();
        for i in 0..5 {
            assert_eq!(new_ws(&s, &d, Some(&org), &format!("{:032x}", 0xdef0000 + i + if plan == "team" { 0 } else { 100 })).0.status, 201);
        }
    }
    // Enterprise: what the contract says.
    let org = org_create(&s, &d, "Contract");
    limits::set_plan(s.app.state(), &org, "enterprise", 0).unwrap();
    s.app.state().db.tx(|tx| tx.execute("UPDATE entitlements SET limits = '{\"workspaces\": 2}' WHERE org_id = $1", &[(&org).into()])).unwrap();
    for i in 0..2 {
        assert_eq!(new_ws(&s, &d, Some(&org), &format!("{:032x}", 0xeee0000 + i)).0.status, 201);
    }
    refused(&new_ws(&s, &d, Some(&org), &format!("{:032x}", 0xeee0009)).0, "workspaces", "enterprise", 2);
}

#[test]
fn seats_at_the_boundary() {
    let s = server("limits-seats");
    let ada = Device::new(&s, "ada@example.com");
    let people: Vec<Device> = (0..9).map(|i| Device::new(&s, &format!("p{i}@example.com"))).collect();
    let email = |i: usize| format!("p{i}@example.com");
    // Free: 3 people, viewers too.
    let (r, k) = new_ws(&s, &ada, None, &ws_id(0));
    assert_eq!(r.status, 201);
    let ws = ws_id(0);
    assert_eq!(add_member(&s, &ada, &ws, &k, &people[0], &email(0), "editor").status, 201);
    assert_eq!(add_member(&s, &ada, &ws, &k, &people[1], &email(1), "viewer").status, 201);
    refused(&add_member(&s, &ada, &ws, &k, &people[2], &email(2), "viewer"), "seats", "free", 3);
    // A person already counted joins another workspace of the org without a new seat.
    let (r, k1) = new_ws(&s, &ada, None, &ws_id(1));
    assert_eq!(r.status, 201);
    assert_eq!(add_member(&s, &ada, &ws_id(1), &k1, &people[0], &email(0), "editor").status, 201);
    // Pro: 3 too (one person and two guests).
    let personal = format!("personal-{}", ada.user_id);
    limits::set_plan(s.app.state(), &personal, "pro", 0).unwrap();
    refused(&add_member(&s, &ada, &ws, &k, &people[2], &email(2), "editor"), "seats", "pro", 3);
    assert_eq!(usage(&s, &ada, &personal)["usage"]["seats"], 3);

    // Team with 2 seats: 2 seats + 5 free guest viewers.
    let team = org_create(&s, &ada, "Team");
    limits::set_plan(s.app.state(), &team, "team", 2).unwrap();
    let tw = ws_id(10);
    let (r, tk) = new_ws(&s, &ada, Some(&team), &tw);
    assert_eq!(r.status, 201);
    assert_eq!(add_member(&s, &ada, &tw, &tk, &people[0], &email(0), "editor").status, 201);
    refused(&add_member(&s, &ada, &tw, &tk, &people[1], &email(1), "editor"), "seats", "team", 2);
    for (i, p) in people.iter().enumerate().take(6).skip(1) {
        assert_eq!(add_member(&s, &ada, &tw, &tk, p, &email(i), "viewer").status, 201, "guest {i}");
    }
    refused(&add_member(&s, &ada, &tw, &tk, &people[6], &email(6), "viewer"), "seats", "team", 2);
    let u = usage(&s, &ada, &team);
    assert_eq!((u["usage"]["seats"].as_i64(), u["usage"]["guests"].as_i64()), (Some(2), Some(5)));
    // A guest made an editor takes a seat: refused while the seats are full.
    let set = |user: &str, role: &str| s.call("PUT", &format!("/v1/workspaces/{tw}/members/{user}"), Some(&ada.session), Some(json!({ "role": role })));
    refused(&set(&people[1].user_id, "editor"), "seats", "team", 2);
    limits::set_plan(s.app.state(), &team, "team", 3).unwrap();
    assert_eq!(set(&people[1].user_id, "editor").status, 200);
    // An org member or a pending org invite takes a seat too.
    refused(&s.call("POST", &format!("/v1/orgs/{team}/members"), Some(&ada.session), Some(json!({ "email": email(7), "role": "member" }))), "seats", "team", 3);
    refused(&s.call("POST", &format!("/v1/orgs/{team}/members"), Some(&ada.session), Some(json!({ "email": "nobody@example.com", "role": "member" }))), "seats", "team", 3);
    // Business: viewers are seats.
    let biz = org_create(&s, &ada, "Biz");
    limits::set_plan(s.app.state(), &biz, "business", 2).unwrap();
    let bw = ws_id(20);
    let (r, bk) = new_ws(&s, &ada, Some(&biz), &bw);
    assert_eq!(r.status, 201);
    assert_eq!(add_member(&s, &ada, &bw, &bk, &people[0], &email(0), "viewer").status, 201);
    refused(&add_member(&s, &ada, &bw, &bk, &people[1], &email(1), "viewer"), "seats", "business", 2);
    // Runner credentials are never seats.
    let wrapped = crypto::wrap_key(&crypto::DeviceKeyPair::generate().public_key(), &bw, 1, &bk).unwrap();
    let r = s.call("POST", &format!("/v1/workspaces/{bw}/runners"), Some(&ada.session), Some(json!({ "publicKey": b64(&[5; 32]), "wrapped": b64(&wrapped) })));
    assert_eq!(r.status, 201, "{}", r.json);
    assert_eq!(usage(&s, &ada, &biz)["usage"]["seats"], 2);
}

#[test]
fn storage_at_the_boundary() {
    // The server-wide limit per workspace (256 MiB by default) out of the way.
    let s = Server::with("limits-storage", |c| {
        c.plan_limits = true;
        c.max_workspace_bytes = 1 << 40;
    });
    let d = Device::new(&s, "ada@example.com");
    let (r, key) = new_ws(&s, &d, None, &ws_id(0));
    assert_eq!(r.status, 201);
    let ws = ws_id(0);
    let personal = format!("personal-{}", d.user_id);
    let put = |version: u64, size: usize| {
        let content = seal(&key, &ws, "b", version, 1, &vec![7u8; size]);
        s.call("PUT", &format!("/v1/workspaces/{ws}/blobs/b"), Some(&d.session), Some(json!({ "version": version, "vv": { "d": version }, "epoch": 1, "content": content })))
    };
    assert_eq!(put(1, 1000).status, 200);
    let stored = || s.app.state().db.tx(|tx| tx.query_one("SELECT LENGTH(content) FROM blobs", &[])).unwrap().unwrap().int(0);
    // The envelope of 1100 bytes of plaintext.
    let size = stored() + 100;
    let mut version = 1;
    for (plan, seats, max) in [("free", 0, 100 * MB), ("pro", 0, 2 * GB), ("team", 2, 12 * GB), ("business", 3, 56 * GB)] {
        limits::set_plan(s.app.state(), &personal, plan, seats).unwrap();
        // As if the rest of the org stored so much that a 1100 byte write fills it exactly.
        let bytes = max - size + stored();
        s.app.state().db.tx(|tx| tx.execute("UPDATE workspaces SET bytes = $1 WHERE id = $2", &[bytes.into(), (&ws).into()])).unwrap();
        version += 1;
        assert_eq!(put(version, 1000 + 100).status, 200, "{plan}: up to the limit");
        let r = put(version + 1, 1000 + 101);
        refused(&r, "storageBytes", plan, max);
        assert_eq!(r.json["used"], max);
        // The same size and smaller still go through; so does a delete.
        version += 1;
        assert_eq!(put(version, 1000 + 100).status, 200, "{plan}: same size");
        version += 1;
        assert_eq!(put(version, 10).status, 200, "{plan}: smaller");
    }
    let del = s.call("PUT", &format!("/v1/workspaces/{ws}/blobs/b"), Some(&d.session), Some(json!({ "version": version + 1, "vv": { "d": version + 1 }, "epoch": 1, "deleted": true })));
    assert_eq!(del.status, 200);
}

#[test]
fn devices_at_the_boundary() {
    let s = server("limits-devices");
    let session = s.sign_in("ada@example.com");
    let add = |n: usize| s.call("POST", "/v1/me/keys", Some(&session), Some(json!({ "name": format!("d{n}"), "publicKey": b64(&[n as u8; 32]) })));
    for n in 0..3 {
        assert_eq!(add(n).status, 201);
    }
    let me = s.call("GET", "/v1/me", Some(&session), None).json["id"].as_str().unwrap().to_string();
    refused(&add(3), "devices", "free", 3);
    limits::set_plan(s.app.state(), &format!("personal-{me}"), "pro", 0).unwrap();
    for n in 3..10 {
        assert_eq!(add(n).status, 201);
    }
    refused(&add(10), "devices", "pro", 10);
    // A member of a Team org gets the Team limit (the best of their orgs) on a Free personal org.
    let bob_session = s.sign_in("bob@example.com");
    let bob = s.call("GET", "/v1/me", Some(&bob_session), None).json["id"].as_str().unwrap().to_string();
    let addb = |n: usize| s.call("POST", "/v1/me/keys", Some(&bob_session), Some(json!({ "name": "b", "publicKey": b64(&[n as u8; 32]) })));
    for n in 0..3 {
        assert_eq!(addb(n).status, 201);
    }
    refused(&addb(3), "devices", "free", 3);
    let team = s.call("POST", "/v1/orgs", Some(&session), Some(json!({ "name": "T" }))).json["id"].as_str().unwrap().to_string();
    limits::set_plan(s.app.state(), &team, "team", 5).unwrap();
    assert_eq!(s.call("POST", &format!("/v1/orgs/{team}/members"), Some(&session), Some(json!({ "email": "bob@example.com", "role": "member" }))).status, 201);
    assert_eq!(addb(3).status, 201, "{bob}");
}

#[test]
fn runners_at_the_boundary() {
    let s = server("limits-runners");
    let d = Device::new(&s, "ada@example.com");
    let mk = |ws: &str, key: &crypto::WorkspaceKey| {
        let keys = crypto::DeviceKeyPair::generate();
        let wrapped = crypto::wrap_key(&keys.public_key(), ws, 1, key).unwrap();
        s.call("POST", &format!("/v1/workspaces/{ws}/runners"), Some(&d.session), Some(json!({ "publicKey": b64(&keys.public_key()), "wrapped": b64(&wrapped) })))
    };
    let (_, k0) = new_ws(&s, &d, None, &ws_id(0));
    let (_, k1) = new_ws(&s, &d, None, &ws_id(1));
    // Free: 5 per org, across its workspaces.
    for i in 0..5 {
        assert_eq!(mk(&ws_id(i % 2), if i % 2 == 0 { &k0 } else { &k1 }).status, 201);
    }
    refused(&mk(&ws_id(1), &k1), "runners", "free", 5);
    // Pro: 10 (the server-wide 10 per workspace stays).
    limits::set_plan(s.app.state(), &format!("personal-{}", d.user_id), "pro", 0).unwrap();
    for i in 5..10 {
        assert_eq!(mk(&ws_id(i % 2), if i % 2 == 0 { &k0 } else { &k1 }).status, 201);
    }
    refused(&mk(&ws_id(1), &k1), "runners", "pro", 10);
    // Team: 10 per seat.
    let team = org_create(&s, &d, "T");
    limits::set_plan(s.app.state(), &team, "team", 1).unwrap();
    let (_, t5) = new_ws(&s, &d, Some(&team), &ws_id(5));
    let (_, t6) = new_ws(&s, &d, Some(&team), &ws_id(6));
    for i in 0..10 {
        assert_eq!(mk(&ws_id(5 + i % 2), if i % 2 == 0 { &t5 } else { &t6 }).status, 201);
    }
    refused(&mk(&ws_id(5), &t5), "runners", "team", 10);
}

#[test]
fn invites_a_day() {
    let s = server("limits-invites");
    let d = Device::new(&s, "ada@example.com");
    let (_, _) = new_ws(&s, &d, None, &ws_id(0));
    let invite = || s.call("POST", &format!("/v1/workspaces/{}/invites", ws_id(0)), Some(&d.session), Some(json!({ "role": "viewer" })));
    for _ in 0..20 {
        assert_eq!(invite().status, 201);
    }
    refused(&invite(), "invitesPerDay", "free", 20);
    // Open invites stay; tomorrow there are 20 more (the server-wide 50 open per workspace stays).
    s.advance(86_400);
    assert_eq!(invite().status, 201);
    assert_eq!(usage(&s, &d, &format!("personal-{}", d.user_id))["usage"]["invitesPerDay"], 1);
}

#[test]
fn a_downgrade_keeps_everything_readable() {
    let s = server("limits-downgrade");
    let d = Device::new(&s, "ada@example.com");
    let personal = format!("personal-{}", d.user_id);
    limits::set_plan(s.app.state(), &personal, "pro", 0).unwrap();
    let mut keys = vec![];
    for i in 0..5 {
        let (r, k) = new_ws(&s, &d, None, &ws_id(i));
        assert_eq!(r.status, 201);
        let content = seal(&k, &ws_id(i), "b", 1, 1, b"data");
        assert_eq!(s.call("PUT", &format!("/v1/workspaces/{}/blobs/b", ws_id(i)), Some(&d.session), Some(json!({ "version": 1, "vv": { "d": 1 }, "epoch": 1, "content": content }))).status, 200);
        keys.push(k);
    }
    // The subscription lapses: Free limits, nothing deleted.
    s.app.state().db.tx(|tx| tx.execute("UPDATE entitlements SET status = 'expired' WHERE org_id = $1", &[(&personal).into()])).unwrap();
    let over = s.app.state().db.tx(|tx| limits::over_limits(s.app.state(), tx)).unwrap();
    assert_eq!(over.iter().map(|o| (o.org.as_str(), o.metric.name(), o.used, o.max)).collect::<Vec<_>>(), [(personal.as_str(), "workspaces", 5, 3)]);
    refused(&new_ws(&s, &d, None, &ws_id(9)).0, "workspaces", "free", 3);
    for (i, key) in keys.iter().enumerate() {
        let ws = ws_id(i);
        let got = s.call("GET", &format!("/v1/workspaces/{ws}/blobs/b"), Some(&d.session), None);
        assert_eq!(open(key, &ws, "b", 1, 1, got.json["content"].as_str().unwrap()), b"data");
        assert_eq!(s.call("GET", &format!("/v1/workspaces/{ws}/changes?since=0"), Some(&d.session), None).status, 200);
        // An edit that doesn't grow the workspace is fine.
        let content = seal(key, &ws, "b", 2, 1, b"edit");
        assert_eq!(s.call("PUT", &format!("/v1/workspaces/{ws}/blobs/b"), Some(&d.session), Some(json!({ "version": 2, "vv": { "d": 2 }, "epoch": 1, "content": content }))).status, 200);
    }
    // Deleting down to the limit makes creates work again.
    for i in 0..3 {
        assert_eq!(s.call("DELETE", &format!("/v1/workspaces/{}", ws_id(i)), Some(&d.session), None).status, 204);
    }
    assert_eq!(new_ws(&s, &d, None, &ws_id(9)).0.status, 201);
}

#[test]
fn a_self_hosted_server_has_no_plan_limits() {
    let s = Server::new("limits-off");
    let d = Device::new(&s, "ada@example.com");
    for i in 0..8 {
        assert_eq!(new_ws(&s, &d, None, &ws_id(i)).0.status, 201);
    }
    for n in 0..8 {
        assert_eq!(s.call("POST", "/v1/me/keys", Some(&d.session), Some(json!({ "publicKey": b64(&[n; 32]) }))).status, 201);
    }
    let u = usage(&s, &d, &format!("personal-{}", d.user_id));
    assert_eq!((u["enforced"].as_bool(), u["usage"]["workspaces"].as_i64(), &u["limits"]["seats"]), (Some(false), Some(8), &Value::Null));
}
