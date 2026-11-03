//! The sync server's API (sync-server/README.md) and the encryption around it: every blob is
//! sealed and opened here with `hitop-sync-crypto`; the server only ever sees envelopes.

use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hitop_core::sync::{BlobText, Push, RemoteBlob};
use colored::Colorize;
use hitop_sync_crypto::{self as crypto, BlobContext, DeviceKeyPair, Error as CryptoError, ReceivedWrap, WorkspaceKey, WrapHeader};
use serde_json::{json, Value};

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).context("base64url")
}

/// An answer the caller handles itself (a 409 conflict, say).
pub struct Reply {
    pub status: u16,
    pub json: Value,
}

pub struct Api {
    base: String,
    session: Option<String>,
    http: reqwest::blocking::Client,
}

impl Api {
    pub fn new(server: &str, session: Option<String>) -> Result<Api> {
        let base = server.trim_end_matches('/').to_string();
        let url = url::Url::parse(&base).with_context(|| format!("{base} is not a URL"))?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if url.scheme() != "https" && !(url.scheme() == "http" && local) {
            bail!("the sync server must be https:// (http:// only for localhost)");
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(concat!("hitop/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Api { base, session, http })
    }

    pub fn raw(&self, method: &str, path: &str, body: Option<Value>) -> Result<Reply> {
        let url = format!("{}{path}", self.base);
        let method = reqwest::Method::from_bytes(method.as_bytes())?;
        let mut req = self.http.request(method, &url);
        if let Some(s) = &self.session {
            req = req.bearer_auth(s);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let res = req.send().with_context(|| format!("can't reach the sync server {}", self.base))?;
        let status = res.status().as_u16();
        let text = res.text().unwrap_or_default();
        let json = serde_json::from_str(&text).unwrap_or(Value::Null);
        Ok(Reply { status, json })
    }

    /// A GET whose answer isn't JSON (the audit export): the bytes as they came, or the error.
    pub fn bytes(&self, path: &str) -> Result<Vec<u8>> {
        let mut req = self.http.get(format!("{}{path}", self.base));
        if let Some(s) = &self.session {
            req = req.bearer_auth(s);
        }
        let res = req.send().with_context(|| format!("can't reach the sync server {}", self.base))?;
        let status = res.status().as_u16();
        let body = res.bytes().context("reading the answer")?.to_vec();
        if (200..300).contains(&status) {
            return Ok(body);
        }
        Err(api_error(&Reply { status, json: serde_json::from_slice(&body).unwrap_or(Value::Null) }))
    }

    /// A call that must succeed (2xx); anything else is an error with the server's message.
    pub fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let r = self.raw(method, path, body)?;
        if (200..300).contains(&r.status) {
            return Ok(r.json);
        }
        Err(api_error(&r))
    }
}

/// A refused key as the person should read it: what happened and what to do.
fn key_error(e: CryptoError, runner: bool) -> anyhow::Error {
    let hint = match &e {
        CryptoError::UntrustedSender { fingerprint: Some(fp), .. } if !runner => format!("\nIf it is right, run: hitop sync trust '{fp}'"),
        CryptoError::UntrustedSender { .. } if runner => "\nThis runner credential doesn't trust the device that wrapped the key: make a new credential (`hitop sync runner create`)".into(),
        CryptoError::MissingKey { .. } => ": ask an owner or editor to run `hitop sync members` (it wraps the key for new devices)".into(),
        CryptoError::LegacyRefused { .. } if runner => "\nMake a new runner credential with hitop 1.2 or newer (`hitop sync runner create`)".into(),
        _ => String::new(),
    };
    let text = format!("{e}{hint}");
    match e {
        CryptoError::Forged { .. } | CryptoError::KeyChanged { .. } | CryptoError::Rollback { .. } => anyhow!("{} {text}", "SECURITY:".red().bold()),
        _ => anyhow!("{text}"),
    }
}

pub fn api_error(r: &Reply) -> anyhow::Error {
    let message = r.json["message"].as_str().unwrap_or("no message");
    let hint = match r.status {
        401 => " (sign in again: hitop sync login)",
        403 => " (your role in this workspace doesn't allow it)",
        _ => "",
    };
    anyhow!("sync server: {message}{hint} [{} {}]", r.status, r.json["error"].as_str().unwrap_or(""))
}

/// Where a device's trusted keys (the devices it accepts workspace keys from) come from.
pub enum Trust {
    /// This computer's pins (`pins.json`) and what it remembers per workspace (`keys.json`).
    Local,
    /// A runner credential's own pins (key id → public key); nothing is remembered between runs.
    /// Empty for a credential made by hitop 1.1, which then accepts only unauthenticated keys.
    Runner(BTreeMap<String, Vec<u8>>),
}

/// A signed-in device: what it needs to open and seal a workspace's blobs.
pub struct Device {
    pub key_id: String,
    pub keys: DeviceKeyPair,
    pub trust: Trust,
}

impl Device {
    pub fn crypto(&self) -> crypto::Device<'_> {
        crypto::Device { key_id: &self.key_id, keys: &self.keys }
    }
}

/// Device key id → public key, as `GET /v1/workspaces/:ws` lists every member's devices.
pub fn listed_keys(info: &Value) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut out = BTreeMap::new();
    for m in info["members"].as_array().cloned().unwrap_or_default() {
        for k in m["keys"].as_array().cloned().unwrap_or_default() {
            out.insert(k["id"].as_str().unwrap_or("").to_string(), unb64(k["publicKey"].as_str().unwrap_or(""))?);
        }
    }
    Ok(out)
}

/// One workspace as this device sees it: its epoch keys and the server's current epoch.
pub struct Workspace<'a> {
    pub api: &'a Api,
    pub device: &'a Device,
    pub id: String,
    pub keys: BTreeMap<u32, WorkspaceKey>,
    pub epoch: u32,
    /// Some of this device's own wraps are format 1 (unauthenticated, hitop 1.1): an owner's
    /// client upgrades the workspace (`upgrade_wraps` in mod.rs).
    pub legacy: bool,
}

