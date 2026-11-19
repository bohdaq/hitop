//! hitop sync for the HITOP iOS app: the same Rust the CLI runs and the web app runs as wasm
//! (`cli/crates/hitop-wasm/src/sync.rs`), behind one C function, so the app makes exactly the
//! decisions every other hitop client makes and never has its own cryptography:
//!
//! - `hitop_core::sync` (the plan: three-way pull/push/conflicts per file, key-by-key merge of
//!   hitop.yaml, environments and globals, the path allowlist) and `hitop_core::format::files`
//!   (a collection ⇄ its folder files, as the CLI writes them);
//! - `hitop_sync_crypto`: format 2 wraps (HPKE Auth mode), `accept_wraps` (the one key acceptance
//!   policy), XChaCha20-Poly1305 blobs, shared environment values.
//!
//! `hitop_sync_call(op, args)` takes the operation's name and its arguments as a JSON object and
//! returns `{"ok": value}` or `{"error": message}` (a NUL-terminated string the caller frees with
//! `hitop_sync_free`). Binary values are base64url without padding, as in hitop-wasm; JSON the
//! caller must read exactly (collections with 64-bit ids) crosses as JSON *text* inside a string.
//! A panic is caught and returned as an error: nothing unwinds into Swift.
//!
//! Feature `grpc` (default, I4) adds the `grpc*` operations: hitop-core's gRPC client, the code
//! `hitop grpc` and the desktop app run (src/grpc.rs).

use std::collections::BTreeMap;
use std::ffi::{c_char, CStr, CString};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hitop_core::format::{files, Folder};
use hitop_core::model::{Collection, Environment};
use hitop_core::sync::{self, BlobText, Push, RemoteBlob, State};
use hitop_sync_crypto::{self as crypto, BlobContext, Device, DeviceKeyPair, ReceivedWrap, Recipient, WorkspaceKey, WorkspaceMemory, WrapHeader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[cfg(feature = "grpc")]
mod grpc;

type R = Result<Value, String>;

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(what: &str, s: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).map_err(|_| format!("{what}: not base64url"))
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

struct Args<'a>(&'a Value);

impl Args<'_> {
    fn str(&self, k: &str) -> Result<&str, String> {
        self.0.get(k).and_then(Value::as_str).ok_or_else(|| format!("argument {k}: a string"))
    }
    fn bytes(&self, k: &str) -> Result<Vec<u8>, String> {
        unb64(k, self.str(k)?)
    }
    fn u64(&self, k: &str) -> Result<u64, String> {
        self.0.get(k).and_then(Value::as_u64).ok_or_else(|| format!("argument {k}: a number"))
    }
    fn u32(&self, k: &str) -> Result<u32, String> {
        u32::try_from(self.u64(k)?).map_err(|_| format!("argument {k}: too large"))
    }
    fn bool(&self, k: &str) -> bool {
        self.0.get(k).and_then(Value::as_bool).unwrap_or(false)
    }
    fn json<T: for<'de> Deserialize<'de>>(&self, k: &str) -> Result<T, String> {
        let v = self.0.get(k).cloned().unwrap_or(Value::Null);
        serde_json::from_value(v).map_err(|e| format!("argument {k}: {e}"))
    }
    /// A value that crosses as JSON text (exact 64-bit numbers).
    fn text_json<T: for<'de> Deserialize<'de>>(&self, k: &str) -> Result<T, String> {
        serde_json::from_str(self.str(k)?).map_err(|e| format!("argument {k}: {e}"))
    }
    fn key(&self, k: &str) -> Result<WorkspaceKey, String> {
        let bytes: [u8; 32] = self.bytes(k)?.try_into().map_err(|_| format!("{k}: 32 bytes"))?;
        Ok(WorkspaceKey::from_bytes(bytes))
    }
    fn device(&self, k: &str) -> Result<DeviceKeyPair, String> {
        DeviceKeyPair::from_secret_bytes(&self.bytes(k)?).map_err(err)
    }
    fn keys(&self, k: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let m: BTreeMap<String, String> = self.json(k)?;
        m.into_iter().map(|(id, pk)| Ok((id, unb64(k, &pk)?))).collect()
    }
}

