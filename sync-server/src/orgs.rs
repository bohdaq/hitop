//! Organizations (HITOP Cloud C1): the unit a plan, seats and limits belong to (limits.rs).
//!
//! Every person has a personal org (`personal-<user id>`, created by migration 4 for existing
//! accounts and on first use for new ones) that owns the workspaces they create without naming an
//! org. A team org has members with the roles owner, admin and member. Org roles say who may manage
//! the org (name, members, plan); they never give access to a workspace: that still takes a
//! workspace membership and the workspace key wrapped for the person's devices, which only a
//! member's client can make. So the server learns nothing new about workspace contents: an org has
//! a name (typed in the portal), its members' accounts and which workspace ids it owns.
//!
//! Who may do what (tests/orgs.rs checks every pair): any member reads the org, its members, usage
//! and limits, creates workspaces in it and moves a workspace they own into it; admins and owners
//! rename it and add, remove and change members and admins; only owners add or remove owners and
//! delete the org (once it has no workspaces). An org keeps at least one owner. A personal org has
//! one member, its person, and can't be renamed, deleted or given members (share a workspace, or
//! make an org). Anyone else gets 404, so org ids can't be probed. Changes go to the audit log, on
//! the chain `org:<id>`.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{body, random_hex, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, normalize_email, User};
use crate::db::{DbError, Tx};
use crate::limits;
use crate::mail::Mail;
use crate::workspaces::{membership, require, Role};

/// Team orgs a person may own (against abuse; personal orgs don't count).
const MAX_OWNED_ORGS: i64 = 20;
const MAX_NAME: usize = 64;

pub fn personal_id(user_id: &str) -> String {
    format!("personal-{user_id}")
}

