//! Blobs: opaque encrypted envelopes with a version and a version vector.
//!
//! A write names the version it creates (current + 1, or 1 for a new blob) and a version vector
//! that must descend from the stored one (every counter at least as high, one strictly higher).
//! Anything else is a conflict (409 with the current metadata): the client fetches, merges,
//! re-encrypts for the next version and tries again. The version is in the blob's associated
//! data, so the server can't serve an old envelope as a newer version. Writes must use the
//! workspace's current key epoch. Deletes are tombstones (`"deleted": true`, no content) so other
//! clients see them in the change feed.

use std::collections::BTreeMap;

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::json;

use crate::api::{b64, body, query, unb64, ApiError, ApiResult, State};
use crate::auth::authenticate;
use crate::workspaces::{membership, require_role, Role};

/// Format byte + epoch (4) + nonce (24) + tag (16): the smallest envelope.
const MIN_ENVELOPE: usize = 1 + 4 + 24 + 16;
const MAX_VV_ENTRIES: usize = 256;
const PAGE: i64 = 500;

pub type VersionVector = BTreeMap<String, u64>;

/// `a` descends from `b`: no counter lower, and the two differ.
pub fn descends(a: &VersionVector, b: &VersionVector) -> bool {
    b.iter().all(|(k, v)| a.get(k).copied().unwrap_or(0) >= *v) && a.iter().any(|(k, v)| *v > b.get(k).copied().unwrap_or(0))
}

fn valid_blob_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn valid_vv(vv: &VersionVector) -> Result<(), ApiError> {
    if vv.is_empty() || vv.len() > MAX_VV_ENTRIES {
        return Err(ApiError::bad(format!("vv needs 1 to {MAX_VV_ENTRIES} entries")));
    }
    for (device, counter) in vv {
        if device.is_empty() || device.len() > 64 || !device.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)) {
            return Err(ApiError::bad("vv keys are device ids: 1 to 64 letters, digits, - or _"));
        }
        if *counter == 0 || *counter > i64::MAX as u64 {
            return Err(ApiError::bad("vv counters must be positive"));
        }
    }
    Ok(())
}

struct Current {
    version: i64,
    vv: VersionVector,
    epoch: i64,
    deleted: bool,
    seq: i64,
    size: i64,
}

impl Current {
    fn json(&self) -> serde_json::Value {
        json!({ "version": self.version, "vv": self.vv, "epoch": self.epoch, "deleted": self.deleted, "seq": self.seq })
    }
}

/// `GET /v1/workspaces/:ws/changes?since=<seq>`: metadata of blobs changed after `since`, oldest
/// first, up to 500 (`more` says there are others).
pub fn changes(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let since: i64 = query(request).get("since").map_or(Ok(0), |s| s.parse()).map_err(|_| ApiError::bad("since must be a number"))?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        let rows = tx.query(
            "SELECT id, version, vv, epoch, deleted, seq, LENGTH(content) FROM blobs WHERE workspace_id = $1 AND seq > $2 ORDER BY seq LIMIT $3",
            &[id.into(), since.into(), (PAGE + 1).into()],
        )?;
        let more = rows.len() as i64 > PAGE;
        let changes: Vec<_> = rows
            .iter()
            .take(PAGE as usize)
            .map(|r| {
                let vv: VersionVector = serde_json::from_str(&r.text(2)).unwrap_or_default();
                json!({ "id": r.text(0), "version": r.int(1), "vv": vv, "epoch": r.int(3), "deleted": r.int(4) != 0, "seq": r.int(5), "size": r.int(6) })
            })
            .collect();
        Ok((200, json!({ "seq": ws.seq, "epoch": ws.epoch, "changes": changes, "more": more })))
    })
}

/// `GET /v1/workspaces/:ws/blobs/:id`: the envelope (base64url) and its metadata.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let blob = params.get("id").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        membership(tx, id, &user)?;
        let row = tx.query_one("SELECT version, vv, epoch, deleted, seq, content FROM blobs WHERE workspace_id = $1 AND id = $2", &[id.into(), blob.into()])?;
        let row = row.ok_or_else(|| ApiError::not_found("no such blob"))?;
        let vv: VersionVector = serde_json::from_str(&row.text(1)).unwrap_or_default();
        Ok((
            200,
            json!({ "id": blob, "version": row.int(0), "vv": vv, "epoch": row.int(2), "deleted": row.int(3) != 0, "seq": row.int(4), "content": b64(&row.bytes(5)) }),
        ))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Put {
    version: i64,
    vv: VersionVector,
    epoch: i64,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    deleted: bool,
}

