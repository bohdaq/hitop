//! Device keys, workspaces, members and wrapped workspace keys.
//!
//! The server holds public keys and HPKE-wrapped workspace keys (opaque to it). What it enforces:
//! a workspace key epoch must be wrapped for **every** device of every member (no more, no less)
//! when it is created or rotated; removing a member is only accepted together with the next
//! epoch's wraps for the remaining devices, so the removed member's key is never used for new
//! writes; blob writes must be under the current epoch (blobs.rs).

use std::collections::{BTreeMap, BTreeSet};

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{b64, body, query, random_hex, unb64, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, User};
use crate::db::Tx;

/// HPKE enc (32) + sealed 32-byte key + tag (16), see hitop-sync-crypto.
pub const WRAPPED_KEY_LEN: usize = 80;
const MAX_KEYS_PER_USER: i64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Editor,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Result<Role, ApiError> {
        match s {
            "owner" => Ok(Role::Owner),
            "editor" => Ok(Role::Editor),
            "viewer" => Ok(Role::Viewer),
            _ => Err(ApiError::bad("role must be owner, editor or viewer")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Editor => "editor",
            Role::Viewer => "viewer",
        }
    }
}

pub fn valid_workspace_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub struct Workspace {
    pub id: String,
    pub epoch: i64,
    pub seq: i64,
    pub bytes: i64,
    pub role: Role,
}

/// The workspace as the caller sees it; 404 (not 403) when they aren't a member, so workspace ids
/// can't be probed.
pub fn membership(tx: &mut Tx, ws: &str, user: &User) -> Result<Workspace, ApiError> {
    let row = tx.query_one(
        "SELECT w.id, w.epoch, w.seq, w.bytes, m.role FROM workspaces w JOIN members m ON m.workspace_id = w.id WHERE w.id = $1 AND m.user_id = $2",
        &[ws.into(), (&user.id).into()],
    )?;
    let row = row.ok_or_else(|| ApiError::not_found("no such workspace"))?;
    Ok(Workspace { id: row.text(0), epoch: row.int(1), seq: row.int(2), bytes: row.int(3), role: Role::parse(&row.text(4))? })
}

