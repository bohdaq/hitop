//! Shared environment values (HITOP Cloud C4, opt-in per environment): the secret values of an
//! environment, sealed end to end for the devices the sharer picks (hitop-sync-crypto
//! `seal_share`, `seal_share_wrap`). The server keeps the sealed share and one wrap of its key per
//! recipient device; it can read neither. Secret values still never go into the workspace's
//! files (format v1): a share is a row of its own, beside them.
//!
//! A share has a version; every write is the next version (`version` = current + 1) with a new key
//! and the full set of wraps, so the server can't hand out an older version as the current one
//! (clients also remember the highest version they opened). Members who can open a share add wraps
//! of the current version for devices that joined later (`POST .../wraps`, never replacing one).
//! Removing a member deletes their devices' wraps here; their clients' copies of values they could
//! open are theirs, which is why the remover's client also re-keys every share it can open.

use std::collections::BTreeSet;

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{b64, body, unb64, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, User};
use crate::db::Tx;
use crate::workspaces::{check_wrap, membership, require_role, Role, WrapIn};

/// The largest sealed share (an environment's secret values).
const MAX_SHARE_BYTES: usize = 64 * 1024;
const MAX_SHARES: i64 = 100;

fn valid_share_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The version a share wrap is for: its `head` field (bytes 1..5).
fn wrap_version(bytes: &[u8]) -> i64 {
    if bytes.len() < 5 {
        return -1;
    }
    i64::from(u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]))
}

/// Checks wraps for `version` from `caller`: shape, sender = one of the caller's keys, the version
/// they name, each for a device key of a member (returned with the member's user id).
fn checked_wraps(tx: &mut Tx, ws: &str, caller: &User, version: i64, wraps: &[WrapIn]) -> Result<Vec<(String, String, Vec<u8>)>, ApiError> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for w in wraps {
        let bytes = unb64("wrapped", &w.wrapped)?;
        if bytes.len() == crate::workspaces::WRAPPED_KEY_LEN {
            return Err(ApiError::bad("share wraps are authenticated (format 2 shape)"));
        }
        check_wrap(tx, caller, &w.key_id, &bytes)?;
        if wrap_version(&bytes) != version {
            return Err(ApiError::bad(format!("the wrap for {} is for version {}, not {version}", w.key_id, wrap_version(&bytes))));
        }
        let owner = tx.query_one(
            "SELECT k.user_id FROM device_keys k JOIN members m ON m.user_id = k.user_id AND m.workspace_id = $1 WHERE k.id = $2 AND k.id NOT IN (SELECT key_id FROM workspace_excluded_keys WHERE workspace_id = $1)",
            &[ws.into(), (&w.key_id).into()],
        )?;
        let Some(owner) = owner else {
            return Err(ApiError::bad(format!("{} is not a device key of a member", w.key_id)));
        };
        if !seen.insert(w.key_id.clone()) {
            return Err(ApiError::bad(format!("two wraps for {}", w.key_id)));
        }
        out.push((w.key_id.clone(), owner.text(0), bytes));
    }
    Ok(out)
}

