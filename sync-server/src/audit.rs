//! The audit log: an append-only record of security-relevant events (sign-in and sign-out,
//! failed sign-ins, SSO links and configuration errors, workspace create and delete, members,
//! roles, invites, key rotations and wraps, runner credentials). Never blob contents, keys or
//! tokens: a row holds the event name, who did it, what it was done to, a small JSON detail, the
//! time, a truncated client IP and the user agent.
//!
//! Rows form one hash chain per workspace (account events, which belong to no workspace, form the
//! chain with the empty workspace id). Each row's hash covers the previous row's hash, so editing
//! or deleting a row breaks every hash after it; `audit_chain` keeps the chain's head (catches a
//! deleted last row) and the anchor left by the retention purge (the last purged row), so the
//! chain still verifies after old rows are purged. The hashed bytes (THREAT_MODEL.md, "Audit
//! log"), so a client can recompute the chain itself:
//!
//! ```text
//! hash = hex(SHA-256("hitop-sync/v1/audit" || 0x00 || for each field: u32 big-endian length || UTF-8 bytes))
//! fields = prev_hash, workspace_id, seq, at, event, actor_id, actor_email, target, detail, ip, user_agent
//! ```
//!
//! (`seq` and `at` in decimal; the first row's `prev_hash` is 64 zeros.) This is tamper-evidence,
//! not tamper-proofing: someone who can write the database can rewrite a whole chain and its head.
//! Clients that remember a head they saw (`hitop sync audit --verify`) catch that too.

use std::net::IpAddr;

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};

use crate::api::{client, query, ApiError, ApiResult, State};
use crate::auth::{authenticate, User};
use crate::db::{DbError, Row, Tx};
use crate::workspaces::{membership, require, Role};

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";
/// The chain of events that belong to no workspace (sign-ins, account keys).
pub const ACCOUNT: &str = "";
const MAX_PAGE: i64 = 500;

/// Every event name the server writes. Clients may show unknown names as they are.
pub const EVENTS: &[&str] = &[
    // account chain
    "signin",
    "signin_failed",
    "signout",
    "passkey_added",
    "device_key_added",
    "sso_linked",
    "sso_config_error",
    // workspace chains
    "workspace_created",
    "workspace_deleted",
    "member_added",
    "member_removed",
    "role_changed",
    "key_rotated",
    "key_wrapped",
    "invite_created",
    "invite_deleted",
    "invite_accepted",
    "runner_created",
    "session_revoked",
];

/// HITOP Cloud events (orgs.rs, limits.rs, tokens.rs): `token_created` and `token_revoked` on the
/// account chain, `workspace_moved` on the workspace's chain and the org chains, the rest on the
/// chain of the org (`org:<id>`).
pub const CLOUD_EVENTS: &[&str] = &[
    "org_created",
    "org_renamed",
    "org_deleted",
    "org_member_added",
    "org_member_removed",
    "org_role_changed",
    "org_invite_created",
    "org_invite_deleted",
    "org_invite_accepted",
    "workspace_moved",
    "plan_changed",
    "token_created",
    "token_revoked",
];

/// Who called: the client IP (truncated, or empty when the server is set not to keep it) and
/// the user agent (at most 200 characters).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Client {
    pub ip: String,
    pub user_agent: String,
}

/// How much of the client IP the log keeps (`HITOP_SYNC_AUDIT_IP`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpPolicy {
    /// IPv4 to its /24 (`203.0.113.0`), IPv6 to its /48 (`2001:db8:1::`). The default.
    Truncated,
    /// No IP at all.
    Off,
}

pub fn truncate_ip(ip: IpAddr) -> String {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    };
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("{}.{}.{}.0", o[0], o[1], o[2])
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}::", s[0], s[1], s[2])
        }
    }
}

/// The client's full IP (rate limits key on it; the log keeps only `client_of`'s): the TCP peer,
/// or the last `X-Forwarded-For` entry when the peer is a trusted proxy.
pub fn real_ip(request: &Request, peer: &str, trusted: &[String]) -> String {
    let peer = peer.trim_start_matches('[').trim_end_matches(']');
    if trusted.iter().any(|t| t == peer) {
        if let Some(h) = request.header("x-forwarded-for") {
            if let Some(last) = h.value.rsplit(',').map(str::trim).find(|s| !s.is_empty()) {
                return last.to_string();
            }
        }
    }
    peer.to_string()
}