pub(crate) fn require(ws: &Workspace, role: Role) -> Result<(), ApiError> {
    if ws.role < role {
        return Err(ApiError::forbidden(format!("this needs the {} role", role.as_str())));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct WrapIn {
    pub(crate) key_id: String,
    pub(crate) wrapped: String,
}

/// Checks that `wraps` holds exactly one well-formed wrapped key for each device key of `users`.
pub(crate) fn exact_wraps(tx: &mut Tx, users: &[String], wraps: &[WrapIn]) -> Result<Vec<(String, Vec<u8>)>, ApiError> {
    let mut expected = BTreeSet::new();
    for user in users {
        for row in tx.query("SELECT id FROM device_keys WHERE user_id = $1", &[user.into()])? {
            expected.insert(row.text(0));
        }
    }
    let mut given = BTreeMap::new();
    for w in wraps {
        let bytes = unb64("wrapped", &w.wrapped)?;
        if bytes.len() != WRAPPED_KEY_LEN {
            return Err(ApiError::bad(format!("wrapped key for {} must be {WRAPPED_KEY_LEN} bytes", w.key_id)));
        }
        if given.insert(w.key_id.clone(), bytes).is_some() {
            return Err(ApiError::bad(format!("two wrapped keys for {}", w.key_id)));
        }
    }
    let given_ids: BTreeSet<String> = given.keys().cloned().collect();
    if given_ids != expected {
        let missing: Vec<_> = expected.difference(&given_ids).cloned().collect();
        let extra: Vec<_> = given_ids.difference(&expected).cloned().collect();
        return Err(ApiError {
            status: 400,
            code: "wraps_mismatch",
            message: "the workspace key must be wrapped for every device key of every member, and nothing else".into(),
            extra: Some(json!({ "missing": missing, "unexpected": extra })),
        });
    }
    Ok(given.into_iter().collect())
}

pub(crate) fn insert_wraps(tx: &mut Tx, ws: &str, epoch: i64, wraps: &[(String, Vec<u8>)]) -> Result<(), ApiError> {
    for (key_id, wrapped) in wraps {
        tx.execute(
            "INSERT INTO wraps (workspace_id, epoch, key_id, wrapped) VALUES ($1, $2, $3, $4) ON CONFLICT (workspace_id, epoch, key_id) DO NOTHING",
            &[ws.into(), epoch.into(), key_id.into(), wrapped.clone().into()],
        )?;
    }
    Ok(())
}

fn member_ids(tx: &mut Tx, ws: &str) -> Result<Vec<String>, ApiError> {
    Ok(tx.query("SELECT user_id FROM members WHERE workspace_id = $1 ORDER BY user_id", &[ws.into()])?.iter().map(|r| r.text(0)).collect())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NewKey {
    #[serde(default)]
    name: String,
    public_key: String,
}

/// `POST /v1/me/keys {"name","publicKey"}`: registers a device's X25519 public key.
pub fn add_device_key(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: NewKey = body(request)?;
    let public_key = unb64("publicKey", &input.public_key)?;
    if public_key.len() != 32 {
        return Err(ApiError::bad("publicKey must be a 32-byte X25519 public key"));
    }
    let name: String = input.name.trim().chars().take(64).collect();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let count = tx.query_one("SELECT COUNT(*) FROM device_keys WHERE user_id = $1", &[(&user.id).into()])?.map_or(0, |r| r.int(0));
        if count >= MAX_KEYS_PER_USER {
            return Err(ApiError::bad("this account has the maximum number of device keys"));
        }
        let id = random_hex::<16>();
        tx.execute(
            "INSERT INTO device_keys (id, user_id, name, public_key, created_at) VALUES ($1, $2, $3, $4, $5)",
            &[(&id).into(), (&user.id).into(), (&name).into(), public_key.into(), state.now().into()],
        )?;
        audit::record(tx, state, Event::new(audit::ACCOUNT, "device_key_added", Some(&user)).target(&user.id).detail(json!({ "keyId": id, "name": name })))?;
        Ok((201, json!({ "id": id })))
    })
}

/// `GET /v1/users/keys?email=`: another account's public keys, to wrap a workspace key for them.
pub fn user_keys(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let email = query(request).get("email").map(|e| e.trim().to_lowercase()).unwrap_or_default();
    state.db.tx(|tx| {
        authenticate(state, tx, request)?.person()?;
        let Some(row) = tx.query_one("SELECT id FROM users WHERE email = $1", &[(&email).into()])? else {
            return Err(ApiError::not_found("no account with this address"));
        };
        let user_id = row.text(0);
        let keys = tx.query("SELECT id, name, public_key FROM device_keys WHERE user_id = $1 ORDER BY created_at, id", &[(&user_id).into()])?;
        Ok((200, json!({ "userId": user_id, "keys": keys.iter().map(|r| json!({ "id": r.text(0), "name": r.text(1), "publicKey": b64(&r.bytes(2)) })).collect::<Vec<_>>() })))
    })
}

/// `GET /v1/workspaces`: the caller's workspaces.
pub fn list(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let rows = tx.query(
            "SELECT w.id, m.role, w.epoch, w.seq FROM workspaces w JOIN members m ON m.workspace_id = w.id WHERE m.user_id = $1 ORDER BY w.created_at, w.id",
            &[(&user.id).into()],
        )?;
        Ok((200, json!({ "workspaces": rows.iter().map(|r| json!({ "id": r.text(0), "role": r.text(1), "epoch": r.int(2), "seq": r.int(3) })).collect::<Vec<_>>() })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewWorkspace {
    id: String,
    wraps: Vec<WrapIn>,
}

/// `POST /v1/workspaces {"id","wraps"}`: the caller becomes owner; epoch 1's key must be
/// wrapped for each of their device keys.
pub fn create(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: NewWorkspace = body(request)?;
    if !valid_workspace_id(&input.id) {
        return Err(ApiError::bad("id must be 32 lower-case hex digits (random, from the client)"));
    }
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        if tx.query_one("SELECT id FROM workspaces WHERE id = $1", &[(&input.id).into()])?.is_some() {
            return Err(ApiError::conflict("exists", "a workspace with this id exists", json!({})));
        }
        let owned = tx.query_one("SELECT COUNT(*) FROM members WHERE user_id = $1 AND role = 'owner'", &[(&user.id).into()])?.map_or(0, |r| r.int(0));
        if owned >= state.config.max_workspaces {
            return Err(ApiError::forbidden("this account owns the maximum number of workspaces"));
        }
        let wraps = exact_wraps(tx, std::slice::from_ref(&user.id), &input.wraps)?;
        if wraps.is_empty() {
            return Err(ApiError::bad("register a device key first (POST /v1/me/keys)"));
        }
        tx.execute("INSERT INTO workspaces (id, epoch, seq, bytes, created_at) VALUES ($1, 1, 0, 0, $2)", &[(&input.id).into(), state.now().into()])?;
        tx.execute("INSERT INTO members (workspace_id, user_id, role) VALUES ($1, $2, 'owner')", &[(&input.id).into(), (&user.id).into()])?;
        insert_wraps(tx, &input.id, 1, &wraps)?;
        audit::record(tx, state, Event::new(&input.id, "workspace_created", Some(&user)).target(&input.id))?;
        Ok((201, json!({ "id": input.id, "epoch": 1 })))
    })
}

/// `GET /v1/workspaces/:ws`: epoch, sequence and members with their device keys; `hasKey` says
/// whether a device has the current epoch's key (a new device of a member doesn't until someone
/// wraps it, `POST .../wraps`).
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        let rows = tx.query(
            "SELECT m.user_id, u.email, m.role, u.runner_of FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 ORDER BY u.email",
            &[id.into()],
        )?;
        let mut members = Vec::new();
        for r in rows {
            let keys = tx.query(
                "SELECT k.id, k.name, k.public_key, (SELECT COUNT(*) FROM wraps x WHERE x.workspace_id = $1 AND x.epoch = $2 AND x.key_id = k.id) FROM device_keys k WHERE k.user_id = $3 ORDER BY k.created_at, k.id",
                &[id.into(), ws.epoch.into(), r.text(0).into()],
            )?;
            members.push(json!({
                "userId": r.text(0),
                "email": r.text(1),
                "role": r.text(2),
                "runner": !r.text(3).is_empty(),
                "keys": keys.iter().map(|k| json!({ "id": k.text(0), "name": k.text(1), "publicKey": b64(&k.bytes(2)), "hasKey": k.int(3) > 0 })).collect::<Vec<_>>(),
            }));
        }
        Ok((200, json!({ "id": ws.id, "epoch": ws.epoch, "seq": ws.seq, "bytes": ws.bytes, "role": ws.role.as_str(), "members": members })))
    })
}