/// `{collection, environments, globals?}`, as hitop-wasm's `FolderJson`.
#[derive(Serialize, Deserialize)]
struct FolderJson {
    collection: Collection,
    #[serde(default)]
    environments: Vec<Environment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    globals: Option<Environment>,
}

#[derive(Deserialize)]
struct WrapIn {
    epoch: u32,
    wrapped: String,
}

/// A refused key as `{kind, message, epoch?, senderKeyId?, fingerprint?}` (hitop-wasm `key_error`).
fn key_error(e: &crypto::Error) -> Value {
    use crypto::Error as E;
    let mut out = json!({ "message": e.to_string() });
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

fn accept_wraps(a: &Args) -> R {
    let me = a.device("secret")?;
    let key_id = a.str("keyId")?;
    let workspace = a.str("workspace")?;
    let wraps: Vec<WrapIn> = a.json("wraps")?;
    let bytes = wraps.iter().map(|w| Ok((w.epoch, unb64("wrapped key", &w.wrapped)?))).collect::<Result<Vec<_>, String>>()?;
    let received: Vec<ReceivedWrap> = bytes.iter().map(|(epoch, wrapped)| ReceivedWrap { epoch: *epoch, wrapped }).collect();
    let pinned = a.keys("pinned")?;
    let listed = a.keys("listed")?;
    let mut memory: WorkspaceMemory = match a.0.get("memory") {
        None | Some(Value::Null) => WorkspaceMemory::default(),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("memory: {e}"))?,
    };
    Ok(match crypto::accept_wraps(&Device { key_id, keys: &me }, workspace, a.u32("currentEpoch")?, &received, &pinned, &listed, &mut memory) {
        Ok(ring) => {
            let notices: Vec<Value> = ring
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
            json!({ "ok": true, "keys": keys, "current": ring.current, "notices": notices, "newPins": new_pins, "memory": memory, "legacy": ring.uses_legacy() })
        }
        Err(e) => json!({ "ok": false, "error": key_error(&e) }),
    })
}

