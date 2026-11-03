//! Audit export and daily anchors (HITOP Cloud C5, Business; THREAT_MODEL item 7).
//!
//! The audit log's hash chains (audit.rs) are tamper evidence only against someone who can't
//! write the database: the operator, or an attacker on the host, could rewrite a whole chain with
//! fresh hashes. Two things move the evidence outside the server:
//!
//! - **Daily anchors.** Once a day the server writes, per org, the head (seq, hash) of each of the
//!   org's chains (`org:<id>` and each workspace in the org), signs it and **mails it to the org's
//!   owners** (when a head moved since the last anchor). Opt-in per org, the anchors are also
//!   public at `GET /v1/audit-anchors/<org>/<day>.json` (for the `status.` site): public anchors
//!   show how many workspaces an org has and their ids, so they're off by default. Once a mail
//!   left, a rewritten chain no longer matches the head it carries.
//! - **Signed export.** `GET /v1/orgs/:org/audit/export` (owners and admins) is JSON lines: a
//!   header, one `chain` line per chain (head, and where the retention purge left it,
//!   `purgedThrough`), every `event` row oldest first per chain, then a `signature` line.
//!   `hitop sync audit --verify --anchor <anchor.json>` recomputes every chain and checks that each
//!   head an anchor names is in the export at that seq with that hash.
//!
//! Signing key: Ed25519, its seed a server secret (`server_secrets`, made on first use); its
//! public half is at `GET /v1/audit/key` and in every anchor and export. The signature says the
//! server made this file; it doesn't make the server honest (it holds the key). What protects
//! against a rewrite is the anchor that already left the server.
//!
//! Signed bytes, never re-serialized JSON:
//!
//! ```text
//! export:  Ed25519("hitop-sync/v1/audit-export" || 0x00 || SHA-256(every byte before the signature line))
//! anchor:  Ed25519(canonical text), the text being, each line ending in "\n":
//!            hitop-sync/v1/audit-anchor
//!            server <public URL>
//!            org <org id>
//!            day <YYYY-MM-DD, UTC>
//!            at <Unix seconds>
//!            chain <chain id> <head seq> <head hash>     (one line per chain, by chain id)
//! ```
//!
//! "Anchor" here is the daily anchor; the retention purge's anchor (`audit_chain.anchor_seq`) is
//! called `purgedThrough` in the export so the two never mix.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::signature::{Ed25519KeyPair, KeyPair};
use rust_web_server::header::Header;
use rust_web_server::request::Request;
use rust_web_server::response::{Response, STATUS_CODE_REASON_PHRASE};
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};

use crate::api::{body, ApiError, ApiResult, State};
use crate::audit::{self, Event, GENESIS};
use crate::db::{DbError, Tx};
use crate::mail::Mail;
use crate::orgs::{self, OrgRole};

pub const EXPORT_LABEL: &[u8] = b"hitop-sync/v1/audit-export\0";
pub const ANCHOR_HEADER: &str = "hitop-sync/v1/audit-anchor";
/// Plans with audit export and anchors (with `HITOP_SYNC_LIMITS=plans`).
pub const AUDIT_PLANS: &[&str] = &["business", "enterprise"];

/// The server's signing key and its id (first 16 hex characters of SHA-256 of the public key).
pub struct Signer {
    pair: Ed25519KeyPair,
    pub key_id: String,
    pub public: Vec<u8>,
}