/// `DELETE /v1/workspaces/:ws` (owner): deletes the workspace and every blob in it.
pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        for table in ["blobs", "wraps", "invites", "members"] {
            tx.execute(&format!("DELETE FROM {table} WHERE workspace_id = $1"), &[id.into()])?;
        }
        tx.execute("DELETE FROM workspaces WHERE id = $1", &[id.into()])?;
        // Runner accounts exist for one workspace only.
        let runners = tx.execute("DELETE FROM users WHERE runner_of = $1", &[id.into()])?;
        // The workspace's log stays (until the retention purge) and ends with this row.
        audit::record(tx, state, Event::new(id, "workspace_deleted", Some(&user)).target(id).detail(json!({ "runnersRemoved": runners })))?;
        Ok((204, Json::Null))
    })
}

/// `GET /v1/workspaces/:ws/wraps?keyId=`: the caller's wrapped keys for every epoch.
pub fn get_wraps(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let key_id = query(request).get("keyId").cloned().unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        if tx.query_one("SELECT id FROM device_keys WHERE id = $1 AND user_id = $2", &[(&key_id).into(), (&user.id).into()])?.is_none() {
            return Err(ApiError::not_found("keyId is not one of your device keys"));
        }
        let rows = tx.query("SELECT epoch, wrapped FROM wraps WHERE workspace_id = $1 AND key_id = $2 ORDER BY epoch", &[id.into(), key_id.into()])?;
        Ok((200, json!({ "epoch": ws.epoch, "wraps": rows.iter().map(|r| json!({ "epoch": r.int(0), "wrapped": b64(&r.bytes(1)) })).collect::<Vec<_>>() })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddWraps {
    epoch: i64,
    wraps: Vec<WrapIn>,
}

/// `POST /v1/workspaces/:ws/wraps {"epoch","wraps"}`: wraps an existing epoch's key for member
/// devices that don't have it yet (a member's new device). Viewers may wrap for their own devices
/// only. Existing wraps are never replaced.
pub fn add_wraps(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let input: AddWraps = body(request)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        if input.epoch < 1 || input.epoch > ws.epoch {
            return Err(ApiError::bad("no such epoch"));
        }
        let mut accepted = Vec::new();
        for w in &input.wraps {
            let bytes = unb64("wrapped", &w.wrapped)?;
            if bytes.len() != WRAPPED_KEY_LEN {
                return Err(ApiError::bad(format!("wrapped key for {} must be {WRAPPED_KEY_LEN} bytes", w.key_id)));
            }
            let owner = tx.query_one(
                "SELECT k.user_id FROM device_keys k JOIN members m ON m.user_id = k.user_id AND m.workspace_id = $1 WHERE k.id = $2",
                &[id.into(), (&w.key_id).into()],
            )?;
            let Some(owner) = owner else {
                return Err(ApiError::bad(format!("{} is not a device key of a member", w.key_id)));
            };
            if ws.role == Role::Viewer && owner.text(0) != user.id {
                return Err(ApiError::forbidden("viewers can only add keys for their own devices"));
            }
            accepted.push((w.key_id.clone(), bytes));
        }
        insert_wraps(tx, id, input.epoch, &accepted)?;
        let keys: Vec<&str> = accepted.iter().map(|(k, _)| k.as_str()).collect();
        audit::record(tx, state, Event::new(id, "key_wrapped", Some(&user)).detail(json!({ "epoch": input.epoch, "keyIds": keys })))?;
        Ok((200, json!({ "epoch": input.epoch, "added": accepted.len() })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddMember {
    email: String,
    role: String,
    wraps: Vec<WrapIn>,
}

/// `POST /v1/workspaces/:ws/members {"email","role","wraps"}` (owner): adds an account that has
/// signed in and registered a device key; the current epoch's key must be wrapped for each of
/// its device keys.
pub fn add_member(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let input: AddMember = body(request)?;
    let role = Role::parse(&input.role)?;
    let email = input.email.trim().to_lowercase();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        let Some(row) = tx.query_one("SELECT id, runner_of FROM users WHERE email = $1", &[(&email).into()])? else {
            return Err(ApiError::not_found("no account with this address (they sign in once first)"));
        };
        if !row.text(1).is_empty() {
            return Err(ApiError::bad("runner credentials belong to one workspace (POST .../runners makes one)"));
        }
        let new_id = row.text(0);
        if tx.query_one("SELECT role FROM members WHERE workspace_id = $1 AND user_id = $2", &[id.into(), (&new_id).into()])?.is_some() {
            return Err(ApiError::conflict("already_member", "already a member", json!({})));
        }
        let wraps = exact_wraps(tx, std::slice::from_ref(&new_id), &input.wraps)?;
        if wraps.is_empty() {
            return Err(ApiError::bad("that account has no device key yet"));
        }
        tx.execute("INSERT INTO members (workspace_id, user_id, role) VALUES ($1, $2, $3)", &[id.into(), (&new_id).into(), role.as_str().into()])?;
        insert_wraps(tx, id, ws.epoch, &wraps)?;
        tx.execute("DELETE FROM invites WHERE workspace_id = $1 AND accepted_by = $2", &[id.into(), (&new_id).into()])?;
        audit::record(tx, state, Event::new(id, "member_added", Some(&user)).target(&new_id).detail(json!({ "email": email, "role": role.as_str(), "epoch": ws.epoch, "keys": wraps.len() })))?;
        Ok((201, json!({ "userId": new_id, "role": role.as_str(), "epoch": ws.epoch })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rotation {
    epoch: i64,
    wraps: Vec<WrapIn>,
}

fn rotate_to(tx: &mut Tx, ws: &Workspace, input: &Rotation) -> Result<(), ApiError> {
    if input.epoch != ws.epoch + 1 {
        return Err(ApiError::conflict("stale_epoch", format!("the next epoch is {}", ws.epoch + 1), json!({ "epoch": ws.epoch })));
    }
    let members = member_ids(tx, &ws.id)?;
    let wraps = exact_wraps(tx, &members, &input.wraps)?;
    insert_wraps(tx, &ws.id, input.epoch, &wraps)?;
    tx.execute("UPDATE workspaces SET epoch = $1 WHERE id = $2", &[input.epoch.into(), (&ws.id).into()])?;
    Ok(())
}

/// `DELETE /v1/workspaces/:ws/members/:user {"epoch","wraps"}` (owner): removes a member and
/// rotates the workspace key in the same transaction. `wraps` is the next epoch's new key for
/// every device of every *remaining* member. The removed member's wrapped keys are deleted.
pub fn remove_member(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let removed = params.get("user").unwrap_or_default().to_string();
    let input: Rotation = body(request).map_err(|_| ApiError::bad("removing a member needs the rotated key: {\"epoch\": current + 1, \"wraps\": [...]}"))?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        let Some(row) = tx.query_one("SELECT m.role, u.email, u.runner_of FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 AND m.user_id = $2", &[id.into(), (&removed).into()])? else {
            return Err(ApiError::not_found("not a member"));
        };
        if row.text(0) == "owner" {
            let owners = tx.query_one("SELECT COUNT(*) FROM members WHERE workspace_id = $1 AND role = 'owner'", &[id.into()])?.map_or(0, |r| r.int(0));
            if owners <= 1 {
                return Err(ApiError::bad("a workspace keeps at least one owner"));
            }
        }
        tx.execute("DELETE FROM members WHERE workspace_id = $1 AND user_id = $2", &[id.into(), (&removed).into()])?;
        tx.execute("DELETE FROM wraps WHERE workspace_id = $1 AND key_id IN (SELECT id FROM device_keys WHERE user_id = $2)", &[id.into(), (&removed).into()])?;
        rotate_to(tx, &ws, &input)?;
        // A removed runner credential stops existing (its sessions and key go with it).
        let runner = tx.execute("DELETE FROM users WHERE id = $1 AND runner_of = $2", &[(&removed).into(), id.into()])? > 0;
        audit::record(tx, state, Event::new(id, "member_removed", Some(&user)).target(&removed).detail(json!({ "email": row.text(1), "role": row.text(0), "runner": runner })))?;
        audit::record(tx, state, Event::new(id, "key_rotated", Some(&user)).detail(json!({ "epoch": input.epoch, "reason": "member_removed" })))?;
        if runner {
            audit::record(tx, state, Event::new(id, "session_revoked", Some(&user)).target(&removed).detail(json!({ "reason": "runner_removed" })))?;
        }
        Ok((200, json!({ "removed": removed, "epoch": input.epoch })))
    })
}

/// `POST /v1/workspaces/:ws/rotate {"epoch","wraps"}` (owner): a new key without a removal
/// (a lost device, routine rotation).
pub fn rotate(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let input: Rotation = body(request)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        rotate_to(tx, &ws, &input)?;
        audit::record(tx, state, Event::new(id, "key_rotated", Some(&user)).detail(json!({ "epoch": input.epoch, "reason": "rotate" })))?;
        Ok((200, json!({ "epoch": input.epoch })))
    })
}

pub fn require_role(ws: &Workspace, role: Role) -> Result<(), ApiError> {
    require(ws, role)
}
