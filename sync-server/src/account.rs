//! Deleting an account (`DELETE /v1/me {"confirm": "<the account's address>"}`): what an app that
//! creates accounts must offer (App Store guideline 5.1.1(v)). The person starts it in hitop
//! (Account → Delete account) and it is done at once; nothing waits for a person at HITOP.
//!
//! **Who may.** A signed-in session of a person (not an access token, not a runner credential),
//! which sends the account's address as `confirm`. The address is no secret to whoever holds the
//! session (`GET /v1/me` shows it): the field makes deletion a deliberate request, while
//! refusing access tokens keeps a leaked automation token from deleting anything.
//!
//! **What goes.** The account and everything that is only the person's: sessions, access tokens,
//! passkeys, password, sign-in links, linked Google/GitHub/Apple/SSO identities, device keys (and
//! every workspace key wrapped for them), key bundles, the personal library with its history, git
//! links, monitors and mock servers, pending invites to the address. Shared things follow the rules
//! below, decided before anything is deleted:
//!
//! - **Workspaces** (shared collections, end-to-end encrypted). One where the person is the only
//!   member (runner credentials don't count) is deleted with its blobs, runs and runners. One with
//!   other members loses the person's membership and wraps, and its owners' clients are asked to
//!   rotate its key (`rotation_requested_by` = `account deleted`; the server can't rotate: it never
//!   holds the key). The only owner of a workspace other people use can't delete the account:
//!   409 `transfer_ownership_first` lists it; they make another member an owner or remove the
//!   others first. A surviving workspace filed under an org that is deleted here (the personal org,
//!   or a team org the person alone is in) moves to the personal org of its first remaining owner
//!   (limits aren't checked: as everywhere, an org over a limit only can't add more).
//! - **Organizations.** The personal org, and a team org the person alone is in, are deleted with
//!   their data. In a team org with other members the person's membership ends; its only owner
//!   gets the same 409 (make another member an owner, or remove the members first).
//! - **Billing.** A subscription of an org deleted here is cancelled at Stripe before anything is
//!   deleted (billing.rs `cancel_for_deletion`): no further invoice. A team org that stays keeps its
//!   subscription; its other owners manage it. Stripe keeps its own records of past payments.
//! - **Sign in with Apple.** Every Apple refresh token kept for the account (apple.rs) is revoked
//!   at Apple first, as Apple requires.
//!
//! Apple, then Stripe, are called outside any database transaction: a first transaction plans
//! (and refuses), then the calls, then a second transaction plans again, refuses if anything changed
//! meanwhile, and deletes. A failed call refuses with 503 `try_again` and deletes nothing; both
//! calls are safe to repeat.
//!
//! **What stays.** The audit log (append-only, hash-chained) keeps its rows, with the address,
//! truncated IP and user agent they hold, until the retention purge
//! (`HITOP_SYNC_AUDIT_RETENTION_DAYS`, default 365); the deletion itself is the row
//! `account_deleted` on the account chain. Encrypted backups keep the old database until they
//! expire (deploy/backup.sh). An org that stays keeps what members put in it. Billing webhook
//! records hold ids, never names or addresses. A short mail says the account was deleted, when
//! the server sends mail.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{bearer, body, ApiError, ApiResult, State};
use crate::apple::KeptToken;
use crate::audit::{self, Event};
use crate::auth::{authenticate, User};
use crate::db::{DbError, Tx};
use crate::mail::Mail;
use crate::orgs;

/// `rotation_requested_by` of a workspace someone left by deleting their account.
pub const ROTATION_MARKER: &str = "account deleted";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirm {
    #[serde(default)]
    confirm: String,
}

/// What deleting an account does, decided before anything changes.
#[derive(Debug, Default, PartialEq, Eq)]
struct Plan {
    delete_workspaces: Vec<String>,
    leave_workspaces: Vec<String>,
    /// (workspace, the org it moves to)
    move_workspaces: Vec<(String, String)>,
    delete_orgs: Vec<String>,
    leave_orgs: Vec<String>,
    /// Stripe subscription ids of orgs in `delete_orgs`.
    subscriptions: Vec<String>,
    apple: Vec<KeptToken>,
}

