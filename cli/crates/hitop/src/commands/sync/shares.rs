//! Shared environment values (HITOP Cloud C4), opt-in per environment: `hitop sync secrets share
//! --env prod` seals the environment's secret values (from your keychain or HITOP_SECRET_*) for the
//! devices you pick: by default every trusted device of the workspace's owners and editors, with
//! `--viewers` the viewers' too, with `--runner <id>` (or `--runners`) runner credentials. The
//! server can't read them (hitop-sync-crypto `seal_share`, a key wrapped per device with
//! `seal_share_wrap`). Values never go into the folder's files.
//!
//! A run merges them under its own values (`hitop run --sync-secrets`: an env var, `--secret` or
//! your keychain still wins). Whoever adds a member or trusts a new device here hands the share to
//! it in the same command; the sharer's next sync does it for any device without a copy. Removing a
//! member re-keys every share this device can open; the values the removed person saw are theirs,
//! so rotate those secrets where they come from.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{anyhow, bail, Result};
use colored::Colorize;
use hitop_sync_crypto as crypto;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::client::{b64, unb64, Api, Device, Trust};
use super::{Recipient, TrustArgs};

/// What a share seals: the environment, its values and who it is for.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub v: u32,
    pub environment: String,
    pub values: BTreeMap<String, String>,
    /// Viewers' devices get it too.
    #[serde(default)]
    pub viewers: bool,
    /// Runner credentials (user ids) that get it.
    #[serde(default)]
    pub runners: Vec<String>,
    /// Every runner credential of the workspace gets it.
    #[serde(default)]
    pub all_runners: bool,
}

/// A share this device opened.
pub struct Opened {
    pub id: String,
    pub version: u32,
    pub content: Content,
    pub key: crypto::WorkspaceKey,
    /// Device key ids that have a wrap of this version.
    pub wrapped_for: Vec<String>,
}

/// Every share of the workspace this device can open, and how many it can't (not shared with it,
/// or from a device it doesn't trust).
pub fn open_all(api: &Api, device: &Device, ws: &str) -> Result<(Vec<Opened>, Vec<String>)> {
    let reply = api.raw("GET", &format!("/v1/workspaces/{ws}/env-shares"), None)?;
    // A server from before C4 has no shares.
    if reply.status == 404 && reply.json["message"] == "no such endpoint" {
        return Ok((vec![], vec![]));
    }
    if !(200..300).contains(&reply.status) {
        return Err(super::client::api_error(&reply));
    }
    let r = reply.json;
    let pins: BTreeMap<String, Vec<u8>> = match &device.trust {
        Trust::Local => super::store::pinned_keys()?,
        Trust::Runner(p) => p.clone(),
    };
    let mut versions = if matches!(device.trust, Trust::Local) { super::store::load_share_versions()? } else { Default::default() };
    let mut opened = Vec::new();
    let mut closed = Vec::new();
    for s in r["shares"].as_array().cloned().unwrap_or_default() {
        let id = s["id"].as_str().unwrap_or("").to_string();
        let version = s["version"].as_u64().unwrap_or(0) as u32;
        let wraps = s["wraps"].as_array().cloned().unwrap_or_default();
        let wrapped_for: Vec<String> = wraps.iter().filter_map(|w| w["keyId"].as_str().map(String::from)).collect();
        let Some(mine) = wraps.iter().find(|w| w["keyId"] == device.key_id.as_str()) else {
            closed.push(format!("{id}: not shared with this device"));
            continue;
        };
        let wrapped = unb64(mine["wrapped"].as_str().unwrap_or(""))?;
        let sender = crypto::share_wrap_sender(&wrapped).map_err(|e| anyhow!("share {id}: {e}"))?;
        let sender_pk = if sender == device.key_id { Some(device.keys.public_key().to_vec()) } else { pins.get(&sender).cloned() };
        let Some(sender_pk) = sender_pk else {
            closed.push(format!("{id}: shared by device {sender}, which this device doesn't trust (compare its fingerprint, then `hitop sync trust`)"));
            continue;
        };
        let remembered = versions.get(ws).and_then(|m| m.get(&id)).copied().unwrap_or(0);
        if version < remembered {
            bail!("{} the server hands out version {version} of shared environment {id}, but this device opened version {remembered} before: refusing an older version", "SECURITY:".red().bold());
        }
        let key = crypto::open_share_wrap(&device.keys, &device.key_id, &sender_pk, ws, &id, version, &wrapped)
            .map_err(|e| anyhow!("{} the shared environment {id} claims to come from device {sender} but doesn't verify against its trusted key: {e}", "SECURITY:".red().bold()))?;
        let text = crypto::open_share(&key, ws, &id, version, &unb64(s["sealed"].as_str().unwrap_or(""))?).map_err(|e| anyhow!("{} shared environment {id} doesn't open: {e}", "SECURITY:".red().bold()))?;
        let content: Content = serde_json::from_slice(&text).map_err(|e| anyhow!("shared environment {id}: {e}"))?;
        versions.entry(ws.to_string()).or_default().insert(id.clone(), version);
        opened.push(Opened { id, version, content, key, wrapped_for });
    }
    if matches!(device.trust, Trust::Local) {
        super::store::save_share_versions(&versions)?;
    }
    Ok((opened, closed))
}