impl Signer {
    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        self.pair.sign(msg).as_ref().to_vec()
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn key_id(public: &[u8]) -> String {
    hex(&Sha256::digest(public))[..16].to_string()
}

pub fn signer(tx: &mut Tx) -> Result<Signer, ApiError> {
    let seed = crate::password::server_secret(tx, "audit_signing_seed")?;
    let pair = Ed25519KeyPair::from_seed_unchecked(&seed)
        .map_err(|_| ApiError::new(500, "internal", "the audit signing key is unusable"))?;
    let public = pair.public_key().as_ref().to_vec();
    Ok(Signer {
        key_id: key_id(&public),
        public,
        pair,
    })
}

/// `GET /v1/audit/key` (no sign-in): the public key exports and anchors are signed with.
pub fn key(state: &State, _: &Request, _: &PathParams) -> ApiResult {
    let s = state.db.tx(signer)?;
    Ok((
        200,
        json!({ "alg": "Ed25519", "keyId": s.key_id, "publicKey": URL_SAFE_NO_PAD.encode(&s.public) }),
    ))
}

/// The org's chains: `org:<id>` and each workspace now in the org, by chain id.
pub fn chains(tx: &mut Tx, org: &str) -> Result<Vec<String>, DbError> {
    let mut out = vec![orgs::chain(org)];
    out.extend(
        tx.query(
            "SELECT id FROM workspaces WHERE org_id = $1 ORDER BY id",
            &[org.into()],
        )?
        .iter()
        .map(|r| r.text(0)),
    );
    out.sort();
    Ok(out)
}

/// (head seq, head hash, purged-through seq, its hash) of a chain.
fn chain_state(tx: &mut Tx, chain: &str) -> Result<(i64, String, i64, String), DbError> {
    Ok(tx
        .query_one("SELECT head_seq, head_hash, anchor_seq, anchor_hash FROM audit_chain WHERE workspace_id = $1", &[chain.into()])?
        .map_or((0, GENESIS.to_string(), 0, GENESIS.to_string()), |r| (r.int(0), r.text(1), r.int(2), r.text(3))))
}

fn audit_caller(
    state: &State,
    tx: &mut Tx,
    request: &Request,
    org: &str,
    need: OrgRole,
    change: bool,
) -> Result<(crate::auth::User, orgs::Org), ApiError> {
    let (user, o) = orgs::org_caller(state, tx, request, org, need, change)?;
    if o.personal {
        return Err(ApiError::bad(
            "audit export and anchors are for an organization, not a personal account",
        ));
    }
    crate::limits::require_plan(
        state,
        tx,
        org,
        "auditExport",
        "audit export and anchors",
        AUDIT_PLANS,
    )?;
    Ok((user, o))
}

/// The export of `org` as bytes (JSON lines, the signature line last).
pub fn export_bytes(state: &State, tx: &mut Tx, org: &str) -> Result<Vec<u8>, ApiError> {
    let signer = signer(tx)?;
    let mut out = Vec::new();
    let line = |v: Json, out: &mut Vec<u8>| {
        out.extend_from_slice(v.to_string().as_bytes());
        out.push(b'\n');
    };
    line(
        json!({ "type": "hitop-audit-export", "version": 1, "server": state.config.public_url.as_str(), "org": org, "exportedAt": state.now(), "keyId": signer.key_id, "publicKey": URL_SAFE_NO_PAD.encode(&signer.public) }),
        &mut out,
    );
    let chains = chains(tx, org)?;
    for c in &chains {
        let (hs, hh, ps, ph) = chain_state(tx, c)?;
        line(
            json!({ "type": "chain", "chain": c, "head": { "seq": hs, "hash": hh }, "purgedThrough": { "seq": ps, "hash": ph } }),
            &mut out,
        );
    }
    for c in &chains {
        let mut after = i64::MIN;
        loop {
            let rows = tx.query(
                "SELECT seq, at, event, actor_id, actor_email, target, detail, ip, user_agent, prev_hash, hash FROM audit_log WHERE workspace_id = $1 AND seq > $2 ORDER BY seq LIMIT 1000",
                &[c.into(), after.into()],
            )?;
            if rows.is_empty() {
                break;
            }
            for r in &rows {
                after = r.int(0);
                line(
                    json!({
                        "type": "event", "chain": c, "seq": r.int(0), "at": r.int(1), "event": r.text(2),
                        "actor": { "id": r.text(3), "email": r.text(4) }, "target": r.text(5),
                        "detail": serde_json::from_str::<Json>(&r.text(6)).unwrap_or(Json::Null), "detailJson": r.text(6),
                        "ip": r.text(7), "userAgent": r.text(8), "prevHash": r.text(9), "hash": r.text(10),
                    }),
                    &mut out,
                );
            }
        }
    }
    let digest = Sha256::digest(&out);
    let sig = signer.sign(&[EXPORT_LABEL, digest.as_slice()].concat());
    line(
        json!({ "type": "signature", "alg": "Ed25519", "keyId": signer.key_id, "sha256": hex(&digest), "signature": URL_SAFE_NO_PAD.encode(sig) }),
        &mut out,
    );
    Ok(out)
}

fn file_response(body: Vec<u8>, content_type: &str, filename: &str) -> Response {
    let range = rust_web_server::range::Range::get_content_range(body, content_type.to_string());
    let mut r = Response::get_response(STATUS_CODE_REASON_PHRASE.n200_ok, None, Some(vec![range]));
    r.headers.push(Header {
        name: "Content-Disposition".into(),
        value: format!("attachment; filename=\"{filename}\"").into(),
    });
    r
}

/// `GET /v1/orgs/:org/audit/export` (owners and admins; Business) → `application/x-ndjson`.
/// Answered outside the JSON router (api.rs) because it isn't one JSON value.
pub fn export(state: &State, request: &Request, org: &str) -> Response {
    let result = state.db.tx(|tx| -> Result<(Vec<u8>, String), ApiError> {
        let (user, _) = audit_caller(state, tx, request, org, OrgRole::Admin, false)?;
        Ok((export_bytes(state, tx, org)?, user.id))
    });
    match result {
        Ok((bytes, user_id)) => {
            let rows = bytes
                .iter()
                .filter(|b| **b == b'\n')
                .count()
                .saturating_sub(2);
            let actor = crate::auth::User {
                id: user_id,
                email: String::new(),
                runner_of: String::new(),
            };
            audit::record_alone(
                state,
                Event::new(&orgs::chain(org), "audit_exported", Some(&actor))
                    .target(org)
                    .detail(json!({ "lines": rows })),
            );
            file_response(
                bytes,
                "application/x-ndjson",
                &format!("hitop-audit-{org}-{}.jsonl", day_of(state.now())),
            )
        }
        Err(e) => crate::api::error_response(&e),
    }
}

// --- anchors ---------------------------------------------------------------------------------

/// `YYYY-MM-DD` (UTC) of a Unix time.
pub fn day_of(t: i64) -> String {
    // Civil from days (Howard Hinnant's algorithm).
    let z = t.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

/// The canonical text an anchor's signature covers.
pub fn anchor_text(
    server: &str,
    org: &str,
    day: &str,
    at: i64,
    chains: &[(String, i64, String)],
) -> String {
    let mut t = format!("{ANCHOR_HEADER}\nserver {server}\norg {org}\nday {day}\nat {at}\n");
    let mut sorted: Vec<_> = chains.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (c, seq, hash) in sorted {
        t += &format!("chain {c} {seq} {hash}\n");
    }
    t
}

/// A signed anchor of `org`'s chain heads now.
pub fn make_anchor(state: &State, tx: &mut Tx, org: &str) -> Result<Json, ApiError> {
    let signer = signer(tx)?;
    let at = state.now();
    let day = day_of(at);
    let mut heads = Vec::new();
    for c in chains(tx, org)? {
        let (seq, hash, _, _) = chain_state(tx, &c)?;
        heads.push((c, seq, hash));
    }
    let server = state.config.public_url.as_str().to_string();
    let text = anchor_text(&server, org, &day, at, &heads);
    let sig = signer.sign(text.as_bytes());
    Ok(json!({
        "type": "hitop-audit-anchor",
        "version": 1,
        "server": server,
        "org": org,
        "day": day,
        "at": at,
        "chains": heads.iter().map(|(c, s, h)| json!({ "chain": c, "seq": s, "hash": h })).collect::<Vec<_>>(),
        "alg": "Ed25519",
        "keyId": signer.key_id,
        "publicKey": URL_SAFE_NO_PAD.encode(&signer.public),
        "signature": URL_SAFE_NO_PAD.encode(sig),
    }))
}

/// An org's anchor settings: (mail to owners, public).
pub fn settings(tx: &mut Tx, org: &str) -> Result<(bool, bool), DbError> {
    Ok(tx
        .query_one(
            "SELECT mail_anchors, public_anchors FROM org_audit_settings WHERE org_id = $1",
            &[org.into()],
        )?
        .map_or((true, false), |r| (r.int(0) != 0, r.int(1) != 0)))
}

fn heads_of(anchor: &Json) -> Json {
    anchor["chains"].clone()
}

/// An anchor and the owners to mail it to.
type Mailing = (Json, Vec<String>);

/// Makes today's anchor for every org that gets them (team orgs; on HITOP Cloud those on Business
/// or Enterprise) and mails it to the owners when a head moved since the org's last anchor. Runs
/// hourly, the first an hour after start (main.rs); at most one anchor an org a day. Returns how many it made.
pub fn daily(state: &State) -> Result<usize, ApiError> {
    if !state.config.audit_anchors {
        return Ok(0);
    }
    let day = day_of(state.now());
    let orgs = state.db.tx(|tx| tx.query("SELECT o.id, o.name, COALESCE(e.plan, 'free'), COALESCE(e.status, 'active') FROM orgs o LEFT JOIN entitlements e ON e.org_id = o.id WHERE o.personal_of = '' ORDER BY o.id", &[]))?;
    let mut made = 0;
    for o in orgs {
        let org = o.text(0);
        if state.config.plan_limits {
            let e = crate::limits::Entitlement {
                org: org.clone(),
                plan: o.text(2),
                seats: 0,
                overrides: Default::default(),
                status: o.text(3),
                period_end: 0,
            };
            if !AUDIT_PLANS.contains(&e.plan_in_force()) {
                continue;
            }
        }
        // None: today's anchor exists; Some(None): made, not mailed; Some(Some(..)): mail it.
        let mail = state.db.tx(|tx| -> Result<Option<Option<Mailing>>, ApiError> {
            if tx.query_one("SELECT day FROM audit_anchors WHERE org_id = $1 AND day = $2", &[(&org).into(), (&day).into()])?.is_some() {
                return Ok(None);
            }
            let anchor = make_anchor(state, tx, &org)?;
            let last = tx.query_one("SELECT body FROM audit_anchors WHERE org_id = $1 ORDER BY day DESC LIMIT 1", &[(&org).into()])?.and_then(|r| serde_json::from_str::<Json>(&r.text(0)).ok());
            let moved = last.as_ref().is_none_or(|l| heads_of(l) != heads_of(&anchor));
            tx.execute("INSERT INTO audit_anchors (org_id, day, body, created_at, mailed_at) VALUES ($1, $2, $3, $4, 0)", &[(&org).into(), (&day).into(), anchor.to_string().into(), state.now().into()])?;
            let (mail_on, _) = settings(tx, &org)?;
            if !(moved && mail_on && state.mailer.enabled()) {
                return Ok(Some(None));
            }
            let owners: Vec<String> = tx
                .query("SELECT u.email FROM org_members m JOIN users u ON u.id = m.user_id WHERE m.org_id = $1 AND m.role = 'owner' AND u.runner_of = '' ORDER BY u.email", &[(&org).into()])?
                .iter()
                .map(|r| r.text(0))
                .collect();
            tx.execute("UPDATE audit_anchors SET mailed_at = $1 WHERE org_id = $2 AND day = $3", &[state.now().into(), (&org).into(), (&day).into()])?;
            Ok(Some(Some((anchor, owners))))
        })?;
        let Some(mail) = mail else { continue };
        made += 1;
        if let Some((anchor, owners)) = mail {
            let name = o.text(1);
            for to in owners {
                state.mailer.send(Mail {
                    to,
                    subject: format!("HITOP: audit anchor for {name}, {day}"),
                    body: format!(
                        "The heads of the audit log of the organization {name} on {day}, signed by the sync server.\n\nKeep this mail. To check later that nobody rewrote the log, save the block below as anchor.json and run:\n\nhitop sync audit --verify --anchor anchor.json\n\n-----BEGIN HITOP AUDIT ANCHOR-----\n{}\n-----END HITOP AUDIT ANCHOR-----\n\nYou get this mail as an owner of the organization, on days its log changed. Owners can turn it off in the HITOP Cloud portal (Organization, Audit).",
                        serde_json::to_string_pretty(&anchor).unwrap_or_default()
                    ),
                });
            }
        }
    }
    Ok(made)
}

/// `GET /v1/orgs/:org/audit/anchors` (owners and admins): the stored anchors, newest first (at
/// most 400), and the settings.
pub fn list(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        audit_caller(state, tx, request, &org, OrgRole::Admin, false)?;
        let rows = tx.query("SELECT day, body, created_at, mailed_at FROM audit_anchors WHERE org_id = $1 ORDER BY day DESC LIMIT 400", &[(&org).into()])?;
        let (mail, public) = settings(tx, &org)?;
        let signer = signer(tx)?;
        Ok((
            200,
            json!({
                "mailAnchors": mail,
                "publicAnchors": public,
                "anchorsOn": state.config.audit_anchors,
                "keyId": signer.key_id,
                "publicUrl": format!("{}v1/audit-anchors/{org}/", state.config.public_url),
                "anchors": rows.iter().map(|r| json!({ "day": r.text(0), "createdAt": r.int(2), "mailedAt": if r.int(3) > 0 { json!(r.int(3)) } else { Json::Null }, "anchor": serde_json::from_str::<Json>(&r.text(1)).unwrap_or(Json::Null) })).collect::<Vec<_>>(),
            }),
        ))
    })
}

