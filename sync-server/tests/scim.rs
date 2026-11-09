//! SCIM 2.0 conformance (HITOP Cloud C6, scim.rs): discovery, tokens per org, create, filter
//! `userName eq`, update (PUT and PATCH), deactivate, delete, group → org and workspace roles, and
//! what SCIM never does (owners, other orgs, plans without it).

mod common;

use common::*;
use serde_json::{json, Value};

const WS: &str = "3a0e0d0c0b0a09080706050403020100";
const WS2: &str = "3b0e0d0c0b0a09080706050403020100";
const PATCH: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";

struct Org {
    s: Server,
    org: String,
    ada: Device,
    token: String,
}

/// Ada owns the org "Corp" (Business when `plans`), a workspace in it and a SCIM token.
fn setup(name: &str, plans: bool) -> Org {
    let s = Server::with(name, |c| c.plan_limits = plans);
    let ada = Device::new(&s, "ada@corp.test");
    let org = s.call("POST", "/v1/orgs", Some(&ada.session), Some(json!({ "name": "Corp" }))).json["id"].as_str().unwrap().to_string();
    if plans {
        hitop_sync_server::limits::set_plan(s.app.state(), &org, "business", 10).unwrap();
    }
    create_workspace(&s, &ada, WS);
    assert_eq!(s.call("POST", &format!("/v1/orgs/{org}/workspaces"), Some(&ada.session), Some(json!({ "workspace": WS }))).status, 200);
    let r = s.call("POST", &format!("/v1/orgs/{org}/scim/tokens"), Some(&ada.session), Some(json!({ "name": "Okta" })));
    assert_eq!(r.status, 201, "{}", r.json);
    let token = r.json["token"].as_str().unwrap().to_string();
    assert!(token.starts_with("hitop_scim_"));
    assert_eq!(r.json["baseUrl"], format!("{ORIGIN}/scim/v2"));
    Org { s, org, ada, token }
}

impl Org {
    fn scim(&self, method: &str, path: &str, body: Option<Value>) -> Reply {
        self.s.call(method, &format!("/scim/v2{path}"), Some(&self.token), body)
    }
    fn create_user(&self, email: &str) -> String {
        let r = self.scim("POST", "/Users", Some(json!({ "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"], "userName": email, "externalId": format!("ext-{email}"), "name": { "givenName": "A", "familyName": "B" }, "emails": [{ "value": email, "primary": true }], "active": true })));
        assert_eq!(r.status, 201, "{}", r.json);
        r.json["id"].as_str().unwrap().to_string()
    }
    fn members(&self) -> Value {
        let r = self.s.call("GET", &format!("/v1/orgs/{}/members", self.org), Some(&self.ada.session), None);
        assert_eq!(r.status, 200, "{}", r.json);
        r.json
    }
    fn org_role(&self, email: &str) -> Option<String> {
        self.members()["members"].as_array().unwrap().iter().find(|m| m["email"] == email).map(|m| m["role"].as_str().unwrap().to_string())
    }
    fn invited(&self, email: &str) -> bool {
        self.members()["invites"].as_array().unwrap().iter().any(|m| m["email"] == email)
    }
    fn ws_invites(&self, ws: &str) -> Vec<Value> {
        let r = self.s.call("GET", &format!("/v1/workspaces/{ws}/invites"), Some(&self.ada.session), None);
        assert_eq!(r.status, 200, "{}", r.json);
        r.json["invites"].as_array().unwrap().clone()
    }
    fn ws_role(&self, ws: &str, user: &str) -> Option<String> {
        let r = self.s.call("GET", &format!("/v1/workspaces/{ws}"), Some(&self.ada.session), None);
        r.json["members"].as_array().unwrap().iter().find(|m| m["userId"] == user).map(|m| m["role"].as_str().unwrap().to_string())
    }
    fn group_roles(&self, group: &str, body: Value) -> Reply {
        self.s.call("PUT", &format!("/v1/orgs/{}/scim/groups/{group}", self.org), Some(&self.ada.session), Some(body))
    }
}

fn patch(ops: Value) -> Value {
    json!({ "schemas": [PATCH], "Operations": ops })
}

