//! Client-side end-to-end encryption for hitop sync (MIT).
//!
//! The sync server (`sync-server/`, AGPL) stores what this crate produces and never sees a key.
//! Nothing here is a new primitive; it only fixes how two standard constructions are used:
//!
//! - **Blobs**: XChaCha20-Poly1305 (`chacha20poly1305`) under the workspace key, with a random
//!   24-byte nonce and associated data that binds the workspace id, the blob id, the blob version
//!   and the key epoch, so the server can't swap, replay under another version, or move a blob.
//! - **Key wrapping**: HPKE (RFC 9180, base mode) with DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and
//!   ChaCha20-Poly1305 (`hpke`) seals the 32-byte workspace key to each member device's public key.
//!   The workspace id and epoch go into HPKE's `info`, so a wrapped key can't be replayed for
//!   another workspace or epoch.
//! - **Rotation**: removing a member means a new random workspace key under the next epoch, wrapped
//!   for every remaining device ([`rotate`]). Old blobs stay readable with the old epoch's key;
//!   clients re-encrypt them under the new epoch when they next write them.
//!
//! Byte layouts are in `THREAT_MODEL.md` and in `vectors/v1.json`, which any other implementation
//! (the web app in T24) must pass.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use hpke::aead::ChaCha20Poly1305 as HpkeChaCha;
use hpke::kdf::HkdfSha256;
use hpke::kem::X25519HkdfSha256;
use hpke::{Deserializable, Kem as _, OpModeR, OpModeS, Serializable};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// First byte of every blob envelope.
pub const BLOB_FORMAT: u8 = 1;
/// Domain separation prefix of the blob associated data.
pub const BLOB_AAD_LABEL: &[u8] = b"hitop-sync/v1/blob";
/// Domain separation prefix of the HPKE `info` for wrapped workspace keys.
pub const WRAP_INFO_LABEL: &[u8] = b"hitop-sync/v1/wrap";
/// Envelope header: format (1) + epoch (4) + nonce (24).
pub const BLOB_HEADER_LEN: usize = 1 + 4 + 24;
/// Poly1305 tag length.
pub const TAG_LEN: usize = 16;
/// Wrapped key: HPKE encapsulated key (32) + sealed workspace key (32) + tag (16).
pub const WRAPPED_KEY_LEN: usize = 32 + 32 + TAG_LEN;
/// X25519 key length (public and secret).
pub const KEY_LEN: usize = 32;

type Kem = X25519HkdfSha256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Authentication failed: wrong key, wrong context, or tampered bytes.
    Decrypt,
    /// The bytes aren't a blob envelope or wrapped key of this format.
    Malformed(&'static str),
    /// The envelope's epoch isn't the one the caller expected.
    EpochMismatch { expected: u32, found: u32 },
    /// An identifier is longer than the 65535 bytes its length prefix allows.
    TooLong,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Decrypt => write!(f, "decryption failed (wrong key or tampered data)"),
            Error::Malformed(what) => write!(f, "malformed {what}"),
            Error::EpochMismatch { expected, found } => write!(f, "blob is under key epoch {found}, expected {expected}"),
            Error::TooLong => write!(f, "identifier too long"),
        }
    }
}

impl std::error::Error for Error {}

fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("the OS random number generator failed");
    bytes
}

/// A workspace's symmetric key for one epoch. Zeroed on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct WorkspaceKey([u8; 32]);

impl std::fmt::Debug for WorkspaceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkspaceKey(..)")
    }
}

impl WorkspaceKey {
    /// A fresh random key from the OS RNG.
    pub fn generate() -> Self {
        WorkspaceKey(random())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        WorkspaceKey(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A member device's HPKE key pair (X25519). The secret never leaves the device; the server
/// stores the public key so other members can wrap workspace keys for it.
#[derive(Clone)]
pub struct DeviceKeyPair {
    secret: <Kem as hpke::Kem>::PrivateKey,
    public: <Kem as hpke::Kem>::PublicKey,
}

impl std::fmt::Debug for DeviceKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKeyPair").field("public", &self.public_key()).finish_non_exhaustive()
    }
}

impl DeviceKeyPair {
    /// A fresh key pair from the OS RNG.
    pub fn generate() -> Self {
        let (secret, public) = Kem::gen_keypair();
        DeviceKeyPair { secret, public }
    }

    /// RFC 9180 DeriveKeyPair from input keying material (at least 32 bytes of entropy). Used for
    /// the test vectors and for recovery codes.
    pub fn derive(ikm: &[u8]) -> Self {
        let (secret, public) = Kem::derive_keypair(ikm);
        DeviceKeyPair { secret, public }
    }

