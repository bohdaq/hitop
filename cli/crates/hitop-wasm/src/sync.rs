//! hitop sync for the web app: hitop-core's sync plan and `hitop-sync-crypto` (the same Rust the
//! CLI runs, compiled to wasm; randomness from `crypto.getRandomValues`). Binary values cross as
//! base64url strings, everything else as JSON text. The transport is `webapp/src/sync.js`.
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hitop_core::format::files::Files;
use hitop_core::sync::{self, BlobText, RemoteBlob, State};
use std::collections::BTreeMap;

use hitop_sync_crypto::{self as crypto, BlobContext, DeviceKeyPair, ReceivedWrap, WorkspaceKey, WorkspaceMemory, WrapHeader};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(what: &str, s: &str) -> Result<Vec<u8>, JsError> {
    URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).map_err(|_| err(format!("{what}: not base64url")))
}

fn key(s: &str) -> Result<WorkspaceKey, JsError> {
    let bytes: [u8; 32] = unb64("workspace key", s)?.try_into().map_err(|_| err("workspace key: 32 bytes"))?;
    Ok(WorkspaceKey::from_bytes(bytes))
}

fn device(secret: &str) -> Result<DeviceKeyPair, JsError> {
    DeviceKeyPair::from_secret_bytes(&unb64("device key", secret)?).map_err(err)
}

pub fn plan_json(local: &str, state: &str, remote: &str, me: &str) -> anyhow::Result<String> {
    let local: Files = serde_json::from_str(local)?;
    let state: State = serde_json::from_str(state)?;
    let remote: Vec<RemoteBlob> = serde_json::from_str(remote)?;
    let plan = sync::plan(&sync::synced_files(&local), &state, &remote, me, &mut || crypto::random_hex::<16>());
    Ok(serde_json::to_string(&plan)?)
}

/// `hitop_core::sync::plan`: `{write, remove, push, conflicts, merged, skipped, state}`.
#[wasm_bindgen(js_name = syncPlan)]
pub fn plan(local_json: &str, state_json: &str, remote_json: &str, me: &str) -> Result<String, JsError> {
    plan_json(local_json, state_json, remote_json, me).map_err(err)
}

/// Records an accepted push in the state.
#[wasm_bindgen(js_name = syncPushed)]
pub fn pushed(state_json: &str, push_json: &str) -> Result<String, JsError> {
    let mut state: State = serde_json::from_str(state_json).map_err(err)?;
    state.pushed(&serde_json::from_str(push_json).map_err(err)?);
    serde_json::to_string(&state).map_err(err)
}

/// Resolves a conflict: `{state, resolution: {action: keepMine|write|remove, text?}}`, or an
/// error when the path isn't in conflict.
#[wasm_bindgen(js_name = syncResolve)]
pub fn resolve(state_json: &str, path: &str, keep_mine: bool) -> Result<String, JsError> {
    let mut state: State = serde_json::from_str(state_json).map_err(err)?;
    let r = state.resolve(path, keep_mine).ok_or_else(|| err(format!("{path} is not in conflict")))?;
    serde_json::to_string(&serde_json::json!({ "state": state, "resolution": r })).map_err(err)
}

#[wasm_bindgen(js_name = syncIsSyncedPath)]
pub fn is_synced_path(path: &str) -> bool {
    sync::is_synced_path(path)
}

/// A new device key pair: `{secret, publicKey}`.
#[wasm_bindgen(js_name = syncKeygen)]
pub fn keygen() -> String {
    let k = DeviceKeyPair::generate();
    serde_json::json!({ "secret": b64(&k.secret_bytes()), "publicKey": b64(&k.public_key()) }).to_string()
}

#[wasm_bindgen(js_name = syncPublicKey)]
pub fn public_key(secret: &str) -> Result<String, JsError> {
    Ok(b64(&device(secret)?.public_key()))
}

#[wasm_bindgen(js_name = syncFingerprint)]
pub fn fingerprint(public_key: &str) -> Result<String, JsError> {
    Ok(crypto::fingerprint(&unb64("public key", public_key)?))
}

#[wasm_bindgen(js_name = syncNewKey)]
pub fn new_key() -> String {
    b64(WorkspaceKey::generate().as_bytes())
}

#[wasm_bindgen(js_name = syncRandomId)]
pub fn random_id() -> String {
    crypto::random_hex::<16>()
}