/// The devices a share is for, from `GET /v1/workspaces/:ws`.
fn intended(info: &Value, c: &Content) -> Result<Vec<Recipient>> {
    let mut out = Vec::new();
    for m in info["members"].as_array().cloned().unwrap_or_default() {
        let runner = m["runner"].as_bool().unwrap_or(false);
        let user = m["userId"].as_str().unwrap_or("");
        let wanted = if runner {
            c.all_runners || c.runners.iter().any(|r| r == user)
        } else {
            match m["role"].as_str().unwrap_or("") {
                "owner" | "editor" => true,
                "viewer" => c.viewers,
                _ => false,
            }
        };
        if wanted {
            let who = if runner { format!("runner credential {user}") } else { m["email"].as_str().unwrap_or("").to_string() };
            out.extend(super::keys_of(&who, &m["keys"])?);
        }
    }
    Ok(out)
}

fn wraps(me: &Device, to: &[Recipient], ws: &str, share: &str, version: u32, key: &crypto::WorkspaceKey) -> Result<Value> {
    let mut out = Vec::new();
    for r in to {
        let w = crypto::seal_share_wrap(&me.crypto(), &crypto::Recipient { key_id: &r.key_id, public_key: &r.public_key }, ws, share, version, key).map_err(|e| anyhow!("{e}"))?;
        out.push(json!({ "keyId": r.key_id, "wrapped": b64(&w) }));
    }
    Ok(Value::Array(out))
}

/// Only trusted devices get values; the rest are named (they get them once trusted).
fn trusted(recipients: Vec<Recipient>, me: &Device) -> Result<(Vec<Recipient>, Vec<String>)> {
    let pins = super::store::load_pins()?;
    let mut ok = Vec::new();
    let mut not = Vec::new();
    for r in recipients {
        if r.key_id == me.key_id || pins.get(&r.key_id) == Some(&b64(&r.public_key)) {
            ok.push(r);
        } else {
            not.push(format!("{} ({}, {})", r.owner, r.name, crypto::fingerprint(&r.public_key)));
        }
    }
    Ok((ok, not))
}

/// Writes a new version of a share (a new key, wraps for every trusted intended device).
fn write(api: &Api, me: &Device, ws: &str, info: &Value, id: &str, version: u32, content: &Content) -> Result<(usize, Vec<String>)> {
    let (to, untrusted) = trusted(intended(info, content)?, me)?;
    if !to.iter().any(|r| r.key_id == me.key_id) {
        bail!("this device would not be a recipient of the share (only owners, editors and the viewers or runners you name are)");
    }
    let key = crypto::WorkspaceKey::generate();
    let sealed = crypto::seal_share(&key, ws, id, version, &serde_json::to_vec(content)?).map_err(|e| anyhow!("{e}"))?;
    api.call("PUT", &format!("/v1/workspaces/{ws}/env-shares/{id}"), Some(json!({ "version": version, "sealed": b64(&sealed), "wraps": wraps(me, &to, ws, id, version, &key)? })))?;
    let mut versions = super::store::load_share_versions()?;
    versions.entry(ws.to_string()).or_default().insert(id.to_string(), version);
    super::store::save_share_versions(&versions)?;
    Ok((to.len(), untrusted))
}