fn refuse_blocked(blockers: Vec<Json>) -> ApiError {
    // Named where the server knows a name (organizations); shared collections have none here
    // (their names are encrypted), so they are counted.
    let mut what: Vec<String> = blockers
        .iter()
        .filter(|b| b["kind"] == "org")
        .map(|b| format!("the organization {}", b["name"].as_str().unwrap_or("")))
        .collect();
    let shared = blockers.iter().filter(|b| b["kind"] == "workspace").count();
    if shared > 0 {
        what.push(if shared == 1 { "1 shared collection".to_string() } else { format!("{shared} shared collections") });
    }
    ApiError::conflict(
        "transfer_ownership_first",
        format!(
            "You are the only owner of {} that other people use: make another member an owner, or remove the other members, then delete the account.",
            what.join(" and ")
        ),
        json!({ "blockers": blockers }),
    )
}

fn try_again() -> ApiError {
    ApiError::new(
        503,
        "try_again",
        "The account can't be deleted right now: nothing was deleted. Try again in a few minutes.",
    )
}

fn plan(tx: &mut Tx, user: &User) -> Result<Plan, ApiError> {
    let mut p = Plan::default();
    let mut blockers = Vec::new();
    let personal = orgs::personal_id(&user.id);

    // Workspaces: (id, the remaining owners, its org) of each one that stays.
    let mut staying: Vec<(String, Vec<String>, String)> = Vec::new();
    let rows = tx.query("SELECT m.workspace_id, m.role, w.org_id FROM members m JOIN workspaces w ON w.id = m.workspace_id WHERE m.user_id = $1 ORDER BY m.workspace_id", &[(&user.id).into()])?;
    for r in rows {
        let (ws, org) = (r.text(0), r.text(2));
        let others = tx.query(
            "SELECT m.user_id, m.role FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 AND m.user_id <> $2 AND u.runner_of = '' ORDER BY m.user_id",
            &[(&ws).into(), (&user.id).into()],
        )?;
        if others.is_empty() {
            p.delete_workspaces.push(ws);
            continue;
        }
        let owners: Vec<String> = others
            .iter()
            .filter(|o| o.text(1) == "owner")
            .map(|o| o.text(0))
            .collect();
        if owners.is_empty() {
            blockers.push(json!({ "kind": "workspace", "id": ws, "members": others.len() }));
            continue;
        }
        p.leave_workspaces.push(ws.clone());
        staying.push((ws, owners, org));
    }

    // Organizations.
    let mut orgs_of: Vec<(String, String, String, String)> = tx
        .query("SELECT o.id, o.name, o.personal_of, m.role FROM org_members m JOIN orgs o ON o.id = m.org_id WHERE m.user_id = $1 ORDER BY o.id", &[(&user.id).into()])?
        .iter()
        .map(|r| (r.text(0), r.text(1), r.text(2), r.text(3)))
        .collect();
    if !orgs_of.iter().any(|o| o.0 == personal)
        && tx
            .query_one("SELECT id FROM orgs WHERE id = $1", &[(&personal).into()])?
            .is_some()
    {
        orgs_of.push((
            personal.clone(),
            String::new(),
            user.id.clone(),
            "owner".into(),
        ));
    }
    for (id, name, personal_of, role) in orgs_of {
        if !personal_of.is_empty() {
            if personal_of == user.id {
                p.delete_orgs.push(id);
            }
            continue;
        }
        let others = tx.query(
            "SELECT user_id, role FROM org_members WHERE org_id = $1 AND user_id <> $2",
            &[(&id).into(), (&user.id).into()],
        )?;
        if others.is_empty() {
            p.delete_orgs.push(id);
        } else if role == "owner" && !others.iter().any(|o| o.text(1) == "owner") {
            blockers
                .push(json!({ "kind": "org", "id": id, "name": name, "members": others.len() }));
        } else {
            p.leave_orgs.push(id);
        }
    }
    p.delete_orgs.sort();

    // A staying workspace of a deleted org moves to its first remaining owner's personal org.
    for (ws, owners, org) in &staying {
        if p.delete_orgs.contains(org) {
            p.move_workspaces
                .push((ws.clone(), orgs::personal_id(&owners[0])));
        }
    }
    // Any other workspace of a deleted org (one the person isn't in) would be left without an org.
    for org in &p.delete_orgs {
        for w in tx.query(
            "SELECT id FROM workspaces WHERE org_id = $1 ORDER BY id",
            &[org.into()],
        )? {
            let ws = w.text(0);
            if !p.delete_workspaces.contains(&ws)
                && !p.move_workspaces.iter().any(|(m, _)| *m == ws)
            {
                let owner = tx.query_one("SELECT MIN(m.user_id) FROM members m JOIN users u ON u.id = m.user_id WHERE m.workspace_id = $1 AND m.role = 'owner' AND u.runner_of = ''", &[(&ws).into()])?.map(|r| r.text(0)).unwrap_or_default();
                if owner.is_empty() {
                    blockers.push(json!({ "kind": "workspace", "id": ws, "members": 0 }));
                } else {
                    p.move_workspaces.push((ws, orgs::personal_id(&owner)));
                }
            }
        }
    }
    if !blockers.is_empty() {
        return Err(refuse_blocked(blockers));
    }
    for org in &p.delete_orgs {
        if let Some(r) = tx.query_one(
            "SELECT ls_subscription_id FROM entitlements WHERE org_id = $1",
            &[org.into()],
        )? {
            if !r.text(0).is_empty() {
                p.subscriptions.push(r.text(0));
            }
        }
    }
    p.apple = crate::apple::tokens_of(tx, &user.id)?;
    Ok(p)
}