fn phrase(entropy: &str) -> Result<crypto::RecoveryPhrase, JsError> {
    let bytes: [u8; 32] = unb64("recovery phrase", entropy)?.try_into().map_err(|_| err("recovery phrase: 32 bytes"))?;
    Ok(crypto::RecoveryPhrase::from_entropy(bytes))
}

/// A new recovery phrase: `{words, entropy}` (24 words to show once; its 32 bytes, base64url).
#[wasm_bindgen(js_name = syncRecoveryNew)]
pub fn recovery_new() -> String {
    let p = crypto::RecoveryPhrase::generate();
    serde_json::json!({ "words": p.to_words(), "entropy": b64(p.entropy()) }).to_string()
}

/// The 32 bytes (base64url) of a phrase as typed; an error saying what is wrong otherwise.
#[wasm_bindgen(js_name = syncRecoveryParse)]
pub fn recovery_parse(words: &str) -> Result<String, JsError> {
    Ok(b64(crypto::RecoveryPhrase::parse(words).map_err(err)?.entropy()))
}

/// Seals the recovery data (JSON text) of account `account_id`: the blob, base64url.
#[wasm_bindgen(js_name = syncRecoverySeal)]
pub fn recovery_seal(entropy: &str, account_id: &str, plaintext: &str) -> Result<String, JsError> {
    Ok(b64(&crypto::seal_recovery(&phrase(entropy)?, account_id, plaintext.as_bytes()).map_err(err)?))
}

/// Opens a recovery blob of account `account_id`: the recovery data (JSON text).
#[wasm_bindgen(js_name = syncRecoveryOpen)]
pub fn recovery_open(entropy: &str, account_id: &str, blob: &str) -> Result<String, JsError> {
    let plain = crypto::open_recovery(&phrase(entropy)?, account_id, &unb64("recovery blob", blob)?).map_err(err)?;
    String::from_utf8(plain).map_err(|_| err("recovery data: not text"))
}

/// A format 1 (legacy, unauthenticated) wrap, as hitop 1.1 made them. The app never makes one;
/// the tests use it to play a malicious server.
#[wasm_bindgen(js_name = syncWrapLegacy)]
pub fn wrap_legacy(public_key: &str, workspace: &str, epoch: u32, workspace_key: &str) -> Result<String, JsError> {
    Ok(b64(&crypto::wrap_key(&unb64("public key", public_key)?, workspace, epoch, &key(workspace_key)?).map_err(err)?))
}

/// A format 2 (authenticated) wrap of `workspace_key` for epoch `epoch`, made by this device
/// (`secret`, server key id `sender_key_id`) for one device; `head` = the workspace's current epoch.
#[wasm_bindgen(js_name = syncSealWrap)]
#[allow(clippy::too_many_arguments)]
pub fn seal_wrap(secret: &str, sender_key_id: &str, recipient_key_id: &str, recipient_public_key: &str, workspace: &str, epoch: u32, head: u32, workspace_key: &str) -> Result<String, JsError> {
    let me = device(secret)?;
    let pk = unb64("public key", recipient_public_key)?;
    let w = crypto::seal_wrap(&crypto::Device { key_id: sender_key_id, keys: &me }, &crypto::Recipient { key_id: recipient_key_id, public_key: &pk }, workspace, epoch, head, &key(workspace_key)?).map_err(err)?;
    Ok(b64(&w))
}

/// `{legacy: true}` for a format 1 wrap, `{legacy: false, sender, head}` for format 2 (unverified).
#[wasm_bindgen(js_name = syncWrapHeader)]
pub fn wrap_header(wrapped: &str) -> Result<String, JsError> {
    Ok(match crypto::wrap_header(&unb64("wrapped key", wrapped)?).map_err(err)? {
        WrapHeader::Legacy => serde_json::json!({ "legacy": true }),
        WrapHeader::Authenticated { sender_key_id, head } => serde_json::json!({ "legacy": false, "sender": sender_key_id, "head": head }),
    }
    .to_string())
}

#[wasm_bindgen(js_name = syncNormalizeFingerprint)]
pub fn normalize_fingerprint(fp: &str) -> String {
    crypto::normalize_fingerprint(fp)
}

#[derive(Deserialize)]
struct WrapIn {
    epoch: u32,
    wrapped: String,
}

