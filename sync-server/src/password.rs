//! Passwords (HITOP Cloud C1b; THREAT_MODEL.md "Passwords"). The password never reaches the
//! server: a client derives two keys from it (hitop-sync-crypto `derive_password_keys`, Argon2id
//! with the account's KDF parameters, then HKDF): an **auth key**, which it sends here, and a
//! **bundle key**, which seals the account's key bundle (recovery.rs, method `password`) and never
//! leaves the client. The server keeps the client's KDF parameters and its own slow hash of the
//! auth key: Argon2id (19 MiB, 2 passes, its own salt; `HITOP_SYNC_PASSWORD_PEPPER` as the Argon2
//! secret when set). A copy of the database therefore needs a password guess, the client's
//! Argon2id and the server's for each try, and even a correct auth key opens no bundle.
//!
//! Endpoints (all answers say the same whether or not an address has an account):
//!
//! - `POST /v1/auth/password/params {email}` → `{kdf}`: the account's KDF parameters; an
//!   address without a password gets made-up ones (a salt derived from a server secret, so they
//!   don't change between asks).
//! - `POST /v1/auth/password/signup {email, kdf, authKey}` → 202: mails a confirmation code. An
//!   address that already has a password gets a mail saying so instead (same answer here).
//! - `POST /v1/auth/password/verify {token}` → a session: the code from that mail. Makes the
//!   account, or adds the password to an existing account without one.
//! - `POST /v1/auth/password/login {email, authKey}` → a session, or 401 `invalid_credentials`
//!   for a wrong address, an account without a password and a wrong password alike (with the
//!   same hashing work). After 5 failures for an address (any address), it waits 30 s, doubling
//!   up to 15 minutes (429 + `Retry-After`).
//! - `POST /v1/auth/password/forgot {email}` → 202: mails a reset code (or, for an address with no
//!   account, a mail saying there is none).
//! - `POST /v1/auth/password/reset {token, kdf, authKey}` → a session and `bundleLost`: the old
//!   password's key bundle can't be opened by the new one, so it is deleted; every other session
//!   of the account ends.
//! - `POST /v1/me/password {currentAuthKey?, kdf, authKey, bundle?}` (signed-in device): sets or
//!   changes the password; `bundle` (the key bundle sealed under the new password, base64url)
//!   replaces the old one in the same transaction, and without it the old one is deleted. Other
//!   sessions end; a mail tells the person.
//!
//! `HITOP_SYNC_PASSWORDS=off` turns password sign-in off.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use webauthn_rs::prelude::Uuid;

use crate::api::{b64, bearer, body, new_token, random_bytes, token_hash, unb64, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{authenticate, new_session, normalize_email, User};
use crate::db::Tx;
use crate::mail::Mail;

/// The client KDF floors and ceilings (hitop-sync-crypto `PasswordKdf::check`): the server
/// refuses to store weaker parameters, and clients refuse to use them.
pub const MIN_MEMORY_KIB: u64 = 64 * 1024;
pub const MIN_ITERATIONS: u64 = 3;
pub const MAX_MEMORY_KIB: u64 = 1024 * 1024;
pub const MAX_ITERATIONS: u64 = 10;
pub const MAX_PARALLELISM: u64 = 4;
/// What a new password gets when the client doesn't say (and what made-up parameters show).
pub const DEFAULT_MEMORY_KIB: u64 = 64 * 1024;
pub const DEFAULT_ITERATIONS: u64 = 3;
pub const DEFAULT_PARALLELISM: u64 = 1;

/// Failures for one address before sign-in waits.
const FREE_FAILURES: u32 = 5;
const FIRST_WAIT: i64 = 30;
const MAX_WAIT: i64 = 15 * 60;
/// Failures older than this are forgotten.
const FAILURE_MEMORY: i64 = 3600;

#[derive(Clone)]
pub struct PasswordConfig {
    pub enabled: bool,
    /// The Argon2 secret (`HITOP_SYNC_PASSWORD_PEPPER` or `_FILE`).
    pub pepper: Option<Vec<u8>>,
    /// The server's hash of the auth key: OWASP's Argon2id floor (the input is a 256-bit key).
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    /// Failure backoff per address (off with `HITOP_SYNC_RATE_LIMITS=off`).
    pub backoff: bool,
}

impl std::fmt::Debug for PasswordConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordConfig").field("enabled", &self.enabled).field("peppered", &self.pepper.is_some()).finish_non_exhaustive()
    }
}