    /// Restore from the 32 secret bytes [`Self::secret_bytes`] returned.
    pub fn from_secret_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let secret = <Kem as hpke::Kem>::PrivateKey::from_bytes(bytes).map_err(|_| Error::Malformed("secret key"))?;
        let public = Kem::sk_to_pk(&secret);
        Ok(DeviceKeyPair { secret, public })
    }

    pub fn public_key(&self) -> [u8; KEY_LEN] {
        let mut out = [0u8; KEY_LEN];
        out.copy_from_slice(self.public.to_bytes().as_slice());
        out
    }

    /// The secret key, for the client's keychain. Zeroize the copy after storing it.
    pub fn secret_bytes(&self) -> [u8; KEY_LEN] {
        let mut out = [0u8; KEY_LEN];
        out.copy_from_slice(self.secret.to_bytes().as_slice());
        out
    }
}

fn push_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), Error> {
    let len = u16::try_from(bytes.len()).map_err(|_| Error::TooLong)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// What a blob's ciphertext is bound to. `version` is the version the blob is written as (the
/// server only accepts it as that version), `epoch` the workspace key epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobContext<'a> {
    pub workspace_id: &'a str,
    pub blob_id: &'a str,
    pub version: u64,
    pub epoch: u32,
}

impl BlobContext<'_> {
    /// `label || u16 len || workspace id || u16 len || blob id || u64 version || u32 epoch`, big endian.
    pub fn associated_data(&self) -> Result<Vec<u8>, Error> {
        let mut aad = Vec::with_capacity(BLOB_AAD_LABEL.len() + 4 + self.workspace_id.len() + self.blob_id.len() + 12);
        aad.extend_from_slice(BLOB_AAD_LABEL);
        push_len_prefixed(&mut aad, self.workspace_id.as_bytes())?;
        push_len_prefixed(&mut aad, self.blob_id.as_bytes())?;
        aad.extend_from_slice(&self.version.to_be_bytes());
        aad.extend_from_slice(&self.epoch.to_be_bytes());
        Ok(aad)
    }
}

/// Encrypts a blob: `format (0x01) || epoch u32 BE || nonce (24) || ciphertext || tag (16)`.
pub fn encrypt_blob(key: &WorkspaceKey, ctx: &BlobContext, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
    encrypt_blob_with_nonce(key, ctx, plaintext, &random::<24>())
}

/// [`encrypt_blob`] with a caller-chosen nonce. Only for test vectors: a repeated nonce under one
/// key breaks confidentiality.
#[doc(hidden)]
pub fn encrypt_blob_with_nonce(key: &WorkspaceKey, ctx: &BlobContext, plaintext: &[u8], nonce: &[u8; 24]) -> Result<Vec<u8>, Error> {
    let aad = ctx.associated_data()?;
    let cipher = XChaCha20Poly1305::new(&Key::from(key.0));
    let ciphertext = cipher
        .encrypt(&XNonce::from(*nonce), Payload { msg: plaintext, aad: &aad })
        .map_err(|_| Error::Malformed("plaintext (too long)"))?;
    let mut out = Vec::with_capacity(BLOB_HEADER_LEN + ciphertext.len());
    out.push(BLOB_FORMAT);
    out.extend_from_slice(&ctx.epoch.to_be_bytes());
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// The key epoch an envelope says it was written under, to pick the key before decrypting.
pub fn blob_epoch(envelope: &[u8]) -> Result<u32, Error> {
    if envelope.len() < BLOB_HEADER_LEN + TAG_LEN {
        return Err(Error::Malformed("blob envelope (too short)"));
    }
    if envelope[0] != BLOB_FORMAT {
        return Err(Error::Malformed("blob envelope (unknown format)"));
    }
    Ok(u32::from_be_bytes([envelope[1], envelope[2], envelope[3], envelope[4]]))
}

/// Decrypts a blob envelope written by [`encrypt_blob`] for exactly this context.
pub fn decrypt_blob(key: &WorkspaceKey, ctx: &BlobContext, envelope: &[u8]) -> Result<Vec<u8>, Error> {
    let epoch = blob_epoch(envelope)?;
    if epoch != ctx.epoch {
        return Err(Error::EpochMismatch { expected: ctx.epoch, found: epoch });
    }
    let aad = ctx.associated_data()?;
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&envelope[5..BLOB_HEADER_LEN]);
    let cipher = XChaCha20Poly1305::new(&Key::from(key.0));
    cipher
        .decrypt(&XNonce::from(nonce), Payload { msg: &envelope[BLOB_HEADER_LEN..], aad: &aad })
        .map_err(|_| Error::Decrypt)
}