/// The audit chain of an org.
pub fn chain(org: &str) -> String {
    format!("org:{org}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrgRole {
    Member,
    Admin,
    Owner,
}

impl OrgRole {
    pub fn parse(s: &str) -> Result<OrgRole, ApiError> {
        match s {
            "owner" => Ok(OrgRole::Owner),
            "admin" => Ok(OrgRole::Admin),
            "member" => Ok(OrgRole::Member),
            _ => Err(ApiError::bad("role must be owner, admin or member")),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            OrgRole::Owner => "owner",
            OrgRole::Admin => "admin",
            OrgRole::Member => "member",
        }
    }
}

/// Makes sure `user` (a person) has a personal org, and turns pending org invites for their address
/// into memberships. Returns the personal org's id.
pub fn ensure_personal(state: &State, tx: &mut Tx, user: &User) -> Result<String, DbError> {
    let id = personal_id(&user.id);
    if !user.runner_of.is_empty() {
        return Ok(id);
    }
    let now = state.now();
    if tx.query_one("SELECT id FROM orgs WHERE id = $1", &[(&id).into()])?.is_none() {
        tx.execute(
            "INSERT INTO orgs (id, name, personal_of, plan, region, billing_ref, created_by, created_at) VALUES ($1, 'Personal', $2, 'free', 'eu', '', $2, $3) ON CONFLICT (id) DO NOTHING",
            &[(&id).into(), (&user.id).into(), now.into()],
        )?;
        tx.execute("INSERT INTO org_members (org_id, user_id, role, created_at) VALUES ($1, $2, 'owner', $3) ON CONFLICT (org_id, user_id) DO NOTHING", &[(&id).into(), (&user.id).into(), now.into()])?;
        tx.execute(
            "INSERT INTO entitlements (org_id, plan, seats, limits, status, period_end, updated_at) VALUES ($1, 'free', 0, '{}', 'active', 0, $2) ON CONFLICT (org_id) DO NOTHING",
            &[(&id).into(), now.into()],
        )?;
    }
    let pending = tx.query("SELECT org_id, role FROM org_invites WHERE email = $1", &[(&user.email).into()])?;
    for p in pending {
        tx.execute("INSERT INTO org_members (org_id, user_id, role, created_at) VALUES ($1, $2, $3, $4) ON CONFLICT (org_id, user_id) DO NOTHING", &[p.text(0).into(), (&user.id).into(), p.text(1).into(), now.into()])?;
        tx.execute("DELETE FROM org_invites WHERE org_id = $1 AND email = $2", &[p.text(0).into(), (&user.email).into()])?;
        audit::record(tx, state, Event::new(&chain(&p.text(0)), "org_invite_accepted", Some(user)).target(&user.id).detail(json!({ "role": p.text(1) })))?;
    }
    Ok(id)
}

/// The caller's org as they see it.
pub struct Org {
    pub id: String,
    pub name: String,
    pub personal: bool,
    pub role: OrgRole,
    pub created_at: i64,
}

/// `org` as `user` sees it; 404 when they aren't a member.
pub fn org_membership(tx: &mut Tx, org: &str, user: &User) -> Result<Org, ApiError> {
    let row = tx.query_one(
        "SELECT o.id, o.name, o.personal_of, m.role, o.created_at FROM orgs o JOIN org_members m ON m.org_id = o.id WHERE o.id = $1 AND m.user_id = $2",
        &[org.into(), (&user.id).into()],
    )?;
    let row = row.ok_or_else(|| ApiError::not_found("no such organization"))?;
    Ok(Org { id: row.text(0), name: row.text(1), personal: !row.text(2).is_empty(), role: OrgRole::parse(&row.text(3))?, created_at: row.int(4) })
}

/// The signed-in person and their role in `org` (404 when not a member, 403 below `need`).
/// `session_only`: an access token isn't enough (changes to SSO, billing, admin actions).
pub fn org_caller(state: &State, tx: &mut Tx, request: &Request, org: &str, need_role: OrgRole, session_only: bool) -> Result<(User, Org), ApiError> {
    let user = person(state, tx, request)?;
    let o = org_membership(tx, org, &user)?;
    if session_only && crate::api::bearer(request).is_some_and(|t| t.starts_with(crate::tokens::PREFIX)) {
        return Err(ApiError::forbidden("this needs a signed-in session, not an access token"));
    }
    need(&o, need_role)?;
    Ok((user, o))
}

fn need(org: &Org, role: OrgRole) -> Result<(), ApiError> {
    if org.role < role {
        return Err(ApiError::forbidden(format!("this needs the {} role in the organization", role.as_str())));
    }
    Ok(())
}

fn not_personal(org: &Org) -> Result<(), ApiError> {
    if org.personal {
        return Err(ApiError::bad("a personal organization has only you; share a workspace, or create an organization for a team"));
    }
    Ok(())
}

/// The signed-in person (runner credentials have no orgs).
fn person(state: &State, tx: &mut Tx, request: &Request) -> Result<User, ApiError> {
    let user = authenticate(state, tx, request)?;
    user.person()?;
    ensure_personal(state, tx, &user)?;
    Ok(user)
}

/// The org a new workspace goes into: `requested` (the caller must be a member) or the caller's
/// personal org.
pub fn org_for_new_workspace(state: &State, tx: &mut Tx, user: &User, requested: Option<&str>) -> Result<String, ApiError> {
    let personal = ensure_personal(state, tx, user)?;
    match requested.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(personal),
        Some(org) => Ok(org_membership(tx, org, user)?.id),
    }
}

fn valid_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
        return Err(ApiError::bad(format!("an organization name is 1 to {MAX_NAME} characters")));
    }
    Ok(name.to_string())
}

fn org_json(tx: &mut Tx, o: &Org) -> Result<Json, ApiError> {
    let e = limits::entitlement(tx, &o.id)?;
    let members = tx.query_one("SELECT COUNT(*) FROM org_members WHERE org_id = $1", &[(&o.id).into()])?.map_or(0, |r| r.int(0));
    let workspaces = tx.query_one("SELECT COUNT(*) FROM workspaces WHERE org_id = $1", &[(&o.id).into()])?.map_or(0, |r| r.int(0));
    Ok(json!({
        "id": o.id,
        "name": o.name,
        "personal": o.personal,
        "role": o.role.as_str(),
        "plan": e.plan,
        "status": e.status,
        "seats": e.seats,
        "region": "eu",
        "createdAt": o.created_at,
        "members": members,
        "workspaces": workspaces,
    }))
}

