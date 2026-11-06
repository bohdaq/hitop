//! The personal library (C8, library.rs): items stored and listed, deltas by `since`, version
//! conflicts in both directions, tombstones, secret values sealed at rest under the host key (never
//! on disk in the clear, never without a host key, only for their owner), bytes counted against the
//! personal org's storage with a 402 past Free, and who may not use it.

mod common;

use common::*;
use hitop_sync_server::hostkey::HostKey;
use serde_json::{json, Value};

const SECRET: &str = "tok-3f9a-library-secret-value";

fn with_host_key(name: &str) -> Server {
    Server::with(name, |c| c.host_key = Some(HostKey::from_bytes([7; 32])))
}

fn put(s: &Server, token: &str, kind: &str, id: &str, data: Value, secrets: Option<Value>, base: i64) -> Reply {
    let mut body = json!({ "data": data, "baseVersion": base });
    if let Some(sec) = secrets {
        body["secrets"] = sec;
    }
    s.call("PUT", &format!("/v1/library/{kind}/{id}"), Some(token), Some(body))
}

fn list(s: &Server, token: &str, since: Option<i64>) -> Value {
    let path = since.map_or("/v1/library".to_string(), |n| format!("/v1/library?since={n}"));
    let r = s.call("GET", &path, Some(token), None);
    assert_eq!(r.status, 200, "{}", r.json);
    r.json
}

#[test]
fn items_round_trip_with_deltas_conflicts_and_tombstones() {
    let s = with_host_key("library-basic");
    let ada = s.sign_in("ada@example.com");
    let other = s.sign_in("ada@example.com"); // a second browser of the same account
    assert_eq!(list(&s, &ada, None)["items"].as_array().unwrap().len(), 0);

    let shop = json!({ "id": 1, "name": "Shop", "requests": [{ "id": 2, "name": "List", "url": "https://api.example.com/items", "method": "GET" }] });
    let r = put(&s, &ada, "collection", "1", shop.clone(), None, 0);
    assert_eq!((r.status, r.json["version"].as_i64()), (200, Some(1)), "{}", r.json);
    let envs = json!({ "environments": [{ "id": "e1", "name": "prod", "variables": { "base": "https://api.example.com", "token": "" }, "secrets": ["token"] }], "activeEnvironmentId": "e1" });
    let r = put(&s, &ada, "environments", "all", envs.clone(), Some(json!({ "e1": { "token": SECRET } })), 0);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["secretsStored"], json!(true));

    // Another browser sees both, secret values included, for its owner.
    let all = list(&s, &other, None);
    assert_eq!(all["secretsStored"], json!(true));
    let items = all["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let col = items.iter().find(|i| i["kind"] == "collection").unwrap();
    assert_eq!((col["id"].as_str(), &col["data"]), (Some("1"), &shop));
    let env = items.iter().find(|i| i["kind"] == "environments").unwrap();
    assert_eq!(env["secrets"]["e1"]["token"], json!(SECRET));
    let cursor = all["cursor"].as_i64().unwrap();
    assert_eq!(list(&s, &other, Some(cursor))["items"].as_array().unwrap().len(), 0, "nothing changed since the cursor");

    // An edit from the first browser; the second, still on version 1, gets a conflict with the server's item.
    let mut renamed = shop.clone();
    renamed["name"] = json!("Shop v2");
    assert_eq!(put(&s, &ada, "collection", "1", renamed.clone(), None, 1).json["version"], json!(2));
    let stale = put(&s, &other, "collection", "1", shop.clone(), None, 1);
    assert_eq!((stale.status, stale.json["error"].as_str()), (409, Some("conflict")), "{}", stale.json);
    assert_eq!((stale.json["current"]["version"].as_i64(), &stale.json["current"]["data"]["name"]), (Some(2), &json!("Shop v2")));
    // A new item at baseVersion 0 when it already exists is a conflict too.
    assert_eq!(put(&s, &other, "collection", "1", shop.clone(), None, 0).status, 409);
    let delta = list(&s, &other, Some(cursor));
    assert_eq!(delta["items"].as_array().unwrap().len(), 1, "only the renamed collection: {delta}");

    // Delete: a stale baseVersion is refused, the right one leaves a tombstone others see; the
    // full list leaves it out; a new item with that id starts again from baseVersion 0.
    assert_eq!(s.call("DELETE", "/v1/library/collection/1?baseVersion=1", Some(&ada), None).status, 409);
    let d = s.call("DELETE", "/v1/library/collection/1?baseVersion=2", Some(&ada), None);
    assert_eq!((d.status, d.json["deleted"].as_bool()), (200, Some(true)), "{}", d.json);
    let after = list(&s, &other, Some(delta["cursor"].as_i64().unwrap()));
    assert_eq!((after["items"][0]["deleted"].as_bool(), after["items"][0].get("data")), (Some(true), None), "{after}");
    assert!(list(&s, &other, None)["items"].as_array().unwrap().iter().all(|i| i["kind"] != "collection"));
    assert_eq!(s.call("DELETE", "/v1/library/collection/1?baseVersion=0", Some(&ada), None).status, 200, "deleting a deleted item is a no-op");
    assert_eq!(put(&s, &other, "collection", "1", shop, None, 0).json["version"], json!(4));
}