#[test]
fn discovery_and_tokens() {
    let o = setup("scim-discovery", false);
    for path in ["/ServiceProviderConfig", "/Schemas", "/ResourceTypes", "/ResourceTypes/User", "/Schemas/urn:ietf:params:scim:schemas:core:2.0:Group"] {
        let r = o.scim("GET", path, None);
        assert_eq!(r.status, 200, "{path}: {}", r.json);
        assert_eq!(r.content_type.as_deref(), Some("application/scim+json"), "{path}");
    }
    let c = o.scim("GET", "/ServiceProviderConfig", None).json;
    assert_eq!(c["patch"]["supported"], true);
    assert_eq!(c["filter"]["supported"], true);
    assert_eq!(c["bulk"]["supported"], false);
    assert_eq!(o.scim("GET", "/ResourceTypes", None).json["totalResults"], 2);

    // No token, a wrong token, a session in place of a SCIM token: 401 in SCIM's error shape.
    for t in [None, Some("hitop_scim_wrong"), Some(o.ada.session.as_str())] {
        let r = o.s.call("GET", "/scim/v2/Users", t, None);
        assert_eq!(r.status, 401, "{}", r.json);
        assert_eq!(r.json["schemas"][0], "urn:ietf:params:scim:api:messages:2.0:Error");
        assert_eq!(r.json["status"], "401");
        assert!(r.header("www-authenticate").is_some());
    }
    // PATCH reaches SCIM (the router has none); unknown paths are SCIM 404s.
    assert_eq!(o.scim("PATCH", "/Users/nope", Some(patch(json!([{ "op": "replace", "path": "active", "value": false }])))).status, 404);
    assert_eq!(o.scim("GET", "/Nope", None).json["status"], "404");

    // Tokens: listed without their value; only owners create them, with a session (not an access
    // token); admins read; revoking ends access at once.
    let bob = Device::new(&o.s, "bob@corp.test");
    assert_eq!(o.s.call("POST", &format!("/v1/orgs/{}/members", o.org), Some(&o.ada.session), Some(json!({ "email": "bob@corp.test", "role": "admin" }))).status, 201);
    let r = o.s.call("GET", &format!("/v1/orgs/{}/scim", o.org), Some(&bob.session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["enabled"], true);
    assert_eq!(r.json["tokens"][0]["name"], "Okta");
    assert!(!r.json.to_string().contains(&o.token));
    assert_eq!(o.s.call("POST", &format!("/v1/orgs/{}/scim/tokens", o.org), Some(&bob.session), Some(json!({ "name": "x" }))).status, 403);
    let pat = o.s.call("POST", "/v1/me/tokens", Some(&o.ada.session), Some(json!({ "name": "ci", "scopes": ["read", "write"] })));
    assert_eq!(pat.status, 201, "{}", pat.json);
    let pat = pat.json["token"].as_str().unwrap().to_string();
    assert_eq!(o.s.call("POST", &format!("/v1/orgs/{}/scim/tokens", o.org), Some(&pat), Some(json!({ "name": "x" }))).status, 403);
    let id = r.json["tokens"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(o.s.call("DELETE", &format!("/v1/orgs/{}/scim/tokens/{id}", o.org), Some(&o.ada.session), None).status, 204);
    assert_eq!(o.scim("GET", "/Users", None).status, 401);
    assert_eq!(o.s.call("GET", &format!("/v1/orgs/{}/scim", o.org), Some(&o.ada.session), None).json["enabled"], false);

    // A personal org has no SCIM; another org's member sees nothing (404).
    let personal = format!("personal-{}", o.ada.user_id);
    assert_eq!(o.s.call("POST", &format!("/v1/orgs/{personal}/scim/tokens"), Some(&o.ada.session), Some(json!({}))).status, 400);
    let eve = Device::new(&o.s, "eve@else.test");
    assert_eq!(o.s.call("GET", &format!("/v1/orgs/{}/scim", o.org), Some(&eve.session), None).status, 404);
    let audit = audit_rows(&o.s, &format!("org:{}", o.org));
    assert!(audit.iter().any(|e| e["event"] == "scim_token_created"));
    assert!(audit.iter().any(|e| e["event"] == "scim_token_revoked"));
}

#[test]
fn create_filter_update_deactivate_delete() {
    let o = setup("scim-users", false);
    let bob = Device::new(&o.s, "bob@corp.test");
    // Create: an account joins the org; an address without one is invited.
    let bob_id = o.create_user("Bob@Corp.test");
    assert_eq!(o.org_role("bob@corp.test").as_deref(), Some("member"));
    let dee_id = o.create_user("dee@corp.test");
    assert!(o.invited("dee@corp.test"));
    // The same userName again: 409 uniqueness. Not an address: 400.
    let r = o.scim("POST", "/Users", Some(json!({ "userName": "bob@corp.test" })));
    assert_eq!(r.status, 409);
    assert_eq!(r.json["scimType"], "uniqueness");
    assert_eq!(o.scim("POST", "/Users", Some(json!({ "userName": "bob" }))).json["scimType"], "invalidValue");

    // Filter userName eq (what IdPs ask before creating): one match, or an empty list.
    let r = o.scim("GET", "/Users?filter=userName%20eq%20%22bob%40corp.test%22", None);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["totalResults"], 1);
    assert_eq!(r.json["Resources"][0]["id"], bob_id.as_str());
    assert_eq!(r.json["Resources"][0]["userName"], "bob@corp.test");
    assert_eq!(r.json["Resources"][0]["externalId"], "ext-Bob@Corp.test");
    assert_eq!(r.json["Resources"][0]["name"]["givenName"], "A");
    let r = o.scim("GET", "/Users?filter=userName+eq+%22nobody%40corp.test%22", None);
    assert_eq!((r.status, r.json["totalResults"].clone(), r.json["Resources"].clone()), (200, json!(0), json!([])));
    assert_eq!(o.scim("GET", "/Users?filter=externalId%20eq%20%22ext-dee%40corp.test%22", None).json["Resources"][0]["id"], dee_id.as_str());
    assert_eq!(o.scim("GET", "/Users?filter=title%20eq%20%22x%22", None).json["scimType"], "invalidFilter");
    // Paging.
    let r = o.scim("GET", "/Users?startIndex=2&count=1", None);
    assert_eq!((r.json["totalResults"].clone(), r.json["itemsPerPage"].clone(), r.json["startIndex"].clone()), (json!(2), json!(1), json!(2)));

    // PATCH (Entra ID style: capitalized op, no path) and PUT update the stored attributes.
    let r = o.scim("PATCH", &format!("/Users/{bob_id}"), Some(patch(json!([{ "op": "Replace", "value": { "displayName": "Bob B", "name.familyName": "Builder" } }, { "op": "add", "path": "title", "value": "ignored" }]))));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["displayName"], "Bob B");
    assert_eq!(r.json["name"]["familyName"], "Builder");
    assert_eq!(r.json["active"], true);
    let r = o.scim("PUT", &format!("/Users/{bob_id}"), Some(json!({ "userName": "bob@corp.test", "displayName": "Robert", "active": true })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["displayName"], "Robert");
    assert!(r.json.get("externalId").is_none(), "PUT replaces: no externalId sent, none kept");

    // Deactivate (active "False", as Entra ID sends it): out of the org; the account stays.
    let r = o.scim("PATCH", &format!("/Users/{bob_id}"), Some(patch(json!([{ "op": "Replace", "path": "active", "value": "False" }]))));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["active"], false);
    assert_eq!(o.org_role("bob@corp.test"), None);
    assert_eq!(o.s.call("GET", "/v1/me", Some(&bob.session), None).status, 200);
    // A PUT that leaves out `active` keeps him deactivated: only an explicit value reactivates.
    let r = o.scim("PUT", &format!("/Users/{bob_id}"), Some(json!({ "userName": "bob@corp.test", "displayName": "Robert" })));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["active"], false);
    assert_eq!(o.org_role("bob@corp.test"), None);
    // Reactivate: back in.
    let r = o.scim("PATCH", &format!("/Users/{bob_id}"), Some(patch(json!([{ "op": "replace", "path": "active", "value": true }]))));
    assert_eq!(r.json["active"], true);
    assert_eq!(o.org_role("bob@corp.test").as_deref(), Some("member"));

    // The invited address signs in: a member (orgs.rs turns the invite into a membership).
    let dee = Device::new(&o.s, "dee@corp.test");
    assert_eq!(o.s.call("GET", "/v1/orgs", Some(&dee.session), None).status, 200);
    assert_eq!(o.org_role("dee@corp.test").as_deref(), Some("member"));
    // A new userName: the old address leaves, the new one is invited.
    let r = o.scim("PATCH", &format!("/Users/{dee_id}"), Some(patch(json!([{ "op": "replace", "path": "userName", "value": "dee.new@corp.test" }]))));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(o.org_role("dee@corp.test"), None);
    assert!(o.invited("dee.new@corp.test"));

    // DELETE: gone from the directory and the org.
    assert_eq!(o.scim("DELETE", &format!("/Users/{bob_id}"), None).status, 204);
    assert_eq!(o.scim("GET", &format!("/Users/{bob_id}"), None).status, 404);
    assert_eq!(o.org_role("bob@corp.test"), None);

    // The org owner is never removed by SCIM.
    let ada_id = o.create_user("ada@corp.test");
    assert_eq!(o.org_role("ada@corp.test").as_deref(), Some("owner"));
    let r = o.scim("PATCH", &format!("/Users/{ada_id}"), Some(patch(json!([{ "op": "replace", "path": "active", "value": false }]))));
    assert_eq!(r.status, 409, "{}", r.json);
    assert_eq!(o.org_role("ada@corp.test").as_deref(), Some("owner"));
    assert_eq!(o.scim("DELETE", &format!("/Users/{ada_id}"), None).status, 409);

    let audit = audit_rows(&o.s, &format!("org:{}", o.org));
    for e in ["scim_user_created", "scim_user_updated", "scim_user_deactivated", "scim_user_reactivated", "scim_user_deleted", "org_member_removed"] {
        assert!(audit.iter().any(|a| a["event"] == e && a["actor"] == "scim:Okta"), "{e}");
    }
}