/// `GET /v1/orgs`: the caller's orgs, personal first.
pub fn list(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let ids: Vec<String> = tx
            .query("SELECT o.id FROM orgs o JOIN org_members m ON m.org_id = o.id WHERE m.user_id = $1 ORDER BY CASE WHEN o.personal_of = '' THEN 1 ELSE 0 END, o.created_at, o.id", &[(&user.id).into()])?
            .iter()
            .map(|r| r.text(0))
            .collect();
        let mut out = Vec::new();
        for id in ids {
            let o = org_membership(tx, &id, &user)?;
            out.push(org_json(tx, &o)?);
        }
        Ok((200, json!({ "orgs": out })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewOrg {
    name: String,
}

/// `POST /v1/orgs {"name"}` → the new org (the caller owns it; Free until a plan is bought).
pub fn create(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: NewOrg = body(request)?;
    let name = valid_name(&input.name)?;
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let owned = tx
            .query_one("SELECT COUNT(*) FROM org_members m JOIN orgs o ON o.id = m.org_id WHERE m.user_id = $1 AND m.role = 'owner' AND o.personal_of = ''", &[(&user.id).into()])?
            .map_or(0, |r| r.int(0));
        if owned >= MAX_OWNED_ORGS {
            return Err(ApiError::forbidden(format!("an account owns at most {MAX_OWNED_ORGS} organizations")));
        }
        let id = random_hex::<16>();
        let now = state.now();
        tx.execute(
            "INSERT INTO orgs (id, name, personal_of, plan, region, billing_ref, created_by, created_at) VALUES ($1, $2, '', 'free', 'eu', '', $3, $4)",
            &[(&id).into(), (&name).into(), (&user.id).into(), now.into()],
        )?;
        tx.execute("INSERT INTO org_members (org_id, user_id, role, created_at) VALUES ($1, $2, 'owner', $3)", &[(&id).into(), (&user.id).into(), now.into()])?;
        tx.execute("INSERT INTO entitlements (org_id, plan, seats, limits, status, period_end, updated_at) VALUES ($1, 'free', 0, '{}', 'active', 0, $2)", &[(&id).into(), now.into()])?;
        audit::record(tx, state, Event::new(&chain(&id), "org_created", Some(&user)).target(&id).detail(json!({ "name": name })))?;
        let o = org_membership(tx, &id, &user)?;
        Ok((201, org_json(tx, &o)?))
    })
}

/// `GET /v1/orgs/:org`.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let o = org_membership(tx, org, &user)?;
        Ok((200, org_json(tx, &o)?))
    })
}

/// `PUT /v1/orgs/:org {"name"}` (admin).
pub fn rename(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    let input: NewOrg = body(request)?;
    let name = valid_name(&input.name)?;
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let o = org_membership(tx, org, &user)?;
        need(&o, OrgRole::Admin)?;
        not_personal(&o)?;
        tx.execute("UPDATE orgs SET name = $1 WHERE id = $2", &[(&name).into(), org.into()])?;
        audit::record(tx, state, Event::new(&chain(org), "org_renamed", Some(&user)).target(org).detail(json!({ "from": o.name, "to": name })))?;
        let o = org_membership(tx, org, &user)?;
        Ok((200, org_json(tx, &o)?))
    })
}

