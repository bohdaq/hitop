//! The org admin console (HITOP Cloud C5, admin.rs): every workspace of the org with its people,
//! every device of its members; transfer a workspace's ownership, ask for a key rotation, remove a
//! lost device everywhere. Admins and owners only, with a session; Team for the basic console,
//! Business for the rest; never any key or content.

mod common;

use common::*;
use serde_json::json;

const WS: &str = "2f0e0d0c0b0a09080706050403020100";

struct Org {
    s: Server,
    org: String,
    ada: Device,
    bob: Device,
    cy: Device,
}

/// Ada owns the org and a workspace in it; Bob edits the workspace; Cy is an org admin with no
/// workspace.
fn setup(name: &str, plans: bool) -> Org {
    let s = Server::with(name, |c| c.plan_limits = plans);
    let ada = Device::new(&s, "ada@corp.test");
    let bob = Device::new(&s, "bob@corp.test");
    let cy = Device::new(&s, "cy@corp.test");
    let org = s
        .call(
            "POST",
            "/v1/orgs",
            Some(&ada.session),
            Some(json!({ "name": "Corp" })),
        )
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    if plans {
        hitop_sync_server::limits::set_plan(s.app.state(), &org, "business", 10).unwrap();
    }
    for (e, role) in [("bob@corp.test", "member"), ("cy@corp.test", "admin")] {
        assert_eq!(
            s.call(
                "POST",
                &format!("/v1/orgs/{org}/members"),
                Some(&ada.session),
                Some(json!({ "email": e, "role": role }))
            )
            .status,
            201
        );
    }
    let k = create_workspace(&s, &ada, WS);
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{org}/workspaces"),
            Some(&ada.session),
            Some(json!({ "workspace": WS }))
        )
        .status,
        200
    );
    let wraps = wraps_for(
        &[(bob.key_id.clone(), bob.keys.public_key().to_vec())],
        WS,
        1,
        &k,
    );
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/workspaces/{WS}/members"),
            Some(&ada.session),
            Some(json!({ "email": "bob@corp.test", "role": "editor", "wraps": wraps }))
        )
        .status,
        201
    );
    Org {
        s,
        org,
        ada,
        bob,
        cy,
    }
}