#[test]
fn groups_give_org_and_workspace_roles() {
    let o = setup("scim-groups", false);
    let bob = Device::new(&o.s, "bob@corp.test");
    let cy = Device::new(&o.s, "cy@corp.test");
    let bob_id = o.create_user("bob@corp.test");
    let cy_id = o.create_user("cy@corp.test");
    // Cy was in the workspace before SCIM (as a viewer).
    let k = o.ada.workspace_key(&o.s, WS, 1);
    let wraps = wraps_for(&[(cy.key_id.clone(), cy.keys.public_key().to_vec())], WS, 1, &k);
    assert_eq!(o.s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&o.ada.session), Some(json!({ "email": "cy@corp.test", "role": "viewer", "wraps": wraps }))).status, 201);

    let r = o.scim("POST", "/Groups", Some(json!({ "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"], "displayName": "API team", "externalId": "g-1", "members": [{ "value": bob_id }] })));
    assert_eq!(r.status, 201, "{}", r.json);
    let group = r.json["id"].as_str().unwrap().to_string();
    assert_eq!(r.json["members"][0]["display"], "bob@corp.test");
    assert_eq!(o.scim("GET", "/Groups?filter=displayName%20eq%20%22API%20team%22", None).json["totalResults"], 1);
    assert!(o.scim("GET", "/Groups?excludedAttributes=members", None).json["Resources"][0].get("members").is_none());
    assert_eq!(o.scim("GET", &format!("/Users/{bob_id}"), None).json["groups"][0]["value"], group.as_str());

    // The admin sets what the group gives: org admin, editor in WS.
    assert_eq!(o.group_roles(&group, json!({ "orgRole": "owner" })).status, 400);
    assert_eq!(o.group_roles(&group, json!({ "workspaces": [{ "workspace": WS, "role": "owner" }] })).status, 400);
    assert_eq!(o.group_roles(&group, json!({ "workspaces": [{ "workspace": WS2, "role": "editor" }] })).status, 404, "not a workspace of the org");
    let r = o.group_roles(&group, json!({ "orgRole": "admin", "workspaces": [{ "workspace": WS, "role": "editor" }] }));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["workspaces"][0]["role"], "editor");
    assert_eq!(o.org_role("bob@corp.test").as_deref(), Some("admin"));
    assert_eq!(o.org_role("cy@corp.test").as_deref(), Some("member"));

    // Bob isn't in WS yet: an invite he has accepted, with his device keys, for the owner's client
    // (`hitop sync members`, then `hitop sync add`), as for any accepted invite.
    let inv = o.ws_invites(WS);
    assert_eq!(inv.len(), 1, "{inv:?}");
    assert_eq!(inv[0]["email"], "bob@corp.test");
    assert_eq!(inv[0]["role"], "editor");
    assert_eq!(inv[0]["accepted"]["userId"], bob.user_id.as_str());
    assert_eq!(inv[0]["accepted"]["keys"][0]["id"], bob.key_id.as_str());
    let wraps = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &k);
    let r = o.s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&o.ada.session), Some(json!({ "email": "bob@corp.test", "role": "editor", "wraps": wraps })));
    assert_eq!(r.status, 201, "{}", r.json);
    assert!(o.ws_invites(WS).is_empty());
    assert_eq!(o.ws_role(WS, &bob.user_id).as_deref(), Some("editor"));

    // Cy joins the group: SCIM sets the role of a member already there.
    let r = o.scim("PATCH", &format!("/Groups/{group}"), Some(patch(json!([{ "op": "add", "path": "members", "value": [{ "value": cy_id }] }]))));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["members"].as_array().unwrap().len(), 2);
    assert_eq!(o.ws_role(WS, &cy.user_id).as_deref(), Some("editor"));
    assert_eq!(o.org_role("cy@corp.test").as_deref(), Some("admin"));

    // Both leave the group (members[value eq], and Entra's remove with a value list): Bob, whom
    // SCIM brought, leaves WS (wraps gone, rotation requested); Cy, there before, stays.
    for (op, who) in [(json!({ "op": "remove", "path": format!("members[value eq \"{bob_id}\"]") }), &bob), (json!({ "op": "Remove", "path": "members", "value": [{ "value": cy_id }] }), &cy)] {
        let r = o.scim("PATCH", &format!("/Groups/{group}"), Some(patch(json!([op]))));
        assert_eq!(r.status, 200, "{} {}", who.user_id, r.json);
    }
    assert_eq!(o.ws_role(WS, &bob.user_id), None);
    assert_eq!(o.ws_role(WS, &cy.user_id).as_deref(), Some("editor"));
    let r = o.s.call("GET", &format!("/v1/workspaces/{WS}/wraps?keyId={}", bob.key_id), Some(&bob.session), None);
    assert_eq!(r.status, 404, "bob is no member: {}", r.json);
    assert_eq!(o.s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&o.ada.session), None).json["rotationRequested"]["by"], "scim:Okta");
    // Without an admin group they're members again (the org uses groups for org roles).
    assert_eq!(o.org_role("bob@corp.test").as_deref(), Some("member"));
    let ws_audit = audit_rows(&o.s, WS);
    assert!(ws_audit.iter().any(|e| e["event"] == "member_removed" && e["actor"] == "scim:Okta"));
    assert!(ws_audit.iter().any(|e| e["event"] == "rotation_requested"));

    // A pending invite goes when the group stops giving the workspace; deleting the group too.
    o.scim("PATCH", &format!("/Groups/{group}"), Some(patch(json!([{ "op": "replace", "path": "members", "value": [{ "value": bob_id }] }]))));
    assert_eq!(o.ws_invites(WS).len(), 1);
    assert_eq!(o.scim("DELETE", &format!("/Groups/{group}"), None).status, 204);
    assert!(o.ws_invites(WS).is_empty());
    assert_eq!(o.scim("GET", &format!("/Groups/{group}"), None).status, 404);

    // An address without an account: an org invite, and a workspace invite that becomes accepted
    // once the person signs in and registers a device.
    let g2 = o.scim("POST", "/Groups", Some(json!({ "displayName": "Viewers" }))).json["id"].as_str().unwrap().to_string();
    assert_eq!(o.group_roles(&g2, json!({ "workspaces": [{ "workspace": WS, "role": "viewer" }] })).status, 200);
    let fay_id = o.create_user("fay@corp.test");
    o.scim("PATCH", &format!("/Groups/{g2}"), Some(patch(json!([{ "op": "add", "path": "members", "value": [{ "value": fay_id }] }]))));
    let inv = o.ws_invites(WS);
    assert_eq!((inv.len(), inv[0]["accepted"].clone()), (1, Value::Null));
    let fay = Device::new(&o.s, "fay@corp.test");
    let inv = o.ws_invites(WS);
    assert_eq!(inv[0]["accepted"]["userId"], fay.user_id.as_str());
    // Another write re-applies the directory over the open invite: it stays, with the new role.
    assert_eq!(o.group_roles(&g2, json!({ "workspaces": [{ "workspace": WS, "role": "editor" }] })).status, 200);
    let inv = o.ws_invites(WS);
    assert_eq!((inv.len(), inv[0]["role"].clone(), inv[0]["accepted"]["userId"].clone()), (1, json!("editor"), json!(fay.user_id)), "{inv:?}");
    let r = o.scim("PATCH", &format!("/Users/{fay_id}"), Some(patch(json!([{ "op": "replace", "path": "displayName", "value": "Fay" }]))));
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(o.ws_invites(WS).len(), 1);
    // Deactivating Fay removes the invite and her org membership.
    o.scim("PATCH", &format!("/Users/{fay_id}"), Some(patch(json!([{ "op": "replace", "path": "active", "value": false }]))));
    assert!(o.ws_invites(WS).is_empty());
    assert_eq!(o.org_role("fay@corp.test"), None);
    // SCIM invites don't count against the owner's open-invite limit, and can't be accepted by a
    // code (nobody has one).
    assert_eq!(o.s.call("POST", &format!("/v1/workspaces/{WS}/invites"), Some(&o.ada.session), Some(json!({ "role": "viewer" }))).status, 201);
}

