//! Team environments (HITOP Cloud, owner decision 2026-11-06): environments a team org's members
//! share, stored server-readable like the personal library. Secret *values* never come here: the
//! apps send an environment with its secret variables empty (their names in `secrets`), and each
//! member keeps their own values on their device.
//!
//!   GET    /v1/orgs/:org/environments              → {environments: [{id, version, updatedBy, updatedAt, data}]}
//!   PUT    /v1/orgs/:org/environments/:id          {data, baseVersion} → {id, version}; 409 with `current`
//!   DELETE /v1/orgs/:org/environments/:id?baseVersion=N
//!
//! Any member may read and write (a personal org has none: it has the personal library). Bytes
//! count against the org's storage.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{body, query, ApiError, ApiResult, State};
use crate::auth::{authenticate, User};
use crate::db::Tx;

const MAX_BYTES: usize = 256 * 1024;
const MAX_PER_ORG: i64 = 500;
const KIND: &str = "environment";

fn member(state: &State, tx: &mut Tx, request: &Request, org: &str) -> Result<User, ApiError> {
    let user = authenticate(state, tx, request)?;
    user.person()?;
    let o = crate::orgs::org_membership(tx, org, &user)?;
    if o.personal {
        return Err(ApiError::bad("a personal organization has no team environments: your own are in your library"));
    }
    Ok(user)
}

fn ids(params: &PathParams) -> Result<(String, String), ApiError> {
    let org = params.get("org").unwrap_or_default().to_string();
    let id = params.get("id").unwrap_or_default().to_string();
    if !id.is_empty() && (id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')) {
        return Err(ApiError::bad("an environment id is 1 to 64 letters, digits, - or _"));
    }
    Ok((org, id))
}

fn item(r: &crate::db::Row) -> Json {
    json!({ "id": r.text(0), "version": r.int(1), "updatedBy": r.text(2), "updatedAt": r.int(3), "data": serde_json::from_str::<Json>(&r.text(4)).unwrap_or(Json::Null) })
}

const COLS: &str = "i.item_id, i.version, COALESCE(u.email, ''), i.updated_at, i.data";

fn current(tx: &mut Tx, org: &str, id: &str) -> Result<Option<Json>, ApiError> {
    Ok(tx
        .query_one(&format!("SELECT {COLS} FROM org_items i LEFT JOIN users u ON u.id = i.updated_by WHERE i.org_id = $1 AND i.kind = $2 AND i.item_id = $3 AND i.deleted = 0"), &[org.into(), KIND.into(), id.into()])?
        .map(|r| item(&r)))
}

fn conflict(cur: Option<Json>) -> ApiError {
    ApiError::conflict("conflict", "someone in the organization changed this environment first; theirs is attached", json!({ "current": cur }))
}

