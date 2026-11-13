//! Sign-in: email magic links and passkeys (WebAuthn, `webauthn-rs`), both ending in a bearer
//! session. Tokens (links and sessions) are 32 random bytes; the database keeps only their
//! SHA-256, so a copy of the database doesn't sign anyone in. Single sign-on (OpenID Connect)
//! is oidc.rs; it ends in the same sessions. Every sign-in, failed sign-in and sign-out goes to
//! the audit log (audit.rs).

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};
use webauthn_rs::prelude::{Passkey, PublicKeyCredential, RegisterPublicKeyCredential, Uuid};

use crate::api::{b64, bearer, body, new_token, random_hex, token_hash, ApiError, ApiResult, Ceremony, State};
use crate::audit::{self, Event};
use crate::db::Tx;
use crate::mail::Mail;

const CEREMONY_TTL: i64 = 5 * 60;
const MAX_CEREMONIES: usize = 10_000;
const MAX_PASSKEYS: i64 = 20;

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub email: String,
    /// For a runner account (a scoped credential for scheduled runs, teams.rs): the one
    /// workspace it reads. Empty for people.
    pub runner_of: String,
}

/// Runner accounts get addresses under this reserved domain (RFC 2606), so no mail reaches them.
pub const RUNNER_DOMAIN: &str = "runner.invalid";

impl User {
    /// Refuses runner accounts: they read one workspace and do nothing else.
    pub fn person(&self) -> Result<(), ApiError> {
        if self.runner_of.is_empty() {
            Ok(())
        } else {
            Err(ApiError::forbidden("a runner credential can only read its workspace"))
        }
    }
}

/// The signed-in user, from `Authorization: Bearer <session>`.
pub fn authenticate(state: &State, tx: &mut Tx, request: &Request) -> Result<User, ApiError> {
    let token = bearer(request).ok_or_else(ApiError::unauthorized)?;
    if token.starts_with(crate::tokens::PREFIX) {
        return crate::tokens::authenticate_token(state, tx, request, &token);
    }
    let row = tx.query_one(
        "SELECT u.id, u.email, u.runner_of, s.device_key_id FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_hash = $1 AND s.expires_at > $2",
        &[token_hash(&token).into(), state.now().into()],
    )?;
    let row = row.ok_or_else(ApiError::unauthorized)?;
    let user = User { id: row.text(0), email: row.text(1), runner_of: row.text(2) };
    // DD3: the device the client says it is (devices.rs).
    if let Some(device) = crate::devices::header_device(request) {
        crate::devices::seen(state, tx, &user.id, &token, &row.text(3), &device)?;
    }
    Ok(user)
}

pub fn normalize_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    let ok = email.len() <= 254 && !email.chars().any(|c| c.is_control() || c.is_whitespace()) && email.parse::<lettre::Address>().is_ok();
    if !ok || email.ends_with(&format!("@{RUNNER_DOMAIN}")) {
        return Err(ApiError::bad("email is not a valid address"));
    }
    Ok(email)
}

/// A new session for `user`, signed in by `method` (`email`, `passkey`, `sso`, `password`,
/// `password_reset`, `google`, `github`); audited.
pub fn new_session(state: &State, tx: &mut Tx, user: &User, method: &str) -> ApiResult {
    let token = new_token();
    let now = state.now();
    let expires = now + state.config.session_ttl_secs;
    tx.execute("DELETE FROM sessions WHERE expires_at <= $1", &[now.into()])?;
    tx.execute(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES ($1, $2, $3, $4)",
        &[token_hash(&token).into(), (&user.id).into(), now.into(), expires.into()],
    )?;
    audit::record(tx, state, Event::new(audit::ACCOUNT, "signin", Some(user)).target(&user.id).detail(json!({ "method": method })))?;
    Ok((200, json!({ "session": token, "expiresAt": expires, "user": { "id": user.id, "email": user.email } })))
}

