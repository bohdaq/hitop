//! Teams: invites, role changes and runner credentials.
//!
//! The server can't add anyone to a workspace by itself: that takes the workspace key wrapped for
//! the new member's device keys, which only a member's client can make. So an invite is a
//! rendezvous: the owner creates one (mailed, or shared as a link), the invitee signs in,
//! registers a device key and accepts it, and the owner's client then sees the accepted invite
//! with the invitee's device keys, checks their fingerprints and adds them
//! (`POST /v1/workspaces/:ws/members` with the wraps).
//!
//! A runner credential is a scoped account for scheduled runs (CI, cron): it belongs to one
//! workspace, has the viewer role, one device key (made by the owner's client) and one long-lived
//! session token. It can read that workspace's blobs and wrapped keys and nothing else: no other
//! workspace, no new keys, no invites, no sign-in. Removing it is removing a member: the key
//! rotates.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};
use webauthn_rs::prelude::Uuid;

use crate::api::{b64, body, new_token, random_hex, token_hash, unb64, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, RUNNER_DOMAIN};
use crate::mail::Mail;
use crate::workspaces::{check_wrap, insert_wraps, membership, require, Role};

const INVITE_TTL: i64 = 7 * 24 * 3600;
const MAX_OPEN_INVITES: i64 = 50;
/// Invite mails per workspace per hour.
const INVITE_MAILS_PER_HOUR: i64 = 20;
const MAX_RUNNERS: i64 = 10;
/// Runner sessions last a year; removing the runner ends them at once.
pub const RUNNER_TTL: i64 = 365 * 24 * 3600;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewInvite {
    #[serde(default)]
    email: Option<String>,
    role: String,
}

/// `POST /v1/workspaces/:ws/invites {"email"?, "role"}` (owner) → `{id, token, link, mailed}`.
/// With an email the invite is mailed and only that address can accept it; without one, anyone
/// with the link can (share it privately). Valid 7 days.
pub fn create(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let input: NewInvite = body(request)?;
    let role = Role::parse(&input.role)?;
    let email = match input.email.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) => {
            let e = e.to_lowercase();
            if e.len() > 254 || e.parse::<lettre::Address>().is_err() || e.ends_with(&format!("@{RUNNER_DOMAIN}")) {
                return Err(ApiError::bad("email is not a valid address"));
            }
            e
        }
        None => String::new(),
    };
    let token = new_token();
    let now = state.now();
    let (invite_id, inviter, mail) = state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        tx.execute("DELETE FROM invites WHERE workspace_id = $1 AND expires_at <= $2", &[id.into(), now.into()])?;
        let open = tx.query_one("SELECT COUNT(*) FROM invites WHERE workspace_id = $1", &[id.into()])?.map_or(0, |r| r.int(0));
        if open >= MAX_OPEN_INVITES {
            return Err(ApiError::new(429, "rate_limited", "this workspace has too many open invites; delete some"));
        }
        let mailed_recently = tx
            .query_one("SELECT COUNT(*) FROM invites WHERE workspace_id = $1 AND email <> '' AND created_at > $2", &[id.into(), (now - 3600).into()])?
            .map_or(0, |r| r.int(0));
        let mail = !email.is_empty() && state.mailer.enabled();
        if mail && mailed_recently >= INVITE_MAILS_PER_HOUR {
            return Err(ApiError::new(429, "rate_limited", "too many invite mails from this workspace; try again in an hour"));
        }
        let org = crate::limits::org_of_workspace(tx, id)?;
        crate::limits::check_invite(state, tx, &org)?;
        crate::limits::meter(state, tx, &org, "invites", 1)?;
        let invite_id = random_hex::<16>();
        tx.execute(
            "INSERT INTO invites (id, workspace_id, token_hash, email, role, created_by, created_at, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[(&invite_id).into(), id.into(), token_hash(&token).into(), (&email).into(), role.as_str().into(), (&user.id).into(), now.into(), (now + INVITE_TTL).into()],
        )?;
        audit::record(tx, state, Event::new(id, "invite_created", Some(&user)).target(&invite_id).detail(json!({ "email": email, "role": role.as_str(), "mailed": mail })))?;
        Ok::<_, ApiError>((invite_id, user.email, mail))
    })?;
    let link = state.config.public_url.join("invite").map(|u| format!("{u}#{token}")).unwrap_or_default();
    if mail {
        state.mailer.send(Mail {
            to: email.clone(),
            subject: "You're invited to a hitop sync workspace".into(),
            body: format!(
                "{inviter} invited you to a shared hitop workspace as {role}.\n\nSign in to hitop sync with this address ({email}), then accept the invite:\n\n  hitop sync accept {token}\n\nor open this link and paste the code into hitop:\n\n{link}\n\nThe workspace owner then confirms your device and you can sync. The invite works for 7 days. If you don't know the sender, ignore this mail.\n",
                role = role.as_str()
            ),
        });
    }
    Ok((201, json!({ "id": invite_id, "token": token, "link": link, "mailed": mail, "expiresAt": now + INVITE_TTL })))
}

