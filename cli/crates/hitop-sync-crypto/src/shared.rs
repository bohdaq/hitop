//! Run reports and shared environment values (since 0.5, HITOP Cloud C4).
//!
//! - **Run reports**: a runner (or a member) seals the full report of a run under the workspace
//!   key of the current epoch ([`seal_run`]). The envelope has the blob layout (`0x01 || u32 epoch
//!   || nonce || ciphertext || tag`) so [`crate::blob_epoch`] picks the key, but its associated
//!   data has its own label and binds the workspace id, the run id and the epoch, so the server
//!   can't serve a report as a file, move it to another run or another workspace.
//! - **Shared environment values** ("shares"): opt-in, per environment. A share has its own random
//!   key (a [`WorkspaceKey`]) that seals the values ([`seal_share`]) and is wrapped for each
//!   recipient device with HPKE Auth mode by the sharing device ([`seal_share_wrap`]), exactly as a
//!   format 2 workspace key wrap but with its own `info` label, the share id and the share's
//!   version: a share wrap can never pass for a workspace key wrap (or the other way round), and a
//!   recipient knows which device made it. Every new version (a re-key after a member was
//!   removed, or new values) has a new key; recipients refuse a version older than one they saw.
//!
//! Byte layouts: `vectors/shared.json` and `sync-server/THREAT_MODEL.md` ("Run history",
//! "Shared environment values").

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use hpke::aead::ChaCha20Poly1305 as HpkeChaCha;
use hpke::kdf::HkdfSha256;
use hpke::{Deserializable, OpModeR, OpModeS, Serializable};
use zeroize::Zeroize;

use crate::{push_len_prefixed, random, Device, DeviceKeyPair, Error, Kem, Recipient, WorkspaceKey, BLOB_FORMAT, BLOB_HEADER_LEN, MAX_KEY_ID_LEN, TAG_LEN, WRAPPED_KEY_LEN, WRAP_V2_FORMAT};

/// Domain separation prefix of a run report's associated data.
pub const RUN_AAD_LABEL: &[u8] = b"hitop-sync/v1/run";
/// Domain separation prefix of a share's associated data.
pub const SHARE_AAD_LABEL: &[u8] = b"hitop-sync/v1/env-share";
/// Domain separation prefix of the HPKE `info` of a share key wrap.
pub const SHARE_WRAP_INFO_LABEL: &[u8] = b"hitop-sync/v1/env-share-wrap";

/// `"hitop-sync/v1/run" || u16 len || workspace id || u16 len || run id || u32 epoch`.
pub fn run_associated_data(workspace_id: &str, run_id: &str, epoch: u32) -> Result<Vec<u8>, Error> {
    let mut aad = Vec::with_capacity(RUN_AAD_LABEL.len() + 8 + workspace_id.len() + run_id.len());
    aad.extend_from_slice(RUN_AAD_LABEL);
    push_len_prefixed(&mut aad, workspace_id.as_bytes())?;
    push_len_prefixed(&mut aad, run_id.as_bytes())?;
    aad.extend_from_slice(&epoch.to_be_bytes());
    Ok(aad)
}

/// `"hitop-sync/v1/env-share" || u16 len || workspace id || u16 len || share id || u32 version`.
pub fn share_associated_data(workspace_id: &str, share_id: &str, version: u32) -> Result<Vec<u8>, Error> {
    let mut aad = Vec::with_capacity(SHARE_AAD_LABEL.len() + 8 + workspace_id.len() + share_id.len());
    aad.extend_from_slice(SHARE_AAD_LABEL);
    push_len_prefixed(&mut aad, workspace_id.as_bytes())?;
    push_len_prefixed(&mut aad, share_id.as_bytes())?;
    aad.extend_from_slice(&version.to_be_bytes());
    Ok(aad)
}

fn seal_envelope(key: &WorkspaceKey, number: u32, aad: &[u8], plaintext: &[u8], nonce: &[u8; 24]) -> Result<Vec<u8>, Error> {
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.as_bytes()));
    let ciphertext = cipher.encrypt(&XNonce::from(*nonce), Payload { msg: plaintext, aad }).map_err(|_| Error::Malformed("plaintext (too long)"))?;
    let mut out = Vec::with_capacity(BLOB_HEADER_LEN + ciphertext.len());
    out.push(BLOB_FORMAT);
    out.extend_from_slice(&number.to_be_bytes());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn envelope_number(envelope: &[u8]) -> Result<u32, Error> {
    if envelope.len() < BLOB_HEADER_LEN + TAG_LEN || envelope[0] != BLOB_FORMAT {
        return Err(Error::Malformed("sealed envelope"));
    }
    Ok(u32::from_be_bytes([envelope[1], envelope[2], envelope[3], envelope[4]]))
}

