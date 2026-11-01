//! hitop sync for the web app: hitop-core's sync plan and `hitop-sync-crypto` (the same Rust the
//! CLI runs, compiled to wasm; randomness from `crypto.getRandomValues`). Binary values cross as
//! base64url strings, everything else as JSON text. The transport is `webapp/src/sync.js`.
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hitop_core::format::files::Files;
use hitop_core::sync::{self, BlobText, RemoteBlob, State};
use hitop_sync_crypto::{self as crypto, BlobContext, DeviceKeyPair, WorkspaceKey};
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

#[wasm_bindgen(js_name = syncWrap)]
pub fn wrap(public_key: &str, workspace: &str, epoch: u32, workspace_key: &str) -> Result<String, JsError> {
    Ok(b64(&crypto::wrap_key(&unb64("public key", public_key)?, workspace, epoch, &key(workspace_key)?).map_err(err)?))
}

#[wasm_bindgen(js_name = syncUnwrap)]
pub fn unwrap(secret: &str, workspace: &str, epoch: u32, wrapped: &str) -> Result<String, JsError> {
    Ok(b64(crypto::unwrap_key(&device(secret)?, workspace, epoch, &unb64("wrapped key", wrapped)?).map_err(err)?.as_bytes()))
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

#[cfg(test)]
mod tests {
    #[test]
    fn the_published_vectors_pass() {
        let n = super::check_vectors_json(include_str!("../../hitop-sync-crypto/vectors/v1.json")).unwrap();
        assert!(n >= 10, "{n}");
    }
}