/// `GET /v1/workspaces/:ws/invites` (owner): open invites; accepted ones carry the account and
/// its device keys, to check fingerprints and add it.
pub fn list(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let now = state.now();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        let rows = tx.query(
            "SELECT i.id, i.email, i.role, i.created_at, i.expires_at, i.accepted_by, COALESCE(u.email, '') FROM invites i LEFT JOIN users u ON u.id = i.accepted_by WHERE i.workspace_id = $1 AND i.expires_at > $2 ORDER BY i.created_at, i.id",
            &[id.into(), now.into()],
        )?;
        let mut out = Vec::new();
        for r in rows {
            let accepted = if r.text(5).is_empty() {
                Json::Null
            } else {
                let keys = tx.query("SELECT id, name, public_key FROM device_keys WHERE user_id = $1 ORDER BY created_at, id", &[r.text(5).into()])?;
                json!({ "userId": r.text(5), "email": r.text(6), "keys": keys.iter().map(|k| json!({ "id": k.text(0), "name": k.text(1), "publicKey": b64(&k.bytes(2)) })).collect::<Vec<_>>() })
            };
            out.push(json!({ "id": r.text(0), "email": r.text(1), "role": r.text(2), "createdAt": r.int(3), "expiresAt": r.int(4), "accepted": accepted }));
        }
        Ok((200, json!({ "invites": out })))
    })
}

/// `DELETE /v1/workspaces/:ws/invites/:id` (owner).
pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let invite = params.get("id").unwrap_or_default();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        if tx.execute("DELETE FROM invites WHERE workspace_id = $1 AND id = $2", &[id.into(), invite.into()])? == 0 {
            return Err(ApiError::not_found("no such invite"));
        }
        audit::record(tx, state, Event::new(id, "invite_deleted", Some(&user)).target(invite))?;
        Ok((204, Json::Null))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Accept {
    token: String,
}

/// The token of an invite code. hitop 1.2 clients hand out `<token>.<inviter's key fingerprint>`
/// (the fingerprint is for the invitee's client, which checks the inviter's key with it); the
/// token is base64url, so it never contains a `.`.
fn invite_token(code: &str) -> &str {
    let code = code.trim();
    code.split('.').next().unwrap_or(code)
}