#[test]
fn secret_values_are_sealed_at_rest_and_only_for_their_owner() {
    let s = with_host_key("library-secrets");
    let ada = s.sign_in("ada@example.com");
    let r = put(&s, &ada, "globals", "all", json!({ "variables": { "apiKey": "" }, "secrets": ["apiKey"] }), Some(json!({ "apiKey": SECRET })), 0);
    assert_eq!(r.status, 200, "{}", r.json);
    let disk = s.disk_bytes();
    assert!(!disk.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()), "a secret value is on disk in the clear");

    // Another account sees none of Ada's library.
    let bob = s.sign_in("bob@example.com");
    assert_eq!(list(&s, &bob, None)["items"].as_array().unwrap().len(), 0);
    assert_eq!(put(&s, &bob, "globals", "all", json!({}), None, 1).status, 409, "Bob's globals don't exist: base 1 conflicts");

    // Without a host key the server stores no secret values and says so.
    let plain = Server::new("library-nokey");
    let t = plain.sign_in("ada@example.com");
    let r = put(&plain, &t, "globals", "all", json!({ "variables": {} }), Some(json!({ "apiKey": SECRET })), 0);
    assert_eq!((r.status, &r.json["secretsStored"]), (200, &json!(false)), "{}", r.json);
    let all = list(&plain, &t, None);
    assert_eq!((&all["secretsStored"], all["items"][0].get("secrets")), (&json!(false), None));
    assert!(!plain.disk_bytes().windows(SECRET.len()).any(|w| w == SECRET.as_bytes()));
}

#[test]
fn library_bytes_count_against_the_personal_plan_and_it_is_no_workspace() {
    let s = Server::with("library-limits", |c| {
        c.plan_limits = true;
        c.host_key = Some(HostKey::from_bytes([7; 32]));
    });
    let ada = s.sign_in("ada@example.com");
    let org = s.call("GET", "/v1/orgs", Some(&ada), None).json["orgs"][0]["id"].as_str().unwrap().to_string();
    let big = "x".repeat(4 << 20);
    for i in 0..24 {
        let r = put(&s, &ada, "collection", &i.to_string(), json!({ "id": i, "name": format!("c{i}"), "blob": big }), None, 0);
        assert_eq!(r.status, 200, "item {i}: {}", r.json);
    }
    let usage = s.call("GET", &format!("/v1/orgs/{org}/usage"), Some(&ada), None).json;
    assert_eq!(usage["usage"]["workspaces"], json!(0), "the library is not a workspace: {usage}");
    assert!(usage["usage"]["storageBytes"].as_i64().unwrap() >= 24 * (4 << 20), "{usage}");
    // The 25th 4 MB item passes 100 MB: refused as a plan limit, nothing stored.
    let r = put(&s, &ada, "collection", "24", json!({ "name": "c24", "blob": big }), None, 0);
    assert_eq!((r.status, r.json["limit"].as_str(), r.json["plan"].as_str()), (402, Some("storageBytes"), Some("free")), "{}", r.json);
    // Shrinking or deleting is never refused, and frees the room.
    assert_eq!(put(&s, &ada, "collection", "0", json!({ "name": "c0" }), None, 1).status, 200);
    assert_eq!(put(&s, &ada, "collection", "24", json!({ "name": "c24", "blob": big }), None, 0).status, 200);
    // An item over 5 MB is refused whatever the plan.
    let r = put(&s, &ada, "collection", "huge", json!({ "blob": "y".repeat(5 << 20) }), None, 0);
    assert_eq!(r.status, 413, "{}", r.json);
}