/// The caller as the log records it. `peer` is the TCP peer; when it is a trusted proxy (the
/// front rws, by default anything on loopback), the client is the last `X-Forwarded-For` entry,
/// which that proxy appended.
pub fn client_of(request: &Request, peer: &str, trusted: &[String], policy: IpPolicy) -> Client {
    let ip = real_ip(request, peer, trusted);
    let ip = match policy {
        IpPolicy::Off => String::new(),
        IpPolicy::Truncated => ip.parse::<IpAddr>().map(truncate_ip).unwrap_or_default(),
    };
    let user_agent = request.header("user-agent").map(|h| h.value.chars().filter(|c| !c.is_control()).take(200).collect()).unwrap_or_default();
    Client { ip, user_agent }
}

/// One event to append.
pub struct Event<'a> {
    pub workspace: &'a str,
    pub event: &'static str,
    pub actor_id: &'a str,
    pub actor_email: &'a str,
    pub target: &'a str,
    pub detail: Json,
}

impl<'a> Event<'a> {
    pub fn new(workspace: &'a str, event: &'static str, actor: Option<&'a User>) -> Self {
        Event { workspace, event, actor_id: actor.map_or("", |u| u.id.as_str()), actor_email: actor.map_or("", |u| u.email.as_str()), target: "", detail: json!({}) }
    }
    pub fn target(mut self, target: &'a str) -> Self {
        self.target = target;
        self
    }
    pub fn detail(mut self, detail: Json) -> Self {
        self.detail = detail;
        self
    }
    pub fn email(mut self, email: &'a str) -> Self {
        self.actor_email = email;
        self
    }
}