impl Default for PasswordConfig {
    fn default() -> Self {
        PasswordConfig { enabled: true, pepper: None, memory_kib: 19 * 1024, iterations: 2, parallelism: 1, backoff: true }
    }
}

impl PasswordConfig {
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<PasswordConfig, String> {
        let enabled = match var("HITOP_SYNC_PASSWORDS").as_deref() {
            None | Some("on") => true,
            Some("off") => false,
            Some(other) => return Err(format!("HITOP_SYNC_PASSWORDS must be on or off, not {other}")),
        };
        let pepper = match (var("HITOP_SYNC_PASSWORD_PEPPER"), var("HITOP_SYNC_PASSWORD_PEPPER_FILE")) {
            (Some(p), _) => Some(p),
            (None, Some(path)) => Some(std::fs::read_to_string(&path).map_err(|e| format!("HITOP_SYNC_PASSWORD_PEPPER_FILE {path}: {e}"))?.trim().to_string()),
            (None, None) => None,
        };
        if let Some(p) = &pepper {
            if p.len() < 16 {
                return Err("HITOP_SYNC_PASSWORD_PEPPER needs at least 16 characters (32 random bytes, base64, is a good one)".into());
            }
        }
        Ok(PasswordConfig { enabled, pepper: pepper.map(String::into_bytes), ..PasswordConfig::default() })
    }
}

/// Failures per address, in memory (one process per database).
#[derive(Default)]
pub struct Backoff {
    /// address → (failures, last failure, wait until)
    map: Mutex<HashMap<String, (u32, i64, i64)>>,
}

impl Backoff {
    /// `Err(seconds)` while the address must wait.
    pub fn check(&self, key: &str, now: i64) -> Result<(), i64> {
        let map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(key) {
            Some((_, _, until)) if *until > now => Err(until - now),
            _ => Ok(()),
        }
    }

    pub fn failed(&self, key: &str, now: i64) {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() > 100_000 {
            map.retain(|_, (_, last, until)| now - *last < FAILURE_MEMORY || *until > now);
        }
        let e = map.entry(key.to_string()).or_insert((0, now, 0));
        if now - e.1 >= FAILURE_MEMORY {
            *e = (0, now, 0);
        }
        e.0 += 1;
        e.1 = now;
        if e.0 >= FREE_FAILURES {
            let wait = FIRST_WAIT.saturating_mul(1 << (e.0 - FREE_FAILURES).min(10)).min(MAX_WAIT);
            e.2 = now + wait;
        }
    }

    pub fn succeeded(&self, key: &str) {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
    }
}

/// A client's KDF parameters as they travel: `{alg, m, t, p, salt}` (salt base64url).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Kdf {
    pub alg: String,
    pub m: u64,
    pub t: u64,
    pub p: u64,
    pub salt: String,
}

impl Kdf {
    fn check(&self) -> Result<(), ApiError> {
        let salt = unb64("kdf.salt", &self.salt)?;
        let ok = self.alg == "argon2id"
            && (MIN_MEMORY_KIB..=MAX_MEMORY_KIB).contains(&self.m)
            && (MIN_ITERATIONS..=MAX_ITERATIONS).contains(&self.t)
            && (1..=MAX_PARALLELISM).contains(&self.p)
            && (16..=64).contains(&salt.len());
        if ok {
            Ok(())
        } else {
            Err(ApiError::bad(format!(
                "kdf must be argon2id with m {MIN_MEMORY_KIB}..={MAX_MEMORY_KIB} KiB, t {MIN_ITERATIONS}..={MAX_ITERATIONS}, p 1..={MAX_PARALLELISM} and a 16 to 64 byte salt"
            )))
        }
    }