/// `DELETE /v1/orgs/:org` (owner): only an org without workspaces (move or delete them first).
pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let o = org_membership(tx, org, &user)?;
        need(&o, OrgRole::Owner)?;
        not_personal(&o)?;
        let n = tx.query_one("SELECT COUNT(*) FROM workspaces WHERE org_id = $1", &[org.into()])?.map_or(0, |r| r.int(0));
        if n > 0 {
            return Err(ApiError::conflict("org_not_empty", format!("the organization still owns {n} workspace(s): delete them or move them first"), json!({ "workspaces": n })));
        }
        for table in ["org_invites", "org_members", "usage_counters", "entitlements"] {
            tx.execute(&format!("DELETE FROM {table} WHERE org_id = $1"), &[org.into()])?;
        }
        tx.execute("DELETE FROM orgs WHERE id = $1", &[org.into()])?;
        audit::record(tx, state, Event::new(&chain(org), "org_deleted", Some(&user)).target(org).detail(json!({ "name": o.name })))?;
        Ok((204, Json::Null))
    })
}

/// `GET /v1/orgs/:org/members`: members and pending invites.
pub fn members(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        org_membership(tx, org, &user)?;
        let rows = tx.query("SELECT m.user_id, u.email, m.role, m.created_at FROM org_members m JOIN users u ON u.id = m.user_id WHERE m.org_id = $1 ORDER BY u.email", &[org.into()])?;
        let members: Vec<Json> = rows.iter().map(|r| json!({ "userId": r.text(0), "email": r.text(1), "role": r.text(2), "since": r.int(3) })).collect();
        let rows = tx.query("SELECT email, role, created_at FROM org_invites WHERE org_id = $1 ORDER BY email", &[org.into()])?;
        let invites: Vec<Json> = rows.iter().map(|r| json!({ "email": r.text(0), "role": r.text(1), "createdAt": r.int(2) })).collect();
        Ok((200, json!({ "members": members, "invites": invites })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddMember {
    email: String,
    role: String,
}

/// `POST /v1/orgs/:org/members {"email","role"}` (admin; an owner only by an owner): adds an
/// account, or invites an address that has none yet (it joins when it first signs in; mailed when
/// the server sends mail). Either takes a seat.
pub fn add_member(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    let input: AddMember = body(request)?;
    let role = OrgRole::parse(&input.role)?;
    let email = normalize_email(&input.email)?;
    let now = state.now();
    let (reply, mail) = state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let o = org_membership(tx, org, &user)?;
        need(&o, OrgRole::Admin)?;
        not_personal(&o)?;
        if role == OrgRole::Owner {
            need(&o, OrgRole::Owner)?;
        }
        let existing = tx.query_one("SELECT id FROM users WHERE email = $1 AND runner_of = ''", &[(&email).into()])?;
        match existing {
            Some(r) => {
                let id = r.text(0);
                if tx.query_one("SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2", &[org.into(), (&id).into()])?.is_some() {
                    return Err(ApiError::conflict("already_member", "already a member of the organization", json!({})));
                }
                limits::check_seat(state, tx, org, &id, true)?;
                tx.execute("INSERT INTO org_members (org_id, user_id, role, created_at) VALUES ($1, $2, $3, $4)", &[org.into(), (&id).into(), role.as_str().into(), now.into()])?;
                audit::record(tx, state, Event::new(&chain(org), "org_member_added", Some(&user)).target(&id).detail(json!({ "email": email, "role": role.as_str() })))?;
                Ok(((201, json!({ "status": "added", "userId": id, "email": email, "role": role.as_str() })), None))
            }
            None => {
                if tx.query_one("SELECT role FROM org_invites WHERE org_id = $1 AND email = $2", &[org.into(), (&email).into()])?.is_some() {
                    return Err(ApiError::conflict("already_invited", "this address is already invited", json!({})));
                }
                limits::check_invite(state, tx, org)?;
                limits::check_seat(state, tx, org, &format!("invite:{email}"), true)?;
                tx.execute("INSERT INTO org_invites (org_id, email, role, created_by, created_at) VALUES ($1, $2, $3, $4, $5)", &[org.into(), (&email).into(), role.as_str().into(), (&user.id).into(), now.into()])?;
                limits::meter(state, tx, org, "invites", 1)?;
                let mailed = state.mailer.enabled();
                audit::record(tx, state, Event::new(&chain(org), "org_invite_created", Some(&user)).target(&email).detail(json!({ "role": role.as_str(), "mailed": mailed })))?;
                let mail = mailed.then(|| Mail {
                    to: email.clone(),
                    subject: format!("You're invited to {} on HITOP Cloud", o.name),
                    body: format!(
                        "{} added you to the organization {} on HITOP Cloud as {}.\n\nSign in with this address ({email}) at {} or in hitop (hitop sync login) and you are a member.\n\nIf you don't know the sender, ignore this mail.\n",
                        user.email,
                        o.name,
                        role.as_str(),
                        state.config.portal_url
                    ),
                });
                Ok(((201, json!({ "status": "invited", "email": email, "role": role.as_str(), "mailed": mailed })), mail))
            }
        }
    })?;
    if let Some(m) = mail {
        state.mailer.send(m);
    }
    Ok(reply)
}

/// `DELETE /v1/orgs/:org/invites/:email` (admin).
pub fn delete_invite(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    let email = percent_encoding::percent_decode_str(params.get("email").unwrap_or_default()).decode_utf8_lossy().trim().to_lowercase();
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let o = org_membership(tx, org, &user)?;
        need(&o, OrgRole::Admin)?;
        if tx.execute("DELETE FROM org_invites WHERE org_id = $1 AND email = $2", &[org.into(), (&email).into()])? == 0 {
            return Err(ApiError::not_found("no such invite"));
        }
        audit::record(tx, state, Event::new(&chain(org), "org_invite_deleted", Some(&user)).target(&email))?;
        Ok((204, Json::Null))
    })
}

