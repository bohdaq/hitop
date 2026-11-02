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

/// What the recovery blob holds (JSON). The web app writes and reads the same shape.
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

fn digest(bytes: &[u8]) -> String {
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
    let keys = crypto::DeviceKeyPair::from_secret_bytes(&unb64(&data.secret)?).map_err(|e| anyhow!("{e}"))?;
    // The device must still be on the account, with the same public key.
    let me = api.call("GET", "/v1/me", None)?;
    let listed = me["keys"].as_array().cloned().unwrap_or_default().into_iter().find(|k| k["id"] == data.key_id.as_str());
    match listed {
        Some(k) if k["publicKey"].as_str().map(unb64).transpose()?.as_deref() == Some(&keys.public_key()[..]) => {}
        Some(_) => bail!("the server lists another key for the recovered device {}: refusing it", data.key_id),
        None => bail!("the recovered device {} is no longer on this account", data.key_id),
    }
    let fresh = account.key_id.clone();
    // This computer becomes the recovered device: its key, its trust, its memory.
    let mut pins = store::load_pins()?;
    pins.remove(&fresh);
    pins.extend(data.pins.clone());
    pins.insert(data.key_id.clone(), b64(&keys.public_key()));
    store::save_pins(&pins)?;
    let mut memory = store::load_all_memory()?;
    for (ws, m) in &data.memory {
        memory.insert(ws.clone(), m.clone());
    }
    store::save_all_memory(&memory)?;
    let new_secret = store::Secret { session: secret.session.clone(), secret: data.secret.clone(), recovery: Some(b64(phrase.entropy())) };
    store::save_secret(&account.server, &new_secret)?;
    account.key_id = data.key_id.clone();
    if !data.device_name.is_empty() {
        account.device_name = data.device_name.clone();
    }
    account.save()?;
    store::save_recovery_hash(&digest(&plain))?;
    if fresh != data.key_id {
        let d = api.raw("DELETE", &format!("/v1/me/keys/{fresh}"), None)?;
        if !(200..300).contains(&d.status) && d.status != 404 {
            eprintln!("{} the key this computer registered at sign-in ({fresh}) stays on your account: {}", "note:".yellow(), d.json["message"].as_str().unwrap_or(""));
        }
    }
    println!("Recovered the device \"{}\" (key fingerprint {}) on this computer.", account.device_name, crypto::fingerprint(&keys.public_key()).bold());
    println!("Your workspaces: hitop sync ls; link a folder: hitop sync link <workspace> -c <folder>");
    Ok(())
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

/// After a sync command: when this device made the account's recovery phrase and its pins or
/// memory changed since the last upload, re-seal and upload the blob. Never fails the command.
pub fn refresh() {
    let run = || -> Result<()> {
        let Some(account) = Account::load()? else { return Ok(()) };
        let Ok(secret) = store::load_secret(&account.server) else { return Ok(()) };
        let Some(phrase) = phrase_of(&secret)? else { return Ok(()) };
        let plain = serde_json::to_vec(&data(&account, &secret)?)?;
        if store::load_recovery_hash()?.as_deref() == Some(digest(&plain).as_str()) {
            return Ok(());
        }
        let api = Api::new(&account.server, Some(secret.session.clone()))?;
        upload(&api, &account, &phrase, &plain)
    };
    if let Err(e) = run() {
        eprintln!("{} couldn't update your recovery data: {e}", "warning:".yellow());
    }
}

fn time(at: i64) -> String {
    hitop_core::report::rfc3339(std::time::UNIX_EPOCH + std::time::Duration::from_secs(at.max(0) as u64))
}