fn open_envelope(key: &WorkspaceKey, aad: &[u8], envelope: &[u8]) -> Result<Vec<u8>, Error> {
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&envelope[5..BLOB_HEADER_LEN]);
    let cipher = XChaCha20Poly1305::new(&Key::from(*key.as_bytes()));
    cipher.decrypt(&XNonce::from(nonce), Payload { msg: &envelope[BLOB_HEADER_LEN..], aad }).map_err(|_| Error::Decrypt)
}

/// Seals a run report (report.json v1, secrets already masked) as run `run_id` of the workspace,
/// under the key of `epoch` (the workspace's current epoch).
pub fn seal_run(key: &WorkspaceKey, workspace_id: &str, run_id: &str, epoch: u32, report: &[u8]) -> Result<Vec<u8>, Error> {
    seal_run_with_nonce(key, workspace_id, run_id, epoch, report, &random::<24>())
}

/// [`seal_run`] with a caller-chosen nonce: test vectors only.
#[doc(hidden)]
pub fn seal_run_with_nonce(key: &WorkspaceKey, workspace_id: &str, run_id: &str, epoch: u32, report: &[u8], nonce: &[u8; 24]) -> Result<Vec<u8>, Error> {
    seal_envelope(key, epoch, &run_associated_data(workspace_id, run_id, epoch)?, report, nonce)
}

/// The epoch a sealed run report is under (pick the key with it).
pub fn run_epoch(envelope: &[u8]) -> Result<u32, Error> {
    envelope_number(envelope)
}

/// Opens a run report sealed for exactly this workspace and run, with the key of the epoch the
/// envelope names ([`run_epoch`]).
pub fn open_run(key: &WorkspaceKey, workspace_id: &str, run_id: &str, envelope: &[u8]) -> Result<Vec<u8>, Error> {
    let epoch = envelope_number(envelope)?;
    open_envelope(key, &run_associated_data(workspace_id, run_id, epoch)?, envelope)
}

/// Seals a share's content (the environment, its values and who it is for) as version `version`
/// of share `share_id`, under the share's own key.
pub fn seal_share(key: &WorkspaceKey, workspace_id: &str, share_id: &str, version: u32, content: &[u8]) -> Result<Vec<u8>, Error> {
    seal_share_with_nonce(key, workspace_id, share_id, version, content, &random::<24>())
}

/// [`seal_share`] with a caller-chosen nonce: test vectors only.
#[doc(hidden)]
pub fn seal_share_with_nonce(key: &WorkspaceKey, workspace_id: &str, share_id: &str, version: u32, content: &[u8], nonce: &[u8; 24]) -> Result<Vec<u8>, Error> {
    if version == 0 {
        return Err(Error::Malformed("share version (starts at 1)"));
    }
    seal_envelope(key, version, &share_associated_data(workspace_id, share_id, version)?, content, nonce)
}

/// Opens version `version` of a share. A server serving another version (an older one, say) fails
/// here: the version is in the envelope and in the associated data.
pub fn open_share(key: &WorkspaceKey, workspace_id: &str, share_id: &str, version: u32, envelope: &[u8]) -> Result<Vec<u8>, Error> {
    let found = envelope_number(envelope)?;
    if found != version {
        return Err(Error::EpochMismatch { expected: version, found });
    }
    open_envelope(key, &share_associated_data(workspace_id, share_id, version)?, envelope)
}

