//! Team environments (teamenvs.rs): an org's members share environments, server-readable, never
//! with secret values; versions conflict like the library; outsiders and personal orgs get none.

mod common;

use common::*;
use serde_json::{json, Value};

fn env(name: &str, base: &str) -> Value {
    json!({ "name": name, "variables": { "base": base, "token": "" }, "secrets": ["token"] })
}

#[test]
fn members_share_environments_without_secret_values() {
    let s = Server::new("team-envs");
    let ada = s.sign_in("ada@example.com");
    let bob = s.sign_in("bob@example.com");
    let eve = s.sign_in("eve@example.com");
    let org = s.call("POST", "/v1/orgs", Some(&ada), Some(json!({ "name": "Acme" }))).json["id"].as_str().unwrap().to_string();
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/members"), Some(&ada), Some(json!({ "email": "bob@example.com", "role": "member" }))).status, 201);
    let path = |id: &str| format!("/v1/orgs/{org}/environments/{id}");

    let r = s.call("PUT", &path("staging"), Some(&ada), Some(json!({ "data": env("Staging", "https://staging.acme.test"), "baseVersion": 0 })));
    assert_eq!((r.status, r.json["version"].as_i64()), (200, Some(1)), "{}", r.json);
    // Bob, a member, sees it and changes it; who changed it shows.
    let list = s.call("GET", &format!("/v1/orgs/{org}/environments"), Some(&bob), None).json;
    assert_eq!(list["environments"][0]["data"]["variables"]["base"], "https://staging.acme.test", "{list}");
    assert_eq!(list["environments"][0]["data"]["org"], json!(org));
    assert_eq!(s.call("PUT", &path("staging"), Some(&bob), Some(json!({ "data": env("Staging", "https://stg2.acme.test"), "baseVersion": 1 }))).status, 200);
    // Ada's stale copy conflicts and gets Bob's back.
    let stale = s.call("PUT", &path("staging"), Some(&ada), Some(json!({ "data": env("Staging", "x"), "baseVersion": 1 })));
    assert_eq!(stale.status, 409);
    assert_eq!(stale.json["current"]["updatedBy"], "bob@example.com", "{}", stale.json);
    // A secret value is refused.
    let leak = s.call("PUT", &path("prod"), Some(&ada), Some(json!({ "data": { "name": "Prod", "variables": { "token": "tok-123" }, "secrets": ["token"] }, "baseVersion": 0 })));
    assert_eq!(leak.status, 400, "{}", leak.json);
    // Outsiders and personal orgs get nothing.
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/environments"), Some(&eve), None).status, 404);
    let personal = s.call("GET", "/v1/orgs", Some(&ada), None).json["orgs"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(s.call("GET", &format!("/v1/orgs/{personal}/environments"), Some(&ada), None).status, 400);
    // Delete with the right version; it's gone for everyone, and a new one can take the id.
    assert_eq!(s.call("DELETE", &format!("{}?baseVersion=2", path("staging")), Some(&ada), None).status, 200);
    assert_eq!(s.call("GET", &format!("/v1/orgs/{org}/environments"), Some(&bob), None).json["environments"], json!([]));
    let again = s.call("PUT", &path("staging"), Some(&bob), Some(json!({ "data": env("Staging", "y"), "baseVersion": 0 })));
    assert_eq!((again.status, again.json["version"].as_i64()), (200, Some(4)), "{}", again.json);
}