    fn to_json(&self) -> Json {
        json!({ "alg": self.alg, "m": self.m, "t": self.t, "p": self.p, "salt": self.salt })
    }
}

fn auth_key(field: &str, value: &str) -> Result<Vec<u8>, ApiError> {
    let k = unb64(field, value)?;
    if k.len() != 32 {
        return Err(ApiError::bad(format!("{field} must be 32 bytes, base64url")));
    }
    Ok(k)
}

fn hasher(state: &State, peppered: bool) -> Result<Argon2<'_>, ApiError> {
    let c = &state.config.password;
    let params = Params::new(c.memory_kib, c.iterations, c.parallelism, Some(32)).map_err(|_| ApiError::new(500, "internal", "password hashing parameters"))?;
    match (&c.pepper, peppered) {
        (Some(p), true) => Argon2::new_with_secret(p, Algorithm::Argon2id, Version::V0x13, params).map_err(|_| ApiError::new(500, "internal", "password pepper")),
        (_, false) => Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params)),
        (None, true) => Err(ApiError::new(500, "pepper_missing", "this password was stored with a pepper the server no longer has (HITOP_SYNC_PASSWORD_PEPPER)")),
    }
}

/// The server's hash of an auth key: a PHC string, and whether it is peppered.
fn hash(state: &State, key: &[u8]) -> Result<(String, bool), ApiError> {
    let peppered = state.config.password.pepper.is_some();
    let salt = SaltString::encode_b64(&random_bytes::<16>()).map_err(|_| ApiError::new(500, "internal", "salt"))?;
    let h = hasher(state, peppered)?.hash_password(key, &salt).map_err(|_| ApiError::new(500, "internal", "password hashing"))?;
    Ok((h.to_string(), peppered))
}

/// Constant-time check of an auth key against a stored hash.
fn verify(state: &State, key: &[u8], stored: &str, peppered: bool) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else { return false };
    match hasher(state, peppered) {
        Ok(h) => h.verify_password(key, &parsed).is_ok(),
        Err(_) => false,
    }
}

/// The same work as a real check, for an address with no password.
fn verify_nothing(state: &State, key: &[u8]) {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let stored = DUMMY.get_or_init(|| hash(state, &[0u8; 32]).map(|(h, _)| h).unwrap_or_default());
    let _ = verify(state, key, stored, false);
}