/// HPKE `info` of a share key wrap: `"hitop-sync/v1/env-share-wrap" || u16 len || workspace id ||
/// u16 len || share id || u32 version || u16 len || sender key id || u16 len || recipient key id`.
pub fn share_wrap_info(workspace_id: &str, share_id: &str, version: u32, sender_key_id: &str, recipient_key_id: &str) -> Result<Vec<u8>, Error> {
    let mut info = Vec::with_capacity(SHARE_WRAP_INFO_LABEL.len() + 12 + workspace_id.len() + share_id.len() + sender_key_id.len() + recipient_key_id.len());
    info.extend_from_slice(SHARE_WRAP_INFO_LABEL);
    push_len_prefixed(&mut info, workspace_id.as_bytes())?;
    push_len_prefixed(&mut info, share_id.as_bytes())?;
    info.extend_from_slice(&version.to_be_bytes());
    push_len_prefixed(&mut info, sender_key_id.as_bytes())?;
    push_len_prefixed(&mut info, recipient_key_id.as_bytes())?;
    Ok(info)
}

/// Wraps a share's key (version `version`) for one device, made and authenticated by `sender`:
/// `0x02 || u32 version || u16 len || sender key id || enc (32) || sealed key (32) || tag (16)`
/// (the shape of a format 2 workspace key wrap, `head` = the version; HPKE Auth mode with
/// [`share_wrap_info`] as `info`).
pub fn seal_share_wrap(sender: &Device, recipient: &Recipient, workspace_id: &str, share_id: &str, version: u32, key: &WorkspaceKey) -> Result<Vec<u8>, Error> {
    if sender.key_id.is_empty() || sender.key_id.len() > MAX_KEY_ID_LEN || recipient.key_id.len() > MAX_KEY_ID_LEN {
        return Err(Error::TooLong);
    }
    let pk = <Kem as hpke::Kem>::PublicKey::from_bytes(recipient.public_key).map_err(|_| Error::Malformed("public key"))?;
    let info = share_wrap_info(workspace_id, share_id, version, sender.key_id, recipient.key_id)?;
    let mode = OpModeS::Auth((sender.keys.secret.clone(), sender.keys.public.clone()));
    let (enc, sealed) = hpke::single_shot_seal::<HpkeChaCha, HkdfSha256, Kem>(&mode, &pk, &info, key.as_bytes(), b"").map_err(|_| Error::Malformed("public key (encapsulation failed)"))?;
    let mut out = Vec::with_capacity(7 + sender.key_id.len() + WRAPPED_KEY_LEN);
    out.push(WRAP_V2_FORMAT);
    out.extend_from_slice(&version.to_be_bytes());
    push_len_prefixed(&mut out, sender.key_id.as_bytes())?;
    out.extend_from_slice(enc.to_bytes().as_slice());
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// The sender key id a share wrap names (unverified until [`open_share_wrap`] succeeds with that
/// sender's pinned key).
pub fn share_wrap_sender(wrapped: &[u8]) -> Result<String, Error> {
    match crate::wrap_header(wrapped)? {
        crate::WrapHeader::Authenticated { sender_key_id, .. } => Ok(sender_key_id),
        crate::WrapHeader::Legacy => Err(Error::Malformed("share wrap (unauthenticated)")),
    }
}

/// Opens a share key wrap made for this device (`recipient_key_id`) and verifies it against
/// `sender_public_key`, the key the caller pinned for the sender the wrap names. Fails with
/// [`Error::Decrypt`] for a wrap made by anyone else, or for another workspace, share, version or
/// device.
pub fn open_share_wrap(device: &DeviceKeyPair, recipient_key_id: &str, sender_public_key: &[u8], workspace_id: &str, share_id: &str, version: u32, wrapped: &[u8]) -> Result<WorkspaceKey, Error> {
    let (sender_key_id, head, hpke_part) = crate::split_v2(wrapped)?;
    if head != version {
        return Err(Error::EpochMismatch { expected: version, found: head });
    }
    let pk_s = <Kem as hpke::Kem>::PublicKey::from_bytes(sender_public_key).map_err(|_| Error::Malformed("sender public key"))?;
    let enc = <Kem as hpke::Kem>::EncappedKey::from_bytes(&hpke_part[..32]).map_err(|_| Error::Malformed("wrapped key"))?;
    let info = share_wrap_info(workspace_id, share_id, version, sender_key_id, recipient_key_id)?;
    let mut opened = hpke::single_shot_open::<HpkeChaCha, HkdfSha256, Kem>(&OpModeR::Auth(pk_s), &device.secret, &enc, &info, &hpke_part[32..], b"").map_err(|_| Error::Decrypt)?;
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&opened);
    opened.zeroize();
    Ok(WorkspaceKey::from_bytes(bytes))
}

#[cfg(test)]
#[path = "tests_shared.rs"]
mod tests;