impl<'a> Workspace<'a> {
    pub fn open(api: &'a Api, device: &'a Device, id: &str) -> Result<Workspace<'a>> {
        let mut ws = Workspace { api, device, id: id.to_string(), keys: BTreeMap::new(), epoch: 0, legacy: false };
        ws.refresh_keys()?;
        Ok(ws)
    }

    /// Opens every epoch's key wrapped for this device and keeps only what
    /// `hitop_sync_crypto::accept_wraps` allows: wraps verified against a trusted device key, no
    /// key that changed for an epoch seen before, no epoch older than one seen before.
    pub fn refresh_keys(&mut self) -> Result<()> {
        let r = self.api.call("GET", &format!("/v1/workspaces/{}/wraps?keyId={}", self.id, self.device.key_id), None)?;
        let epoch = r["epoch"].as_u64().unwrap_or(0) as u32;
        let mut wraps = Vec::new();
        for w in r["wraps"].as_array().cloned().unwrap_or_default() {
            wraps.push((w["epoch"].as_u64().unwrap_or(0) as u32, unb64(w["wrapped"].as_str().unwrap_or(""))?));
        }
        let runner = matches!(self.device.trust, Trust::Runner(_));
        let (pinned, mut memory) = match &self.device.trust {
            Trust::Local => (super::store::pinned_keys()?, super::store::load_memory(&self.id)?),
            // A runner keeps no memory; one with pins (hitop 1.2) accepts authenticated keys only.
            Trust::Runner(pins) => (pins.clone(), crypto::WorkspaceMemory { strict: !pins.is_empty(), ..Default::default() }),
        };
        // The server's member list only when a sender isn't trusted: for its fingerprint, or to
        // match the inviter's fingerprint from an invite code.
        let unknown_sender = wraps.iter().any(|(_, w)| matches!(crypto::wrap_header(w), Ok(WrapHeader::Authenticated { sender_key_id, .. }) if !pinned.contains_key(&sender_key_id)));
        let listed = if unknown_sender { listed_keys(&self.api.call("GET", &format!("/v1/workspaces/{}", self.id), None)?)? } else { BTreeMap::new() };
        self.legacy = wraps.iter().any(|(_, w)| crypto::wrap_header(w) == Ok(WrapHeader::Legacy));
        let received: Vec<ReceivedWrap> = wraps.iter().map(|(e, w)| ReceivedWrap { epoch: *e, wrapped: w }).collect();
        let ring = crypto::accept_wraps(&self.device.crypto(), &self.id, epoch, &received, &pinned, &listed, &mut memory).map_err(|e| key_error(e, runner))?;
        if !runner {
            if !ring.new_pins.is_empty() {
                let mut pins = super::store::load_pins()?;
                for (id, pk) in &ring.new_pins {
                    pins.insert(id.clone(), b64(pk));
                }
                super::store::save_pins(&pins)?;
            }
            super::store::save_memory(&self.id, &memory)?;
        }
        for n in &ring.notices {
            let label = match n {
                crypto::Notice::SenderKeyChanged { .. } => "warning:".red().bold(),
                crypto::Notice::SenderPinned { .. } | crypto::Notice::KnownKeyFromUntrusted { .. } => "note:".cyan(),
                _ => "warning:".yellow(),
            };
            eprintln!("{label} {n}");
        }
        if ring.uses_legacy() && matches!(&self.device.trust, Trust::Runner(p) if p.is_empty()) {
            eprintln!("{} this runner credential was made by hitop 1.1: make a new one (`hitop sync runner create`) and remove the old one", "warning:".yellow());
        }
        self.epoch = ring.current;
        self.keys = ring.keys;
        Ok(())
    }

    pub fn current_key(&self) -> &WorkspaceKey {
        &self.keys[&self.epoch]
    }

    /// Every change after `since`, decrypted, and the server's sequence number.
    pub fn changes(&self, since: u64) -> Result<(Vec<RemoteBlob>, u64)> {
        let mut out = Vec::new();
        let mut cursor = since;
        let mut seq;
        loop {
            let r = self.api.call("GET", &format!("/v1/workspaces/{}/changes?since={cursor}", self.id), None)?;
            seq = r["seq"].as_u64().unwrap_or(0);
            let changes = r["changes"].as_array().cloned().unwrap_or_default();
            for c in &changes {
                cursor = cursor.max(c["seq"].as_u64().unwrap_or(0));
                out.push(self.fetch(c)?);
            }
            if !r["more"].as_bool().unwrap_or(false) || changes.is_empty() {
                break;
            }
        }
        Ok((out, seq))
    }

    fn fetch(&self, meta: &Value) -> Result<RemoteBlob> {
        let blob = meta["id"].as_str().unwrap_or("").to_string();
        let vv = serde_json::from_value(meta["vv"].clone()).unwrap_or_default();
        let version = meta["version"].as_u64().unwrap_or(0);
        if meta["deleted"].as_bool().unwrap_or(false) {
            return Ok(RemoteBlob { blob, version, vv, deleted: true, path: None, text: None });
        }
        let r = self.api.call("GET", &format!("/v1/workspaces/{}/blobs/{blob}", self.id), None)?;
        // The version asked for must be the one opened: it is in the associated data, so an old
        // envelope served as a newer version fails to decrypt.
        let version = r["version"].as_u64().unwrap_or(version);
        let vv = serde_json::from_value(r["vv"].clone()).unwrap_or(vv);
        let text = self.open_blob(&blob, version, r["content"].as_str().unwrap_or(""))?;
        let b = BlobText::decode(&text).map_err(|e| anyhow!("blob {blob}: {e}"))?;
        Ok(RemoteBlob { blob, version, vv, deleted: false, path: Some(b.path), text: Some(b.text) })
    }

    pub fn open_blob(&self, blob: &str, version: u64, content: &str) -> Result<Vec<u8>> {
        let envelope = unb64(content)?;
        let epoch = crypto::blob_epoch(&envelope).map_err(|e| anyhow!("blob {blob}: {e}"))?;
        let key = self.keys.get(&epoch).ok_or_else(|| anyhow!("blob {blob} is under key epoch {epoch}, which this device doesn't have"))?;
        crypto::decrypt_blob(key, &BlobContext { workspace_id: &self.id, blob_id: blob, version, epoch }, &envelope)
            .map_err(|e| anyhow!("blob {blob} version {version}: {e} (refusing it)"))
    }

    fn seal(&self, blob: &str, version: u64, plaintext: &[u8]) -> Result<String> {
        let ctx = BlobContext { workspace_id: &self.id, blob_id: blob, version, epoch: self.epoch };
        Ok(b64(&crypto::encrypt_blob(self.current_key(), &ctx, plaintext).map_err(|e| anyhow!("{e}"))?))
    }

    /// Sends one write. `Ok(None)`: accepted; `Ok(Some(code))`: refused with 409 (`conflict` or
    /// `stale_epoch`), sync again.
    pub fn put(&self, push: &Push) -> Result<Option<String>> {
        let mut body = json!({ "version": push.version, "vv": push.vv, "epoch": self.epoch });
        match &push.text {
            Some(text) => body["content"] = json!(self.seal(&push.blob, push.version, &BlobText::encode(&push.path, text))?),
            None => body["deleted"] = json!(true),
        }
        let r = self.api.raw("PUT", &format!("/v1/workspaces/{}/blobs/{}", self.id, push.blob), Some(body))?;
        match r.status {
            200 => Ok(None),
            409 => Ok(Some(r.json["error"].as_str().unwrap_or("conflict").to_string())),
            _ => Err(api_error(&r)),
        }
    }

    /// Rewrites every live blob still under an older epoch with the current key (after a member
    /// was removed): what the removed member could read stays readable to them only in the copies
    /// they already have. Returns how many were rewritten.
    pub fn reencrypt_all(&self, me: &str) -> Result<usize> {
        let (all, _) = self.changes(0)?;
        let mut n = 0;
        for b in all.into_iter().filter(|b| !b.deleted) {
            let meta = self.api.call("GET", &format!("/v1/workspaces/{}/blobs/{}", self.id, b.blob), None)?;
            let envelope = unb64(meta["content"].as_str().unwrap_or(""))?;
            if crypto::blob_epoch(&envelope).ok() == Some(self.epoch) {
                continue;
            }
            let mut vv = b.vv.clone();
            *vv.entry(me.to_string()).or_insert(0) += 1;
            let push = Push { path: b.path.clone().unwrap_or_default(), blob: b.blob.clone(), version: b.version + 1, vv, text: b.text.clone() };
            // A concurrent write already put it under the new key.
            if self.put(&push)?.is_none() {
                n += 1;
            }
        }
        Ok(n)
    }
}
