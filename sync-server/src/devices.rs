//! DD3: a person's devices (their device keys): which ones there are, when each was last used,
//! renaming one, and removing one, also one that holds workspace keys.
//!
//! A client says which device it is with the `X-Hitop-Device: <key id>` header. On a session
//! request with it, the session is tied to that device (so removing the device ends its sessions)
//! and the device's `last_used_at` moves on, at most every 10 minutes. Clients that don't send it
//! (hitop 1.2 and older) work as before; their devices show no last use.
//!
//! Removing a device that holds workspace keys (`DELETE /v1/me/keys/:id?force=true`) does what the
//! org admin console's device removal does (admin.rs), for every workspace of the person: its
//! wraps go and a key rotation is requested so the other members' devices rotate the key it could
//! open; the key itself is deleted, so nothing can be wrapped for it again. The device's sessions
//! end. The device a request comes from can't remove itself (it signs out instead).

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{bearer, body, query, token_hash, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, User};
use crate::db::Tx;

pub const HEADER: &str = "x-hitop-device";
/// `last_used_at` moves at most this often.
const SEEN_EVERY: i64 = 600;

/// The device key id a request names, when it looks like one.
pub fn header_device(request: &Request) -> Option<String> {
    let v = request.header(HEADER)?.value.trim().to_string();
    (!v.is_empty() && v.len() <= 64 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).then_some(v)
}

/// A session request from device `device` (`X-Hitop-Device`): the session is tied to it and the
/// device marked used. `current` is the device the session had.
pub fn seen(state: &State, tx: &mut Tx, user_id: &str, token: &str, current: &str, device: &str) -> Result<(), ApiError> {
    let now = state.now();
    let mine = "SELECT id FROM device_keys WHERE id = $1 AND user_id = $2";
    if current != device {
        if tx.query_one(mine, &[device.into(), user_id.into()])?.is_none() {
            return Ok(());
        }
        tx.execute("UPDATE sessions SET device_key_id = $1 WHERE token_hash = $2", &[device.into(), token_hash(token).into()])?;
    }
    tx.execute(
        "UPDATE device_keys SET last_used_at = $1 WHERE id = $2 AND user_id = $3 AND last_used_at < $4",
        &[now.into(), device.into(), user_id.into(), (now - SEEN_EVERY).into()],
    )?;
    Ok(())
}

/// A person's own session (not an access token, not a runner).
fn person(state: &State, tx: &mut Tx, request: &Request) -> Result<User, ApiError> {
    if bearer(request).is_some_and(|t| t.starts_with(crate::tokens::PREFIX)) {
        return Err(ApiError::forbidden("devices are managed from a signed-in device, not with an access token"));
    }
    let user = authenticate(state, tx, request)?;
    user.person()?;
    Ok(user)
}

/// The workspaces device `key` holds a key of (a wrap at any epoch), and of those the ones where
/// no other device holds one at the current epoch (only this device can open them now).
fn held(tx: &mut Tx, key: &str) -> Result<(Vec<String>, Vec<String>), ApiError> {
    let held: Vec<String> = tx.query("SELECT DISTINCT workspace_id FROM wraps WHERE key_id = $1 ORDER BY workspace_id", &[key.into()])?.iter().map(|r| r.text(0)).collect();
    let mut only = vec![];
    for ws in &held {
        let others = tx
            .query_one(
                "SELECT COUNT(*) FROM wraps w JOIN workspaces s ON s.id = w.workspace_id WHERE w.workspace_id = $1 AND w.epoch = s.epoch AND w.key_id <> $2",
                &[ws.into(), key.into()],
            )?
            .map_or(0, |r| r.int(0));
        if others == 0 {
            only.push(ws.clone());
        }
    }
    Ok((held, only))
}