fn keys_in(what: &str, json: &str) -> Result<BTreeMap<String, Vec<u8>>, JsError> {
    let m: BTreeMap<String, String> = serde_json::from_str(json).map_err(|e| err(format!("{what}: {e}")))?;
    m.into_iter().map(|(id, pk)| Ok((id, unb64(what, &pk)?))).collect()
}

/// The error of a refused key as `{kind, message, epoch?, senderKeyId?, fingerprint?}`.
fn key_error(e: &crypto::Error) -> serde_json::Value {
    use crypto::Error as E;
    let mut out = serde_json::json!({ "message": e.to_string() });
    let (kind, epoch) = match e {
        E::UntrustedSender { epoch, sender_key_id, fingerprint } => {
            out["senderKeyId"] = sender_key_id.as_str().into();
            out["fingerprint"] = fingerprint.as_deref().into();
            ("untrustedSender", Some(*epoch))
        }
        E::Forged { epoch, sender_key_id } => {
            out["senderKeyId"] = sender_key_id.as_str().into();
            ("forged", Some(*epoch))
        }
        E::KeyChanged { epoch } => ("keyChanged", Some(*epoch)),
        E::Rollback { current, .. } => ("rollback", Some(*current)),
        E::LegacyRefused { epoch } => ("legacyRefused", Some(*epoch)),
        E::MissingKey { epoch } => ("missingKey", Some(*epoch)),
        _ => ("malformed", None),
    };
    out["kind"] = kind.into();
    if let Some(e) = epoch {
        out["epoch"] = e.into();
    }
    out
}

/// `hitop_sync_crypto::accept_wraps` for this device (`secret`, `key_id`): `wraps` =
/// `[{epoch, wrapped}]` from the server, `pinned` and `listed` = `{keyId: publicKey}` (this
/// device's pins; the server's member device keys, or `{}`), `memory` = the workspace's
/// `WorkspaceMemory` JSON (`{}` when none). Never throws for a refused key: `{ok: true, keys:
/// {epoch: key}, current, notices: [{kind, ..., message}], newPins: {keyId: publicKey}, memory}`
/// or `{ok: false, error: {kind, message, ...}}`.
#[wasm_bindgen(js_name = syncAcceptWraps)]
#[allow(clippy::too_many_arguments)]
pub fn accept_wraps(secret: &str, key_id: &str, workspace: &str, current_epoch: u32, wraps_json: &str, pinned_json: &str, listed_json: &str, memory_json: &str) -> Result<String, JsError> {
    let me = device(secret)?;
    let wraps: Vec<WrapIn> = serde_json::from_str(wraps_json).map_err(err)?;
    let bytes = wraps.iter().map(|w| Ok((w.epoch, unb64("wrapped key", &w.wrapped)?))).collect::<Result<Vec<_>, JsError>>()?;
    let received: Vec<ReceivedWrap> = bytes.iter().map(|(epoch, wrapped)| ReceivedWrap { epoch: *epoch, wrapped }).collect();
    let pinned = keys_in("pinned", pinned_json)?;
    let listed = keys_in("listed", listed_json)?;
    let mut memory: WorkspaceMemory = serde_json::from_str(if memory_json.trim().is_empty() { "{}" } else { memory_json }).map_err(err)?;
    let out = match crypto::accept_wraps(&crypto::Device { key_id, keys: &me }, workspace, current_epoch, &received, &pinned, &listed, &mut memory) {
        Ok(ring) => {
            let notices: Vec<serde_json::Value> = ring
                .notices
                .iter()
                .map(|n| {
                    let mut v = serde_json::to_value(n).unwrap_or_default();
                    v["message"] = n.to_string().into();
                    v
                })
                .collect();
            let keys: BTreeMap<String, String> = ring.keys.iter().map(|(e, k)| (e.to_string(), b64(k.as_bytes()))).collect();
            let new_pins: BTreeMap<&str, String> = ring.new_pins.iter().map(|(id, pk)| (id.as_str(), b64(pk))).collect();
            serde_json::json!({ "ok": true, "keys": keys, "current": ring.current, "notices": notices, "newPins": new_pins, "memory": memory, "legacy": ring.uses_legacy() })
        }
        Err(e) => serde_json::json!({ "ok": false, "error": key_error(&e) }),
    };
    Ok(out.to_string())
}

