//! `hitop sync recovery create|use|status|remove`: a 24-word recovery phrase that brings this
//! account's device back on a new computer after the old one is lost (sync-server/THREAT_MODEL.md
//! "Recovery").
//!
//! The phrase seals a **recovery blob** (`hitop_sync_crypto::seal_recovery`, bound to the account
//! id) that the server keeps: this device's key (id, secret, name), its pins and what it remembers
//! about each workspace's keys. `use` on a new computer (after `hitop sync login` there) opens it
//! and becomes that device again, so the team's wraps for it work and its trust decisions carry
//! over; the key the new login registered is deleted while it holds nothing. The device keeps the
//! phrase's bytes next to its own secret key and re-seals the blob whenever its pins or memory
//! change, so a recovery restores what the device knew last.

use std::collections::BTreeMap;
use std::io::{IsTerminal, Read};

use anyhow::{anyhow, bail, Context, Result};
use colored::Colorize;
use hitop_sync_crypto::{self as crypto, RecoveryPhrase, WorkspaceMemory};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::client::{b64, unb64, Api};
use super::store::{self, Account, Pins};

/// The key bundle the recovery blob holds (JSON; the web app writes and reads the same shape).
/// Sealed under an unlock key: the recovery phrase, and the password's bundle key
/// (`hitop_sync_crypto::seal_key_bundle`, server method `password`; password.rs).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryData {
    pub v: u32,
    pub key_id: String,
    /// The device's X25519 secret key, base64url.
    pub secret: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub pins: Pins,
    #[serde(default)]
    pub memory: BTreeMap<String, WorkspaceMemory>,
}

fn data(account: &Account, secret: &store::Secret) -> Result<RecoveryData> {
    Ok(RecoveryData { v: 1, key_id: account.key_id.clone(), secret: secret.secret.clone(), device_name: account.device_name.clone(), pins: store::load_pins()?, memory: store::load_all_memory()? })
}

pub(super) fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn phrase_of(secret: &store::Secret) -> Result<Option<RecoveryPhrase>> {
    let Some(b) = &secret.recovery else { return Ok(None) };
    let bytes: [u8; 32] = unb64(b)?.try_into().map_err(|_| anyhow!("the stored recovery phrase is damaged"))?;
    Ok(Some(RecoveryPhrase::from_entropy(bytes)))
}

/// Seals and uploads the blob; remembers what was uploaded.
fn upload(api: &Api, account: &Account, phrase: &RecoveryPhrase, plain: &[u8]) -> Result<()> {
    let sealed = crypto::seal_recovery(phrase, &account.user_id, plain).map_err(|e| anyhow!("{e}"))?;
    api.call("PUT", "/v1/me/recovery", Some(json!({ "blob": b64(&sealed) })))?;
    store::save_recovery_hash(&digest(plain))
}

pub fn create() -> Result<()> {
    let account = Account::load()?.ok_or_else(|| anyhow!("not signed in: run `hitop sync login --email <you>` first"))?;
    let mut secret = store::load_secret(&account.server)?;
    let api = Api::new(&account.server, Some(secret.session.clone()))?;
    let had = !api.call("GET", "/v1/me", None)?["recovery"].is_null();
    let phrase = RecoveryPhrase::generate();
    let plain = serde_json::to_vec(&data(&account, &secret)?)?;
    upload(&api, &account, &phrase, &plain)?;
    // The password's bundle (if this device holds it) gets the same plaintext, so the one hash
    // stays true for both.
    if let Some(key) = live_password_key(&api, &account, &mut secret)? {
        api.call("PUT", "/v1/me/recovery?method=password", Some(json!({ "blob": seal_for_password(&account, &key, &plain)? })))?;
    }
    secret.recovery = Some(b64(phrase.entropy()));
    store::save_secret(&account.server, &secret)?;
    eprintln!("Your recovery phrase for {} on {}. Write it down and keep it offline (paper, a password manager).", account.email, account.server);
    eprintln!("Anyone with it and access to your mailbox can become this device; hitop shows it only now.");
    if had {
        eprintln!("{} it replaces your previous phrase, which no longer recovers anything.", "note:".yellow());
    }
    println!("{}", phrase.to_words());
    eprintln!("On a new computer: hitop sync login --email {}, then hitop sync recovery use", account.email);
    Ok(())
}