#[test]
fn admins_see_every_workspace_and_device_and_transfer_ownership() {
    let t = setup("admin-console", false);
    let (s, org) = (&t.s, &t.org);
    // Cy, an org admin with no workspace of their own, sees the workspace and its people.
    let r = s.call(
        "GET",
        &format!("/v1/orgs/{org}/admin"),
        Some(&t.cy.session),
        None,
    );
    assert_eq!(r.status, 200, "{}", r.json);
    let ws = &r.json["workspaces"][0];
    assert_eq!(
        (
            ws["id"].as_str(),
            ws["epoch"].as_i64(),
            ws["rotationRequested"].is_null()
        ),
        (Some(WS), Some(1), true)
    );
    let people: Vec<(&str, &str)> = ws["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["email"].as_str().unwrap(), m["role"].as_str().unwrap()))
        .collect();
    assert_eq!(
        people,
        vec![("ada@corp.test", "owner"), ("bob@corp.test", "editor")]
    );
    let devices = r.json["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 3);
    let bobs = devices
        .iter()
        .find(|d| d["email"] == "bob@corp.test")
        .unwrap();
    assert_eq!(
        (bobs["keyId"].as_str(), bobs["orgWorkspaces"].as_i64()),
        (Some(t.bob.key_id.as_str()), Some(1))
    );
    // No key material beyond public keys, no workspace contents.
    assert!(!r.json.to_string().contains("wrapped"));
    // But Cy is still not in the workspace: no key, no reads.
    assert_eq!(
        s.call(
            "GET",
            &format!("/v1/workspaces/{WS}"),
            Some(&t.cy.session),
            None
        )
        .status,
        404
    );
    // A plain member doesn't see the console.
    assert_eq!(
        s.call(
            "GET",
            &format!("/v1/orgs/{org}/admin"),
            Some(&t.bob.session),
            None
        )
        .status,
        403
    );

    // Ownership to Bob; Ada becomes an editor.
    let to = |user: &str, extra: serde_json::Value| {
        let mut b = json!({ "userId": user });
        for (k, v) in extra.as_object().unwrap() {
            b[k] = v.clone();
        }
        s.call(
            "PUT",
            &format!("/v1/orgs/{org}/admin/workspaces/{WS}/owner"),
            Some(&t.cy.session),
            Some(b),
        )
    };
    assert_eq!(
        to(&t.cy.user_id, json!({})).status,
        400,
        "only a member of the workspace, who holds its key"
    );
    let r = to(&t.bob.user_id, json!({}));
    assert_eq!(r.status, 200, "{}", r.json);
    let roles = s
        .call(
            "GET",
            &format!("/v1/workspaces/{WS}"),
            Some(&t.bob.session),
            None,
        )
        .json;
    let roles: Vec<(String, String)> = roles["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["email"].as_str().unwrap().into(),
                m["role"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        roles,
        vec![
            ("ada@corp.test".into(), "editor".into()),
            ("bob@corp.test".into(), "owner".into())
        ]
    );
    // Back, keeping Bob as an owner.
    assert_eq!(
        to(&t.ada.user_id, json!({ "keepOwners": true })).status,
        200
    );
    let owners = s
        .call(
            "GET",
            &format!("/v1/workspaces/{WS}"),
            Some(&t.ada.session),
            None,
        )
        .json["members"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "owner")
        .count();
    assert_eq!(owners, 2);
    // Not with an access token; not for a workspace outside the org.
    let token = s
        .call(
            "POST",
            "/v1/me/tokens",
            Some(&t.cy.session),
            Some(json!({ "name": "ci", "scopes": ["read", "write"] })),
        )
        .json["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        s.call(
            "PUT",
            &format!("/v1/orgs/{org}/admin/workspaces/{WS}/owner"),
            Some(&token),
            Some(json!({ "userId": t.ada.user_id }))
        )
        .status,
        403
    );
    let other = "3f0e0d0c0b0a09080706050403020100";
    create_workspace(s, &t.ada, other);
    assert_eq!(
        s.call(
            "PUT",
            &format!("/v1/orgs/{org}/admin/workspaces/{other}/owner"),
            Some(&t.cy.session),
            Some(json!({ "userId": t.ada.user_id }))
        )
        .status,
        404
    );
    let ws_events: Vec<String> = audit_rows(s, WS)
        .iter()
        .map(|r| r["event"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        ws_events
            .iter()
            .filter(|e| *e == "owner_transferred")
            .count(),
        2
    );
}

#[test]
fn forced_rotation_and_removing_a_lost_device_from_the_orgs_workspaces() {
    let t = setup("admin-actions", false);
    let (s, org) = (&t.s, &t.org);
    // Ask Ada's clients to rotate: the flag shows to members until a rotation clears it.
    let r = s.call(
        "POST",
        &format!("/v1/orgs/{org}/admin/workspaces/{WS}/rotation"),
        Some(&t.cy.session),
        None,
    );
    assert_eq!(r.status, 202, "{}", r.json);
    let got = s
        .call(
            "GET",
            &format!("/v1/workspaces/{WS}"),
            Some(&t.ada.session),
            None,
        )
        .json;
    assert_eq!(got["rotationRequested"]["by"], "cy@corp.test");
    let listed = s
        .call("GET", "/v1/workspaces", Some(&t.ada.session), None)
        .json;
    assert_eq!(listed["workspaces"][0]["rotationRequested"], true);
    let k2 = hitop_sync_crypto::WorkspaceKey::generate();
    let keys = member_keys(s, WS, &t.ada.session);
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/workspaces/{WS}/rotate"),
            Some(&t.ada.session),
            Some(json!({ "epoch": 2, "wraps": wraps_for(&keys, WS, 2, &k2) }))
        )
        .status,
        200
    );
    assert!(s
        .call(
            "GET",
            &format!("/v1/workspaces/{WS}"),
            Some(&t.ada.session),
            None
        )
        .json["rotationRequested"]
        .is_null());
    assert!(
        s.call("GET", "/v1/workspaces", Some(&t.ada.session), None)
            .json["workspaces"][0]
            .get("rotationRequested")
            .is_none(),
        "older clients see the shape they know"
    );

    // Bob's laptop is stolen: Cy removes it from the org's workspaces. Its wrapped keys there go,
    // the workspace asks for a rotation, and the next rotation no longer needs (or accepts) a wrap
    // for it. Bob's own personal workspace, and the device key itself, are not the org's: they
    // stay untouched (owner decision 2026-11-03).
    const MINE: &str = "3f0e0d0c0b0a09080706050403020100";
    create_workspace(s, &t.bob, MINE);
    let second = Device::another(s, "bob@corp.test", &t.bob.session);
    let r = s.call(
        "DELETE",
        &format!("/v1/orgs/{org}/admin/devices/{}", t.bob.key_id),
        Some(&t.cy.session),
        None,
    );
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(r.json["rotationRequested"], json!([WS]), "only the org's workspace");
    let count = |sql: &str, ws: &str| {
        s.app
            .state()
            .db
            .tx(|tx| tx.query_one(sql, &[t.bob.key_id.as_str().into(), ws.into()]))
            .unwrap()
            .unwrap()
            .int(0)
    };
    let wraps = "SELECT COUNT(*) FROM wraps WHERE key_id = $1 AND workspace_id = $2";
    assert_eq!(count(wraps, WS), 0, "gone from the org's workspace");
    assert_eq!(count(wraps, MINE), 1, "kept in Bob's personal workspace");
    let device = s.app.state().db.tx(|tx| tx.query_one("SELECT COUNT(*) FROM device_keys WHERE id = $1", &[t.bob.key_id.as_str().into()])).unwrap().unwrap().int(0);
    assert_eq!(device, 1, "the device key stays on Bob's account");
    // Nobody can give the removed laptop the org workspace's key again: not Bob, not an owner.
    let k_now = hitop_sync_crypto::WorkspaceKey::generate();
    let epoch = s.call("GET", &format!("/v1/workspaces/{WS}"), Some(&t.ada.session), None).json["epoch"].as_u64().unwrap() as u32;
    let again = wraps_for(&[(t.bob.key_id.clone(), t.bob.keys.public_key().to_vec())], WS, epoch, &k_now);
    for who in [&t.bob, &t.ada] {
        let r = s.call("POST", &format!("/v1/workspaces/{WS}/wraps"), Some(&who.session), Some(json!({ "epoch": epoch, "wraps": again })));
        assert_eq!(r.status, 400, "{}", r.json);
        assert!(r.json["message"].as_str().unwrap().contains("removed from this workspace by an organization admin"), "{}", r.json);
    }
    // The removed laptop still opens Bob's own workspace, with no rotation asked there.
    let mine = s.call("GET", &format!("/v1/workspaces/{MINE}"), Some(&t.bob.session), None);
    assert_eq!(mine.status, 200, "{}", mine.json);
    assert!(mine.json["rotationRequested"].is_null());
    let listed = s.call("GET", &format!("/v1/orgs/{org}/admin"), Some(&t.cy.session), None).json;
    let laptop = listed["devices"].as_array().unwrap().iter().find(|d| d["keyId"] == t.bob.key_id.as_str()).cloned().unwrap();
    assert_eq!(laptop["orgWorkspaces"], 0, "listed, with no org workspace left");
    assert!(!s
        .call(
            "GET",
            &format!("/v1/workspaces/{WS}"),
            Some(&t.ada.session),
            None
        )
        .json["rotationRequested"]
        .is_null());
    let keys = member_keys(s, WS, &t.ada.session);
    assert!(
        !keys.iter().any(|(id, _)| *id == t.bob.key_id)
            && keys.iter().any(|(id, _)| *id == second.key_id)
    );
    let k3 = hitop_sync_crypto::WorkspaceKey::generate();
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/workspaces/{WS}/rotate"),
            Some(&t.ada.session),
            Some(json!({ "epoch": 3, "wraps": wraps_for(&keys, WS, 3, &k3) }))
        )
        .status,
        200
    );
    // Bob still signs in and works from his other device.
    assert_eq!(
        s.call(
            "GET",
            &format!("/v1/workspaces/{WS}"),
            Some(&second.session),
            None
        )
        .status,
        200
    );
    // Only a device of an org member; the plain member can't.
    assert_eq!(
        s.call(
            "DELETE",
            &format!("/v1/orgs/{org}/admin/devices/{}", second.key_id),
            Some(&t.bob.session),
            None
        )
        .status,
        403
    );
    let outsider = Device::new(s, "zed@else.test");
    assert_eq!(
        s.call(
            "DELETE",
            &format!("/v1/orgs/{org}/admin/devices/{}", outsider.key_id),
            Some(&t.cy.session),
            None
        )
        .status,
        404
    );
    let events: Vec<String> = audit_rows(s, WS)
        .iter()
        .map(|r| r["event"].as_str().unwrap().to_string())
        .collect();
    for e in ["rotation_requested", "device_removed"] {
        assert!(events.iter().any(|x| x == e), "{e} in {events:?}");
    }
}