#[test]
fn another_orgs_token_sees_nothing_of_this_org() {
    let o = setup("scim-two-orgs", false);
    let bob_id = o.create_user("bob@corp.test");
    let zed = Device::new(&o.s, "zed@other.test");
    let other = o.s.call("POST", "/v1/orgs", Some(&zed.session), Some(json!({ "name": "Other" }))).json["id"].as_str().unwrap().to_string();
    let t = o.s.call("POST", &format!("/v1/orgs/{other}/scim/tokens"), Some(&zed.session), Some(json!({}))).json["token"].as_str().unwrap().to_string();
    let r = o.s.call("GET", "/scim/v2/Users", Some(&t), None);
    assert_eq!(r.json["totalResults"], 0);
    assert_eq!(o.s.call("GET", &format!("/scim/v2/Users/{bob_id}"), Some(&t), None).status, 404);
    assert_eq!(o.s.call("DELETE", &format!("/scim/v2/Users/{bob_id}"), Some(&t), None).status, 404);
    // Groups can't name another org's users.
    let r = o.s.call("POST", "/scim/v2/Groups", Some(&t), Some(json!({ "displayName": "x", "members": [{ "value": bob_id }] })));
    assert_eq!(r.status, 400, "{}", r.json);
    // Nor map another org's workspace.
    let g = o.s.call("POST", "/scim/v2/Groups", Some(&t), Some(json!({ "displayName": "y" }))).json["id"].as_str().unwrap().to_string();
    assert_eq!(o.s.call("PUT", &format!("/v1/orgs/{other}/scim/groups/{g}"), Some(&zed.session), Some(json!({ "workspaces": [{ "workspace": WS, "role": "viewer" }] }))).status, 404);
}