fn call(op: &str, a: &Args) -> R {
    match op {
        // ---- device keys and fingerprints
        "keygen" => {
            let k = DeviceKeyPair::generate();
            Ok(json!({ "secret": b64(&k.secret_bytes()), "publicKey": b64(&k.public_key()) }))
        }
        "publicKey" => Ok(b64(&a.device("secret")?.public_key()).into()),
        "fingerprint" => Ok(crypto::fingerprint(&a.bytes("publicKey")?).into()),
        "normalizeFingerprint" => Ok(crypto::normalize_fingerprint(a.str("fingerprint")?).into()),
        "newKey" => Ok(b64(WorkspaceKey::generate().as_bytes()).into()),
        "randomId" => Ok(crypto::random_hex::<16>().into()),

        // ---- wraps
        "sealWrap" => {
            let me = a.device("secret")?;
            let sender = Device { key_id: a.str("senderKeyId")?, keys: &me };
            let pk = a.bytes("recipientPublicKey")?;
            let recipient = Recipient { key_id: a.str("recipientKeyId")?, public_key: &pk };
            Ok(b64(&crypto::seal_wrap(&sender, &recipient, a.str("workspace")?, a.u32("epoch")?, a.u32("head")?, &a.key("key")?).map_err(err)?).into())
        }
        "openWrap" => {
            let o = crypto::open_wrap(&a.device("secret")?, a.str("recipientKeyId")?, &a.bytes("senderPublicKey")?, a.str("workspace")?, a.u32("epoch")?, &a.bytes("wrapped")?).map_err(err)?;
            Ok(json!({ "key": b64(o.key.as_bytes()), "sender": o.sender_key_id, "head": o.head }))
        }
        "wrapLegacy" => Ok(b64(&crypto::wrap_key(&a.bytes("publicKey")?, a.str("workspace")?, a.u32("epoch")?, &a.key("key")?).map_err(err)?).into()),
        "unwrapLegacy" => Ok(b64(crypto::unwrap_key(&a.device("secret")?, a.str("workspace")?, a.u32("epoch")?, &a.bytes("wrapped")?).map_err(err)?.as_bytes()).into()),
        "wrapInfoLegacy" => Ok(b64(&crypto::wrap_info(a.str("workspace")?, a.u32("epoch")?).map_err(err)?).into()),
        "wrapInfo" => Ok(b64(&crypto::wrap_info_v2(a.str("workspace")?, a.u32("epoch")?, a.u32("head")?, a.str("senderKeyId")?, a.str("recipientKeyId")?).map_err(err)?).into()),
        "wrapHeader" => Ok(match crypto::wrap_header(&a.bytes("wrapped")?).map_err(err)? {
            WrapHeader::Legacy => json!({ "legacy": true }),
            WrapHeader::Authenticated { sender_key_id, head } => json!({ "legacy": false, "sender": sender_key_id, "head": head }),
        }),
        "acceptWraps" => accept_wraps(a),
        "keyCommitment" => Ok(crypto::key_commitment(a.str("workspace")?, a.u32("epoch")?, &a.key("key")?).into()),

        // ---- blobs
        "blobEpoch" => Ok(crypto::blob_epoch(&a.bytes("envelope")?).map_err(err)?.into()),
        "seal" => {
            let ctx = BlobContext { workspace_id: a.str("workspace")?, blob_id: a.str("blob")?, version: a.u64("version")?, epoch: a.u32("epoch")? };
            Ok(b64(&crypto::encrypt_blob(&a.key("key")?, &ctx, &BlobText::encode(a.str("path")?, a.str("text")?)).map_err(err)?).into())
        }
        "open" => {
            let env = a.bytes("envelope")?;
            let epoch = crypto::blob_epoch(&env).map_err(err)?;
            let (blob, version) = (a.str("blob")?, a.u64("version")?);
            let ctx = BlobContext { workspace_id: a.str("workspace")?, blob_id: blob, version, epoch };
            let plain = crypto::decrypt_blob(&a.key("key")?, &ctx, &env).map_err(|e| format!("blob {blob} version {version}: {e}"))?;
            let b = BlobText::decode(&plain)?;
            Ok(json!({ "path": b.path, "text": b.text }))
        }
        // Raw envelopes, for the published vectors (`nonce` only there: a repeated nonce breaks
        // confidentiality).
        "encryptBlob" => {
            let ctx = BlobContext { workspace_id: a.str("workspace")?, blob_id: a.str("blob")?, version: a.u64("version")?, epoch: a.u32("epoch")? };
            let nonce: [u8; 24] = a.bytes("nonce")?.try_into().map_err(|_| "nonce: 24 bytes".to_string())?;
            Ok(b64(&crypto::encrypt_blob_with_nonce(&a.key("key")?, &ctx, &a.bytes("plaintext")?, &nonce).map_err(err)?).into())
        }
        "decryptBlob" => {
            let ctx = BlobContext { workspace_id: a.str("workspace")?, blob_id: a.str("blob")?, version: a.u64("version")?, epoch: a.u32("epoch")? };
            Ok(b64(&crypto::decrypt_blob(&a.key("key")?, &ctx, &a.bytes("envelope")?).map_err(err)?).into())
        }
        "blobAad" => {
            let ctx = BlobContext { workspace_id: a.str("workspace")?, blob_id: a.str("blob")?, version: a.u64("version")?, epoch: a.u32("epoch")? };
            Ok(b64(&ctx.associated_data().map_err(err)?).into())
        }

        // ---- shared environment values (hitop-sync-crypto 0.5)
        "shareWrapSender" => Ok(crypto::share_wrap_sender(&a.bytes("wrapped")?).map_err(err)?.into()),
        "shareOpen" => {
            let me = a.device("secret")?;
            let (ws, id, version) = (a.str("workspace")?, a.str("shareId")?, a.u32("version")?);
            let key = crypto::open_share_wrap(&me, a.str("keyId")?, &a.bytes("senderPublicKey")?, ws, id, version, &a.bytes("wrapped")?).map_err(err)?;
            let content = crypto::open_share(&key, ws, id, version, &a.bytes("sealed")?).map_err(err)?;
            Ok(json!({ "key": b64(key.as_bytes()), "content": String::from_utf8(content).map_err(|_| "share content: not text".to_string())? }))
        }
        "shareSeal" => Ok(b64(&crypto::seal_share(&a.key("key")?, a.str("workspace")?, a.str("shareId")?, a.u32("version")?, a.str("content")?.as_bytes()).map_err(err)?).into()),
        "shareWrap" => {
            let me = a.device("secret")?;
            let sender = Device { key_id: a.str("senderKeyId")?, keys: &me };
            let pk = a.bytes("recipientPublicKey")?;
            let recipient = Recipient { key_id: a.str("recipientKeyId")?, public_key: &pk };
            Ok(b64(&crypto::seal_share_wrap(&sender, &recipient, a.str("workspace")?, a.str("shareId")?, a.u32("version")?, &a.key("key")?).map_err(err)?).into())
        }

        // ---- the plan (hitop_core::sync)
        "plan" => {
            let local: files::Files = a.json("local")?;
            let state: State = a.json("state")?;
            let remote: Vec<RemoteBlob> = a.json("remote")?;
            let me = a.str("me")?;
            let plan = sync::plan(&sync::synced_files(&local), &state, &remote, me, &mut || crypto::random_hex::<16>());
            serde_json::to_value(plan).map_err(err)
        }
        "pushed" => {
            let mut state: State = a.json("state")?;
            let push: Push = a.json("push")?;
            state.pushed(&push);
            serde_json::to_value(state).map_err(err)
        }
        "resolve" => {
            let mut state: State = a.json("state")?;
            let resolution = state.resolve(a.str("path")?, a.bool("keepMine")).ok_or("that file isn't in conflict")?;
            Ok(json!({ "state": state, "resolution": resolution }))
        }
        "isSyncedPath" => Ok(sync::is_synced_path(a.str("path")?).into()),
        "mergeText" => {
            let opt = |k: &str| a.0.get(k).and_then(Value::as_str).map(str::to_string);
            let (b, m, t) = (opt("base"), opt("mine"), opt("theirs"));
            Ok(match sync::merge_text(a.str("path")?, b.as_deref(), m.as_deref(), t.as_deref()) {
                Some(merged) => json!({ "merged": true, "text": merged }),
                None => json!({ "merged": false }),
            })
        }

        // ---- collection folders (hitop_core::format::files)
        "readFiles" => {
            let files: files::Files = a.json("files")?;
            let folder = files::read_files(&files, a.str("name")?).map_err(|e| format!("{e:#}"))?;
            Ok(serde_json::to_string(&FolderJson { collection: folder.collection, environments: folder.environments, globals: folder.globals }).map_err(err)?.into())
        }
        "toFiles" => {
            let f: FolderJson = a.text_json("folder")?;
            let out = files::to_files(&Folder { collection: f.collection, environments: f.environments, globals: f.globals }).map_err(|e| format!("{e:#}"))?;
            serde_json::to_value(out).map_err(err)
        }
        "planFolder" => {
            let existing: Vec<String> = a.json("existing")?;
            let files: files::Files = a.json("files")?;
            serde_json::to_value(files::plan_sync(&existing, &files).map_err(|e| format!("{e:#}"))?).map_err(err)
        }
        #[cfg(feature = "grpc")]
        other if other.starts_with("grpc") => grpc::call(other, a),
        other => Err(format!("unknown operation {other}")),
    }
}