fn owners(tx: &mut Tx, org: &str) -> Result<i64, DbError> {
    Ok(tx.query_one("SELECT COUNT(*) FROM org_members WHERE org_id = $1 AND role = 'owner'", &[org.into()])?.map_or(0, |r| r.int(0)))
}

fn member_role(tx: &mut Tx, org: &str, user: &str) -> Result<OrgRole, ApiError> {
    let row = tx.query_one("SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2", &[org.into(), user.into()])?;
    OrgRole::parse(&row.ok_or_else(|| ApiError::not_found("not a member of the organization"))?.text(0))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetRole {
    role: String,
}

/// `PUT /v1/orgs/:org/members/:user {"role"}` (admin; to or from owner only by an owner).
pub fn set_role(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    let target = params.get("user").unwrap_or_default().to_string();
    let input: SetRole = body(request)?;
    let role = OrgRole::parse(&input.role)?;
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let o = org_membership(tx, org, &user)?;
        need(&o, OrgRole::Admin)?;
        not_personal(&o)?;
        let from = member_role(tx, org, &target)?;
        if from == OrgRole::Owner || role == OrgRole::Owner {
            need(&o, OrgRole::Owner)?;
        }
        if from == OrgRole::Owner && role != OrgRole::Owner && owners(tx, org)? <= 1 {
            return Err(ApiError::bad("an organization keeps at least one owner"));
        }
        tx.execute("UPDATE org_members SET role = $1 WHERE org_id = $2 AND user_id = $3", &[role.as_str().into(), org.into(), (&target).into()])?;
        audit::record(tx, state, Event::new(&chain(org), "org_role_changed", Some(&user)).target(&target).detail(json!({ "from": from.as_str(), "to": role.as_str() })))?;
        Ok((200, json!({ "userId": target, "role": role.as_str() })))
    })
}