/// `GET /v1/workspaces/:ws/env-shares` (members; runner credentials see only shares wrapped for
/// them): `{shares: [{id, version, sealed, updatedBy, updatedAt, wraps: [{keyId, userId,
/// wrapped}]}]}`, the wraps of the current version.
pub fn list(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        membership(tx, id, &user)?;
        let runner = !user.runner_of.is_empty();
        let mut shares = Vec::new();
        for s in tx.query("SELECT id, version, sealed, created_by, updated_by, updated_at FROM env_shares WHERE workspace_id = $1 ORDER BY id", &[id.into()])? {
            let wraps = tx.query(
                "SELECT key_id, user_id, wrapped FROM env_share_wraps WHERE workspace_id = $1 AND share_id = $2 AND version = $3 ORDER BY key_id",
                &[id.into(), s.text(0).into(), s.int(1).into()],
            )?;
            if runner && !wraps.iter().any(|w| w.text(1) == user.id) {
                continue;
            }
            let wraps: Vec<Json> = wraps.iter().map(|w| json!({ "keyId": w.text(0), "userId": w.text(1), "wrapped": b64(&w.bytes(2)) })).collect();
            shares.push(json!({ "id": s.text(0), "version": s.int(1), "sealed": b64(&s.bytes(2)), "createdBy": s.text(3), "updatedBy": s.text(4), "updatedAt": s.int(5), "wraps": wraps }));
        }
        Ok((200, json!({ "shares": shares })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Put {
    version: i64,
    sealed: String,
    wraps: Vec<WrapIn>,
}

/// `PUT /v1/workspaces/:ws/env-shares/:id {version, sealed, wraps}` (editor or owner): a new share
/// (version 1) or the next version of one (current + 1, else 409 `conflict` with the current
/// version), with the wraps of its new key for every recipient device. A new share needs a plan
/// with shared environment values (Pro and up; 402 `limit`).
pub fn put(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let share = params.get("id").unwrap_or_default();
    if !valid_share_id(share) {
        return Err(ApiError::bad("share ids are 32 lower-case hex digits (random, from the client)"));
    }
    let input: Put = body(request)?;
    let sealed = unb64("sealed", &input.sealed)?;
    if sealed.len() > MAX_SHARE_BYTES || sealed.len() < 1 + 4 + 24 + 16 || sealed[0] != 1 || wrap_version(&sealed) != input.version {
        return Err(ApiError::bad(format!("sealed must be a sealed share of this version, at most {MAX_SHARE_BYTES} bytes")));
    }
    if input.wraps.is_empty() {
        return Err(ApiError::bad("a share needs at least one wrap (your own devices)"));
    }
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let ws = membership(tx, id, &user)?;
        require_role(&ws, Role::Editor)?;
        let current = tx.query_one("SELECT version FROM env_shares WHERE workspace_id = $1 AND id = $2", &[id.into(), share.into()])?.map(|r| r.int(0));
        let expected = current.map_or(1, |v| v + 1);
        if input.version != expected {
            return Err(ApiError::conflict("conflict", format!("expected version {expected}"), json!({ "version": current })));
        }
        if current.is_none() {
            let org = crate::limits::org_of_workspace(tx, id)?;
            let (plan, ent) = crate::runs::run_plan(state, tx, &org)?;
            if let (false, Some(e)) = (plan.shared_secrets, &ent) {
                return Err(crate::runs::refused(state, tx, e, "sharedSecrets", "shared environment values", 0, 0));
            }
            let n = tx.query_one("SELECT COUNT(*) FROM env_shares WHERE workspace_id = $1", &[id.into()])?.map_or(0, |r| r.int(0));
            if n >= MAX_SHARES {
                return Err(ApiError::forbidden(format!("a workspace has at most {MAX_SHARES} shared environments")));
            }
        }
        let wraps = checked_wraps(tx, id, &user, input.version, &input.wraps)?;
        let now = state.now();
        tx.execute(
            "INSERT INTO env_shares (workspace_id, id, version, sealed, created_by, updated_by, updated_at) VALUES ($1, $2, $3, $4, $5, $5, $6)
             ON CONFLICT (workspace_id, id) DO UPDATE SET version = excluded.version, sealed = excluded.sealed, updated_by = excluded.updated_by, updated_at = excluded.updated_at",
            &[id.into(), share.into(), input.version.into(), sealed.into(), (&user.id).into(), now.into()],
        )?;
        tx.execute("DELETE FROM env_share_wraps WHERE workspace_id = $1 AND share_id = $2", &[id.into(), share.into()])?;
        for (key_id, owner, bytes) in &wraps {
            tx.execute(
                "INSERT INTO env_share_wraps (workspace_id, share_id, key_id, user_id, version, wrapped) VALUES ($1, $2, $3, $4, $5, $6)",
                &[id.into(), share.into(), key_id.into(), owner.into(), input.version.into(), bytes.clone().into()],
            )?;
        }
        let people: BTreeSet<&str> = wraps.iter().map(|(_, u, _)| u.as_str()).collect();
        audit::record(tx, state, Event::new(id, "env_share_updated", Some(&user)).target(share).detail(json!({ "version": input.version, "devices": wraps.len(), "accounts": people.len() })))?;
        Ok((200, json!({ "id": share, "version": input.version, "wraps": wraps.len() })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddWraps {
    version: i64,
    wraps: Vec<WrapIn>,
}

/// `POST /v1/workspaces/:ws/env-shares/:id/wraps {version, wraps}` (members who can open the share;
/// viewers for their own devices only): wraps of the current version's key for devices that have
/// none (a new member's, a member's new device). An existing wrap is never replaced.
pub fn add_wraps(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let share = params.get("id").unwrap_or_default();
    let input: AddWraps = body(request)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let ws = membership(tx, id, &user)?;
        let Some(current) = tx.query_one("SELECT version FROM env_shares WHERE workspace_id = $1 AND id = $2", &[id.into(), share.into()])?.map(|r| r.int(0)) else {
            return Err(ApiError::not_found("no such share"));
        };
        if input.version != current {
            return Err(ApiError::conflict("conflict", format!("the share is at version {current}"), json!({ "version": current })));
        }
        let wraps = checked_wraps(tx, id, &user, current, &input.wraps)?;
        if ws.role == Role::Viewer && wraps.iter().any(|(_, owner, _)| *owner != user.id) {
            return Err(ApiError::forbidden("viewers can only add wraps for their own devices"));
        }
        let mut stored = 0;
        for (key_id, owner, bytes) in &wraps {
            stored += tx.execute(
                "INSERT INTO env_share_wraps (workspace_id, share_id, key_id, user_id, version, wrapped) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (workspace_id, share_id, key_id) DO NOTHING",
                &[id.into(), share.into(), key_id.into(), owner.into(), current.into(), bytes.clone().into()],
            )?;
        }
        Ok((200, json!({ "version": current, "stored": stored })))
    })
}

/// `DELETE /v1/workspaces/:ws/env-shares/:id` (editor or owner): stops sharing (the share and its
/// wraps are deleted; values members already opened stay on their devices).
pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let share = params.get("id").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let ws = membership(tx, id, &user)?;
        require_role(&ws, Role::Editor)?;
        tx.execute("DELETE FROM env_share_wraps WHERE workspace_id = $1 AND share_id = $2", &[id.into(), share.into()])?;
        if tx.execute("DELETE FROM env_shares WHERE workspace_id = $1 AND id = $2", &[id.into(), share.into()])? == 0 {
            return Err(ApiError::not_found("no such share"));
        }
        audit::record(tx, state, Event::new(id, "env_share_deleted", Some(&user)).target(share))?;
        Ok((204, Json::Null))
    })
}
