//! The personal library (HITOP Cloud C8, spec/CLOUD_SERVICE.md "Personal library"): a signed-in
//! person's own collections, environments and globals, stored on the server so every browser they
//! sign in to has them.
//!
//! **The server can read the library** (owner decision 2026-11-05). This is not a workspace and not
//! end to end encrypted: shared workspaces stay end to end as before (THREAT_MODEL.md). Secret
//! values are the exception inside the library: the client sends them apart from the item, and the
//! server seals them at rest under the host key (hostkey.rs, label
//! `library-secrets/<user>/<kind>/<id>`) and returns them only to their owner. Without a host key
//! the server doesn't store them (`secretsStored: false`) and they stay in the browser.
//!
//! An item is `(kind, id)`: `collection` (one per collection), `environments` (id `all`: the list
//! without secret values, and the active id) or `globals` (id `all`). Each write bumps the item's
//! `version` (the client's `baseVersion` must match: 409 `conflict` with the server's item
//! otherwise) and the account's `seq`, so `GET /v1/library?since=<seq>` returns only what changed.
//! A delete keeps a tombstone (`deleted: true`) for the other browsers.
//!
//! Bytes (data + sealed secrets) count against the item's org's `storageBytes` (limits.rs
//! `stored_bytes`): a collection whose data names an `org` the person belongs to is filed there
//! (the app's org switcher), anything else under the personal org. The library is never a
//! workspace. Runner credentials can't use it.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Map, Value as Json};

use crate::api::{body, query, ApiError, ApiResult, State};
use crate::auth::{authenticate, User};
use crate::db::{Row, Tx};

/// The largest item (its JSON data plus its secret values).
pub const MAX_ITEM_BYTES: usize = 5 << 20;
/// Items one account may hold (tombstones included).
const MAX_ITEMS: i64 = 5000;
const MAX_ID: usize = 64;
const KINDS: [&str; 3] = ["collection", "environments", "globals"];

fn person(state: &State, tx: &mut Tx, request: &Request) -> Result<User, ApiError> {
    let user = authenticate(state, tx, request)?;
    user.person()?;
    Ok(user)
}

fn item_ref(params: &PathParams) -> Result<(String, String), ApiError> {
    let kind = params.get("kind").unwrap_or_default().to_string();
    let id = params.get("id").unwrap_or_default().to_string();
    if !KINDS.contains(&kind.as_str()) {
        return Err(ApiError::bad("kind is collection, environments or globals"));
    }
    if id.is_empty() || id.len() > MAX_ID || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err(ApiError::bad(format!("an item id is 1 to {MAX_ID} letters, digits, - _ or .")));
    }
    if kind != "collection" && id != "all" {
        return Err(ApiError::bad(format!("{kind} has one item, id all")));
    }
    Ok((kind, id))
}

fn label(user: &str, kind: &str, id: &str) -> String {
    format!("library-secrets/{user}/{kind}/{id}")
}

/// An item as the API returns it; secret values opened for their owner when the host key is set.
fn item_json(state: &State, user: &str, r: &Row) -> Json {
    // Columns: kind, item_id, version, seq, data, secrets, updated_at, deleted.
    let (kind, id) = (r.text(0), r.text(1));
    let deleted = r.int(7) != 0;
    let mut out = json!({ "kind": kind, "id": id, "version": r.int(2), "seq": r.int(3), "updatedAt": r.int(6), "deleted": deleted });
    if !deleted {
        out["data"] = serde_json::from_str(&r.text(4)).unwrap_or(Json::Null);
        let sealed = r.bytes(5);
        if !sealed.is_empty() {
            if let Some(key) = &state.config.host_key {
                if let Ok(plain) = key.open(&label(user, &kind, &id), &sealed) {
                    out["secrets"] = serde_json::from_slice(&plain).unwrap_or(Json::Null);
                }
            }
        }
    }
    out
}

const COLUMNS: &str = "kind, item_id, version, seq, data, secrets, updated_at, deleted";