fn read_phrase(file: Option<&str>) -> Result<String> {
    if let Some(f) = file {
        return if f == "-" {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            Ok(s)
        } else {
            std::fs::read_to_string(f).with_context(|| f.to_string())
        };
    }
    if std::io::stdin().is_terminal() {
        return super::ask("Recovery phrase (24 words): ");
    }
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    Ok(s)
}

pub fn use_phrase(file: Option<&str>) -> Result<()> {
    let mut account = Account::load()?.ok_or_else(|| anyhow!("sign in on this computer first: hitop sync login --email <you>, then hitop sync recovery use"))?;
    let secret = store::load_secret(&account.server)?;
    let api = Api::new(&account.server, Some(secret.session.clone()))?;
    let phrase = RecoveryPhrase::parse(&read_phrase(file)?).map_err(|e| anyhow!("{e}"))?;
    let r = api.raw("GET", "/v1/me/recovery", None)?;
    if r.status == 404 {
        bail!("{} has no recovery phrase set on {}", account.email, account.server);
    }
    if !(200..300).contains(&r.status) {
        return Err(super::client::api_error(&r));
    }
    let sealed = unb64(r.json["blob"].as_str().unwrap_or(""))?;
    let plain = crypto::open_recovery(&phrase, &account.user_id, &sealed).map_err(|_| anyhow!("this recovery phrase doesn't open {}'s recovery data (a wrong phrase, or one replaced since)", account.email))?;
    let data: RecoveryData = serde_json::from_slice(&plain).context("the recovery data is damaged")?;
    // A password key this computer holds keeps sealing the same device (the next sync re-seals).
    let password = secret.password.clone();
    let fp = become_device(&api, &mut account, &secret.session, &data, Some(b64(phrase.entropy())), password)?;
    store::save_recovery_hash(&digest(&plain))?;
    println!("Recovered the device \"{}\" (key fingerprint {}) on this computer.", account.device_name, fp.bold());
    println!("Your workspaces: hitop sync ls; link a folder: hitop sync link <workspace> -c <folder>");
    Ok(())
}

/// Makes this computer the device in `data` (opened with the recovery phrase or the password):
/// its key, its trust, its memory. The device must still be on the account with the same public
/// key; the key this computer registered at sign-in (if any) is deleted while it holds nothing.
/// Returns the device's key fingerprint.
pub(super) fn become_device(api: &Api, account: &mut Account, session: &str, data: &RecoveryData, recovery: Option<String>, password: Option<String>) -> Result<String> {
    let keys = crypto::DeviceKeyPair::from_secret_bytes(&unb64(&data.secret)?).map_err(|e| anyhow!("{e}"))?;
    let me = api.call("GET", "/v1/me", None)?;
    let listed = me["keys"].as_array().cloned().unwrap_or_default().into_iter().find(|k| k["id"] == data.key_id.as_str());
    match listed {
        Some(k) if k["publicKey"].as_str().map(unb64).transpose()?.as_deref() == Some(&keys.public_key()[..]) => {}
        Some(_) => bail!("the server lists another key for the device {} in your key bundle: refusing it", data.key_id),
        None => bail!("the device {} in your key bundle is no longer on this account", data.key_id),
    }
    let fresh = account.key_id.clone();
    let mut pins = store::load_pins()?;
    if !fresh.is_empty() && fresh != data.key_id {
        pins.remove(&fresh);
    }
    pins.extend(data.pins.clone());
    pins.insert(data.key_id.clone(), b64(&keys.public_key()));
    store::save_pins(&pins)?;
    let mut memory = store::load_all_memory()?;
    for (ws, m) in &data.memory {
        memory.insert(ws.clone(), m.clone());
    }
    store::save_all_memory(&memory)?;
    store::save_secret(&account.server, &store::Secret { session: session.to_string(), secret: data.secret.clone(), recovery, password })?;
    account.key_id = data.key_id.clone();
    if !data.device_name.is_empty() {
        account.device_name = data.device_name.clone();
    }
    account.save()?;
    if !fresh.is_empty() && fresh != data.key_id {
        let d = api.raw("DELETE", &format!("/v1/me/keys/{fresh}"), None)?;
        if !(200..300).contains(&d.status) && d.status != 404 {
            eprintln!("{} the key this computer registered at sign-in ({fresh}) stays on your account: {}", "note:".yellow(), d.json["message"].as_str().unwrap_or(""));
        }
    }
    Ok(crypto::fingerprint(&keys.public_key()))
}

