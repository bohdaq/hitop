//! Passwords for hitop sync (HITOP Cloud C1b; sync-server/THREAT_MODEL.md "Passwords"):
//! `hitop sync login` (by default), `hitop sync signup`, `hitop sync password set|forgot|reset|unlock`.
//!
//! The password never leaves this computer. `hitop_sync_crypto::derive_password_keys` turns it
//! and the account's parameters (from the server) into an **auth key**, the only thing sent, and a
//! **bundle key**, which seals this device's key bundle (the same bundle as the recovery phrase:
//! recovery.rs `RecoveryData`) for the server to keep under method `password`. A password sign-in
//! on a new computer opens that bundle and becomes the device in it: same key, same trust, no
//! trust step. A reset by mail can't open the old bundle: the server deletes it, and the device
//! comes back only with the recovery phrase or from another device.

use std::io::{BufRead, IsTerminal};

use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use hitop_sync_crypto::{self as crypto, PasswordKdf, PasswordKeys};
use serde_json::{json, Value};

use super::client::{api_error, b64, unb64, Api};
use super::recovery::{self, RecoveryData};
use super::store::{self, Account};

fn kdf_json(k: &PasswordKdf) -> Value {
    json!({ "alg": "argon2id", "m": k.memory_kib, "t": k.iterations, "p": k.parallelism, "salt": b64(&k.salt) })
}

fn kdf_of(v: &Value) -> Result<PasswordKdf> {
    if v["alg"] != "argon2id" {
        bail!("the server asks for password hashing hitop doesn't know ({})", v["alg"]);
    }
    let n = |k: &str| v[k].as_u64().and_then(|x| u32::try_from(x).ok()).ok_or_else(|| anyhow!("the server's password parameters have no {k}"));
    Ok(PasswordKdf { memory_kib: n("m")?, iterations: n("t")?, parallelism: n("p")?, salt: unb64(v["salt"].as_str().unwrap_or(""))? })
}

/// The account's parameters, then the keys (refuses parameters below hitop's floor).
fn keys_for(anon: &Api, email: &str, password: &str) -> Result<PasswordKeys> {
    let p = anon.call("POST", "/v1/auth/password/params", Some(json!({ "email": email })))?;
    let kdf = kdf_of(&p["kdf"])?;
    crypto::derive_password_keys(password, &kdf).map_err(|e| anyhow!("{e}"))
}

fn stdin_line() -> Result<String> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let line = line.trim_end_matches(['\n', '\r']).to_string();
    if line.is_empty() {
        bail!("no password on stdin");
    }
    Ok(line)
}

fn tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// A password: the next line of stdin with `--password-stdin`, else asked on a terminal.
fn read(prompt: &str, from_stdin: bool) -> Result<String> {
    if from_stdin {
        return stdin_line();
    }
    if !tty() {
        bail!("a password is needed: give it on stdin with --password-stdin (or sign in with --code, --sso, --google or --github)");
    }
    let p = rpassword::prompt_password(prompt)?;
    if p.is_empty() {
        bail!("no password given");
    }
    Ok(p)
}

/// A new password: asked twice on a terminal; checked for length.
fn read_new(prompt: &str, from_stdin: bool) -> Result<String> {
    let p = read(prompt, from_stdin)?;
    crypto::check_new_password(&p).map_err(|e| anyhow!("{e}"))?;
    if !from_stdin && read("Repeat it: ", false)? != p {
        bail!("the passwords don't match");
    }
    Ok(p)
}

fn server_of(server: Option<String>) -> String {
    server.unwrap_or_else(super::default_server)
}

/// `hitop sync login --email` (the default): password sign-in.
pub fn login(email: &str, from_stdin: bool, server: Option<String>, name: Option<String>) -> Result<()> {
    let server = server_of(server);
    let anon = Api::new(&server, None)?;
    let password = read(&format!("Password for {email}: "), from_stdin)?;
    let keys = keys_for(&anon, email, &password)?;
    let r = anon.raw("POST", "/v1/auth/password/login", Some(json!({ "email": email, "authKey": b64(&keys.auth_key) })))?;
    match r.status {
        200 => super::finish_login(&server, &r.json, email, name, Some(keys.bundle_key)),
        401 => bail!("the email address or the password is wrong (forgot it? hitop sync password forgot --email {email}; no password yet? hitop sync login --email {email} --code)"),
        _ => Err(api_error(&r)),
    }
}