/// `POST /v1/orgs/:org/audit/anchors` (owners and admins): an anchor of the heads right now,
/// signed but not stored or mailed (to keep next to an export).
pub fn now(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        audit_caller(state, tx, request, &org, OrgRole::Admin, false)?;
        Ok((200, make_anchor(state, tx, &org)?))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SettingsIn {
    #[serde(default)]
    mail_anchors: Option<bool>,
    #[serde(default)]
    public_anchors: Option<bool>,
}

/// `PUT /v1/orgs/:org/audit/settings {mailAnchors?, publicAnchors?}` (owner, a session).
pub fn put_settings(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let input: SettingsIn = body(request)?;
    state.db.tx(|tx| {
        let (user, _) = audit_caller(state, tx, request, &org, OrgRole::Owner, true)?;
        let (mail, public) = settings(tx, &org)?;
        let (mail, public) = (input.mail_anchors.unwrap_or(mail), input.public_anchors.unwrap_or(public));
        tx.execute(
            "INSERT INTO org_audit_settings (org_id, mail_anchors, public_anchors, updated_by, updated_at) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (org_id) DO UPDATE SET mail_anchors = $2, public_anchors = $3, updated_by = $4, updated_at = $5",
            &[(&org).into(), mail.into(), public.into(), (&user.id).into(), state.now().into()],
        )?;
        audit::record(tx, state, Event::new(&orgs::chain(&org), "audit_settings_changed", Some(&user)).target(&org).detail(json!({ "mailAnchors": mail, "publicAnchors": public })))?;
        Ok((200, json!({ "mailAnchors": mail, "publicAnchors": public })))
    })
}

/// `GET /v1/audit-anchors/:org/:day[.json]` (no sign-in): an anchor of an org that made its
/// anchors public; 404 otherwise (the same answer whether or not the org exists).
pub fn public(state: &State, _: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let day = params
        .get("day")
        .unwrap_or_default()
        .trim_end_matches(".json")
        .to_string();
    state.db.tx(|tx| {
        let none = || ApiError::not_found("no such public anchor");
        let (_, public) = settings(tx, &org)?;
        if !public {
            return Err(none());
        }
        let r = tx
            .query_one(
                "SELECT body FROM audit_anchors WHERE org_id = $1 AND day = $2",
                &[(&org).into(), (&day).into()],
            )?
            .ok_or_else(none)?;
        Ok((
            200,
            serde_json::from_str::<Json>(&r.text(0)).unwrap_or(Json::Null),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_utc_dates() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(1_700_000_000), "2023-11-14");
        assert_eq!(day_of(951_782_400), "2000-02-29");
        assert_eq!(day_of(1_793_750_399), "2026-11-03");
    }

    #[test]
    fn anchor_text_is_pinned() {
        let t = anchor_text(
            "https://sync.example/",
            "o1",
            "2026-11-03",
            1_793_700_000,
            &[
                ("ws2".into(), 3, "bb".into()),
                ("org:o1".into(), 2, "aa".into()),
            ],
        );
        assert_eq!(t, "hitop-sync/v1/audit-anchor\nserver https://sync.example/\norg o1\nday 2026-11-03\nat 1793700000\nchain org:o1 2 aa\nchain ws2 3 bb\n");
    }
}