/// `hitop sync secrets share --env <env>`.
#[allow(clippy::too_many_arguments)]
pub fn share(dir: &Path, env: &str, viewers: bool, runners: Vec<String>, all_runners: bool, yes: bool, trust: &TrustArgs, flags: &crate::commands::secret::SecretFlags) -> Result<()> {
    let (s, ws_id, info) = super::team(dir)?;
    if info["role"] == "viewer" {
        bail!("viewers can't share values (an owner or editor does)");
    }
    let source = crate::commands::source::Source::open(&dir.to_string_lossy())?;
    let Some(environment) = source.environment(Some(env))? else { bail!("{} has no environment {env}", dir.display()) };
    if environment.secrets.is_empty() {
        bail!("the environment {env} has no secret variables (`secrets:` in environments/{env}.yaml)");
    }
    let resolved = crate::commands::secret::resolve(&source.workspace(), Some(&environment), flags)?;
    let values: BTreeMap<String, String> = environment.secrets.iter().filter_map(|n| resolved.get(n).map(|v| (n.clone(), v.to_string()))).collect();
    for r in &runners {
        if !info["members"].as_array().is_some_and(|m| m.iter().any(|m| m["userId"] == r.as_str() && m["runner"] == true)) {
            bail!("{r} is not a runner credential of this workspace (`hitop sync members` lists them)");
        }
    }
    let (opened, _) = open_all(&s.api, &s.device, &ws_id)?;
    let existing = opened.iter().find(|o| o.content.environment == env);
    let content = Content { v: 1, environment: env.to_string(), values, viewers, runners, all_runners };
    let recipients = intended(&info, &content)?;
    super::check_trust(&recipients.iter().filter(|r| r.key_id != s.device.key_id).map(|r| Recipient { key_id: r.key_id.clone(), public_key: r.public_key.clone(), owner: r.owner.clone(), name: r.name.clone() }).collect::<Vec<_>>(), trust)?;
    let mut people: Vec<String> = recipients.iter().map(|r| r.owner.clone()).collect();
    people.sort();
    people.dedup();
    eprintln!("Sharing the values of {} secret(s) of {env} ({}) end to end with: {}", content.values.len(), content.values.keys().cloned().collect::<Vec<_>>().join(", "), people.join(", "));
    eprintln!("They can read these values on their devices; the server can't. Removing someone later can't take back what they saw.");
    if !yes {
        if !super::interactive() {
            bail!("pass --yes to share (after checking the list above)");
        }
        let a = super::ask("Share them? [y/N] ")?;
        if !a.eq_ignore_ascii_case("y") && !a.eq_ignore_ascii_case("yes") {
            bail!("not shared");
        }
    }
    let (id, version) = match existing {
        Some(o) => (o.id.clone(), o.version + 1),
        None => (crypto::random_hex::<16>(), 1),
    };
    let (n, untrusted) = write(&s.api, &s.device, &ws_id, &info, &id, version, &content)?;
    println!("Shared {env}'s secret values with {n} device(s) (version {version}).");
    if !untrusted.is_empty() {
        eprintln!("{} not yet for these devices you haven't trusted: {}", "note:".yellow(), untrusted.join(", "));
    }
    Ok(())
}

/// `hitop sync secrets ls`: the shared environments, their secret names (never values) and who has
/// them.
pub fn ls(dir: &Path) -> Result<()> {
    let (s, ws_id, info) = super::team(dir)?;
    let (opened, closed) = open_all(&s.api, &s.device, &ws_id)?;
    if opened.is_empty() && closed.is_empty() {
        println!("No shared environment values. Share one: hitop sync secrets share --env <env> -c {}", dir.display());
    }
    let owner_of = |key: &str| -> String {
        info["members"].as_array().into_iter().flatten().find(|m| m["keys"].as_array().is_some_and(|k| k.iter().any(|k| k["id"] == key))).map(|m| if m["runner"] == true { format!("runner {}", m["userId"].as_str().unwrap_or("")) } else { m["email"].as_str().unwrap_or("").to_string() }).unwrap_or_else(|| "?".into())
    };
    for o in &opened {
        let mut who: Vec<String> = o.wrapped_for.iter().map(|k| owner_of(k)).collect();
        who.sort();
        who.dedup();
        println!("{} (version {}): {}", o.content.environment.bold(), o.version, o.content.values.keys().cloned().collect::<Vec<_>>().join(", "));
        println!("    shared with: {}", who.join(", "));
    }
    for c in &closed {
        println!("{} {c}", "can't open".yellow());
    }
    Ok(())
}