/// Answer for an address that must sign in by SSO (`HITOP_SYNC_SSO_REQUIRED`).
fn sso_only(state: &State, email: &str, method: &str) -> ApiError {
    audit::record_alone(state, Event::new(audit::ACCOUNT, "signin_failed", None).email(email).detail(json!({ "method": method, "reason": "sso_required" })));
    ApiError::new(403, "sso_required", "this address signs in with single sign-on (hitop sync login --sso, or Sign in with SSO)")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailStart {
    email: String,
}

/// `POST /v1/auth/email {"email"}`: mails a sign-in link. The same answer whether or not the
/// address has an account (one is made when the link is used).
pub fn email_start(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    if !state.mailer.enabled() {
        return Err(ApiError::new(503, "mail_off", "email sign-in isn't configured on this server"));
    }
    let input: EmailStart = body(request)?;
    let email = normalize_email(&input.email)?;
    if crate::oidc::sso_required_for(state, &email) {
        return Err(sso_only(state, &email, "email"));
    }
    let token = new_token();
    let now = state.now();
    state.db.tx(|tx| -> Result<(), ApiError> {
        tx.execute("DELETE FROM magic_links WHERE expires_at < $1", &[(now - 86_400).into()])?;
        let hour_ago = now - 3600;
        let for_address = tx.query_one("SELECT COUNT(*) FROM magic_links WHERE email = $1 AND created_at > $2", &[(&email).into(), hour_ago.into()])?.map_or(0, |r| r.int(0));
        let total = tx.query_one("SELECT COUNT(*) FROM magic_links WHERE created_at > $1", &[hour_ago.into()])?.map_or(0, |r| r.int(0));
        if for_address >= state.config.links_per_email_hour || total >= state.config.links_per_hour {
            return Err(ApiError::new(429, "rate_limited", "too many sign-in links; try again in an hour"));
        }
        tx.execute(
            "INSERT INTO magic_links (token_hash, email, created_at, expires_at, used) VALUES ($1, $2, $3, $4, 0)",
            &[token_hash(&token).into(), (&email).into(), now.into(), (now + state.config.link_ttl_secs).into()],
        )?;
        Ok(())
    })?;
    let link = state.config.public_url.join("signin").map(|u| format!("{u}#{token}")).unwrap_or_default();
    let minutes = state.config.link_ttl_secs / 60;
    state.mailer.send(Mail {
        to: email,
        subject: "Sign in to hitop sync".into(),
        body: format!(
            "Open this link to sign in to hitop sync:\n\n{link}\n\nor paste this code into hitop:\n\n{token}\n\nIt works once, for {minutes} minutes. If you didn't ask for it, ignore this mail.\n"
        ),
    });
    Ok((202, json!({ "sent": true })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailVerify {
    token: String,
}

/// `POST /v1/auth/email/verify {"token"}` → a session. Each link works once.
pub fn email_verify(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: EmailVerify = body(request)?;
    let now = state.now();
    let result = state.db.tx(|tx| {
        let hash = token_hash(input.token.trim());
        let row = tx.query_one("SELECT email FROM magic_links WHERE token_hash = $1 AND used = 0 AND expires_at > $2", &[(&hash).into(), now.into()])?;
        let Some(row) = row else {
            return Err(ApiError::new(401, "invalid_link", "this sign-in link is invalid, used or expired"));
        };
        tx.execute("UPDATE magic_links SET used = 1 WHERE token_hash = $1", &[hash.into()])?;
        let email = row.text(0);
        // A link mailed before SSO became required for this address.
        if crate::oidc::sso_required_for(state, &email) {
            return Err(ApiError::new(403, "sso_required", format!("{email} signs in with single sign-on")));
        }
        let user = match tx.query_one("SELECT id FROM users WHERE email = $1", &[(&email).into()])? {
            Some(r) => User { id: r.text(0), email, runner_of: String::new() },
            None => {
                let user = User { id: Uuid::new_v4().to_string(), email, runner_of: String::new() };
                tx.execute("INSERT INTO users (id, email, created_at) VALUES ($1, $2, $3)", &[(&user.id).into(), (&user.email).into(), now.into()])?;
                user
            }
        };
        new_session(state, tx, &user, "email")
    });
    if let Err(e) = &result {
        audit::record_alone(state, Event::new(audit::ACCOUNT, "signin_failed", None).detail(json!({ "method": "email", "reason": e.code })));
    }
    result
}

/// `GET /v1/auth/methods`: what a sign-in page can offer on this server.
pub fn methods(state: &State, _: &Request, _: &PathParams) -> ApiResult {
    let mail = state.mailer.enabled();
    let mut sso = state.config.oidc.as_ref().map(|c| json!({ "name": c.name, "required": c.sso_required, "domains": c.allowed_domains }));
    // Orgs with their own SSO (orgsso.rs): offer "Sign in with SSO", which asks for the address.
    if state.org_sso.any() {
        let s = sso.get_or_insert_with(|| json!({ "name": "SSO", "required": false, "domains": [] }));
        s["orgSso"] = json!(true);
    }
    Ok((
        200,
        json!({
            "password": { "enabled": state.config.password.enabled, "signup": state.config.password.enabled && mail, "reset": state.config.password.enabled && mail },
            "emailCode": mail,
            "passkey": true,
            "sso": sso,
            "google": state.config.google.is_some(),
            "github": state.config.github.is_some(),
            "apple": state.config.apple.is_some(),
        }),
    ))
}

/// `POST /v1/auth/logout`: ends this session.
pub fn logout(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let token = bearer(request).ok_or_else(ApiError::unauthorized)?;
    state.db.tx(|tx| -> Result<(), ApiError> {
        if let Ok(user) = authenticate(state, tx, request) {
            audit::record(tx, state, Event::new(audit::ACCOUNT, "signout", Some(&user)).target(&user.id))?;
        }
        tx.execute("DELETE FROM sessions WHERE token_hash = $1", &[token_hash(&token).into()])?;
        Ok(())
    })?;
    Ok((204, Json::Null))
}

/// `GET /v1/me`: the account, its device keys and passkeys.
pub fn me(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        let keys = tx.query("SELECT id, name, public_key FROM device_keys WHERE user_id = $1 ORDER BY created_at, id", &[(&user.id).into()])?;
        let passkeys = tx.query("SELECT id, name FROM passkeys WHERE user_id = $1 ORDER BY created_at, id", &[(&user.id).into()])?;
        let recovery = tx.query_one("SELECT updated_at FROM recovery_blobs WHERE user_id = $1 AND method = 'phrase'", &[(&user.id).into()])?;
        let bundles = tx.query("SELECT method, updated_at FROM recovery_blobs WHERE user_id = $1", &[(&user.id).into()])?;
        let password = crate::password::status_of(tx, &user.id)?;
        Ok((
            200,
            json!({
                "id": user.id,
                "email": user.email,
                "keys": keys.iter().map(|r| json!({ "id": r.text(0), "name": r.text(1), "publicKey": b64(&r.bytes(2)) })).collect::<Vec<_>>(),
                // DD3: this server reads `X-Hitop-Device` and has `/v1/me/devices` (CORS allows
                // the header); browsers send it only to servers that say so.
                "devices": true,
                "passkeys": passkeys.iter().map(|r| json!({ "id": r.text(0), "name": r.text(1) })).collect::<Vec<_>>(),
                // Since C2: when a recovery phrase was last set (null: none). Only the phrase, as
                // hitop 1.2 and the 1.2 web app read it.
                "recovery": recovery.map(|r| json!({ "updatedAt": r.int(0) })),
                // Since C1b: whether the account has a password, and every key bundle by method.
                "password": password,
                "bundles": bundles.iter().map(|r| (r.text(0), json!({ "updatedAt": r.int(1) }))).collect::<serde_json::Map<_, _>>(),
            }),
        ))
    })
}

fn user_passkeys(tx: &mut Tx, user_id: &str) -> Result<Vec<Passkey>, ApiError> {
    let rows = tx.query("SELECT data FROM passkeys WHERE user_id = $1", &[user_id.into()])?;
    Ok(rows.iter().filter_map(|r| serde_json::from_str(&r.text(0)).ok()).collect())
}

fn remember(state: &State, ceremony: Ceremony) -> String {
    let id = random_hex::<16>();
    let now = state.now();
    let mut map = state.ceremonies.lock().unwrap_or_else(|e| e.into_inner());
    map.retain(|_, (expires, _)| *expires > now);
    if map.len() >= MAX_CEREMONIES {
        // Full: drop the oldest rather than refuse everyone.
        if let Some(oldest) = map.iter().min_by_key(|(_, (e, _))| *e).map(|(k, _)| k.clone()) {
            map.remove(&oldest);
        }
    }
    map.insert(id.clone(), (now + CEREMONY_TTL, ceremony));
    id
}

fn take(state: &State, id: &str) -> Result<Ceremony, ApiError> {
    let mut map = state.ceremonies.lock().unwrap_or_else(|e| e.into_inner());
    match map.remove(id) {
        Some((expires, c)) if expires > state.now() => Ok(c),
        _ => Err(ApiError::new(400, "unknown_challenge", "unknown or expired passkey challenge; start again")),
    }
}

/// `POST /v1/auth/passkey/register/start` (signed in): options for `navigator.credentials.create`.
pub fn passkey_register_start(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let (user, existing) = state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let existing = user_passkeys(tx, &user.id)?;
        Ok::<_, ApiError>((user, existing))
    })?;
    if existing.len() as i64 >= MAX_PASSKEYS {
        return Err(ApiError::bad("this account has the maximum number of passkeys"));
    }
    let uuid = Uuid::parse_str(&user.id).map_err(|_| ApiError::bad("account id isn't a UUID"))?;
    let exclude = existing.iter().map(|p| p.cred_id().clone()).collect::<Vec<_>>();
    let (options, reg) = state
        .webauthn
        .start_passkey_registration(uuid, &user.email, &user.email, Some(exclude))
        .map_err(|e| ApiError::bad(format!("passkey registration: {e}")))?;
    let id = remember(state, Ceremony::Register { user_id: user.id, state: reg });
    Ok((200, json!({ "challengeId": id, "options": options })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RegisterFinish {
    challenge_id: String,
    #[serde(default)]
    name: String,
    credential: RegisterPublicKeyCredential,
}

/// `POST /v1/auth/passkey/register/finish {"challengeId","name","credential"}` (signed in).
pub fn passkey_register_finish(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: RegisterFinish = body(request)?;
    let user = state.db.tx(|tx| authenticate(state, tx, request))?;
    let Ceremony::Register { user_id, state: reg } = take(state, &input.challenge_id)? else {
        return Err(ApiError::bad("that challenge is not a registration"));
    };
    if user_id != user.id {
        return Err(ApiError::forbidden("that challenge belongs to another account"));
    }
    let passkey = state.webauthn.finish_passkey_registration(&input.credential, &reg).map_err(|e| ApiError::new(400, "passkey_rejected", format!("passkey rejected: {e}")))?;
    let id = b64(passkey.cred_id().as_ref());
    let name: String = if input.name.trim().is_empty() { "passkey".into() } else { input.name.trim().chars().take(64).collect() };
    let data = serde_json::to_string(&passkey).map_err(|_| ApiError::new(500, "internal", "internal error"))?;
    let now = state.now();
    state.db.tx(|tx| {
        if tx.query_one("SELECT id FROM passkeys WHERE id = $1", &[(&id).into()])?.is_some() {
            return Err(ApiError::conflict("passkey_exists", "this passkey is already registered", json!({})));
        }
        tx.execute("INSERT INTO passkeys (id, user_id, name, data, created_at) VALUES ($1, $2, $3, $4, $5)", &[(&id).into(), (&user.id).into(), (&name).into(), data.into(), now.into()])?;
        audit::record(tx, state, Event::new(audit::ACCOUNT, "passkey_added", Some(&user)).target(&user.id).detail(json!({ "passkey": id, "name": name })))?;
        Ok((201, json!({ "id": id, "name": name })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginStart {
    email: String,
}

/// `POST /v1/auth/passkey/login/start {"email"}`: options for `navigator.credentials.get`.
pub fn passkey_login_start(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: LoginStart = body(request)?;
    let email = normalize_email(&input.email)?;
    if crate::oidc::sso_required_for(state, &email) {
        return Err(sso_only(state, &email, "passkey"));
    }
    let (user_id, passkeys) = state.db.tx(|tx| {
        let Some(row) = tx.query_one("SELECT id FROM users WHERE email = $1", &[(&email).into()])? else {
            return Err(ApiError::new(404, "no_passkey", "no passkey for this address; sign in by email"));
        };
        let id = row.text(0);
        let passkeys = user_passkeys(tx, &id)?;
        Ok((id, passkeys))
    })?;
    if passkeys.is_empty() {
        return Err(ApiError::new(404, "no_passkey", "no passkey for this address; sign in by email"));
    }
    let (options, auth) = state.webauthn.start_passkey_authentication(&passkeys).map_err(|e| ApiError::bad(format!("passkey sign-in: {e}")))?;
    let id = remember(state, Ceremony::Login { user_id, state: auth });
    Ok((200, json!({ "challengeId": id, "options": options })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LoginFinish {
    challenge_id: String,
    credential: PublicKeyCredential,
}

/// `POST /v1/auth/passkey/login/finish {"challengeId","credential"}` → a session.
pub fn passkey_login_finish(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let input: LoginFinish = body(request)?;
    let Ceremony::Login { user_id, state: auth } = take(state, &input.challenge_id)? else {
        return Err(ApiError::bad("that challenge is not a sign-in"));
    };
    let result = match state.webauthn.finish_passkey_authentication(&input.credential, &auth) {
        Ok(r) => r,
        Err(e) => {
            audit::record_alone(state, Event::new(audit::ACCOUNT, "signin_failed", None).target(&user_id).detail(json!({ "method": "passkey", "reason": "passkey_rejected" })));
            return Err(ApiError::new(401, "passkey_rejected", format!("passkey rejected: {e}")));
        }
    };
    let cred_id = b64(result.cred_id().as_ref());
    state.db.tx(|tx| {
        let Some(row) = tx.query_one("SELECT data FROM passkeys WHERE id = $1 AND user_id = $2", &[(&cred_id).into(), (&user_id).into()])? else {
            return Err(ApiError::new(401, "passkey_rejected", "unknown passkey"));
        };
        if let Ok(mut passkey) = serde_json::from_str::<Passkey>(&row.text(0)) {
            // Keeps the signature counter and backup state current (clone detection).
            if passkey.update_credential(&result) == Some(true) {
                if let Ok(data) = serde_json::to_string(&passkey) {
                    tx.execute("UPDATE passkeys SET data = $1 WHERE id = $2", &[data.into(), (&cred_id).into()])?;
                }
            }
        }
        let email = tx.query_one("SELECT email FROM users WHERE id = $1", &[(&user_id).into()])?.map(|r| r.text(0)).unwrap_or_default();
        if crate::oidc::sso_required_for(state, &email) {
            return Err(ApiError::new(403, "sso_required", format!("{email} signs in with single sign-on")));
        }
        new_session(state, tx, &User { id: user_id.clone(), email, runner_of: String::new() }, "passkey")
    })
}
