//! Account recovery (HITOP Cloud C2, THREAT_MODEL.md "Recovery"): the server keeps opaque key
//! bundles per account and **unlock method**, sealed on a client: `phrase` (a key from the 24-word
//! recovery phrase the person wrote down) today; a password-derived key is the next method, stored
//! as its own row (`?method=`, [`METHODS`]), with no change to the format. It can't open it and never sees the phrase; it only stores, returns and deletes it,
//! and audits each of those. A device brought back from it may delete the key its sign-in on the
//! new computer registered, while that key holds no workspace key yet (`DELETE /v1/me/keys/:id`).
//!
//! Only a signed-in device's session may use these endpoints: not a runner credential, not an
//! access token (a leaked token must not be able to replace or delete the blob).

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::json;

use crate::api::{b64, bearer, body, unb64, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, User};
use crate::db::Tx;

/// The unlock methods a key bundle can be stored for (`?method=`, default `phrase`).
pub const METHODS: &[&str] = &["phrase"];

fn method(request: &Request) -> Result<String, ApiError> {
    let m = crate::api::query(request).get("method").cloned().unwrap_or_else(|| "phrase".into());
    if METHODS.contains(&m.as_str()) {
        Ok(m)
    } else {
        Err(ApiError::bad(format!("unknown unlock method {m:?} (known: {})", METHODS.join(", "))))
    }
}

/// The largest recovery blob (a device key, its pins and what it remembers per workspace).
pub const MAX_RECOVERY_BYTES: usize = 256 * 1024;

fn device_session(state: &State, tx: &mut Tx, request: &Request) -> Result<User, ApiError> {
    if bearer(request).is_some_and(|t| t.starts_with(crate::tokens::PREFIX)) {
        return Err(ApiError::forbidden("recovery needs a signed-in device, not an access token"));
    }
    let user = authenticate(state, tx, request)?;
    user.person()?;
    Ok(user)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PutRecovery {
    blob: String,
}

/// `PUT /v1/me/recovery {"blob"}`: stores (or replaces) the account's recovery blob.
pub fn put(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: PutRecovery = body(request)?;
    let blob = unb64("blob", &input.blob)?;
    let method = method(request)?;
    if blob.is_empty() || blob.len() > MAX_RECOVERY_BYTES {
        return Err(ApiError::new(413, "too_large", format!("a recovery blob has 1 to {MAX_RECOVERY_BYTES} bytes")));
    }
    state.db.tx(|tx| {
        let user = device_session(state, tx, request)?;
        let now = state.now();
        let replaced = tx.query_one("SELECT created_at FROM recovery_blobs WHERE user_id = $1 AND method = $2", &[(&user.id).into(), (&method).into()])?.is_some();
        tx.execute(
            "INSERT INTO recovery_blobs (user_id, method, blob, created_at, updated_at) VALUES ($1, $2, $3, $4, $4)
             ON CONFLICT (user_id, method) DO UPDATE SET blob = excluded.blob, updated_at = excluded.updated_at",
            &[(&user.id).into(), (&method).into(), blob.clone().into(), now.into()],
        )?;
        audit::record(tx, state, Event::new(audit::ACCOUNT, "recovery_set", Some(&user)).target(&user.id).detail(json!({ "method": method, "bytes": blob.len(), "replaced": replaced })))?;
        Ok((200, json!({ "updatedAt": now })))
    })
}

/// `GET /v1/me/recovery`: the blob (404 when there is none). Audited: a person sees in their
/// account log when it was fetched.
pub fn get(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let method = method(request)?;
    state.db.tx(|tx| {
        let user = device_session(state, tx, request)?;
        let Some(row) = tx.query_one("SELECT blob, created_at, updated_at FROM recovery_blobs WHERE user_id = $1 AND method = $2", &[(&user.id).into(), (&method).into()])? else {
            return Err(ApiError::not_found("this account has no recovery phrase set"));
        };
        audit::record(tx, state, Event::new(audit::ACCOUNT, "recovery_fetched", Some(&user)).target(&user.id).detail(json!({ "method": method })))?;
        Ok((200, json!({ "blob": b64(&row.bytes(0)), "createdAt": row.int(1), "updatedAt": row.int(2) })))
    })
}

/// `DELETE /v1/me/recovery`: forgets the blob (the phrase no longer recovers anything).
pub fn delete(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let method = method(request)?;
    state.db.tx(|tx| {
        let user = device_session(state, tx, request)?;
        let n = tx.execute("DELETE FROM recovery_blobs WHERE user_id = $1 AND method = $2", &[(&user.id).into(), (&method).into()])?;
        if n == 0 {
            return Err(ApiError::not_found("this account has no recovery phrase set"));
        }
        audit::record(tx, state, Event::new(audit::ACCOUNT, "recovery_removed", Some(&user)).target(&user.id).detail(json!({ "method": method })))?;
        Ok((204, json!(null)))
    })
}

/// `DELETE /v1/me/keys/:id`: removes one of the caller's own device keys that holds no workspace
/// key (one registered by signing in on a new computer before the old device was recovered there).
/// A key with wraps stays: removing it would take a member device out from under its team.
pub fn delete_key(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let key_id = params.get("id").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        let user = device_session(state, tx, request)?;
        if tx.query_one("SELECT id FROM device_keys WHERE id = $1 AND user_id = $2", &[(&key_id).into(), (&user.id).into()])?.is_none() {
            return Err(ApiError::not_found("no such device key on your account"));
        }
        let wraps = tx.query_one("SELECT COUNT(*) FROM wraps WHERE key_id = $1", &[(&key_id).into()])?.map_or(0, |r| r.int(0));
        if wraps > 0 {
            return Err(ApiError::conflict("key_in_use", "this device key holds workspace keys; it stays", json!({ "wraps": wraps })));
        }
        tx.execute("DELETE FROM device_keys WHERE id = $1", &[(&key_id).into()])?;
        audit::record(tx, state, Event::new(audit::ACCOUNT, "device_key_removed", Some(&user)).target(&user.id).detail(json!({ "keyId": key_id })))?;
        Ok((204, json!(null)))
    })
}