/// `GET /v1/library?since=<seq>`: the account's items changed after `seq` (every item, tombstones
/// left out, without it), the cursor to pass next time, and whether secret values are stored.
pub fn list(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let q = query(request);
    let since: Option<i64> = q.get("since").map(|s| s.parse().map_err(|_| ApiError::bad("since must be a number"))).transpose()?;
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let rows = match since {
            Some(since) => tx.query(&format!("SELECT {COLUMNS} FROM library_items WHERE user_id = $1 AND seq > $2 ORDER BY seq"), &[(&user.id).into(), since.into()])?,
            None => tx.query(&format!("SELECT {COLUMNS} FROM library_items WHERE user_id = $1 AND deleted = 0 ORDER BY seq"), &[(&user.id).into()])?,
        };
        let cursor = tx.query_one("SELECT COALESCE(MAX(seq), 0) FROM library_items WHERE user_id = $1", &[(&user.id).into()])?.map_or(0, |r| r.int(0));
        let items: Vec<Json> = rows.iter().map(|r| item_json(state, &user.id, r)).collect();
        let git = crate::gitsync::states(state, tx, &user.id)?;
        Ok((200, json!({ "items": items, "cursor": cursor, "secretsStored": state.config.host_key.is_some(), "git": git })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Put {
    data: Json,
    #[serde(default)]
    secrets: Option<Map<String, Json>>,
    #[serde(default)]
    base_version: i64,
}

/// The item now: `(version, bytes, deleted)`, or None.
fn current(tx: &mut Tx, user: &str, kind: &str, id: &str) -> Result<Option<(i64, i64, bool)>, ApiError> {
    Ok(tx
        .query_one("SELECT version, bytes, deleted FROM library_items WHERE user_id = $1 AND kind = $2 AND item_id = $3", &[user.into(), kind.into(), id.into()])?
        .map(|r| (r.int(0), r.int(1), r.int(2) != 0)))
}

fn conflict(state: &State, tx: &mut Tx, user: &str, kind: &str, id: &str) -> Result<ApiError, ApiError> {
    let row = tx.query_one(&format!("SELECT {COLUMNS} FROM library_items WHERE user_id = $1 AND kind = $2 AND item_id = $3"), &[user.into(), kind.into(), id.into()])?;
    let item = row.map_or(Json::Null, |r| item_json(state, user, &r));
    Ok(ApiError::conflict("conflict", "this item changed in another browser; the server's version is attached", json!({ "current": item })))
}

fn next_seq(tx: &mut Tx, user: &str) -> Result<i64, ApiError> {
    Ok(tx.query_one("SELECT COALESCE(MAX(seq), 0) + 1 FROM library_items WHERE user_id = $1", &[user.into()])?.map_or(1, |r| r.int(0)))
}

/// `PUT /v1/library/:kind/:id {data, secrets?, baseVersion}` → `{version, seq, secretsStored}`.
/// `baseVersion` is the version the client last saw (0 for a new item or one it saw deleted).
pub fn put(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let (kind, id) = item_ref(params)?;
    let input: Put = body(request)?;
    if !input.data.is_object() {
        return Err(ApiError::bad("data is a JSON object"));
    }
    let data = input.data.to_string();
    let secrets = input.secrets.filter(|s| !s.is_empty()).map(|s| Json::Object(s).to_string());
    if data.len() + secrets.as_ref().map_or(0, String::len) > MAX_ITEM_BYTES {
        return Err(ApiError::new(413, "too_large", format!("a library item is at most {} MB", MAX_ITEM_BYTES >> 20)));
    }
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let personal = crate::orgs::ensure_personal(state, tx, &user)?;
        // A collection names its org in its data (the app's org switcher); 404 if not a member.
        let org = match input.data.get("org").and_then(Json::as_str).map(str::trim).filter(|o| !o.is_empty()) {
            Some(o) if kind == "collection" && o != personal => crate::orgs::org_membership(tx, o, &user)?.id,
            _ => personal,
        };
        let old_org = tx
            .query_one("SELECT org_id FROM library_items WHERE user_id = $1 AND kind = $2 AND item_id = $3", &[(&user.id).into(), (&kind).into(), (&id).into()])?
            .map(|r| r.text(0));
        let now_item = current(tx, &user.id, &kind, &id)?;
        let seen = match now_item {
            Some((version, _, false)) => version,
            Some((_, _, true)) | None => 0,
        };
        if input.base_version != seen {
            return Err(conflict(state, tx, &user.id, &kind, &id)?);
        }
        if now_item.is_none() {
            let count = tx.query_one("SELECT COUNT(*) FROM library_items WHERE user_id = $1", &[(&user.id).into()])?.map_or(0, |r| r.int(0));
            if count >= MAX_ITEMS {
                return Err(ApiError::forbidden(format!("a library holds at most {MAX_ITEMS} items")));
            }
        }
        let sealed = match (&secrets, &state.config.host_key) {
            (Some(s), Some(key)) => key.seal(&label(&user.id, &kind, &id), s.as_bytes()),
            _ => Vec::new(),
        };
        let bytes = (data.len() + sealed.len()) as i64;
        // Moved to another org: all of it is new there.
        let before = match now_item {
            Some((_, b, false)) if old_org.as_deref() == Some(org.as_str()) => b,
            _ => 0,
        };
        crate::limits::check_storage(state, tx, &org, bytes - before)?;
        let version = now_item.map_or(0, |(v, _, _)| v) + 1;
        let seq = next_seq(tx, &user.id)?;
        let now = state.now();
        if now_item.is_some() {
            tx.execute(
                "UPDATE library_items SET version = $1, seq = $2, data = $3, secrets = $4, bytes = $5, updated_at = $6, deleted = 0, org_id = $7 WHERE user_id = $8 AND kind = $9 AND item_id = $10",
                &[version.into(), seq.into(), (&data).into(), sealed.as_slice().into(), bytes.into(), now.into(), (&org).into(), (&user.id).into(), (&kind).into(), (&id).into()],
            )?;
        } else {
            tx.execute(
                "INSERT INTO library_items (user_id, org_id, kind, item_id, version, seq, data, secrets, bytes, updated_at, deleted) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 0)",
                &[(&user.id).into(), (&org).into(), (&kind).into(), (&id).into(), version.into(), seq.into(), (&data).into(), sealed.as_slice().into(), bytes.into(), now.into()],
            )?;
        }
        crate::gitsync::touched(state, tx, &user.id, &kind, &id)?;
        Ok((200, json!({ "kind": kind, "id": id, "version": version, "seq": seq, "updatedAt": now, "secretsStored": secrets.is_none() || !sealed.is_empty() })))
    })
}

/// `DELETE /v1/library/:kind/:id?baseVersion=<version>` → `{version, seq}`: a tombstone. A delete of
/// an item that is already gone answers 200 (nothing to do).
pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let (kind, id) = item_ref(params)?;
    let q = query(request);
    let base: i64 = q.get("baseVersion").map_or(Ok(0), |s| s.parse()).map_err(|_| ApiError::bad("baseVersion must be a number"))?;
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        match current(tx, &user.id, &kind, &id)? {
            None | Some((_, _, true)) => Ok((200, json!({ "kind": kind, "id": id, "deleted": true }))),
            Some((version, _, false)) if version != base => Err(conflict(state, tx, &user.id, &kind, &id)?),
            Some((version, _, false)) => {
                let seq = next_seq(tx, &user.id)?;
                let now = state.now();
                tx.execute(
                    "UPDATE library_items SET version = $1, seq = $2, data = '', secrets = $3, bytes = 0, updated_at = $4, deleted = 1 WHERE user_id = $5 AND kind = $6 AND item_id = $7",
                    &[(version + 1).into(), seq.into(), (&[] as &[u8]).into(), now.into(), (&user.id).into(), (&kind).into(), (&id).into()],
                )?;
                crate::gitsync::deleted(tx, &user.id, &kind, &id)?;
                Ok((200, json!({ "kind": kind, "id": id, "version": version + 1, "seq": seq, "deleted": true })))
            }
        }
    })
}