#[test]
fn team_gets_the_basic_console_business_the_actions() {
    let t = setup("admin-plans", true);
    let (s, org) = (&t.s, &t.org);
    hitop_sync_server::limits::set_plan(s.app.state(), org, "team", 10).unwrap();
    let r = s.call(
        "GET",
        &format!("/v1/orgs/{org}/admin"),
        Some(&t.ada.session),
        None,
    );
    assert_eq!(r.status, 200);
    assert_eq!(
        r.json["actions"],
        json!({ "transferOwner": true, "forceRotation": false, "removeDevice": false })
    );
    let r = s.call(
        "POST",
        &format!("/v1/orgs/{org}/admin/workspaces/{WS}/rotation"),
        Some(&t.ada.session),
        None,
    );
    assert_eq!(
        (r.status, r.json["limit"].as_str()),
        (402, Some("adminActions"))
    );
    assert_eq!(
        s.call(
            "DELETE",
            &format!("/v1/orgs/{org}/admin/devices/{}", t.bob.key_id),
            Some(&t.ada.session),
            None
        )
        .status,
        402
    );
    hitop_sync_server::limits::set_plan(s.app.state(), org, "free", 0).unwrap();
    let r = s.call(
        "GET",
        &format!("/v1/orgs/{org}/admin"),
        Some(&t.ada.session),
        None,
    );
    assert_eq!(
        (r.status, r.json["limit"].as_str()),
        (402, Some("adminConsole"))
    );
    hitop_sync_server::limits::set_plan(s.app.state(), org, "business", 10).unwrap();
    assert_eq!(
        s.call(
            "POST",
            &format!("/v1/orgs/{org}/admin/workspaces/{WS}/rotation"),
            Some(&t.ada.session),
            None
        )
        .status,
        202
    );
}