pub fn list(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let (org, _) = ids(params)?;
    state.db.tx(|tx| {
        member(state, tx, request, &org)?;
        let rows = tx.query(&format!("SELECT {COLS} FROM org_items i LEFT JOIN users u ON u.id = i.updated_by WHERE i.org_id = $1 AND i.kind = $2 AND i.deleted = 0 ORDER BY i.created_at, i.item_id"), &[(&org).into(), KIND.into()])?;
        Ok((200, json!({ "environments": rows.iter().map(item).collect::<Vec<_>>() })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Put {
    data: Json,
    #[serde(default)]
    base_version: i64,
}

/// Secret values must not be here: every name in `secrets` has an empty value.
fn check(data: &Json) -> Result<(), ApiError> {
    if !data.is_object() || !data["name"].as_str().is_some_and(|n| !n.trim().is_empty()) {
        return Err(ApiError::bad("data is an environment: {name, variables, secrets}"));
    }
    let secrets: Vec<&str> = data["secrets"].as_array().into_iter().flatten().filter_map(Json::as_str).collect();
    if secrets.iter().any(|n| data["variables"][*n].as_str().is_some_and(|v| !v.is_empty())) {
        return Err(ApiError::bad("secret values stay on your devices: send secret variables empty"));
    }
    Ok(())
}

pub fn put(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let (org, id) = ids(params)?;
    let input: Put = body(request)?;
    check(&input.data)?;
    let mut data = input.data;
    if let Some(m) = data.as_object_mut() {
        m.insert("id".into(), json!(id));
        m.insert("org".into(), json!(org));
    }
    let text = data.to_string();
    if text.len() > MAX_BYTES {
        return Err(ApiError::new(413, "too_large", "a team environment is at most 256 KB"));
    }
    state.db.tx(|tx| {
        let user = member(state, tx, request, &org)?;
        let row = tx
            .query_one("SELECT version, bytes, deleted FROM org_items WHERE org_id = $1 AND kind = $2 AND item_id = $3", &[(&org).into(), KIND.into(), (&id).into()])?
            .map(|r| (r.int(0), r.int(1), r.int(2) != 0));
        // What the client must have seen: the live version, or 0 for a new or deleted one.
        let seen = match row {
            Some((v, _, false)) => v,
            _ => 0,
        };
        if input.base_version != seen {
            return Err(conflict(current(tx, &org, &id)?));
        }
        if !matches!(row, Some((_, _, false))) {
            let n = tx.query_one("SELECT COUNT(*) FROM org_items WHERE org_id = $1 AND kind = $2 AND deleted = 0", &[(&org).into(), KIND.into()])?.map_or(0, |r| r.int(0));
            if n >= MAX_PER_ORG {
                return Err(ApiError::forbidden(format!("an organization holds at most {MAX_PER_ORG} team environments")));
            }
        }
        let before = match row {
            Some((_, b, false)) => b,
            _ => 0,
        };
        let bytes = text.len() as i64;
        crate::limits::check_storage(state, tx, &org, bytes - before)?;
        let now = state.now();
        // Versions only grow, also across a delete, so an old copy never matches again.
        let version = row.map_or(0, |(v, _, _)| v) + 1;
        if row.is_some() {
            tx.execute(
                "UPDATE org_items SET version = $1, data = $2, bytes = $3, updated_by = $4, updated_at = $5, deleted = 0 WHERE org_id = $6 AND kind = $7 AND item_id = $8",
                &[version.into(), (&text).into(), bytes.into(), (&user.id).into(), now.into(), (&org).into(), KIND.into(), (&id).into()],
            )?;
        } else {
            tx.execute(
                "INSERT INTO org_items (org_id, kind, item_id, version, data, bytes, created_at, updated_by, updated_at, deleted) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 0)",
                &[(&org).into(), KIND.into(), (&id).into(), version.into(), (&text).into(), bytes.into(), now.into(), (&user.id).into(), now.into()],
            )?;
        }
        Ok((200, json!({ "id": id, "version": version })))
    })
}

pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let (org, id) = ids(params)?;
    let base: i64 = query(request).get("baseVersion").map_or(Ok(0), |s| s.parse()).map_err(|_| ApiError::bad("baseVersion must be a number"))?;
    state.db.tx(|tx| {
        let user = member(state, tx, request, &org)?;
        let Some(cur) = current(tx, &org, &id)? else { return Ok((200, json!({ "id": id, "deleted": true }))) };
        let version = cur["version"].as_i64().unwrap_or(0);
        if version != base {
            return Err(conflict(Some(cur)));
        }
        tx.execute(
            "UPDATE org_items SET version = $1, data = '{}', bytes = 0, updated_by = $2, updated_at = $3, deleted = 1 WHERE org_id = $4 AND kind = $5 AND item_id = $6",
            &[(version + 1).into(), (&user.id).into(), state.now().into(), (&org).into(), KIND.into(), (&id).into()],
        )?;
        Ok((200, json!({ "id": id, "deleted": true })))
    })
}