/// A collection of `user`'s library: its data and version (None: missing or deleted). Git sync.
pub(crate) fn collection_item(tx: &mut Tx, user: &str, id: &str) -> Result<Option<(Json, i64)>, ApiError> {
    let row = tx.query_one("SELECT data, version FROM library_items WHERE user_id = $1 AND kind = 'collection' AND item_id = $2 AND deleted = 0", &[user.into(), id.into()])?;
    Ok(row.map(|r| (serde_json::from_str(&r.text(0)).unwrap_or(Json::Null), r.int(1))))
}

/// The server writes a collection item (Git sync brought GitHub's changes): only while it is still at
/// version `expect` (false otherwise: a save came in first). Not a save: it doesn't make a push due.
pub(crate) fn store_from_server(state: &State, tx: &mut Tx, user: &str, id: &str, data: &Json, expect: i64) -> Result<bool, ApiError> {
    let Some((version, before, false)) = current(tx, user, "collection", id)? else { return Ok(false) };
    if version != expect {
        return Ok(false);
    }
    let text = data.to_string();
    if text.len() > MAX_ITEM_BYTES {
        return Err(ApiError::new(413, "too_large", format!("the collection on GitHub is over {} MB", MAX_ITEM_BYTES >> 20)));
    }
    let org = tx
        .query_one("SELECT org_id FROM library_items WHERE user_id = $1 AND kind = 'collection' AND item_id = $2", &[user.into(), id.into()])?
        .map(|r| r.text(0))
        .filter(|o| !o.is_empty())
        .unwrap_or_else(|| crate::orgs::personal_id(user));
    let sealed_len = tx.query_one("SELECT secrets FROM library_items WHERE user_id = $1 AND kind = 'collection' AND item_id = $2", &[user.into(), id.into()])?.map_or(0, |r| r.bytes(0).len());
    let bytes = (text.len() + sealed_len) as i64;
    crate::limits::check_storage(state, tx, &org, bytes - before)?;
    let seq = next_seq(tx, user)?;
    tx.execute(
        "UPDATE library_items SET version = $1, seq = $2, data = $3, bytes = $4, updated_at = $5 WHERE user_id = $6 AND kind = 'collection' AND item_id = $7",
        &[(version + 1).into(), seq.into(), (&text).into(), bytes.into(), state.now().into(), user.into(), id.into()],
    )?;
    Ok(true)
}