/// The key bundle of this device, as JSON (what the phrase and the password seal).
pub(super) fn bundle_plaintext(account: &Account, secret: &store::Secret) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&data(account, secret)?)?)
}

/// Seals this device's bundle under a password's bundle key for the server (`?method=password`).
pub(super) fn seal_for_password(account: &Account, bundle_key: &[u8; 32], plain: &[u8]) -> Result<String> {
    Ok(b64(&crypto::seal_key_bundle(bundle_key, &account.user_id, plain).map_err(|e| anyhow!("{e}"))?))
}

/// The bundle key this device holds for the password, when it is still the one that opens the
/// server's password bundle and that bundle is this device's. Otherwise (the password changed or
/// was reset elsewhere, or the bundle is another device's) the device forgets the key, so it
/// never overwrites a newer bundle.
fn live_password_key(api: &Api, account: &Account, secret: &mut store::Secret) -> Result<Option<[u8; 32]>> {
    let Some(k) = &secret.password else { return Ok(None) };
    let key: [u8; 32] = unb64(k)?.try_into().map_err(|_| anyhow!("the stored password key is damaged"))?;
    let r = api.raw("GET", "/v1/me/recovery?method=password", None)?;
    let ours = r.status == 200
        && crypto::open_key_bundle(&key, &account.user_id, &unb64(r.json["blob"].as_str().unwrap_or(""))?)
            .ok()
            .and_then(|p| serde_json::from_slice::<RecoveryData>(&p).ok())
            .is_some_and(|d| d.key_id == account.key_id);
    // No bundle at all (deleted by a change without one): seal ours again under the same key.
    if ours || r.status == 404 {
        return Ok(Some(key));
    }
    secret.password = None;
    store::save_secret(&account.server, secret)?;
    Ok(None)
}

pub fn status() -> Result<()> {
    let account = Account::load()?.ok_or_else(|| anyhow!("not signed in"))?;
    let secret = store::load_secret(&account.server)?;
    let api = Api::new(&account.server, Some(secret.session.clone()))?;
    let me = api.call("GET", "/v1/me", None)?;
    match me["recovery"]["updatedAt"].as_i64() {
        Some(at) => println!("Recovery phrase set; recovery data last updated {}", time(at)),
        None => println!("No recovery phrase: `hitop sync recovery create` makes one"),
    }
    if secret.recovery.is_some() {
        println!("This device keeps the recovery data up to date after each sync command.");
    } else if !me["recovery"].is_null() {
        println!("This device didn't make the phrase, so it doesn't update the recovery data.");
    }
    Ok(())
}

pub fn remove() -> Result<()> {
    let account = Account::load()?.ok_or_else(|| anyhow!("not signed in"))?;
    let mut secret = store::load_secret(&account.server)?;
    let api = Api::new(&account.server, Some(secret.session.clone()))?;
    api.call("DELETE", "/v1/me/recovery", None)?;
    secret.recovery = None;
    store::save_secret(&account.server, &secret)?;
    println!("Removed the recovery data: the phrase no longer recovers anything.");
    Ok(())
}

/// After a sync command: when this device holds the account's recovery phrase or password key
/// and its pins or memory changed since the last upload, re-seal and upload its bundle for each.
/// Never fails the command.
pub fn refresh() {
    let run = || -> Result<()> {
        let Some(account) = Account::load()? else { return Ok(()) };
        let Ok(mut secret) = store::load_secret(&account.server) else { return Ok(()) };
        let phrase = phrase_of(&secret)?;
        if phrase.is_none() && secret.password.is_none() {
            return Ok(());
        }
        let plain = serde_json::to_vec(&data(&account, &secret)?)?;
        if store::load_recovery_hash()?.as_deref() == Some(digest(&plain).as_str()) {
            return Ok(());
        }
        let api = Api::new(&account.server, Some(secret.session.clone()))?;
        if let Some(key) = live_password_key(&api, &account, &mut secret)? {
            api.call("PUT", "/v1/me/recovery?method=password", Some(json!({ "blob": seal_for_password(&account, &key, &plain)? })))?;
        }
        match phrase {
            Some(p) => upload(&api, &account, &p, &plain),
            None => store::save_recovery_hash(&digest(&plain)),
        }
    };
    if let Err(e) = run() {
        eprintln!("{} couldn't update your recovery data: {e}", "warning:".yellow());
    }
}

fn time(at: i64) -> String {
    hitop_core::report::rfc3339(std::time::UNIX_EPOCH + std::time::Duration::from_secs(at.max(0) as u64))
}
