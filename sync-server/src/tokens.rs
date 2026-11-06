//! Personal access tokens for the sync API (HITOP Cloud C1, made in the portal): a bearer token
//! like a session, for scripts and integrations, with scopes and an expiry, listed and revocable.
//!
//! A token is `hitop_pat_` + 32 random bytes (base64url); the database keeps only its SHA-256, so
//! a copy of the database doesn't sign anyone in. Scopes: `read` (GET requests only) or `read` and
//! `write` (everything the account may do). Tokens can't list, make or revoke tokens: that takes a
//! signed-in session. A token is no key: it can't decrypt anything (the device keys stay on the
//! devices). Creating and revoking one go to the account's audit log (`token_created`,
//! `token_revoked`).

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{bearer, body, new_token, random_hex, token_hash, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, User};
use crate::db::Tx;

pub const PREFIX: &str = "hitop_pat_";
const MAX_TOKENS: i64 = 50;
const MAX_DAYS: i64 = 365;

/// The account behind a personal access token, if its scopes allow this request.
pub fn authenticate_token(state: &State, tx: &mut Tx, request: &Request, token: &str) -> Result<User, ApiError> {
    authenticate_token_for(state, tx, token, request.method != "GET")
}

/// The account behind a personal access token that must have the `write` scope when `write` is
/// set (the hosted MCP endpoint: every call is a POST, so the tool decides).
pub fn authenticate_token_for(state: &State, tx: &mut Tx, token: &str, write: bool) -> Result<User, ApiError> {
    let now = state.now();
    let row = tx.query_one(
        "SELECT t.id, t.scopes, t.last_used_at, u.id, u.email, u.runner_of FROM api_tokens t JOIN users u ON u.id = t.user_id WHERE t.token_hash = $1 AND t.expires_at > $2",
        &[token_hash(token).into(), now.into()],
    )?;
    let row = row.ok_or_else(ApiError::unauthorized)?;
    let can_write = row.text(1).split(' ').any(|s| s == "write");
    if !can_write && write {
        return Err(ApiError::new(403, "token_scope", "this access token is read-only (scope read)"));
    }
    if row.int(2) < now - 60 {
        tx.execute("UPDATE api_tokens SET last_used_at = $1 WHERE id = $2", &[now.into(), row.text(0).into()])?;
    }
    Ok(User { id: row.text(3), email: row.text(4), runner_of: row.text(5) })
}

/// A signed-in session (not an access token): token management needs one.
fn session_user(state: &State, tx: &mut Tx, request: &Request) -> Result<User, ApiError> {
    if bearer(request).is_some_and(|t| t.starts_with(PREFIX)) {
        return Err(ApiError::forbidden("access tokens can't manage access tokens; sign in"));
    }
    let user = authenticate(state, tx, request)?;
    user.person()?;
    Ok(user)
}

/// `GET /v1/me/tokens`: the account's tokens (never the token itself).
pub fn list(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    state.db.tx(|tx| {
        let user = session_user(state, tx, request)?;
        let rows = tx.query("SELECT id, name, scopes, created_at, expires_at, last_used_at FROM api_tokens WHERE user_id = $1 ORDER BY created_at, id", &[(&user.id).into()])?;
        let now = state.now();
        let tokens: Vec<Json> = rows
            .iter()
            .map(|r| {
                json!({
                    "id": r.text(0),
                    "name": r.text(1),
                    "scopes": r.text(2).split(' ').collect::<Vec<_>>(),
                    "createdAt": r.int(3),
                    "expiresAt": r.int(4),
                    "lastUsedAt": if r.int(5) == 0 { Json::Null } else { r.int(5).into() },
                    "expired": r.int(4) <= now,
                })
            })
            .collect();
        Ok((200, json!({ "tokens": tokens })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NewToken {
    name: String,
    scopes: Vec<String>,
    #[serde(default = "default_days")]
    expires_in_days: i64,
}

fn default_days() -> i64 {
    90
}

/// `POST /v1/me/tokens {"name","scopes","expiresInDays"}` → `{id, token, ...}`: the token is
/// shown once.
pub fn create(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: NewToken = body(request)?;
    let name: String = input.name.trim().chars().filter(|c| !c.is_control()).take(64).collect();
    if name.is_empty() {
        return Err(ApiError::bad("a token needs a name"));
    }
    let mut scopes: Vec<&str> = Vec::new();
    for s in &input.scopes {
        match s.as_str() {
            "read" | "write" => {
                if !scopes.contains(&s.as_str()) {
                    scopes.push(s.as_str())
                }
            }
            _ => return Err(ApiError::bad("scopes are read and write")),
        }
    }
    if !scopes.contains(&"read") {
        return Err(ApiError::bad("every token has the read scope (add write to allow changes)"));
    }
    scopes.sort();
    let scopes = scopes.join(" ");
    if !(1..=MAX_DAYS).contains(&input.expires_in_days) {
        return Err(ApiError::bad(format!("expiresInDays is 1 to {MAX_DAYS}")));
    }
    state.db.tx(|tx| {
        let user = session_user(state, tx, request)?;
        let now = state.now();
        tx.execute("DELETE FROM api_tokens WHERE user_id = $1 AND expires_at <= $2", &[(&user.id).into(), (now - 30 * 86_400).into()])?;
        let count = tx.query_one("SELECT COUNT(*) FROM api_tokens WHERE user_id = $1", &[(&user.id).into()])?.map_or(0, |r| r.int(0));
        if count >= MAX_TOKENS {
            return Err(ApiError::forbidden(format!("an account has at most {MAX_TOKENS} access tokens; revoke some")));
        }
        let id = random_hex::<8>();
        let token = format!("{PREFIX}{}", new_token());
        let expires = now + input.expires_in_days * 86_400;
        tx.execute(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at, expires_at, last_used_at) VALUES ($1, $2, $3, $4, $5, $6, $7, 0)",
            &[(&id).into(), (&user.id).into(), (&name).into(), token_hash(&token).into(), (&scopes).into(), now.into(), expires.into()],
        )?;
        audit::record(tx, state, Event::new(audit::ACCOUNT, "token_created", Some(&user)).target(&user.id).detail(json!({ "tokenId": id, "name": name, "scopes": scopes, "expiresAt": expires })))?;
        Ok((201, json!({ "id": id, "token": token, "name": name, "scopes": scopes.split(' ').collect::<Vec<_>>(), "createdAt": now, "expiresAt": expires })))
    })
}

/// `DELETE /v1/me/tokens/:id`: revokes a token at once.
pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("id").unwrap_or_default();
    state.db.tx(|tx| {
        let user = session_user(state, tx, request)?;
        let Some(row) = tx.query_one("SELECT name FROM api_tokens WHERE id = $1 AND user_id = $2", &[id.into(), (&user.id).into()])? else {
            return Err(ApiError::not_found("no such token"));
        };
        tx.execute("DELETE FROM api_tokens WHERE id = $1", &[id.into()])?;
        audit::record(tx, state, Event::new(audit::ACCOUNT, "token_revoked", Some(&user)).target(&user.id).detail(json!({ "tokenId": id, "name": row.text(0) })))?;
        Ok((204, Json::Null))
    })
}