#[test]
fn bad_requests_tokens_and_runners() {
    let s = Server::new("library-auth");
    let ada = s.sign_in("ada@example.com");
    assert_eq!(s.call("GET", "/v1/library", None, None).status, 401);
    assert_eq!(put(&s, &ada, "workspace", "1", json!({}), None, 0).status, 400, "unknown kind");
    assert_eq!(put(&s, &ada, "environments", "prod", json!({}), None, 0).status, 400, "environments has one item, all");
    assert_eq!(put(&s, &ada, "collection", "a/b", json!({}), None, 0).status, 404, "a slash is no id");
    assert_eq!(put(&s, &ada, "collection", "1", json!([1, 2]), None, 0).status, 400, "data is an object");

    // A read token reads, a write token writes.
    let mk = |scopes: Value| s.call("POST", "/v1/me/tokens", Some(&ada), Some(json!({ "name": "t", "scopes": scopes }))).json["token"].as_str().unwrap().to_string();
    let read = mk(json!(["read"]));
    let write = mk(json!(["read", "write"]));
    assert_eq!(put(&s, &read, "collection", "1", json!({ "name": "x" }), None, 0).status, 403);
    assert_eq!(put(&s, &write, "collection", "1", json!({ "name": "x" }), None, 0).status, 200);
    assert_eq!(list(&s, &read, None)["items"].as_array().unwrap().len(), 1);

    // A runner credential can't use a library.
    let owner = Device::new(&s, "owner@example.com");
    let ws = format!("{:032x}", 0x11b);
    let key = create_workspace(&s, &owner, &ws);
    let kp = hitop_sync_crypto::DeviceKeyPair::generate();
    let wrapped = hitop_sync_crypto::wrap_key(&kp.public_key(), &ws, 1, &key).unwrap();
    let runner = s.call("POST", &format!("/v1/workspaces/{ws}/runners"), Some(&owner.session), Some(json!({ "name": "ci", "publicKey": b64(&kp.public_key()), "wrapped": b64(&wrapped) })));
    assert_eq!(runner.status, 201, "{}", runner.json);
    let rt = runner.json["token"].as_str().unwrap();
    assert_eq!(s.call("GET", "/v1/library", Some(rt), None).status, 403);
}

#[test]
fn a_collection_naming_an_org_is_filed_and_counted_there() {
    let s = Server::new("library-orgs");
    let ada = s.sign_in("ada@example.com");
    let bob = s.sign_in("bob@example.com");
    let personal = s.call("GET", "/v1/orgs", Some(&ada), None).json["orgs"][0]["id"].as_str().unwrap().to_string();
    let team = s.call("POST", "/v1/orgs", Some(&ada), Some(json!({ "name": "Acme" }))).json["id"].as_str().unwrap().to_string();
    let bytes = |org: &str| s.call("GET", &format!("/v1/orgs/{org}/usage"), Some(&ada), None).json["usage"]["storageBytes"].as_i64().unwrap();
    let blob = "x".repeat(10_000);
    // In the team org: counted there, not in the personal org; the org stays in the data.
    let r = put(&s, &ada, "collection", "1", json!({ "id": 1, "name": "Team API", "org": team, "blob": blob }), None, 0);
    assert_eq!(r.status, 200, "{}", r.json);
    assert!(bytes(&team) >= 10_000 && bytes(&personal) < 10_000, "team {} personal {}", bytes(&team), bytes(&personal));
    assert_eq!(list(&s, &ada, None)["items"][0]["data"]["org"], json!(team));
    // Moved back to personal: the bytes move with it.
    assert_eq!(put(&s, &ada, "collection", "1", json!({ "id": 1, "name": "Team API", "org": personal, "blob": blob }), None, 1).status, 200);
    assert!(bytes(&team) < 10_000 && bytes(&personal) >= 10_000);
    // An org the person isn't in is refused (404), and nothing is stored.
    let r = put(&s, &bob, "collection", "9", json!({ "id": 9, "name": "Sneaky", "org": team }), None, 0);
    assert_eq!(r.status, 404, "{}", r.json);
    assert_eq!(list(&s, &bob, None)["items"], json!([]));
}

#[test]
fn each_request_keeps_its_history() {
    let s = Server::new("library-history");
    let ada = s.sign_in("ada@example.com");
    let req = |url: &str| json!({ "id": 11, "name": "List orders", "method": "GET", "url": url });
    let other = json!({ "id": 12, "name": "Health", "method": "GET", "url": "/health" });
    assert_eq!(put(&s, &ada, "collection", "1", json!({ "id": 1, "name": "Shop", "requests": [req("/v1/orders"), other] }), None, 0).status, 200);
    assert_eq!(put(&s, &ada, "collection", "1", json!({ "id": 1, "name": "Shop", "requests": [req("/v2/orders"), other] }), None, 1).status, 200);
    assert_eq!(put(&s, &ada, "collection", "1", json!({ "id": 1, "name": "Shop renamed", "requests": [req("/v2/orders")] }), None, 2).status, 200);
    let h = |rid: &str| s.call("GET", &format!("/v1/library/collection/1/history?request={rid}"), Some(&ada), None).json["history"].clone();
    let orders = h("11");
    let urls: Vec<&str> = orders.as_array().unwrap().iter().map(|e| e["request"]["url"].as_str().unwrap()).collect();
    assert_eq!(urls, ["/v2/orders", "/v1/orders"], "a change makes an entry, a save that leaves it alone doesn't: {orders}");
    assert_eq!(orders[0]["source"], "app");
    let health = h("12");
    assert_eq!((health[0]["deleted"].as_bool(), health.as_array().unwrap().len()), (Some(true), 2), "a removed request leaves a tombstone: {health}");
    let bob = s.sign_in("bob@example.com");
    assert_eq!(s.call("GET", "/v1/library/collection/1/history?request=11", Some(&bob), None).json["history"], json!([]), "only the owner's");
}