/// The org a workspace moves to exists (a personal org is made on first use).
fn ensure_personal_of(state: &State, tx: &mut Tx, org: &str) -> Result<(), ApiError> {
    let Some(id) = org.strip_prefix("personal-") else {
        return Ok(());
    };
    if let Some(r) = tx.query_one(
        "SELECT id, email, runner_of FROM users WHERE id = $1",
        &[id.into()],
    )? {
        orgs::ensure_personal(
            state,
            tx,
            &User {
                id: r.text(0),
                email: r.text(1),
                runner_of: r.text(2),
            },
        )?;
    }
    Ok(())
}

fn execute(
    state: &State,
    tx: &mut Tx,
    user: &User,
    p: &Plan,
    cancelled: usize,
    apple: crate::apple::Revoked,
) -> Result<Json, ApiError> {
    let now = state.now();
    let uid = user.id.as_str();
    for ws in &p.leave_workspaces {
        tx.execute(
            "DELETE FROM members WHERE workspace_id = $1 AND user_id = $2",
            &[ws.into(), uid.into()],
        )?;
        tx.execute("DELETE FROM wraps WHERE workspace_id = $1 AND key_id IN (SELECT id FROM device_keys WHERE user_id = $2)", &[ws.into(), uid.into()])?;
        tx.execute(
            "DELETE FROM env_share_wraps WHERE workspace_id = $1 AND user_id = $2",
            &[ws.into(), uid.into()],
        )?;
        tx.execute("UPDATE workspaces SET rotation_requested_at = $1, rotation_requested_by = $2 WHERE id = $3", &[now.into(), ROTATION_MARKER.into(), ws.into()])?;
        audit::record(
            tx,
            state,
            Event::new(ws, "member_removed", Some(user))
                .target(uid)
                .detail(json!({ "email": user.email, "reason": "account_deleted" })),
        )?;
        audit::record(
            tx,
            state,
            Event::new(ws, "rotation_requested", None)
                .target(ws)
                .detail(json!({ "reason": "account_deleted" })),
        )?;
    }
    for (ws, to) in &p.move_workspaces {
        ensure_personal_of(state, tx, to)?;
        let from = crate::limits::org_of_workspace(tx, ws)?;
        tx.execute(
            "UPDATE workspaces SET org_id = $1 WHERE id = $2",
            &[to.into(), ws.into()],
        )?;
        let detail =
            json!({ "workspace": ws, "from": from, "to": to, "reason": "account_deleted" });
        audit::record(
            tx,
            state,
            Event::new(ws, "workspace_moved", Some(user))
                .target(to)
                .detail(detail.clone()),
        )?;
        audit::record(
            tx,
            state,
            Event::new(&orgs::chain(to), "workspace_moved", Some(user))
                .target(ws)
                .detail(detail),
        )?;
    }
    for ws in &p.delete_workspaces {
        let runners = crate::workspaces::delete_rows(tx, ws)?;
        audit::record(
            tx,
            state,
            Event::new(ws, "workspace_deleted", Some(user))
                .target(ws)
                .detail(json!({ "runnersRemoved": runners, "reason": "account_deleted" })),
        )?;
    }
    for org in &p.leave_orgs {
        let role = tx
            .query_one(
                "SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2",
                &[org.into(), uid.into()],
            )?
            .map(|r| r.text(0))
            .unwrap_or_default();
        tx.execute(
            "DELETE FROM org_members WHERE org_id = $1 AND user_id = $2",
            &[org.into(), uid.into()],
        )?;
        audit::record(
            tx,
            state,
            Event::new(&orgs::chain(org), "org_member_removed", Some(user))
                .target(uid)
                .detail(json!({ "role": role, "reason": "account_deleted" })),
        )?;
    }
    for org in &p.delete_orgs {
        let left = tx
            .query_one(
                "SELECT COUNT(*) FROM workspaces WHERE org_id = $1",
                &[org.into()],
            )?
            .map_or(0, |r| r.int(0));
        if left > 0 {
            return Err(DbError(format!(
                "account deletion: org {org} still owns {left} workspace(s)"
            ))
            .into());
        }
        orgs::delete_rows(tx, org)?;
        audit::record(
            tx,
            state,
            Event::new(&orgs::chain(org), "org_deleted", Some(user))
                .target(org)
                .detail(json!({ "reason": "account_deleted" })),
        )?;
    }
    // What is the person's alone. Most of these go with the users row by foreign key too; they are
    // deleted by name so nothing depends on the database enforcing that.
    tx.execute(
        "DELETE FROM env_share_wraps WHERE user_id = $1",
        &[uid.into()],
    )?;
    tx.execute(
        "DELETE FROM wraps WHERE key_id IN (SELECT id FROM device_keys WHERE user_id = $1)",
        &[uid.into()],
    )?;
    tx.execute("DELETE FROM workspace_excluded_keys WHERE key_id IN (SELECT id FROM device_keys WHERE user_id = $1)", &[uid.into()])?;
    tx.execute(
        "DELETE FROM monitor_runs WHERE monitor_id IN (SELECT id FROM monitors WHERE user_id = $1)",
        &[uid.into()],
    )?;
    for table in [
        "sessions",
        "api_tokens",
        "passkeys",
        "password_credentials",
        "recovery_blobs",
        "oidc_identities",
        "apple_tokens",
        "device_keys",
        "library_items",
        "request_history",
        "git_links",
        "git_installations",
        "monitors",
        "mocks",
        "members",
        "org_members",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE user_id = $1"),
            &[uid.into()],
        )?;
    }
    for table in ["magic_links", "password_tokens", "org_invites"] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE email = $1"),
            &[(&user.email).into()],
        )?;
    }
    tx.execute(
        "DELETE FROM invites WHERE email = $1 OR accepted_by = $2",
        &[(&user.email).into(), uid.into()],
    )?;
    // Who made or last changed what in things that stay (orgs, shared values, runs, settings):
    // no longer this person.
    for (table, column) in [
        ("orgs", "created_by"),
        ("invites", "created_by"),
        ("org_invites", "created_by"),
        ("runs", "uploader"),
        ("alert_settings", "updated_by"),
        ("env_shares", "created_by"),
        ("env_shares", "updated_by"),
        ("org_sso", "updated_by"),
        ("org_domains", "created_by"),
        ("org_audit_settings", "updated_by"),
        ("scim_tokens", "created_by"),
        ("workspace_excluded_keys", "excluded_by"),
        ("org_items", "updated_by"),
        ("workspaces", "rotation_requested_by"),
    ] {
        tx.execute(
            &format!("UPDATE {table} SET {column} = '' WHERE {column} = $1 OR {column} = $2"),
            &[uid.into(), (&user.email).into()],
        )?;
    }
    // A trial stays counted for the org and the mail domain (one per domain), not for the person.
    tx.execute(
        "UPDATE trials SET user_id = '' WHERE user_id = $1",
        &[uid.into()],
    )?;
    tx.execute("DELETE FROM users WHERE id = $1", &[uid.into()])?;
    let summary = json!({
        "deleted": true,
        "workspacesDeleted": p.delete_workspaces.len(),
        "workspacesLeft": p.leave_workspaces.len(),
        "workspacesMoved": p.move_workspaces.len(),
        "orgsDeleted": p.delete_orgs.len(),
        "orgsLeft": p.leave_orgs.len(),
        "subscriptionsCancelled": cancelled,
        "appleRevoked": apple.revoked,
        "appleNotRevoked": apple.skipped,
    });
    audit::record(
        tx,
        state,
        Event::new(audit::ACCOUNT, "account_deleted", Some(user))
            .target(uid)
            .detail(summary.clone()),
    )?;
    Ok(summary)
}