/// The hash of one row (see the module docs).
#[allow(clippy::too_many_arguments)]
pub fn row_hash(prev: &str, workspace: &str, seq: i64, at: i64, event: &str, actor_id: &str, actor_email: &str, target: &str, detail: &str, ip: &str, user_agent: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"hitop-sync/v1/audit\0");
    for field in [prev, workspace, &seq.to_string(), &at.to_string(), event, actor_id, actor_email, target, detail, ip, user_agent] {
        h.update((field.len() as u32).to_be_bytes());
        h.update(field.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Appends `ev` in the caller's transaction, so an event commits together with what it records
/// (and never records something that rolled back).
pub fn record(tx: &mut Tx, state: &State, ev: Event) -> Result<(), DbError> {
    let c = client();
    let at = state.now();
    let detail = ev.detail.to_string();
    let head = tx.query_one("SELECT head_seq, head_hash FROM audit_chain WHERE workspace_id = $1", &[ev.workspace.into()])?;
    let (seq, prev) = head.map_or((1, GENESIS.to_string()), |r| (r.int(0) + 1, r.text(1)));
    let hash = row_hash(&prev, ev.workspace, seq, at, ev.event, ev.actor_id, ev.actor_email, ev.target, &detail, &c.ip, &c.user_agent);
    tx.execute(
        "INSERT INTO audit_log (workspace_id, seq, at, event, actor_id, actor_email, target, detail, ip, user_agent, prev_hash, hash) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        &[
            ev.workspace.into(),
            seq.into(),
            at.into(),
            ev.event.into(),
            ev.actor_id.into(),
            ev.actor_email.into(),
            ev.target.into(),
            detail.into(),
            c.ip.into(),
            c.user_agent.into(),
            prev.into(),
            (&hash).into(),
        ],
    )?;
    tx.execute(
        "INSERT INTO audit_chain (workspace_id, head_seq, head_hash, anchor_seq, anchor_hash) VALUES ($1, $2, $3, 0, $4) ON CONFLICT (workspace_id) DO UPDATE SET head_seq = $2, head_hash = $3",
        &[ev.workspace.into(), seq.into(), hash.into(), GENESIS.into()],
    )?;
    Ok(())
}

/// Records an event in a transaction of its own: for failures, whose handler transaction rolls
/// back. Errors are logged, never returned (the caller is already answering with an error).
pub fn record_alone(state: &State, ev: Event) {
    if let Err(e) = state.db.tx(|tx| record(tx, state, ev)) {
        eprintln!("audit: {e}");
    }
}

const COLUMNS: &str = "seq, at, event, actor_id, actor_email, target, detail, ip, user_agent, prev_hash, hash, workspace_id";

fn row_json(r: &Row) -> Json {
    json!({
        "seq": r.int(0),
        "at": r.int(1),
        "event": r.text(2),
        "actor": { "id": r.text(3), "email": r.text(4) },
        "target": r.text(5),
        "detail": serde_json::from_str::<Json>(&r.text(6)).unwrap_or(Json::Null),
        // The exact text the hash covers (clients recompute the chain from it).
        "detailJson": r.text(6),
        "ip": r.text(7),
        "userAgent": r.text(8),
        "prevHash": r.text(9),
        "hash": r.text(10),
    })
}

fn chain_json(tx: &mut Tx, ws: &str) -> Result<Json, DbError> {
    let row = tx.query_one("SELECT head_seq, head_hash, anchor_seq, anchor_hash FROM audit_chain WHERE workspace_id = $1", &[ws.into()])?;
    Ok(match row {
        Some(r) => json!({ "head": { "seq": r.int(0), "hash": r.text(1) }, "anchor": { "seq": r.int(2), "hash": r.text(3) } }),
        None => json!({ "head": { "seq": 0, "hash": GENESIS }, "anchor": { "seq": 0, "hash": GENESIS } }),
    })
}

/// One page, newest first. Filters: `event`, `actor` (email), `since`/`until` (Unix seconds),
/// `before` (a seq: the page after that row), `limit` (default 100, at most 500). `only_actor`
/// limits the page to events the account did or that were done to it (its own account log).
fn page(tx: &mut Tx, ws: &str, request: &Request, only_actor: Option<&str>) -> Result<Json, ApiError> {
    let q = query(request);
    let int = |name: &str, default: i64| -> Result<i64, ApiError> { q.get(name).map_or(Ok(default), |v| v.parse().map_err(|_| ApiError::bad(format!("{name} must be a number")))) };
    let limit = int("limit", 100)?.clamp(1, MAX_PAGE);
    let before = int("before", i64::MAX)?;
    let since = int("since", 0)?;
    let until = int("until", i64::MAX)?;
    let mut sql = format!("SELECT {COLUMNS} FROM audit_log WHERE workspace_id = $1 AND seq < $2 AND at >= $3 AND at <= $4");
    let mut params: Vec<crate::db::Value> = vec![ws.into(), before.into(), since.into(), until.into()];
    if let Some(event) = q.get("event").filter(|e| !e.is_empty()) {
        params.push(event.as_str().into());
        sql += &format!(" AND event = ${}", params.len());
    }
    if let Some(actor) = q.get("actor").filter(|e| !e.is_empty()) {
        params.push(actor.trim().to_lowercase().into());
        sql += &format!(" AND actor_email = ${}", params.len());
    }
    if let Some(me) = only_actor {
        params.push(me.into());
        let n = params.len();
        sql += &format!(" AND (actor_id = ${n} OR target = ${n})");
    }
    params.push((limit + 1).into());
    sql += &format!(" ORDER BY seq DESC LIMIT ${}", params.len());
    let mut rows = tx.query(&sql, &params)?;
    let more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let next = if more { rows.last().map(|r| json!(r.int(0))) } else { None }.unwrap_or(Json::Null);
    let mut out = json!({ "events": rows.iter().map(row_json).collect::<Vec<_>>(), "next": next });
    if only_actor.is_none() {
        let chain = chain_json(tx, ws)?;
        out["head"] = chain["head"].clone();
        out["anchor"] = chain["anchor"].clone();
    }
    Ok(out)
}

/// `GET /v1/workspaces/:ws/audit` (owner): the workspace's log, newest first, paginated.
pub fn list(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        Ok((200, page(tx, id, request, None)?))
    })
}

/// `GET /v1/me/audit`: the caller's own account events (sign-ins, failed sign-ins, keys).
pub fn mine(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        Ok((200, page(tx, ACCOUNT, request, Some(&user.id))?))
    })
}

/// What [`verify`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub checked: i64,
    pub head_seq: i64,
    pub head_hash: String,
    pub anchor_seq: i64,
    /// The first problem, or `None` when the chain is intact.
    pub problem: Option<String>,
}