/// `hitop sync secrets unshare --env <env>`.
pub fn unshare(dir: &Path, env: &str) -> Result<()> {
    let (s, ws_id, _) = super::team(dir)?;
    let (opened, _) = open_all(&s.api, &s.device, &ws_id)?;
    let Some(o) = opened.iter().find(|o| o.content.environment == env) else { bail!("{env} isn't shared (or not with this device)") };
    s.api.call("DELETE", &format!("/v1/workspaces/{ws_id}/env-shares/{}", o.id), None)?;
    println!("{env}'s values are no longer shared. Devices that opened them keep what they saw: rotate those secrets where they come from if that matters.");
    Ok(())
}

/// Hands every share this device can open to the intended devices that have no copy yet (a new
/// member, a member's new device, a runner named later), if this device trusts them. Quiet unless
/// it did something; errors are notes (it runs after other commands).
pub fn reseal(api: &Api, me: &Device, ws: &str) {
    let run = || -> Result<usize> {
        let (opened, _) = open_all(api, me, ws)?;
        if opened.is_empty() {
            return Ok(0);
        }
        let info = api.call("GET", &format!("/v1/workspaces/{ws}"), None)?;
        if info["role"] == "viewer" {
            return Ok(0);
        }
        let mut n = 0;
        for o in &opened {
            let missing: Vec<Recipient> = intended(&info, &o.content)?.into_iter().filter(|r| !o.wrapped_for.contains(&r.key_id)).collect();
            let (to, _) = trusted(missing, me)?;
            if to.is_empty() {
                continue;
            }
            let r = api.call("POST", &format!("/v1/workspaces/{ws}/env-shares/{}/wraps", o.id), Some(json!({ "version": o.version, "wraps": wraps(me, &to, ws, &o.id, o.version, &o.key)? })))?;
            n += r["stored"].as_u64().unwrap_or(0) as usize;
        }
        Ok(n)
    };
    match run() {
        Ok(0) => {}
        Ok(n) => eprintln!("Handed the shared environment values to {n} new device(s)."),
        Err(e) => eprintln!("{} shared environment values not handed to new devices: {e:#}", "note:".yellow()),
    }
}

/// After a member was removed: a new key (and version) for every share this device can open, for
/// the devices still meant to have it. Returns how many shares were re-keyed, and whether some
/// couldn't be (shared without this device).
pub fn rekey_all(api: &Api, me: &Device, ws: &str) -> Result<(usize, usize)> {
    let (opened, closed) = open_all(api, me, ws)?;
    let info = api.call("GET", &format!("/v1/workspaces/{ws}"), None)?;
    let mut n = 0;
    for o in &opened {
        write(api, me, ws, &info, &o.id, o.version + 1, &o.content)?;
        n += 1;
    }
    Ok((n, closed.len()))
}

/// The shared values of environment `env` for a run, as keychain accounts of `keychain_ws`
/// (`<workspace>/<env>/<name>`), so they slot in under the run's own values.
pub fn values_for_run(dir: &Path, env: &str, keychain_ws: &str) -> Result<BTreeMap<String, String>> {
    let t = super::target(dir)?;
    let (opened, closed) = open_all(&t.api, &t.device, &t.workspace)?;
    let found = opened.into_iter().find(|o| o.content.environment == env);
    match found {
        Some(o) => Ok(o.content.values.into_iter().map(|(k, v)| (hitop_core::secrets::account(keychain_ws, env, &k), v)).collect()),
        None => {
            if !closed.is_empty() {
                eprintln!("{} {} shared environment(s) this device can't open: {}", "note:".yellow(), closed.len(), closed.join("; "));
            }
            Ok(BTreeMap::new())
        }
    }
}