/// `GET /v1/me/devices` → `{devices: [{id, name, createdAt, lastUsedAt, workspaces, onlyHere,
/// thisDevice}], limit}`: oldest first; `lastUsedAt` null when never seen (older clients);
/// `workspaces` = how many shared workspaces' keys it holds, `onlyHere` = how many only it holds.
pub fn list(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let me = header_device(request);
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let rows = tx.query("SELECT id, name, created_at, last_used_at FROM device_keys WHERE user_id = $1 ORDER BY created_at, id", &[(&user.id).into()])?;
        let mut devices = vec![];
        for r in rows {
            let id = r.text(0);
            let (held, only) = held(tx, &id)?;
            let last = r.int(3);
            devices.push(json!({
                "id": id,
                "name": r.text(1),
                "createdAt": r.int(2),
                "lastUsedAt": if last > 0 { json!(last) } else { Json::Null },
                "workspaces": held.len(),
                "onlyHere": only.len(),
                "thisDevice": me.as_deref() == Some(id.as_str()),
            }));
        }
        let limit = if state.config.plan_limits { crate::limits::device_limit(tx, &user.id)? } else { None };
        Ok((200, json!({ "devices": devices, "limit": limit })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    name: String,
}

/// `PUT /v1/me/keys/:id {name}`: renames one of your devices.
pub fn rename(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let key = params.get("id").unwrap_or_default().to_string();
    let input: Rename = body(request)?;
    let name = input.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(ApiError::bad("a device name is 1 to 80 characters"));
    }
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        if tx.execute("UPDATE device_keys SET name = $1 WHERE id = $2 AND user_id = $3", &[(&name).into(), (&key).into(), (&user.id).into()])? == 0 {
            return Err(ApiError::not_found("no such device on your account"));
        }
        audit::record(tx, state, Event::new(audit::ACCOUNT, "device_renamed", Some(&user)).target(&user.id).detail(json!({ "keyId": key, "name": name })))?;
        Ok((200, json!({ "id": key, "name": name })))
    })
}

/// `DELETE /v1/me/keys/:id[?force=true]`: removes one of your devices. A device that holds
/// workspace keys needs `force` (409 `key_in_use` with `{wraps, workspaces, onlyHere}` without it);
/// then it is excluded from those workspaces and a key rotation is requested in each. Its sessions
/// end. The device the request comes from can't remove itself (409 `this_device`).
pub fn remove(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let key = params.get("id").unwrap_or_default().to_string();
    let force = query(request).get("force").is_some_and(|v| v == "true" || v == "1");
    let me = header_device(request);
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let Some(k) = tx.query_one("SELECT name FROM device_keys WHERE id = $1 AND user_id = $2", &[(&key).into(), (&user.id).into()])? else {
            return Err(ApiError::not_found("no such device key on your account"));
        };
        if me.as_deref() == Some(key.as_str()) {
            return Err(ApiError::conflict("this_device", "this is the device you are using: sign out here instead", json!({})));
        }
        let (held, only) = held(tx, &key)?;
        let wraps = tx.query_one("SELECT COUNT(*) FROM wraps WHERE key_id = $1", &[(&key).into()])?.map_or(0, |r| r.int(0));
        if wraps > 0 && !force {
            return Err(ApiError::conflict(
                "key_in_use",
                "this device key holds workspace keys; remove it with force=true (its workspaces then rotate their key)",
                json!({ "wraps": wraps, "workspaces": held.len(), "onlyHere": only.len() }),
            ));
        }
        // The key itself is deleted below, so no rotation can wrap for it again (the org admin
        // console keeps the key and excludes it instead). Workspaces where it held a key rotate.
        for ws in &held {
            tx.execute("DELETE FROM env_share_wraps WHERE key_id = $1 AND workspace_id = $2", &[(&key).into(), ws.into()])?;
            tx.execute("DELETE FROM wraps WHERE key_id = $1 AND workspace_id = $2", &[(&key).into(), ws.into()])?;
            crate::admin::request_rotation(tx, state, ws, &user, "device_removed")?;
            audit::record(tx, state, Event::new(ws, "device_removed", Some(&user)).target(&user.id).detail(json!({ "keyId": key, "email": user.email, "by": "owner" })))?;
        }
        let ended = tx.execute("DELETE FROM sessions WHERE user_id = $1 AND device_key_id = $2", &[(&user.id).into(), (&key).into()])?;
        tx.execute("DELETE FROM device_keys WHERE id = $1", &[(&key).into()])?;
        audit::record(
            tx,
            state,
            Event::new(audit::ACCOUNT, "device_key_removed", Some(&user)).target(&user.id).detail(json!({ "keyId": key, "name": k.text(0), "workspaces": held.len(), "sessionsEnded": ended })),
        )?;
        if held.is_empty() && ended == 0 {
            return Ok((204, json!(null)));
        }
        Ok((200, json!({ "removed": key, "rotationRequested": held, "onlyHere": only.len(), "sessionsEnded": ended })))
    })
}