/// `{"ok": value}` or `{"error": message}` for `op` with `args` (a JSON object).
pub fn call_json(op: &str, args: &str) -> String {
    let out = match serde_json::from_str::<Value>(if args.trim().is_empty() { "{}" } else { args }) {
        Err(e) => Err(format!("arguments: {e}")),
        Ok(v) => std::panic::catch_unwind(|| call(op, &Args(&v))).unwrap_or_else(|_| Err(format!("{op}: internal error"))),
    };
    match out {
        Ok(v) => json!({ "ok": v }).to_string(),
        Err(e) => json!({ "error": e }).to_string(),
    }
}

/// # Safety
/// `op` and `args` are NUL-terminated UTF-8 strings (or null). The result is a NUL-terminated
/// string owned by the caller: free it with [`hitop_sync_free`].
#[no_mangle]
pub unsafe extern "C" fn hitop_sync_call(op: *const c_char, args: *const c_char) -> *mut c_char {
    let text = |p: *const c_char| if p.is_null() { String::new() } else { CStr::from_ptr(p).to_string_lossy().into_owned() };
    let out = call_json(&text(op), &text(args));
    CString::new(out).unwrap_or_else(|_| CString::new(r#"{"error":"NUL in result"}"#).unwrap()).into_raw()
}

/// # Safety
/// `s` came from [`hitop_sync_call`] and isn't used afterwards.
#[no_mangle]
pub unsafe extern "C" fn hitop_sync_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

#[cfg(test)]
mod tests {
    use super::call_json;
    use serde_json::Value;

    fn ok(op: &str, args: &str) -> Value {
        let v: Value = serde_json::from_str(&call_json(op, args)).unwrap();
        assert!(v.get("error").is_none(), "{op}: {v}");
        v["ok"].clone()
    }

    #[test]
    fn a_wrap_round_trip_and_errors() {
        let a = ok("keygen", "{}");
        let key = ok("newKey", "{}");
        let ws = "3f6c1a9e0b7d4c2a8e5f1b3d7a9c0e24";
        let wrapped = ok("sealWrap", &serde_json::json!({ "secret": a["secret"], "senderKeyId": "ka", "recipientKeyId": "ka", "recipientPublicKey": a["publicKey"], "workspace": ws, "epoch": 1, "head": 1, "key": key }).to_string());
        let r = ok("acceptWraps", &serde_json::json!({ "secret": a["secret"], "keyId": "ka", "workspace": ws, "currentEpoch": 1, "wraps": [{ "epoch": 1, "wrapped": wrapped }], "pinned": { "ka": a["publicKey"] }, "listed": {}, "memory": { "strict": true } }).to_string());
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["keys"]["1"], key);
        // Not pinned: refused, with the sender named.
        let r = ok("acceptWraps", &serde_json::json!({ "secret": a["secret"], "keyId": "ka", "workspace": ws, "currentEpoch": 1, "wraps": [{ "epoch": 1, "wrapped": wrapped }], "pinned": {}, "listed": {}, "memory": {} }).to_string());
        assert_eq!(r["error"]["kind"], "untrustedSender", "{r}");
        let v: Value = serde_json::from_str(&call_json("nope", "{}")).unwrap();
        assert!(v["error"].as_str().unwrap().contains("unknown"));
        let v: Value = serde_json::from_str(&call_json("seal", "{}")).unwrap();
        assert!(v["error"].as_str().unwrap().contains("argument"));
    }

    #[test]
    fn folders_round_trip() {
        let folder = r#"{"collection":{"id":18446744073709551000,"name":"Shop","requests":[{"id":7,"name":"List","method":"GET","url":"https://x/y","headers":[],"body":""}]},"environments":[]}"#;
        let files = ok("toFiles", &serde_json::json!({ "folder": folder }).to_string());
        assert!(files["hitop.yaml"].as_str().unwrap().contains("Shop"));
        let back = ok("readFiles", &serde_json::json!({ "files": files, "name": "x" }).to_string());
        assert!(back.as_str().unwrap().contains("18446744073709551000"), "{back}");
    }
}
