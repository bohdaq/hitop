//! The org admin console's endpoints (HITOP Cloud C5; the portal's Admin page).
//!
//! Org owners and admins see every workspace of the org with its members and roles, and every
//! device key of the org's members, with no workspace membership of their own: metadata the server
//! already holds, never contents (an org role never gives a workspace key; orgs.rs).
//!
//! - `GET /v1/orgs/:org/admin` (Team "basic admin", Business, Enterprise): workspaces, members,
//!   device keys.
//! - `PUT /v1/orgs/:org/admin/workspaces/:ws/owner {"userId", "keepOwners"?}`: makes a member of
//!   the workspace its owner; the other owners become editors unless `keepOwners`. The new owner
//!   already holds the workspace key (they're a member), so nothing needs re-wrapping.
//! - `POST /v1/orgs/:org/admin/workspaces/:ws/rotation` (Business): asks the workspace's owners to
//!   rotate its key. The server can't rotate (it holds no key): it sets `rotationRequested` (on
//!   `GET /v1/workspaces/:ws` and the list), clients of owners say so, and the next rotation
//!   (`POST .../rotate`, or a removal) clears it.
//! - `DELETE /v1/orgs/:org/admin/devices/:key` (Business): takes a member's device (a lost or
//!   stolen laptop) out of **the org's workspaces only** (owner decision 2026-11-03): its wrapped
//!   workspace keys and shared-value wraps in the org's workspaces go, and each of those workspaces
//!   gets `rotationRequested`. The device key itself, and its access to the member's personal and
//!   other orgs' workspaces, stay untouched: an org admin governs the org, not the person's account.
//!   The person's other devices keep working.
//!
//! Changes need a signed-in session (not an access token) and are recorded on the org's chain and
//! on each workspace's chain they touch.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{b64, body, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::User;
use crate::db::Tx;
use crate::orgs::{self, OrgRole};

pub const BASIC_PLANS: &[&str] = &["team", "business", "enterprise"];
pub const FULL_PLANS: &[&str] = &["business", "enterprise"];

fn caller(
    state: &State,
    tx: &mut Tx,
    request: &Request,
    org: &str,
    change: bool,
    full: bool,
) -> Result<(User, orgs::Org), ApiError> {
    let (user, o) = orgs::org_caller(state, tx, request, org, OrgRole::Admin, change)?;
    if o.personal {
        return Err(ApiError::bad(
            "the admin console is for an organization, not a personal account",
        ));
    }
    if full {
        crate::limits::require_plan(
            state,
            tx,
            org,
            "adminActions",
            "forced key rotation and device removal",
            FULL_PLANS,
        )?;
    } else {
        crate::limits::require_plan(
            state,
            tx,
            org,
            "adminConsole",
            "the admin console",
            BASIC_PLANS,
        )?;
    }
    Ok((user, o))
}

fn org_workspace(tx: &mut Tx, org: &str, ws: &str) -> Result<(), ApiError> {
    tx.query_one(
        "SELECT id FROM workspaces WHERE id = $1 AND org_id = $2",
        &[ws.into(), org.into()],
    )?
    .map(|_| ())
    .ok_or_else(|| ApiError::not_found("no such workspace in this organization"))
}