/// `PUT /v1/workspaces/:ws/blobs/:id {"version","vv","epoch","content"|"deleted"}` (owner or
/// editor) → `{"version","seq"}`, or 409 `conflict` / `stale_epoch` with the current state.
pub fn put(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let blob = params.get("id").unwrap_or_default();
    if !valid_blob_id(blob) {
        return Err(ApiError::bad("blob ids are 1 to 128 letters, digits, '.', '-' or '_' (use random ids: the server sees them)"));
    }
    let input: Put = body(request)?;
    valid_vv(&input.vv)?;
    let content = match (&input.content, input.deleted) {
        (Some(_), true) => return Err(ApiError::bad("a deleted blob has no content")),
        (None, false) => return Err(ApiError::bad("content is required (or \"deleted\": true)")),
        (None, true) => Vec::new(),
        (Some(c), false) => {
            let bytes = unb64("content", c)?;
            if bytes.len() > state.config.max_blob_bytes {
                return Err(ApiError::new(413, "too_large", format!("blobs are at most {} bytes", state.config.max_blob_bytes)));
            }
            // A format check, not cryptography: the envelope header must name the declared epoch.
            if bytes.len() < MIN_ENVELOPE || bytes[0] != 1 || i64::from(u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]])) != input.epoch {
                return Err(ApiError::bad("content is not a format 1 blob envelope for this epoch"));
            }
            bytes
        }
    };
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require_role(&ws, Role::Editor)?;
        let current = tx
            .query_one("SELECT version, vv, epoch, deleted, seq, LENGTH(content) FROM blobs WHERE workspace_id = $1 AND id = $2", &[id.into(), blob.into()])?
            .map(|r| Current { version: r.int(0), vv: serde_json::from_str(&r.text(1)).unwrap_or_default(), epoch: r.int(2), deleted: r.int(3) != 0, seq: r.int(4), size: r.int(5) });
        if input.epoch != ws.epoch {
            return Err(ApiError::conflict("stale_epoch", format!("write with the current key epoch {}", ws.epoch), json!({ "epoch": ws.epoch })));
        }
        let (expected, old_size) = current.as_ref().map_or((1, 0), |c| (c.version + 1, c.size));
        let base_vv = current.as_ref().map(|c| c.vv.clone()).unwrap_or_default();
        if input.version != expected || !descends(&input.vv, &base_vv) {
            let message = if input.version != expected { format!("expected version {expected}") } else { "the version vector doesn't descend from the stored one".to_string() };
            return Err(ApiError::conflict("conflict", message, json!({ "current": current.as_ref().map(Current::json) })));
        }
        let bytes = ws.bytes - old_size + content.len() as i64;
        if bytes > state.config.max_workspace_bytes {
            return Err(ApiError::new(413, "quota", format!("the workspace would exceed {} bytes", state.config.max_workspace_bytes)));
        }
        let seq = ws.seq + 1;
        let vv = serde_json::to_string(&input.vv).unwrap_or_default();
        tx.execute(
            "INSERT INTO blobs (workspace_id, id, version, vv, epoch, deleted, content, seq, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (workspace_id, id) DO UPDATE SET version = excluded.version, vv = excluded.vv, epoch = excluded.epoch, deleted = excluded.deleted, content = excluded.content, seq = excluded.seq, updated_at = excluded.updated_at",
            &[id.into(), blob.into(), input.version.into(), vv.into(), input.epoch.into(), input.deleted.into(), content.into(), seq.into(), state.now().into()],
        )?;
        tx.execute("UPDATE workspaces SET seq = $1, bytes = $2 WHERE id = $3", &[seq.into(), bytes.into(), id.into()])?;
        Ok((200, json!({ "version": input.version, "seq": seq })))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vv(pairs: &[(&str, u64)]) -> VersionVector {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn version_vector_descent() {
        assert!(descends(&vv(&[("a", 1)]), &vv(&[])));
        assert!(descends(&vv(&[("a", 2)]), &vv(&[("a", 1)])));
        assert!(descends(&vv(&[("a", 1), ("b", 1)]), &vv(&[("a", 1)])));
        // Equal, older, and concurrent vectors don't descend.
        assert!(!descends(&vv(&[("a", 1)]), &vv(&[("a", 1)])));
        assert!(!descends(&vv(&[("a", 1)]), &vv(&[("a", 2)])));
        assert!(!descends(&vv(&[("a", 2)]), &vv(&[("a", 1), ("b", 1)])));
        // A merge of two concurrent vectors descends from both.
        let merged = vv(&[("a", 2), ("b", 2)]);
        assert!(descends(&merged, &vv(&[("a", 2), ("b", 1)])) && descends(&merged, &vv(&[("a", 1), ("b", 2)])));
    }
}