#[wasm_bindgen(js_name = syncBlobEpoch)]
pub fn blob_epoch(envelope: &str) -> Result<u32, JsError> {
    crypto::blob_epoch(&unb64("envelope", envelope)?).map_err(err)
}

/// Seals one file (`{"v":1,"path","text"}`) as blob `blob`, version `version`, under `epoch`.
#[wasm_bindgen(js_name = syncSeal)]
pub fn seal(workspace_key: &str, workspace: &str, blob: &str, version: u64, epoch: u32, path: &str, text: &str) -> Result<String, JsError> {
    let ctx = BlobContext { workspace_id: workspace, blob_id: blob, version, epoch };
    Ok(b64(&crypto::encrypt_blob(&key(workspace_key)?, &ctx, &BlobText::encode(path, text)).map_err(err)?))
}

/// Opens a blob sealed for exactly this workspace, blob id and version: `{path, text}`.
#[wasm_bindgen(js_name = syncOpen)]
pub fn open(workspace_key: &str, workspace: &str, blob: &str, version: u64, envelope: &str) -> Result<String, JsError> {
    let env = unb64("envelope", envelope)?;
    let epoch = crypto::blob_epoch(&env).map_err(err)?;
    let ctx = BlobContext { workspace_id: workspace, blob_id: blob, version, epoch };
    let plain = crypto::decrypt_blob(&key(workspace_key)?, &ctx, &env).map_err(|e| err(format!("blob {blob} version {version}: {e}")))?;
    let b = BlobText::decode(&plain).map_err(err)?;
    serde_json::to_string(&serde_json::json!({ "path": b.path, "text": b.text })).map_err(err)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vectors {
    blobs: Vec<BlobVector>,
    blob_failures: Vec<BlobVector>,
    wraps: Vec<WrapVector>,
    wrap_failures: Vec<WrapVector>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobVector {
    key: String,
    workspace_id: String,
    blob_id: String,
    version: u64,
    epoch: u32,
    #[serde(default)]
    nonce: String,
    #[serde(default)]
    plaintext: String,
    #[serde(default)]
    aad: String,
    envelope: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapVector {
    #[serde(default)]
    recipient_ikm: String,
    #[serde(default)]
    recipient_secret: String,
    #[serde(default)]
    recipient_public: String,
    workspace_id: String,
    epoch: u32,
    #[serde(default)]
    info: String,
    wrapped: String,
    #[serde(default)]
    workspace_key: String,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(s.len().is_multiple_of(2), "odd hex");
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(Into::into)).collect()
}

/// Runs `vectors/v1.json` (the published test vectors) through this build: blobs both ways,
/// wraps in the open direction, and every failure case. Returns how many cases passed.
pub fn check_vectors_json(json: &str) -> anyhow::Result<usize> {
    let v: Vectors = serde_json::from_str(json)?;
    let mut n = 0;
    let k = |s: &str| -> anyhow::Result<WorkspaceKey> { Ok(WorkspaceKey::from_bytes(unhex(s)?.try_into().map_err(|_| anyhow::anyhow!("key length"))?)) };
    for b in &v.blobs {
        let c = BlobContext { workspace_id: &b.workspace_id, blob_id: &b.blob_id, version: b.version, epoch: b.epoch };
        anyhow::ensure!(hex(&c.associated_data()?) == b.aad, "aad of {}", b.blob_id);
        let nonce: [u8; 24] = unhex(&b.nonce)?.try_into().map_err(|_| anyhow::anyhow!("nonce length"))?;
        anyhow::ensure!(hex(&crypto::encrypt_blob_with_nonce(&k(&b.key)?, &c, &unhex(&b.plaintext)?, &nonce)?) == b.envelope, "envelope of {}", b.blob_id);
        anyhow::ensure!(hex(&crypto::decrypt_blob(&k(&b.key)?, &c, &unhex(&b.envelope)?)?) == b.plaintext, "plaintext of {}", b.blob_id);
        n += 1;
    }
    for b in &v.blob_failures {
        let c = BlobContext { workspace_id: &b.workspace_id, blob_id: &b.blob_id, version: b.version, epoch: b.epoch };
        anyhow::ensure!(crypto::decrypt_blob(&k(&b.key)?, &c, &unhex(&b.envelope)?).is_err(), "a blob failure case opened");
        n += 1;
    }
    for w in &v.wraps {
        let d = DeviceKeyPair::derive(&unhex(&w.recipient_ikm)?);
        anyhow::ensure!(hex(&d.secret_bytes()) == w.recipient_secret && hex(&d.public_key()) == w.recipient_public, "DeriveKeyPair");
        anyhow::ensure!(hex(&crypto::wrap_info(&w.workspace_id, w.epoch)?) == w.info, "wrap info");
        anyhow::ensure!(hex(crypto::unwrap_key(&d, &w.workspace_id, w.epoch, &unhex(&w.wrapped)?)?.as_bytes()) == w.workspace_key, "unwrapped key");
        n += 1;
    }
    for w in &v.wrap_failures {
        let d = DeviceKeyPair::from_secret_bytes(&unhex(&w.recipient_secret)?)?;
        anyhow::ensure!(crypto::unwrap_key(&d, &w.workspace_id, w.epoch, &unhex(&w.wrapped)?).is_err(), "a wrap failure case opened");
        n += 1;
    }
    Ok(n)
}

#[wasm_bindgen(js_name = syncCheckVectors)]
pub fn check_vectors(json: &str) -> Result<usize, JsError> {
    check_vectors_json(json).map_err(|e| err(format!("{e:#}")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorsV2 {
    wraps: Vec<WrapV2>,
    wrap_failures: Vec<WrapV2Failure>,
    commitments: Vec<CommitmentV2>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapV2 {
    sender_public: String,
    sender_key_id: String,
    recipient_secret: String,
    recipient_key_id: String,
    workspace_id: String,
    epoch: u32,
    head: u32,
    info: String,
    wrapped: String,
    workspace_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapV2Failure {
    sender_public: String,
    recipient_secret: String,
    recipient_key_id: String,
    workspace_id: String,
    epoch: u32,
    wrapped: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitmentV2 {
    workspace_id: String,
    epoch: u32,
    workspace_key: String,
    commitment: String,
}

/// Runs `vectors/v2.json` (format 2 wraps, key commitments) through this build.
pub fn check_vectors_v2_json(json: &str) -> anyhow::Result<usize> {
    let v: VectorsV2 = serde_json::from_str(json)?;
    let mut n = 0;
    for w in &v.wraps {
        let d = DeviceKeyPair::from_secret_bytes(&unhex(&w.recipient_secret)?)?;
        anyhow::ensure!(hex(&crypto::wrap_info_v2(&w.workspace_id, w.epoch, w.head, &w.sender_key_id, &w.recipient_key_id)?) == w.info, "wrap info");
        let o = crypto::open_wrap(&d, &w.recipient_key_id, &unhex(&w.sender_public)?, &w.workspace_id, w.epoch, &unhex(&w.wrapped)?)?;
        anyhow::ensure!(hex(o.key.as_bytes()) == w.workspace_key && o.sender_key_id == w.sender_key_id && o.head == w.head, "opened wrap");
        n += 1;
    }
    for f in &v.wrap_failures {
        let d = DeviceKeyPair::from_secret_bytes(&unhex(&f.recipient_secret)?)?;
        anyhow::ensure!(crypto::open_wrap(&d, &f.recipient_key_id, &unhex(&f.sender_public)?, &f.workspace_id, f.epoch, &unhex(&f.wrapped)?).is_err(), "a wrap failure case opened");
        n += 1;
    }
    for c in &v.commitments {
        let key = WorkspaceKey::from_bytes(unhex(&c.workspace_key)?.try_into().map_err(|_| anyhow::anyhow!("key length"))?);
        anyhow::ensure!(crypto::key_commitment(&c.workspace_id, c.epoch, &key) == c.commitment, "key commitment");
        n += 1;
    }
    Ok(n)
}

#[wasm_bindgen(js_name = syncCheckVectorsV2)]
pub fn check_vectors_v2(json: &str) -> Result<usize, JsError> {
    check_vectors_v2_json(json).map_err(|e| err(format!("{e:#}")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_published_vectors_pass() {
        let n = super::check_vectors_json(include_str!("../../hitop-sync-crypto/vectors/v1.json")).unwrap();
        assert!(n >= 10, "{n}");
        let n = super::check_vectors_v2_json(include_str!("../../hitop-sync-crypto/vectors/v2.json")).unwrap();
        assert!(n >= 10, "{n}");
    }
}