/// `GET /v1/orgs/:org/admin`.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        caller(state, tx, request, &org, false, false)?;
        let mut workspaces = Vec::new();
        for w in tx.query("SELECT id, epoch, bytes, created_at, rotation_requested_at, rotation_requested_by FROM workspaces WHERE org_id = $1 ORDER BY created_at, id", &[(&org).into()])? {
            let members: Vec<Json> = tx
                .query("SELECT m.user_id, u.email, m.role, u.runner_of FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 ORDER BY u.email", &[w.text(0).into()])?
                .iter()
                .map(|m| json!({ "userId": m.text(0), "email": m.text(1), "role": m.text(2), "runner": !m.text(3).is_empty() }))
                .collect();
            workspaces.push(json!({
                "id": w.text(0),
                "epoch": w.int(1),
                "bytes": w.int(2),
                "createdAt": w.int(3),
                "rotationRequested": if w.int(4) > 0 { json!({ "at": w.int(4), "by": w.text(5) }) } else { Json::Null },
                "members": members,
            }));
        }
        let mut devices = Vec::new();
        for k in tx.query(
            "SELECT k.id, k.user_id, u.email, k.name, k.public_key, k.created_at FROM device_keys k JOIN org_members m ON m.user_id = k.user_id JOIN users u ON u.id = k.user_id WHERE m.org_id = $1 ORDER BY u.email, k.created_at, k.id",
            &[(&org).into()],
        )? {
            let n = tx
                .query_one("SELECT COUNT(DISTINCT x.workspace_id) FROM wraps x JOIN workspaces w ON w.id = x.workspace_id WHERE x.key_id = $1 AND w.org_id = $2 AND x.epoch = w.epoch", &[k.text(0).into(), (&org).into()])?
                .map_or(0, |r| r.int(0));
            devices.push(json!({ "keyId": k.text(0), "userId": k.text(1), "email": k.text(2), "name": k.text(3), "publicKey": b64(&k.bytes(4)), "createdAt": k.int(5), "orgWorkspaces": n }));
        }
        let full = !state.config.plan_limits || FULL_PLANS.contains(&crate::limits::entitlement(tx, &org)?.plan_in_force());
        Ok((200, json!({ "org": org, "actions": { "transferOwner": true, "forceRotation": full, "removeDevice": full }, "workspaces": workspaces, "devices": devices })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct OwnerIn {
    user_id: String,
    #[serde(default)]
    keep_owners: bool,
}

/// `PUT /v1/orgs/:org/admin/workspaces/:ws/owner {"userId", "keepOwners"?}`.
pub fn transfer_owner(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let ws = params.get("ws").unwrap_or_default().to_string();
    let input: OwnerIn = body(request)?;
    state.db.tx(|tx| {
        let (user, _) = caller(state, tx, request, &org, true, false)?;
        org_workspace(tx, &org, &ws)?;
        let Some(m) = tx.query_one("SELECT m.role, u.email, u.runner_of FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 AND m.user_id = $2", &[(&ws).into(), (&input.user_id).into()])? else {
            return Err(ApiError::bad("the new owner must already be a member of the workspace (they need its key)"));
        };
        if !m.text(2).is_empty() {
            return Err(ApiError::bad("a runner credential can't own a workspace"));
        }
        let before: Vec<String> = tx.query("SELECT user_id FROM members WHERE workspace_id = $1 AND role = 'owner' AND user_id <> $2", &[(&ws).into(), (&input.user_id).into()])?.iter().map(|r| r.text(0)).collect();
        tx.execute("UPDATE members SET role = 'owner' WHERE workspace_id = $1 AND user_id = $2", &[(&ws).into(), (&input.user_id).into()])?;
        if !input.keep_owners {
            tx.execute("UPDATE members SET role = 'editor' WHERE workspace_id = $1 AND role = 'owner' AND user_id <> $2", &[(&ws).into(), (&input.user_id).into()])?;
        }
        let detail = json!({ "email": m.text(1), "from": m.text(0), "previousOwners": before, "keepOwners": input.keep_owners, "by": "org_admin" });
        audit::record(tx, state, Event::new(&ws, "owner_transferred", Some(&user)).target(&input.user_id).detail(detail.clone()))?;
        audit::record(tx, state, Event::new(&orgs::chain(&org), "owner_transferred", Some(&user)).target(&ws).detail(detail))?;
        Ok((200, json!({ "workspace": ws, "owner": input.user_id, "previousOwners": before, "keepOwners": input.keep_owners })))
    })
}

fn request_rotation(
    tx: &mut Tx,
    state: &State,
    ws: &str,
    user: &User,
    reason: &str,
) -> Result<(), ApiError> {
    tx.execute("UPDATE workspaces SET rotation_requested_at = $1, rotation_requested_by = $2 WHERE id = $3", &[state.now().into(), (&user.email).into(), ws.into()])?;
    audit::record(
        tx,
        state,
        Event::new(ws, "rotation_requested", Some(user))
            .target(ws)
            .detail(json!({ "reason": reason })),
    )?;
    Ok(())
}

/// `POST /v1/orgs/:org/admin/workspaces/:ws/rotation`.
pub fn force_rotation(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let ws = params.get("ws").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        let (user, _) = caller(state, tx, request, &org, true, true)?;
        org_workspace(tx, &org, &ws)?;
        request_rotation(tx, state, &ws, &user, "org_admin")?;
        audit::record(
            tx,
            state,
            Event::new(&orgs::chain(&org), "rotation_requested", Some(&user)).target(&ws),
        )?;
        Ok((202, json!({ "workspace": ws, "rotationRequested": true })))
    })
}