/// HPKE `info` for a wrapped key: `label || u16 len || workspace id || u32 epoch`, big endian.
pub fn wrap_info(workspace_id: &str, epoch: u32) -> Result<Vec<u8>, Error> {
    let mut info = Vec::with_capacity(WRAP_INFO_LABEL.len() + 6 + workspace_id.len());
    info.extend_from_slice(WRAP_INFO_LABEL);
    push_len_prefixed(&mut info, workspace_id.as_bytes())?;
    info.extend_from_slice(&epoch.to_be_bytes());
    Ok(info)
}

/// Wraps `key` for one device: `enc (32) || HPKE ciphertext of the key (32) || tag (16)`, with
/// [`wrap_info`] as `info` and empty associated data.
pub fn wrap_key(recipient_public_key: &[u8], workspace_id: &str, epoch: u32, key: &WorkspaceKey) -> Result<Vec<u8>, Error> {
    let pk = <Kem as hpke::Kem>::PublicKey::from_bytes(recipient_public_key).map_err(|_| Error::Malformed("public key"))?;
    let info = wrap_info(workspace_id, epoch)?;
    let (enc, sealed) = hpke::single_shot_seal::<HpkeChaCha, HkdfSha256, Kem>(&OpModeS::Base, &pk, &info, &key.0, b"")
        .map_err(|_| Error::Malformed("public key (encapsulation failed)"))?;
    let mut out = Vec::with_capacity(WRAPPED_KEY_LEN);
    out.extend_from_slice(enc.to_bytes().as_slice());
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Unwraps a key [`wrap_key`] made for this device, workspace and epoch.
pub fn unwrap_key(device: &DeviceKeyPair, workspace_id: &str, epoch: u32, wrapped: &[u8]) -> Result<WorkspaceKey, Error> {
    if wrapped.len() != WRAPPED_KEY_LEN {
        return Err(Error::Malformed("wrapped key (wrong length)"));
    }
    let enc = <Kem as hpke::Kem>::EncappedKey::from_bytes(&wrapped[..32]).map_err(|_| Error::Malformed("wrapped key"))?;
    let info = wrap_info(workspace_id, epoch)?;
    let mut opened = hpke::single_shot_open::<HpkeChaCha, HkdfSha256, Kem>(&OpModeR::Base, &device.secret, &enc, &info, &wrapped[32..], b"")
        .map_err(|_| Error::Decrypt)?;
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&opened);
    opened.zeroize();
    Ok(WorkspaceKey(bytes))
}

/// A device that gets a wrapped key: an id the caller tracks (the server's key id) and its
/// public key.
#[derive(Debug, Clone, Copy)]
pub struct Recipient<'a> {
    pub key_id: &'a str,
    pub public_key: &'a [u8],
}

/// `(key id, wrapped key)` pairs, one per recipient device.
pub type Wraps = Vec<(String, Vec<u8>)>;

/// A new workspace (`epoch` 1) or a rotation (`epoch` = current + 1): a fresh random key and its
/// wraps for every recipient, as `(key id, wrapped key)`.
pub fn rotate(workspace_id: &str, epoch: u32, recipients: &[Recipient]) -> Result<(WorkspaceKey, Wraps), Error> {
    let key = WorkspaceKey::generate();
    let wraps = recipients
        .iter()
        .map(|r| Ok((r.key_id.to_string(), wrap_key(r.public_key, workspace_id, epoch, &key)?)))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok((key, wraps))
}

/// A device public key's fingerprint, for people to compare out of band before trusting a key:
/// the first 16 bytes of SHA-256(`"hitop-sync/v1/fingerprint"` || public key) as eight groups of
/// four hex digits. Clients pin a key once it is confirmed (trust on first use) and refuse a
/// changed key for the same key id.
pub fn fingerprint(public_key: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(b"hitop-sync/v1/fingerprint");
    h.update(public_key);
    let digest = h.finalize();
    digest[..16].chunks(2).map(|c| format!("{:02x}{:02x}", c[0], c[1])).collect::<Vec<_>>().join(" ")
}

/// A random identifier: `N` bytes from the OS RNG as lower-case hex (workspace ids: 16, blob
/// ids: 16).
pub fn random_hex<const N: usize>() -> String {
    random::<N>().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests;