/// `hitop sync signup`: a new account (or a password for an account without one), confirmed by the
/// mailed code. In two runs without a terminal: without `--code` (mails it), then with it.
pub fn signup(email: &str, code: Option<String>, from_stdin: bool, server: Option<String>, name: Option<String>) -> Result<()> {
    let server = server_of(server);
    let anon = Api::new(&server, None)?;
    if let Some(c) = code {
        // The second run: confirm, then sign in with the password given at the first.
        let password = read(&format!("Password you chose for {email}: "), from_stdin)?;
        confirm(&anon, &c)?;
        return sign_in_after_signup(&anon, &server, email, &password, name);
    }
    let password = read_new(&format!("New password for {email} (8 characters or more): "), from_stdin)?;
    let kdf = PasswordKdf::generate();
    let keys = crypto::derive_password_keys(&password, &kdf).map_err(|e| anyhow!("{e}"))?;
    anon.call("POST", "/v1/auth/password/signup", Some(json!({ "email": email, "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })))?;
    eprintln!("A confirmation mail is on its way to {email}.");
    if !tty() {
        println!("Then: hitop sync signup --email {email} --code <code from the mail> (with the same password)");
        return Ok(());
    }
    let c = super::ask("Code from the mail (or the part after # in its link): ")?;
    confirm(&anon, &c)?;
    sign_in_after_signup(&anon, &server, email, &password, name)
}

fn confirm(anon: &Api, code: &str) -> Result<()> {
    let code = code.rsplit('#').next().unwrap_or(code).trim();
    let r = anon.raw("POST", "/v1/auth/password/verify", Some(json!({ "token": code })))?;
    match r.status {
        200 => Ok(()),
        // Already confirmed (the account has a password now): sign in with it.
        409 => Ok(()),
        _ => Err(api_error(&r)),
    }
}

/// After the code: a password sign-in (checks the password is the one signed up with, and gives
/// the bundle key).
fn sign_in_after_signup(anon: &Api, server: &str, email: &str, password: &str, name: Option<String>) -> Result<()> {
    let keys = keys_for(anon, email, password)?;
    let r = anon.raw("POST", "/v1/auth/password/login", Some(json!({ "email": email, "authKey": b64(&keys.auth_key) })))?;
    if r.status == 401 {
        bail!("the account is confirmed, but that isn't the password you chose: hitop sync login --email {email}");
    }
    if r.status != 200 {
        return Err(api_error(&r));
    }
    println!("Account confirmed.");
    super::finish_login(server, &r.json, email, name, Some(keys.bundle_key))
}

/// This device's bundle, if the server's password bundle is this device's or there is none: seals
/// it under `bundle_key` and keeps the key. Another device's bundle stays as it is.
pub fn claim_bundle(api: &Api, account: &Account, bundle_key: &[u8; 32]) -> Result<()> {
    let mut secret = store::load_secret(&account.server)?;
    let r = api.raw("GET", "/v1/me/recovery?method=password", None)?;
    let ours = match r.status {
        404 => true,
        200 => crypto::open_key_bundle(bundle_key, &account.user_id, &unb64(r.json["blob"].as_str().unwrap_or(""))?)
            .ok()
            .and_then(|p| serde_json::from_slice::<RecoveryData>(&p).ok())
            .is_some_and(|d| d.key_id == account.key_id),
        _ => return Err(api_error(&r)),
    };
    if !ours {
        return Ok(());
    }
    secret.password = Some(b64(bundle_key));
    let plain = recovery::bundle_plaintext(account, &secret)?;
    api.call("PUT", "/v1/me/recovery?method=password", Some(json!({ "blob": recovery::seal_for_password(account, bundle_key, &plain)? })))?;
    store::save_secret(&account.server, &secret)?;
    Ok(())
}

/// A password sign-in on a computer without this account's device: opens the password's bundle
/// and becomes that device. `None` when there is no bundle (or the password doesn't open it: it
/// was sealed under an earlier password), so the caller registers a new key.
pub fn adopt(api: &Api, server: &str, email: &str, user_id: &str, session: &str, bundle_key: &[u8; 32]) -> Result<Option<String>> {
    let r = api.raw("GET", "/v1/me/recovery?method=password", None)?;
    if r.status == 404 {
        return Ok(None);
    }
    if r.status != 200 {
        return Err(api_error(&r));
    }
    let Ok(plain) = crypto::open_key_bundle(bundle_key, user_id, &unb64(r.json["blob"].as_str().unwrap_or(""))?) else {
        eprintln!("{} your password didn't open the key bundle on the server; this computer gets a new device key", "note:".yellow());
        return Ok(None);
    };
    let data: RecoveryData = serde_json::from_slice(&plain).context("the key bundle is damaged")?;
    // A previous account on this computer (another server or person) is replaced, as any sign-in does.
    let mut account = Account { server: server.to_string(), email: email.to_string(), user_id: user_id.to_string(), key_id: String::new(), device_name: String::new() };
    let fp = recovery::become_device(api, &mut account, session, &data, None, Some(b64(bundle_key)))?;
    store::save_recovery_hash(&recovery::digest(&plain))?;
    Ok(Some(fp))
}

/// `hitop sync password set`: add or change the password; re-seals the bundle in the same request.
pub fn set(from_stdin: bool) -> Result<()> {
    let account = Account::load()?.ok_or_else(|| anyhow!("not signed in: run `hitop sync login` first"))?;
    let mut secret = store::load_secret(&account.server)?;
    let api = Api::new(&account.server, Some(secret.session.clone()))?.with_device(&account.key_id);
    let anon = Api::new(&account.server, None)?;
    let me = api.call("GET", "/v1/me", None)?;
    let has = me["password"]["set"] == true;
    let current = if has { Some(keys_for(&anon, &account.email, &read("Current password: ", from_stdin)?)?) } else { None };
    let new_password = read_new("New password (8 characters or more): ", from_stdin)?;
    let kdf = PasswordKdf::generate();
    let keys = crypto::derive_password_keys(&new_password, &kdf).map_err(|e| anyhow!("{e}"))?;
    // What the new password seals: the bundle as it is (another device's), or this device's.
    let mut ours = true;
    let mut plain = recovery::bundle_plaintext(&account, &secret)?;
    if let Some(cur) = &current {
        let r = api.raw("GET", "/v1/me/recovery?method=password", None)?;
        if r.status == 200 {
            match crypto::open_key_bundle(&cur.bundle_key, &account.user_id, &unb64(r.json["blob"].as_str().unwrap_or(""))?) {
                Ok(p) => {
                    let d: Option<RecoveryData> = serde_json::from_slice(&p).ok();
                    if d.is_some_and(|d| d.key_id != account.key_id) {
                        ours = false;
                        plain = p;
                    }
                }
                Err(_) => eprintln!("{} the current password didn't open the key bundle on the server; it is replaced with this device's", "note:".yellow()),
            }
        }
    }
    let bundle = recovery::seal_for_password(&account, &keys.bundle_key, &plain)?;
    let mut body = json!({ "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key), "bundle": bundle });
    if let Some(c) = &current {
        body["currentAuthKey"] = json!(b64(&c.auth_key));
    }
    let r = api.raw("POST", "/v1/me/password", Some(body))?;
    match (r.status, r.json["error"].as_str()) {
        (200, _) => {}
        (403, Some("wrong_password")) => bail!("the current password is wrong"),
        _ => return Err(api_error(&r)),
    }
    secret.password = ours.then(|| b64(&keys.bundle_key));
    store::save_secret(&account.server, &secret)?;
    if ours {
        store::save_recovery_hash(&recovery::digest(&plain))?;
    }
    println!("Password {} for {}. Your other devices were signed out: they sign in again with it.", if has { "changed" } else { "set" }, account.email);
    println!("On a new computer, hitop sync login --email {} unlocks your keys with it.", account.email);
    Ok(())
}

/// `hitop sync password forgot`.
pub fn forgot(email: &str, server: Option<String>) -> Result<()> {
    let anon = Api::new(&server_of(server), None)?;
    anon.call("POST", "/v1/auth/password/forgot", Some(json!({ "email": email })))?;
    eprintln!("If {email} has an account, a reset mail is on its way.");
    eprintln!("{} a new password signs you in, but it can't unlock your end-to-end encrypted workspaces on a new computer:", "Before you reset:".yellow().bold());
    eprintln!("the old password sealed those keys. Remembered it? Just sign in. Otherwise you'll need your recovery phrase");
    eprintln!("(hitop sync recovery use) or a device that is still signed in.");
    println!("Then: hitop sync password reset --code <code from the mail>");
    Ok(())
}

/// `hitop sync password reset --code`: a new password; the old one's bundle is gone.
pub fn reset(code: &str, from_stdin: bool, server: Option<String>, name: Option<String>) -> Result<()> {
    let previous = Account::load()?;
    let server = server.or_else(|| previous.as_ref().map(|a| a.server.clone())).unwrap_or_else(super::default_server);
    let anon = Api::new(&server, None)?;
    let new_password = read_new("New password (8 characters or more): ", from_stdin)?;
    let kdf = PasswordKdf::generate();
    let keys = crypto::derive_password_keys(&new_password, &kdf).map_err(|e| anyhow!("{e}"))?;
    let code = code.rsplit('#').next().unwrap_or(code).trim();
    let r = anon.raw("POST", "/v1/auth/password/reset", Some(json!({ "token": code, "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })))?;
    if r.status != 200 {
        return Err(api_error(&r));
    }
    let email = r.json["user"]["email"].as_str().unwrap_or("").to_string();
    let user_id = r.json["user"]["id"].as_str().unwrap_or("").to_string();
    let kept = previous.as_ref().is_some_and(|a| a.user_id == user_id && a.server.trim_end_matches('/') == server.trim_end_matches('/'));
    // This computer's device (if it is this account's) stays and seals itself under the new
    // password. A new computer gets a new key but doesn't claim the password's bundle: it holds no
    // workspace key, and the next password sign-in of a device that does (or the recovery phrase)
    // should be what the password unlocks.
    super::finish_login(&server, &r.json, &email, name, kept.then_some(keys.bundle_key))?;
    println!("Password reset for {email}.");
    if r.json["bundleLost"] == true {
        eprintln!("{} the key bundle your old password sealed is gone: nobody could open it any more.", "note:".yellow().bold());
    }
    if !kept {
        eprintln!("{} this computer is a new device with no workspace keys. Get them back with your recovery phrase", "note:".yellow().bold());
        eprintln!("(hitop sync recovery use), or from a device that is still signed in (its owner runs hitop sync members and trusts this one).");
    }
    Ok(())
}

/// `hitop sync password unlock`: after a sign-in by code, Google or GitHub, the password's bundle
/// makes this computer the account's device.
pub fn unlock(from_stdin: bool) -> Result<()> {
    let account = Account::load()?.ok_or_else(|| anyhow!("sign in on this computer first: hitop sync login"))?;
    let secret = store::load_secret(&account.server)?;
    let api = Api::new(&account.server, Some(secret.session.clone()))?.with_device(&account.key_id);
    let anon = Api::new(&account.server, None)?;
    let keys = keys_for(&anon, &account.email, &read(&format!("Password for {}: ", account.email), from_stdin)?)?;
    let r = api.raw("GET", "/v1/me/recovery?method=password", None)?;
    if r.status == 404 {
        bail!("{} has no key bundle sealed with a password (set one on your device: hitop sync password set)", account.email);
    }
    if r.status != 200 {
        return Err(api_error(&r));
    }
    let plain = crypto::open_key_bundle(&keys.bundle_key, &account.user_id, &unb64(r.json["blob"].as_str().unwrap_or(""))?).map_err(|_| anyhow!("that password doesn't open your key bundle (a wrong password, or one changed since)"))?;
    let data: RecoveryData = serde_json::from_slice(&plain).context("the key bundle is damaged")?;
    let mut account = account;
    let fp = recovery::become_device(&api, &mut account, &secret.session, &data, secret.recovery.clone(), Some(b64(&keys.bundle_key)))?;
    store::save_recovery_hash(&recovery::digest(&plain))?;
    println!("Unlocked: this computer is your device \"{}\" (key fingerprint {}).", account.device_name, fp.bold());
    Ok(())
}