/// `DELETE /v1/orgs/:org/admin/devices/:key`.
pub fn remove_device(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let key = params.get("key").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        let (user, _) = caller(state, tx, request, &org, true, true)?;
        let Some(k) = tx.query_one(
            "SELECT k.user_id, u.email, k.name FROM device_keys k JOIN org_members m ON m.user_id = k.user_id JOIN users u ON u.id = k.user_id WHERE k.id = $1 AND m.org_id = $2",
            &[(&key).into(), (&org).into()],
        )?
        else {
            return Err(ApiError::not_found("no such device key of a member of this organization"));
        };
        // Only the org's workspaces: the member's personal and other orgs' workspaces, and the
        // device key itself, stay as they are. Every org workspace the member is in excludes the
        // key from now on (else the next rotation would wrap the new key for it again); those
        // where it held a key also ask for a rotation.
        let now = state.now();
        for ws in tx.query("SELECT m.workspace_id FROM members m JOIN workspaces s ON s.id = m.workspace_id WHERE m.user_id = $1 AND s.org_id = $2", &[k.text(0).into(), (&org).into()])? {
            tx.execute(
                "INSERT INTO workspace_excluded_keys (workspace_id, key_id, excluded_at, excluded_by) VALUES ($1, $2, $3, $4) ON CONFLICT (workspace_id, key_id) DO NOTHING",
                &[ws.text(0).into(), (&key).into(), now.into(), (&user.id).into()],
            )?;
        }
        let held: Vec<String> = tx
            .query("SELECT DISTINCT w.workspace_id FROM wraps w JOIN workspaces s ON s.id = w.workspace_id WHERE w.key_id = $1 AND s.org_id = $2 ORDER BY w.workspace_id", &[(&key).into(), (&org).into()])?
            .iter()
            .map(|r| r.text(0))
            .collect();
        for ws in &held {
            tx.execute("DELETE FROM env_share_wraps WHERE key_id = $1 AND workspace_id = $2", &[(&key).into(), ws.into()])?;
            tx.execute("DELETE FROM wraps WHERE key_id = $1 AND workspace_id = $2", &[(&key).into(), ws.into()])?;
        }
        for ws in &held {
            request_rotation(tx, state, ws, &user, "device_removed")?;
            audit::record(tx, state, Event::new(ws, "device_removed", Some(&user)).target(&k.text(0)).detail(json!({ "keyId": key, "email": k.text(1), "by": "org_admin" })))?;
        }
        audit::record(tx, state, Event::new(&orgs::chain(&org), "device_removed", Some(&user)).target(&k.text(0)).detail(json!({ "keyId": key, "email": k.text(1), "name": k.text(2), "workspaces": held.len() })))?;
        Ok((200, json!({ "removed": key, "rotationRequested": held })))
    })
}