#[test]
fn plans_business_and_enterprise_only() {
    let o = setup("scim-plans", true);
    o.create_user("bob@corp.test");
    for plan in ["team", "pro", "free"] {
        hitop_sync_server::limits::set_plan(o.s.app.state(), &o.org, plan, 10).unwrap();
        let r = o.scim("GET", "/Users", None);
        assert_eq!(r.status, 402, "{plan}: {}", r.json);
        assert_eq!(r.json["schemas"][0], "urn:ietf:params:scim:api:messages:2.0:Error");
        let r = o.s.call("POST", &format!("/v1/orgs/{}/scim/tokens", o.org), Some(&o.ada.session), Some(json!({})));
        assert_eq!((r.status, r.json["limit"].clone()), (402, json!("scim")), "{plan}: {}", r.json);
    }
    // Revoking still works on a lapsed plan; Enterprise has it.
    let id = o.s.app.state().db.tx(|tx| tx.query_one("SELECT id FROM scim_tokens", &[])).unwrap().unwrap().text(0);
    hitop_sync_server::limits::set_plan(o.s.app.state(), &o.org, "enterprise", 10).unwrap();
    assert_eq!(o.scim("GET", "/Users", None).json["totalResults"], 1);
    hitop_sync_server::limits::set_plan(o.s.app.state(), &o.org, "team", 10).unwrap();
    assert_eq!(o.s.call("DELETE", &format!("/v1/orgs/{}/scim/tokens/{id}", o.org), Some(&o.ada.session), None).status, 204);
    // Seats: a directory larger than the plan's seats is refused (402), nothing half-applied.
    let o = setup("scim-seats", true);
    hitop_sync_server::limits::set_plan(o.s.app.state(), &o.org, "business", 2).unwrap();
    o.create_user("bob@corp.test");
    let r = o.scim("POST", "/Users", Some(json!({ "userName": "cy@corp.test" })));
    assert_eq!(r.status, 402, "{}", r.json);
    assert_eq!(o.scim("GET", "/Users", None).json["totalResults"], 1);
}