/// `DELETE /v1/orgs/:org/members/:user` (admin, or the member leaving; an owner only by an owner).
/// Workspace memberships stay: removing someone from a workspace rotates its key, which only its
/// owner's client can do.
pub fn remove_member(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    let target = params.get("user").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let o = org_membership(tx, org, &user)?;
        not_personal(&o)?;
        let role = member_role(tx, org, &target)?;
        if target != user.id {
            need(&o, if role == OrgRole::Owner { OrgRole::Owner } else { OrgRole::Admin })?;
        }
        if role == OrgRole::Owner && owners(tx, org)? <= 1 {
            return Err(ApiError::bad("an organization keeps at least one owner"));
        }
        tx.execute("DELETE FROM org_members WHERE org_id = $1 AND user_id = $2", &[org.into(), (&target).into()])?;
        audit::record(tx, state, Event::new(&chain(org), "org_member_removed", Some(&user)).target(&target).detail(json!({ "role": role.as_str() })))?;
        Ok((204, Json::Null))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrgWorkspace {
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    wraps: Vec<crate::workspaces::WrapIn>,
}

/// `POST /v1/orgs/:org/workspaces` (member): `{"id","wraps"}` creates a workspace in the org (as
/// `POST /v1/workspaces` with `org`); `{"workspace"}` moves a workspace the caller owns into it.
pub fn workspaces(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let input: OrgWorkspace = body(request)?;
    let Some(ws) = input.workspace else {
        let id = input.id.ok_or_else(|| ApiError::bad("send {\"id\",\"wraps\"} to create a workspace or {\"workspace\"} to move one"))?;
        return crate::workspaces::create_with(state, request, crate::workspaces::NewWorkspace { id, wraps: input.wraps, org: Some(org) });
    };
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let o = org_membership(tx, &org, &user)?;
        let w = membership(tx, &ws, &user)?;
        require(&w, Role::Owner)?;
        let from = limits::org_of_workspace(tx, &ws)?;
        if from == org {
            return Ok((200, json!({ "workspace": ws, "org": org })));
        }
        limits::check_move(state, tx, &org, &ws)?;
        tx.execute("UPDATE workspaces SET org_id = $1 WHERE id = $2", &[(&org).into(), (&ws).into()])?;
        let detail = json!({ "workspace": ws, "from": from, "to": org });
        audit::record(tx, state, Event::new(&ws, "workspace_moved", Some(&user)).target(&org).detail(detail.clone()))?;
        audit::record(tx, state, Event::new(&chain(&org), "workspace_moved", Some(&user)).target(&ws).detail(detail.clone()))?;
        if !from.is_empty() {
            audit::record(tx, state, Event::new(&chain(&from), "workspace_moved", Some(&user)).target(&ws).detail(detail))?;
        }
        Ok((200, json!({ "workspace": ws, "org": o.id })))
    })
}

fn limits_json(state: &State, tx: &mut Tx, org: &str) -> Result<Json, ApiError> {
    crate::billing::expire(state, tx, Some(org))?;
    let e = limits::entitlement(tx, org)?;
    let enforced = state.config.plan_limits;
    let limits = if enforced { e.limits().json() } else { limits::Limits { seats: None, guest_viewers: 0, workspaces: None, storage_bytes: None, devices: None, runners: None, invites_per_day: None }.json() };
    Ok(json!({
        "org": org,
        "plan": e.plan,
        "planInForce": e.plan_in_force(),
        "status": e.status,
        "seats": e.seats,
        "periodEnd": e.period_end,
        "grandfathered": e.overrides.get("grandfathered").and_then(Json::as_bool).unwrap_or(false),
        "enforced": enforced,
        "limits": limits,
        "upgradeUrl": format!("{}orgs/{org}/plan", state.config.portal_url),
        "runs": crate::runs::plan_json(state, tx, org)?,
    }))
}

/// `GET /v1/orgs/:org/limits` (member): the plan and the limits in force (`null` = none; all
/// `null` with `enforced: false` on a self-hosted server).
pub fn get_limits(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        org_membership(tx, org, &user)?;
        Ok((200, limits_json(state, tx, org)?))
    })
}

/// `GET /v1/orgs/:org/usage` (member): what the org uses, next to its limits.
pub fn get_usage(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default();
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        org_membership(tx, org, &user)?;
        let u = limits::usage(state, tx, org)?;
        let mut out = limits_json(state, tx, org)?;
        out["usage"] = u.json();
        out["usage"]["runsToday"] = crate::runs::runs_today(state, tx, org)?.into();
        Ok((200, out))
    })
}