/// `POST /v1/invites/accept {"token"}` (signed in, with a device key) → `{workspaceId, role}`.
/// Membership starts when the owner adds the account with the wrapped key.
pub fn accept(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: Accept = body(request)?;
    let now = state.now();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let row = tx.query_one(
            "SELECT id, workspace_id, email, role, accepted_by FROM invites WHERE token_hash = $1 AND expires_at > $2",
            &[token_hash(invite_token(&input.token)).into(), now.into()],
        )?;
        let Some(row) = row else {
            return Err(ApiError::new(404, "invalid_invite", "this invite is invalid or expired"));
        };
        if !row.text(2).is_empty() && row.text(2) != user.email {
            return Err(ApiError::forbidden("this invite is for another address; sign in with the address it was sent to"));
        }
        if !row.text(4).is_empty() && row.text(4) != user.id {
            return Err(ApiError::conflict("already_accepted", "someone else accepted this invite", json!({})));
        }
        if tx.query_one("SELECT role FROM members WHERE workspace_id = $1 AND user_id = $2", &[row.text(1).into(), (&user.id).into()])?.is_some() {
            return Err(ApiError::conflict("already_member", "already a member", json!({})));
        }
        if tx.query_one("SELECT id FROM device_keys WHERE user_id = $1", &[(&user.id).into()])?.is_none() {
            return Err(ApiError::bad("register a device key first (POST /v1/me/keys)"));
        }
        tx.execute("UPDATE invites SET accepted_by = $1, accepted_at = $2 WHERE id = $3", &[(&user.id).into(), now.into(), row.text(0).into()])?;
        audit::record(tx, state, Event::new(&row.text(1), "invite_accepted", Some(&user)).target(&row.text(0)).detail(json!({ "role": row.text(3) })))?;
        Ok((200, json!({ "workspaceId": row.text(1), "role": row.text(3) })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetRole {
    role: String,
}

/// `PUT /v1/workspaces/:ws/members/:user {"role"}` (owner). A workspace keeps one owner; runner
/// credentials stay viewers. No key change: every role holds the same workspace key (roles say
/// what the server accepts, see THREAT_MODEL.md).
pub fn set_role(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let target = params.get("user").unwrap_or_default().to_string();
    let input: SetRole = body(request)?;
    let role = Role::parse(&input.role)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        let Some(row) = tx.query_one("SELECT m.role, u.runner_of FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 AND m.user_id = $2", &[id.into(), (&target).into()])? else {
            return Err(ApiError::not_found("not a member"));
        };
        if !row.text(1).is_empty() && role != Role::Viewer {
            return Err(ApiError::bad("runner credentials are viewers"));
        }
        if row.text(0) == "owner" && role != Role::Owner {
            let owners = tx.query_one("SELECT COUNT(*) FROM members WHERE workspace_id = $1 AND role = 'owner'", &[id.into()])?.map_or(0, |r| r.int(0));
            if owners <= 1 {
                return Err(ApiError::bad("a workspace keeps at least one owner"));
            }
        }
        if row.text(1).is_empty() && role != Role::Viewer {
            // A guest viewer who becomes an editor takes a seat.
            let org = crate::limits::org_of_workspace(tx, id)?;
            crate::limits::check_seat(state, tx, &org, &target, true)?;
        }
        tx.execute("UPDATE members SET role = $1 WHERE workspace_id = $2 AND user_id = $3", &[role.as_str().into(), id.into(), (&target).into()])?;
        audit::record(tx, state, Event::new(id, "role_changed", Some(&user)).target(&target).detail(json!({ "from": row.text(0), "to": role.as_str() })))?;
        Ok((200, json!({ "userId": target, "role": role.as_str() })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NewRunner {
    #[serde(default)]
    name: String,
    public_key: String,
    /// The current epoch's workspace key, wrapped for `public_key`.
    wrapped: String,
}

/// `POST /v1/workspaces/:ws/runners {"name","publicKey","wrapped"}` (owner) → `{userId, keyId,
/// token, epoch, expiresAt}`: a runner credential. The owner's client made the key pair; the
/// secret half and `token` go together into the CI secret (`HITOP_SYNC_RUNNER`).
pub fn create_runner(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let id = params.get("ws").unwrap_or_default();
    let input: NewRunner = body(request)?;
    let public_key = unb64("publicKey", &input.public_key)?;
    if public_key.len() != 32 {
        return Err(ApiError::bad("publicKey must be a 32-byte X25519 public key"));
    }
    let wrapped = unb64("wrapped", &input.wrapped)?;
    let name: String = input.name.trim().chars().take(64).collect();
    let name = if name.is_empty() { "runner".to_string() } else { name };
    let token = new_token();
    let now = state.now();
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let ws = membership(tx, id, &user)?;
        require(&ws, Role::Owner)?;
        check_wrap(tx, &user, "the runner", &wrapped)?;
        let runners = tx.query_one("SELECT COUNT(*) FROM users WHERE runner_of = $1", &[id.into()])?.map_or(0, |r| r.int(0));
        if runners >= MAX_RUNNERS {
            return Err(ApiError::forbidden(format!("a workspace has at most {MAX_RUNNERS} runner credentials")));
        }
        let org = crate::limits::org_of_workspace(tx, id)?;
        crate::limits::check_runner(state, tx, &org)?;
        let runner_id = Uuid::new_v4().to_string();
        let email = format!("{runner_id}@{RUNNER_DOMAIN}");
        tx.execute("INSERT INTO users (id, email, created_at, runner_of) VALUES ($1, $2, $3, $4)", &[(&runner_id).into(), email.into(), now.into(), id.into()])?;
        let key_id = random_hex::<16>();
        tx.execute(
            "INSERT INTO device_keys (id, user_id, name, public_key, created_at) VALUES ($1, $2, $3, $4, $5)",
            &[(&key_id).into(), (&runner_id).into(), name.into(), public_key.into(), now.into()],
        )?;
        tx.execute("INSERT INTO members (workspace_id, user_id, role) VALUES ($1, $2, 'viewer')", &[id.into(), (&runner_id).into()])?;
        insert_wraps(tx, id, ws.epoch, &[(key_id.clone(), wrapped.clone())])?;
        let expires = now + RUNNER_TTL;
        tx.execute(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES ($1, $2, $3, $4)",
            &[token_hash(&token).into(), (&runner_id).into(), now.into(), expires.into()],
        )?;
        audit::record(tx, state, Event::new(id, "runner_created", Some(&user)).target(&runner_id).detail(json!({ "keyId": key_id, "epoch": ws.epoch, "expiresAt": expires })))?;
        Ok((201, json!({ "userId": runner_id, "keyId": key_id, "token": token, "epoch": ws.epoch, "expiresAt": expires })))
    })
}