/// A secret of this server kept in its database (made on first use).
pub(crate) fn server_secret(tx: &mut Tx, name: &str) -> Result<Vec<u8>, ApiError> {
    if let Some(r) = tx.query_one("SELECT value FROM server_secrets WHERE name = $1", &[name.into()])? {
        return Ok(r.bytes(0));
    }
    tx.execute("INSERT INTO server_secrets (name, value) VALUES ($1, $2) ON CONFLICT (name) DO NOTHING", &[name.into(), random_bytes::<32>().to_vec().into()])?;
    Ok(tx.query_one("SELECT value FROM server_secrets WHERE name = $1", &[name.into()])?.map(|r| r.bytes(0)).unwrap_or_default())
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// Made-up parameters for an address without a password: the same every time it's asked.
fn made_up_kdf(tx: &mut Tx, email: &str) -> Result<Json, ApiError> {
    let secret = server_secret(tx, "password-params")?;
    let salt = hmac_sha256(&secret, format!("hitop-sync/v1/made-up-salt\0{email}").as_bytes());
    Ok(json!({ "alg": "argon2id", "m": DEFAULT_MEMORY_KIB, "t": DEFAULT_ITERATIONS, "p": DEFAULT_PARALLELISM, "salt": b64(&salt[..16]) }))
}

fn enabled(state: &State) -> Result<(), ApiError> {
    if state.config.password.enabled {
        Ok(())
    } else {
        Err(ApiError::new(404, "passwords_off", "password sign-in is turned off on this server"))
    }
}

fn mail_on(state: &State) -> Result<(), ApiError> {
    if state.mailer.enabled() {
        Ok(())
    } else {
        Err(ApiError::new(503, "mail_off", "this server can't send mail, so it can't confirm an address or reset a password"))
    }
}

/// A person's account by address (never a runner account).
fn person(tx: &mut Tx, email: &str) -> Result<Option<User>, ApiError> {
    Ok(tx.query_one("SELECT id, email FROM users WHERE email = $1 AND runner_of = ''", &[email.into()])?.map(|r| User { id: r.text(0), email: r.text(1), runner_of: String::new() }))
}

struct Credential {
    kdf: String,
    hash: String,
    peppered: bool,
}

fn credential(tx: &mut Tx, user_id: &str) -> Result<Option<Credential>, ApiError> {
    Ok(tx.query_one("SELECT kdf, hash, peppered FROM password_credentials WHERE user_id = $1", &[user_id.into()])?.map(|r| Credential { kdf: r.text(0), hash: r.text(1), peppered: r.int(2) != 0 }))
}

fn store_credential(tx: &mut Tx, state: &State, user_id: &str, kdf: &str, hash: &str, peppered: bool) -> Result<(), ApiError> {
    let now = state.now();
    tx.execute(
        "INSERT INTO password_credentials (user_id, kdf, hash, peppered, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $5)
         ON CONFLICT (user_id) DO UPDATE SET kdf = excluded.kdf, hash = excluded.hash, peppered = excluded.peppered, updated_at = excluded.updated_at",
        &[user_id.into(), kdf.into(), hash.into(), i64::from(peppered).into(), now.into()],
    )?;
    Ok(())
}

/// Mails per address and hour, counted over sign-in links and password mails together.
fn mail_budget(tx: &mut Tx, state: &State, email: &str) -> Result<(), ApiError> {
    let hour_ago = state.now() - 3600;
    let mine = tx.query_one("SELECT COUNT(*) FROM password_tokens WHERE email = $1 AND created_at > $2", &[email.into(), hour_ago.into()])?.map_or(0, |r| r.int(0))
        + tx.query_one("SELECT COUNT(*) FROM magic_links WHERE email = $1 AND created_at > $2", &[email.into(), hour_ago.into()])?.map_or(0, |r| r.int(0));
    let total = tx.query_one("SELECT COUNT(*) FROM password_tokens WHERE created_at > $1", &[hour_ago.into()])?.map_or(0, |r| r.int(0));
    if mine >= state.config.links_per_email_hour || total >= state.config.links_per_hour {
        return Err(ApiError::new(429, "rate_limited", "too many mails to this address; try again in an hour"));
    }
    Ok(())
}

/// Stores a mailed token (purpose `signup`, `reset`, or `notice` = a mail with no code).
/// `pending` = (kdf, hash, peppered) of a sign-up's password.
fn store_token(tx: &mut Tx, state: &State, token: &str, email: &str, purpose: &str, pending: Option<(&str, &str, bool)>) -> Result<(), ApiError> {
    let now = state.now();
    let (kdf, hash, peppered) = pending.unwrap_or(("", "", false));
    tx.execute("DELETE FROM password_tokens WHERE expires_at < $1", &[(now - 86_400).into()])?;
    tx.execute(
        "INSERT INTO password_tokens (token_hash, email, purpose, kdf, hash, peppered, created_at, expires_at, used) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        &[token_hash(token).into(), email.into(), purpose.into(), kdf.into(), hash.into(), i64::from(peppered).into(), now.into(), (now + state.config.link_ttl_secs).into(), i64::from(purpose == "notice").into()],
    )?;
    Ok(())
}

fn code_mail(state: &State, token: &str) -> String {
    let link = state.config.public_url.join("signin").map(|u| format!("{u}#{token}")).unwrap_or_default();
    format!("{link}\n\nor paste this code into hitop:\n\n{token}\n\nIt works once, for {} minutes.", state.config.link_ttl_secs / 60)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailOnly {
    email: String,
}

/// `POST /v1/auth/password/params {email}` → `{kdf}`.
pub fn params(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    enabled(state)?;
    let input: EmailOnly = body(request)?;
    let email = normalize_email(&input.email)?;
    state.db.tx(|tx| {
        let real = match person(tx, &email)? {
            Some(u) => credential(tx, &u.id)?.and_then(|c| serde_json::from_str::<Json>(&c.kdf).ok()),
            None => None,
        };
        let kdf = match real {
            Some(k) => k,
            None => made_up_kdf(tx, &email)?,
        };
        Ok((200, json!({ "kdf": kdf })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Signup {
    email: String,
    kdf: Kdf,
    auth_key: String,
}

/// `POST /v1/auth/password/signup {email, kdf, authKey}` → 202.
pub fn signup(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    enabled(state)?;
    mail_on(state)?;
    let input: Signup = body(request)?;
    let email = normalize_email(&input.email)?;
    if crate::oidc::sso_required_for(state, &email) {
        return Err(ApiError::new(403, "sso_required", "this address signs in with single sign-on (hitop sync login --sso, or Sign in with SSO)"));
    }
    input.kdf.check()?;
    let key = auth_key("authKey", &input.auth_key)?;
    // Hashed either way, so both answers take as long.
    let (hash, peppered) = hash(state, &key)?;
    let kdf = input.kdf.to_json().to_string();
    let token = new_token();
    let has_password = state.db.tx(|tx| -> Result<bool, ApiError> {
        mail_budget(tx, state, &email)?;
        let has = match person(tx, &email)? {
            Some(u) => credential(tx, &u.id)?.is_some(),
            None => false,
        };
        if has {
            store_token(tx, state, &new_token(), &email, "notice", None)?;
        } else {
            store_token(tx, state, &token, &email, "signup", Some((&kdf, &hash, peppered)))?;
        }
        Ok(has)
    })?;
    let body = if has_password {
        "Someone (you?) tried to make a hitop sync account with this address, but it already has one with a password.\n\nSign in with your password instead. Forgot it? Use \"Forgot password\" in hitop (hitop sync password forgot).\n\nIf it wasn't you, ignore this mail: nothing changed.\n".to_string()
    } else {
        format!("Confirm your address to finish making your hitop sync account. Open this link:\n\n{}\n\nIf you didn't ask for it, ignore this mail: no account is made.\n", code_mail(state, &token))
    };
    state.mailer.send(Mail { to: email, subject: "Confirm your hitop sync account".into(), body });
    Ok((202, json!({ "sent": true })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenOnly {
    token: String,
}

/// A mailed token of `purpose`, used up: (email, kdf, hash, peppered).
fn take_token(tx: &mut Tx, state: &State, token: &str, purpose: &str) -> Result<(String, String, String, bool), ApiError> {
    let hash = token_hash(token.trim().rsplit('#').next().unwrap_or("").trim());
    let row = tx.query_one(
        "SELECT email, kdf, hash, peppered FROM password_tokens WHERE token_hash = $1 AND purpose = $2 AND used = 0 AND expires_at > $3",
        &[(&hash).into(), purpose.into(), state.now().into()],
    )?;
    let Some(r) = row else {
        return Err(ApiError::new(401, "invalid_code", "this code is invalid, used or expired; ask for a new one"));
    };
    tx.execute("UPDATE password_tokens SET used = 1 WHERE token_hash = $1", &[hash.into()])?;
    Ok((r.text(0), r.text(1), r.text(2), r.int(3) != 0))
}

/// `POST /v1/auth/password/verify {token}` → a session (`created`: a new account).
pub fn verify_signup(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    enabled(state)?;
    let input: TokenOnly = body(request)?;
    let result = state.db.tx(|tx| {
        let (email, kdf, hash, peppered) = take_token(tx, state, &input.token, "signup")?;
        if crate::oidc::sso_required_for(state, &email) {
            return Err(ApiError::new(403, "sso_required", format!("{email} signs in with single sign-on")));
        }
        let (user, created) = match person(tx, &email)? {
            Some(u) => {
                if credential(tx, &u.id)?.is_some() {
                    return Err(ApiError::new(409, "password_exists", "this account already has a password: sign in with it"));
                }
                (u, false)
            }
            None => {
                let u = User { id: Uuid::new_v4().to_string(), email, runner_of: String::new() };
                tx.execute("INSERT INTO users (id, email, created_at) VALUES ($1, $2, $3)", &[(&u.id).into(), (&u.email).into(), state.now().into()])?;
                (u, true)
            }
        };
        store_credential(tx, state, &user.id, &kdf, &hash, peppered)?;
        audit::record(tx, state, Event::new(audit::ACCOUNT, "password_set", Some(&user)).target(&user.id).detail(json!({ "via": "signup", "created": created })))?;
        let (status, mut reply) = new_session(state, tx, &user, "password")?;
        reply["created"] = json!(created);
        Ok((status, reply))
    });
    if let Err(e) = &result {
        audit::record_alone(state, Event::new(audit::ACCOUNT, "signin_failed", None).detail(json!({ "method": "password_signup", "reason": e.code })));
    }
    result
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Login {
    email: String,
    auth_key: String,
}

fn wait(retry_after: i64) -> ApiError {
    ApiError {
        status: 429,
        code: "rate_limited",
        message: format!("too many wrong passwords for this address; try again in {retry_after} s (or reset the password)"),
        extra: Some(json!({ "limit": "password", "retryAfter": retry_after })),
    }
}

fn invalid() -> ApiError {
    ApiError::new(401, "invalid_credentials", "the email address or the password is wrong")
}

/// `POST /v1/auth/password/login {email, authKey}` → a session.
pub fn login(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    enabled(state)?;
    let input: Login = body(request)?;
    let email = normalize_email(&input.email)?;
    let key = auth_key("authKey", &input.auth_key)?;
    let now = state.now();
    let backoff = state.config.password.backoff;
    if backoff {
        state.password_failures.check(&email, now).map_err(wait)?;
    }
    if crate::oidc::sso_required_for(state, &email) {
        return Err(ApiError::new(403, "sso_required", "this address signs in with single sign-on (hitop sync login --sso, or Sign in with SSO)"));
    }
    let found = state.db.tx(|tx| -> Result<_, ApiError> {
        let Some(user) = person(tx, &email)? else { return Ok(None) };
        Ok(credential(tx, &user.id)?.map(|c| (user, c)))
    })?;
    let ok = match &found {
        Some((_, c)) => verify(state, &key, &c.hash, c.peppered),
        None => {
            verify_nothing(state, &key);
            false
        }
    };
    let Some((user, _)) = found.filter(|_| ok) else {
        if backoff {
            state.password_failures.failed(&email, now);
        }
        audit::record_alone(state, Event::new(audit::ACCOUNT, "signin_failed", None).email(&email).detail(json!({ "method": "password", "reason": "invalid_credentials" })));
        return Err(invalid());
    };
    state.password_failures.succeeded(&email);
    state.db.tx(|tx| new_session(state, tx, &user, "password"))
}

/// `POST /v1/auth/password/forgot {email}` → 202.
pub fn forgot(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    enabled(state)?;
    mail_on(state)?;
    let input: EmailOnly = body(request)?;
    let email = normalize_email(&input.email)?;
    if crate::oidc::sso_required_for(state, &email) {
        return Err(ApiError::new(403, "sso_required", "this address signs in with single sign-on; its password is managed there"));
    }
    let token = new_token();
    let exists = state.db.tx(|tx| -> Result<bool, ApiError> {
        mail_budget(tx, state, &email)?;
        let exists = person(tx, &email)?.is_some();
        if exists {
            store_token(tx, state, &token, &email, "reset", None)?;
        } else {
            store_token(tx, state, &new_token(), &email, "notice", None)?;
        }
        Ok(exists)
    })?;
    let body = if exists {
        format!(
            "Someone (you?) asked to reset the password of your hitop sync account. To choose a new one, open:\n\n{}\n\n\
             Before you do: a reset signs you in, but it can't unlock your end-to-end encrypted workspaces on a new device. \
             The old password sealed those keys and nobody (the server included) can open them without it. To get them back, \
             use your recovery phrase (hitop sync recovery use) or a device that is still signed in. Remembered the password? \
             Ignore this mail: nothing changed.\n",
            code_mail(state, &token)
        )
    } else {
        "Someone (you?) asked to reset a hitop sync password for this address, but there is no account with it. If it wasn't you, ignore this mail.\n".to_string()
    };
    state.mailer.send(Mail { to: email, subject: "Reset your hitop sync password".into(), body });
    Ok((202, json!({ "sent": true })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Reset {
    token: String,
    kdf: Kdf,
    auth_key: String,
}

/// `POST /v1/auth/password/reset {token, kdf, authKey}` → a session and `bundleLost`.
pub fn reset(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    enabled(state)?;
    let input: Reset = body(request)?;
    input.kdf.check()?;
    let key = auth_key("authKey", &input.auth_key)?;
    let (hash, peppered) = hash(state, &key)?;
    let kdf = input.kdf.to_json().to_string();
    let result = state.db.tx(|tx| {
        let (email, _, _, _) = take_token(tx, state, &input.token, "reset")?;
        let Some(user) = person(tx, &email)? else {
            return Err(ApiError::new(401, "invalid_code", "this account no longer exists"));
        };
        let had = credential(tx, &user.id)?.is_some();
        store_credential(tx, state, &user.id, &kdf, &hash, peppered)?;
        let lost = tx.execute("DELETE FROM recovery_blobs WHERE user_id = $1 AND method = 'password'", &[(&user.id).into()])? > 0;
        let ended = tx.execute("DELETE FROM sessions WHERE user_id = $1", &[(&user.id).into()])?;
        audit::record(tx, state, Event::new(audit::ACCOUNT, "password_reset", Some(&user)).target(&user.id).detail(json!({ "hadPassword": had, "bundleLost": lost, "sessionsEnded": ended })))?;
        let (status, mut reply) = new_session(state, tx, &user, "password_reset")?;
        reply["bundleLost"] = json!(lost);
        Ok((status, reply))
    });
    if let Ok((_, r)) = &result {
        if let Some(e) = r["user"]["email"].as_str() {
            state.password_failures.succeeded(e);
        }
    }
    result
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SetPassword {
    #[serde(default)]
    current_auth_key: Option<String>,
    kdf: Kdf,
    auth_key: String,
    #[serde(default)]
    bundle: Option<String>,
}

/// `POST /v1/me/password {currentAuthKey?, kdf, authKey, bundle?}`: sets or changes the password.
pub fn set(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    enabled(state)?;
    if bearer(request).is_some_and(|t| t.starts_with(crate::tokens::PREFIX)) {
        return Err(ApiError::forbidden("setting a password needs a signed-in device, not an access token"));
    }
    let input: SetPassword = body(request)?;
    input.kdf.check()?;
    let key = auth_key("authKey", &input.auth_key)?;
    let current = input.current_auth_key.as_deref().map(|k| auth_key("currentAuthKey", k)).transpose()?;
    let bundle = input.bundle.as_deref().map(|b| unb64("bundle", b)).transpose()?;
    if bundle.as_ref().is_some_and(|b| b.is_empty() || b.len() > crate::recovery::MAX_RECOVERY_BYTES) {
        return Err(ApiError::new(413, "too_large", "a key bundle has 1 to 262144 bytes"));
    }
    let (user, existing) = state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let existing = credential(tx, &user.id)?;
        Ok::<_, ApiError>((user, existing))
    })?;
    let now = state.now();
    if let Some(c) = &existing {
        if state.config.password.backoff {
            state.password_failures.check(&user.email, now).map_err(wait)?;
        }
        let Some(current) = &current else {
            return Err(ApiError::new(403, "current_password_required", "this account has a password: give the current one to change it"));
        };
        if !verify(state, current, &c.hash, c.peppered) {
            if state.config.password.backoff {
                state.password_failures.failed(&user.email, now);
            }
            audit::record_alone(state, Event::new(audit::ACCOUNT, "password_change_failed", Some(&user)).target(&user.id));
            return Err(ApiError::new(403, "wrong_password", "the current password is wrong"));
        }
    }
    let (hash, peppered) = hash(state, &key)?;
    let kdf = input.kdf.to_json().to_string();
    let this_session = bearer(request).map(|t| token_hash(&t)).unwrap_or_default();
    let reply = state.db.tx(|tx| {
        store_credential(tx, state, &user.id, &kdf, &hash, peppered)?;
        let bundle_state = match &bundle {
            Some(b) => {
                tx.execute(
                    "INSERT INTO recovery_blobs (user_id, method, blob, created_at, updated_at) VALUES ($1, 'password', $2, $3, $3)
                     ON CONFLICT (user_id, method) DO UPDATE SET blob = excluded.blob, updated_at = excluded.updated_at",
                    &[(&user.id).into(), b.clone().into(), now.into()],
                )?;
                "replaced"
            }
            None => {
                if tx.execute("DELETE FROM recovery_blobs WHERE user_id = $1 AND method = 'password'", &[(&user.id).into()])? > 0 {
                    "deleted"
                } else {
                    "none"
                }
            }
        };
        let ended = tx.execute("DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2", &[(&user.id).into(), (&this_session).into()])?;
        let event = if existing.is_some() { "password_changed" } else { "password_set" };
        audit::record(tx, state, Event::new(audit::ACCOUNT, event, Some(&user)).target(&user.id).detail(json!({ "bundle": bundle_state, "sessionsEnded": ended })))?;
        Ok::<_, ApiError>(json!({ "updatedAt": now, "bundle": bundle_state, "sessionsEnded": ended }))
    })?;
    state.password_failures.succeeded(&user.email);
    let what = if existing.is_some() { "changed" } else { "added to your account" };
    state.mailer.send(Mail {
        to: user.email.clone(),
        subject: format!("Your hitop sync password was {}", if existing.is_some() { "changed" } else { "set" }),
        body: format!(
            "The password of your hitop sync account was {what}. Other devices were signed out and sign in again with it.\n\nIf it wasn't you, reset the password now (Forgot password in hitop) and check your account's audit log (hitop sync audit --mine).\n"
        ),
    });
    Ok((200, reply))
}

/// For `GET /v1/me`: `{set, updatedAt}`.
pub fn status_of(tx: &mut Tx, user_id: &str) -> Result<Json, ApiError> {
    Ok(match tx.query_one("SELECT updated_at FROM password_credentials WHERE user_id = $1", &[user_id.into()])? {
        Some(r) => json!({ "set": true, "updatedAt": r.int(0) }),
        None => json!({ "set": false }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_waits_after_five_failures_and_doubles() {
        let b = Backoff::default();
        for i in 0..4 {
            b.failed("a@x", 100 + i);
            assert!(b.check("a@x", 100 + i).is_ok());
        }
        b.failed("a@x", 104);
        assert_eq!(b.check("a@x", 104), Err(30));
        assert!(b.check("b@x", 104).is_ok(), "another address");
        assert!(b.check("a@x", 134).is_ok());
        b.failed("a@x", 134);
        assert_eq!(b.check("a@x", 134), Err(60));
        for i in 0..20 {
            b.failed("a@x", 200 + i);
        }
        assert_eq!(b.check("a@x", 219), Err(MAX_WAIT));
        b.succeeded("a@x");
        assert!(b.check("a@x", 220).is_ok());
        // Forgotten after an hour without failures.
        let c = Backoff::default();
        for i in 0..4 {
            c.failed("c@x", i);
        }
        c.failed("c@x", 4 + FAILURE_MEMORY);
        assert!(c.check("c@x", 4 + FAILURE_MEMORY).is_ok());
    }

    #[test]
    fn hmac_matches_rfc_4231_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }
}