/// Recomputes one chain from its anchor to its head.
pub fn verify(tx: &mut Tx, ws: &str) -> Result<Verification, DbError> {
    let chain = tx.query_one("SELECT head_seq, head_hash, anchor_seq, anchor_hash FROM audit_chain WHERE workspace_id = $1", &[ws.into()])?;
    let (head_seq, head_hash, anchor_seq, anchor_hash) = chain.map_or((0, GENESIS.to_string(), 0, GENESIS.to_string()), |r| (r.int(0), r.text(1), r.int(2), r.text(3)));
    let mut v = Verification { checked: 0, head_seq, head_hash: head_hash.clone(), anchor_seq, problem: None };
    let (mut expect_seq, mut prev) = (anchor_seq + 1, anchor_hash);
    let mut after = i64::MIN;
    loop {
        let rows = tx.query(&format!("SELECT {COLUMNS} FROM audit_log WHERE workspace_id = $1 AND seq > $2 ORDER BY seq LIMIT 1000"), &[ws.into(), after.into()])?;
        if rows.is_empty() {
            break;
        }
        for r in &rows {
            let seq = r.int(0);
            after = seq;
            if seq != expect_seq {
                v.problem = Some(if seq > expect_seq { format!("row {expect_seq} is missing") } else { format!("row {seq} is before the anchor ({anchor_seq})") });
                return Ok(v);
            }
            if r.text(9) != prev {
                v.problem = Some(format!("row {seq} doesn't follow the row before it (prev_hash differs)"));
                return Ok(v);
            }
            let hash = row_hash(&prev, &r.text(11), seq, r.int(1), &r.text(2), &r.text(3), &r.text(4), &r.text(5), &r.text(6), &r.text(7), &r.text(8));
            if hash != r.text(10) {
                v.problem = Some(format!("row {seq} was changed (its hash doesn't match its contents)"));
                return Ok(v);
            }
            v.checked += 1;
            prev = hash;
            expect_seq = seq + 1;
        }
    }
    if expect_seq - 1 != head_seq || prev != head_hash {
        v.problem = Some(if expect_seq - 1 < head_seq { format!("rows {expect_seq} to {head_seq} are missing") } else { "the chain doesn't end at its recorded head".to_string() });
    }
    Ok(v)
}

/// `GET /v1/workspaces/:ws/audit/verify` (owner) → `{ok, checked, head, anchorSeq, problem}`.
pub fn verify_endpoint(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        let v = verify(tx, id)?;
        Ok((200, json!({ "ok": v.problem.is_none(), "checked": v.checked, "head": { "seq": v.head_seq, "hash": v.head_hash }, "anchorSeq": v.anchor_seq, "problem": v.problem })))
    })
}

/// Deletes rows older than the retention period (`HITOP_SYNC_AUDIT_RETENTION_DAYS`, 0 = keep
/// forever), chain by chain, moving each chain's anchor to the last purged row. Returns how many
/// rows went. Runs at start and hourly (main.rs).
pub fn purge(state: &State) -> Result<u64, DbError> {
    let days = state.config.audit_retention_days;
    if days <= 0 {
        return Ok(0);
    }
    let cutoff = state.now() - days * 86_400;
    state.db.tx(|tx| {
        let mut total = 0;
        let chains = tx.query("SELECT DISTINCT workspace_id FROM audit_log WHERE at < $1", &[cutoff.into()])?;
        for c in chains {
            let ws = c.text(0);
            // Rows are appended in time order, so everything up to the newest old row goes.
            let Some(last) = tx.query_one("SELECT seq, hash FROM audit_log WHERE workspace_id = $1 AND at < $2 ORDER BY seq DESC LIMIT 1", &[(&ws).into(), cutoff.into()])? else { continue };
            total += tx.execute("DELETE FROM audit_log WHERE workspace_id = $1 AND seq <= $2", &[(&ws).into(), last.int(0).into()])?;
            tx.execute("UPDATE audit_chain SET anchor_seq = $1, anchor_hash = $2 WHERE workspace_id = $3", &[last.int(0).into(), last.text(1).into(), ws.into()])?;
        }
        Ok(total)
    })
}

/// Verifies every chain (`hitop-sync-server audit-verify`, for operators): `(chain, result)`.
pub fn verify_all(state: &State) -> Result<Vec<(String, Verification)>, DbError> {
    state.db.tx(|tx| {
        let chains = tx.query("SELECT workspace_id FROM audit_chain ORDER BY workspace_id", &[])?;
        let mut out = Vec::new();
        for c in chains {
            let ws = c.text(0);
            let v = verify(tx, &ws)?;
            out.push((ws, v));
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ips_are_truncated() {
        assert_eq!(truncate_ip("203.0.113.77".parse().unwrap()), "203.0.113.0");
        assert_eq!(truncate_ip("2001:db8:1:2:3:4:5:6".parse().unwrap()), "2001:db8:1::");
        assert_eq!(truncate_ip("::ffff:198.51.100.9".parse().unwrap()), "198.51.100.0");
    }

    #[test]
    fn the_hash_is_pinned() {
        // Clients recompute this (checked independently with Python's hashlib); changing it
        // breaks every client's verification.
        let h = row_hash(GENESIS, "", 1, 1_700_000_000, "signin", "u1", "a@example.com", "", "{\"method\":\"email\"}", "203.0.113.0", "hitop/1.0");
        assert_eq!(h, "af2334f0d9bdfd192619c1a7f10ddb9b3fbc8c6ab807a1c81eac9c5d66dbe170");
    }
}