/// `DELETE /v1/me {"confirm": "<the account's address>"}` → 200 with what was deleted (module docs).
pub fn delete(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: Confirm = body(request)
        .map_err(|_| ApiError::bad("send {\"confirm\": \"<the account's email address>\"}"))?;
    if bearer(request).is_some_and(|t| t.starts_with(crate::tokens::PREFIX)) {
        return Err(ApiError::forbidden(
            "deleting the account needs a signed-in session, not an access token",
        ));
    }
    let check = |tx: &mut Tx| -> Result<(User, Plan), ApiError> {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        if input.confirm.trim().to_lowercase() != user.email.to_lowercase() {
            return Err(ApiError::new(
                400,
                "confirm_mismatch",
                "confirm must be this account's email address",
            ));
        }
        let p = plan(tx, &user)?;
        Ok((user, p))
    };
    let (user, first) = state.db.tx(check)?;

    // Outside any transaction: revoke Apple's tokens first (a refusal here leaves everything as it
    // was, the plan included), then end subscriptions (the last step that can fail).
    let apple = crate::apple::revoke(state, &first.apple).map_err(|e| {
        eprintln!("hitop-sync-server: account deletion of {}: Sign in with Apple revoke: {e}", user.id);
        try_again()
    })?;
    let mut cancelled = 0;
    if !first.subscriptions.is_empty() {
        if state.config.billing.is_none() {
            eprintln!("hitop-sync-server: account deletion of {}: subscriptions {:?} but billing isn't configured", user.id, first.subscriptions);
            return Err(try_again());
        }
        for sub in &first.subscriptions {
            match crate::billing::cancel_for_deletion(state, sub) {
                Ok(now) => cancelled += usize::from(now),
                Err(e) => {
                    eprintln!("hitop-sync-server: account deletion of {}: cancelling {sub}: {}", user.id, e.message);
                    return Err(try_again());
                }
            }
        }
    }

    let (summary, deleted_orgs) = state.db.tx(|tx| -> Result<(Json, bool), ApiError> {
        let (again, p) = check(tx)?;
        if again.id != user.id {
            return Err(ApiError::unauthorized());
        }
        // Subscriptions or Apple tokens may have come meanwhile; so may members.
        if p != first {
            return Err(ApiError::conflict(
                "changed",
                "Something changed while deleting: nothing was deleted. Try again.",
                json!({}),
            ));
        }
        let summary = execute(state, tx, &user, &p, cancelled, apple)?;
        Ok((summary, !p.delete_orgs.is_empty()))
    })?;
    if deleted_orgs {
        if let Err(e) = crate::orgsso::reload(state) {
            eprintln!("hitop-sync-server: after an account deletion: {e}");
        }
    }
    if state.mailer.enabled() {
        state.mailer.send(Mail {
            to: user.email.clone(),
            subject: "Your HITOP Cloud account was deleted".into(),
            body: format!(
                "The HITOP Cloud account {} was deleted at your request, with its synced library, devices and memberships.\n\nCollections and requests saved on your devices stay there. You can create a new account at any time.\n\nIf you didn't ask for this, reply to this mail.\n",
                user.email
            ),
        });
    }
    Ok((200, summary))
}
