//! Client-side end-to-end encryption for hitop sync (MIT).
//!
//! The sync server (`sync-server/`, AGPL) stores what this crate produces and never sees a key.
//! Nothing here is a new primitive; it only fixes how two standard constructions are used:
//!
//! - **Blobs**: XChaCha20-Poly1305 (`chacha20poly1305`) under the workspace key, with a random
//!   24-byte nonce and associated data that binds the workspace id, the blob id, the blob version
//!   and the key epoch, so the server can't swap, replay under another version, or move a blob.
//! - **Key wrapping** (format 2, since 0.2): HPKE (RFC 9180) in **Auth mode** with
//!   DHKEM(X25519, HKDF-SHA256), HKDF-SHA256 and ChaCha20-Poly1305 (`hpke`) seals the 32-byte
//!   workspace key to each member device's public key *with the wrapping device's own X25519 key*,
//!   so the recipient learns which device made the wrap ([`seal_wrap`], [`open_wrap`]). The
//!   workspace id, the epoch, the wrapper's current epoch ("head") and both device key ids go into
//!   HPKE's `info`, so a wrap can't be replayed for another workspace, epoch or device, and a
//!   device learns how far the workspace's epochs went when it was handed a key.
//! - **Accepting keys**: [`accept_wraps`] is the one policy every client runs: a key is used only
//!   when its wrap opens under the *pinned* key of the device that made it (trust on first use,
//!   fingerprints compared out of band), the workspace never goes back to an older epoch than the
//!   device has seen, and the key of an epoch the device already holds never changes
//!   ([`WorkspaceMemory`]).
//! - **Legacy wraps** (format 1, HPKE base mode, [`wrap_key`] / [`unwrap_key`]) don't say who made
//!   them; hitop 1.1 wrote them. [`accept_wraps`] takes them only for a workspace this device knew
//!   before it became strict, with a [`Notice`], and clients replace them with format 2 wraps.
//! - **Rotation**: removing a member means a new random workspace key under the next epoch, wrapped
//!   for every remaining device ([`rotate`]). Old blobs stay readable with the old epoch's key;
//!   clients re-encrypt them under the new epoch when they next write them.
//!
//! Byte layouts are in `sync-server/THREAT_MODEL.md`, `vectors/v1.json` (blobs, format 1 wraps)
//! and `vectors/v2.json` (format 2 wraps, key commitments), which every implementation must pass.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use hpke::aead::ChaCha20Poly1305 as HpkeChaCha;
use hpke::kdf::HkdfSha256;
use hpke::kem::X25519HkdfSha256;
use hpke::{Deserializable, Kem as _, OpModeR, OpModeS, Serializable};
use zeroize::{Zeroize, ZeroizeOnDrop};

mod keyring;
pub use keyring::{accept_wraps, key_commitment, normalize_fingerprint, Keyring, Notice, ReceivedWrap, WorkspaceMemory};

/// First byte of every blob envelope.
pub const BLOB_FORMAT: u8 = 1;
/// Domain separation prefix of the blob associated data.
pub const BLOB_AAD_LABEL: &[u8] = b"hitop-sync/v1/blob";
/// Domain separation prefix of the HPKE `info` for format 1 (legacy, unauthenticated) wraps.
pub const WRAP_INFO_LABEL: &[u8] = b"hitop-sync/v1/wrap";
/// Domain separation prefix of the HPKE `info` for format 2 (authenticated) wraps.
pub const WRAP_V2_INFO_LABEL: &[u8] = b"hitop-sync/v2/wrap";
/// First byte of a format 2 wrap. A format 1 wrap has no format byte (it is exactly
/// [`WRAPPED_KEY_LEN`] bytes long).
pub const WRAP_V2_FORMAT: u8 = 2;
/// Longest device key id a format 2 wrap carries.
pub const MAX_KEY_ID_LEN: usize = 128;
/// Envelope header: format (1) + epoch (4) + nonce (24).
pub const BLOB_HEADER_LEN: usize = 1 + 4 + 24;
/// Poly1305 tag length.
pub const TAG_LEN: usize = 16;
/// The HPKE part of a wrap: encapsulated key (32) + sealed workspace key (32) + tag (16). A format
/// 1 wrap is exactly this; a format 2 wrap ends with it.
pub const WRAPPED_KEY_LEN: usize = 32 + 32 + TAG_LEN;
/// X25519 key length (public and secret).
pub const KEY_LEN: usize = 32;

type Kem = X25519HkdfSha256;

/// Everything that can go wrong. The variants from [`Error::UntrustedSender`] on are what
/// [`accept_wraps`] refuses: show them to the person, they are what an attack looks like.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Authentication failed: wrong key, wrong context, or tampered bytes.
    Decrypt,
    /// The bytes aren't a blob envelope or wrapped key of this format.
    Malformed(&'static str),
    /// The envelope's epoch isn't the one the caller expected.
    EpochMismatch { expected: u32, found: u32 },
    /// An identifier is longer than its length prefix (or [`MAX_KEY_ID_LEN`]) allows.
    TooLong,
    /// A wrap for the current epoch names a sender device this device hasn't pinned. When the
    /// server listed that device, `fingerprint` is its key's fingerprint, to compare with its
    /// owner out of band before trusting it.
    UntrustedSender { epoch: u32, sender_key_id: String, fingerprint: Option<String> },
    /// A wrap names a pinned sender but doesn't open under that sender's pinned key: forged (by the
    /// server, or anyone who can store wraps), or the sender's key was replaced.
    Forged { epoch: u32, sender_key_id: String },
    /// A wrap for an epoch this device already holds opens to a *different* key: someone swapped
    /// the workspace key.
    KeyChanged { epoch: u32 },
    /// The server says the current epoch is older than one this device has seen (a rotation
    /// hidden or rolled back).
    Rollback { current: u32, known: u32 },
    /// A format 1 (unauthenticated) wrap for the current epoch of a workspace this device only
    /// accepts authenticated wraps for.
    LegacyRefused { epoch: u32 },
    /// No usable key for the workspace's current epoch (not wrapped for this device yet).
    MissingKey { epoch: u32 },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Decrypt => write!(f, "decryption failed (wrong key or tampered data)"),
            Error::Malformed(what) => write!(f, "malformed {what}"),
            Error::EpochMismatch { expected, found } => write!(f, "blob is under key epoch {found}, expected {expected}"),
            Error::TooLong => write!(f, "identifier too long"),
            Error::UntrustedSender { epoch, sender_key_id, fingerprint: Some(fp) } => write!(
                f,
                "the workspace key (epoch {epoch}) was handed to this device by device {sender_key_id}, key fingerprint {fp}, which you haven't trusted: compare the fingerprint with its owner, then trust it"
            ),
            Error::UntrustedSender { epoch, sender_key_id, fingerprint: None } => {
                write!(f, "the workspace key (epoch {epoch}) was handed to this device by device {sender_key_id}, which you haven't trusted and the server doesn't list as a member device: refusing it")
            }
            Error::Forged { epoch, sender_key_id } => write!(
                f,
                "the workspace key (epoch {epoch}) claims to come from device {sender_key_id} but doesn't verify against the key you trusted for it: forged or substituted, refusing it"
            ),
            Error::KeyChanged { epoch } => write!(f, "the workspace key of epoch {epoch} is not the one this device accepted before: someone substituted it, refusing it"),
            Error::Rollback { current, known } => write!(
                f,
                "the server says the workspace is at key epoch {current}, but this device has seen epoch {known}: a key rotation is being hidden, refusing to continue"
            ),
            Error::LegacyRefused { epoch } => write!(
                f,
                "the workspace key (epoch {epoch}) was handed out without authentication (by hitop before 1.2), which this device no longer accepts for this workspace: ask an owner to sync with hitop 1.2 or newer"
            ),
            Error::MissingKey { epoch } => write!(f, "this device has no key for the workspace's current epoch {epoch} yet"),
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
/// stores the public key so other members can wrap workspace keys for it. Since format 2 wraps
/// the same key also authenticates the wraps this device makes, so its fingerprint is what
/// people compare.
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

pub(crate) fn push_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), Error> {
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

// ---- key wrapping ----------------------------------------------------------------------------

/// HPKE `info` for a format 1 (legacy) wrap: `label || u16 len || workspace id || u32 epoch`, big
/// endian.
pub fn wrap_info(workspace_id: &str, epoch: u32) -> Result<Vec<u8>, Error> {
    let mut info = Vec::with_capacity(WRAP_INFO_LABEL.len() + 6 + workspace_id.len());
    info.extend_from_slice(WRAP_INFO_LABEL);
    push_len_prefixed(&mut info, workspace_id.as_bytes())?;
    info.extend_from_slice(&epoch.to_be_bytes());
    Ok(info)
}

/// Format 1 (legacy) wrap: HPKE base mode, `enc (32) || HPKE ciphertext of the key (32) || tag
/// (16)`, with [`wrap_info`] as `info` and empty associated data. **Unauthenticated**: anyone who
/// knows the recipient's public key (the server does) can make one. Kept for the test vectors and
/// for workspaces made by hitop 1.1; new wraps are [`seal_wrap`].
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

/// Opens a format 1 (legacy) wrap [`wrap_key`] made for this device, workspace and epoch. It says
/// nothing about who made the wrap: [`accept_wraps`] decides when such a key may be used.
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

/// This device: its key id (the server's) and its key pair. As the sender of a wrap its X25519
/// key authenticates the wrap (HPKE Auth mode); as the recipient it opens wraps made for it.
#[derive(Debug, Clone, Copy)]
pub struct Device<'a> {
    pub key_id: &'a str,
    pub keys: &'a DeviceKeyPair,
}

/// A device that gets a wrapped key: an id the caller tracks (the server's key id) and its
/// public key.
#[derive(Debug, Clone, Copy)]
pub struct Recipient<'a> {
    pub key_id: &'a str,
    pub public_key: &'a [u8],
}

/// What a wrap is, read from its bytes before opening it: which sender key to verify it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapHeader {
    /// Format 1: no sender, no authentication.
    Legacy,
    /// Format 2: made by the device `sender_key_id`, which then knew epochs up to `head`. Both are
    /// authenticated only once [`open_wrap`] succeeds.
    Authenticated { sender_key_id: String, head: u32 },
}

/// Reads a wrap's format and, for format 2, its sender key id and head (unverified).
pub fn wrap_header(wrapped: &[u8]) -> Result<WrapHeader, Error> {
    if wrapped.len() == WRAPPED_KEY_LEN {
        return Ok(WrapHeader::Legacy);
    }
    split_v2(wrapped).map(|(sender_key_id, head, _)| WrapHeader::Authenticated { sender_key_id: sender_key_id.to_string(), head })
}

fn split_v2(wrapped: &[u8]) -> Result<(&str, u32, &[u8]), Error> {
    if wrapped.len() < 7 + 1 + WRAPPED_KEY_LEN || wrapped[0] != WRAP_V2_FORMAT {
        return Err(Error::Malformed("wrapped key (unknown format)"));
    }
    let head = u32::from_be_bytes([wrapped[1], wrapped[2], wrapped[3], wrapped[4]]);
    let len = u16::from_be_bytes([wrapped[5], wrapped[6]]) as usize;
    if len == 0 || len > MAX_KEY_ID_LEN || wrapped.len() != 7 + len + WRAPPED_KEY_LEN {
        return Err(Error::Malformed("wrapped key (wrong length)"));
    }
    let sender = std::str::from_utf8(&wrapped[7..7 + len]).map_err(|_| Error::Malformed("wrapped key (sender key id)"))?;
    Ok((sender, head, &wrapped[7 + len..]))
}

/// HPKE `info` for a format 2 wrap: `"hitop-sync/v2/wrap" || u16 len || workspace id || u32 epoch
/// || u32 head || u16 len || sender key id || u16 len || recipient key id`, big endian, UTF-8.
pub fn wrap_info_v2(workspace_id: &str, epoch: u32, head: u32, sender_key_id: &str, recipient_key_id: &str) -> Result<Vec<u8>, Error> {
    let mut info = Vec::with_capacity(WRAP_V2_INFO_LABEL.len() + 14 + workspace_id.len() + sender_key_id.len() + recipient_key_id.len());
    info.extend_from_slice(WRAP_V2_INFO_LABEL);
    push_len_prefixed(&mut info, workspace_id.as_bytes())?;
    info.extend_from_slice(&epoch.to_be_bytes());
    info.extend_from_slice(&head.to_be_bytes());
    push_len_prefixed(&mut info, sender_key_id.as_bytes())?;
    push_len_prefixed(&mut info, recipient_key_id.as_bytes())?;
    Ok(info)
}

/// Format 2 wrap of `key` (epoch `epoch` of `workspace_id`) for one device, made and authenticated
/// by `sender`: `0x02 || u32 head || u16 len || sender key id || enc (32) || sealed key (32) || tag
/// (16)`, HPKE Auth mode with [`wrap_info_v2`] as `info` and empty associated data. `head` is the
/// newest epoch the sender knows (the workspace's current epoch), at least `epoch`.
pub fn seal_wrap(sender: &Device, recipient: &Recipient, workspace_id: &str, epoch: u32, head: u32, key: &WorkspaceKey) -> Result<Vec<u8>, Error> {
    if head < epoch {
        return Err(Error::Malformed("wrap head (older than its epoch)"));
    }
    if sender.key_id.is_empty() || sender.key_id.len() > MAX_KEY_ID_LEN || recipient.key_id.len() > MAX_KEY_ID_LEN {
        return Err(Error::TooLong);
    }
    let pk = <Kem as hpke::Kem>::PublicKey::from_bytes(recipient.public_key).map_err(|_| Error::Malformed("public key"))?;
    let info = wrap_info_v2(workspace_id, epoch, head, sender.key_id, recipient.key_id)?;
    let mode = OpModeS::Auth((sender.keys.secret.clone(), sender.keys.public.clone()));
    let (enc, sealed) = hpke::single_shot_seal::<HpkeChaCha, HkdfSha256, Kem>(&mode, &pk, &info, &key.0, b"")
        .map_err(|_| Error::Malformed("public key (encapsulation failed)"))?;
    let mut out = Vec::with_capacity(7 + sender.key_id.len() + WRAPPED_KEY_LEN);
    out.push(WRAP_V2_FORMAT);
    out.extend_from_slice(&head.to_be_bytes());
    push_len_prefixed(&mut out, sender.key_id.as_bytes())?;
    out.extend_from_slice(enc.to_bytes().as_slice());
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// A format 2 wrap, opened and verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedWrap {
    pub key: WorkspaceKey,
    /// The device that made it (its key verified the wrap).
    pub sender_key_id: String,
    /// The newest epoch that device knew when it made the wrap.
    pub head: u32,
}

/// Opens a format 2 wrap made for this device (`recipient_key_id`) and verifies it against
/// `sender_public_key`: the key the caller *pinned* for the sender the wrap names
/// ([`wrap_header`]). Fails with [`Error::Decrypt`] when the wrap wasn't made by the holder of that
/// key, or was made for another workspace, epoch or device.
pub fn open_wrap(device: &DeviceKeyPair, recipient_key_id: &str, sender_public_key: &[u8], workspace_id: &str, epoch: u32, wrapped: &[u8]) -> Result<OpenedWrap, Error> {
    let (sender_key_id, head, hpke_part) = split_v2(wrapped)?;
    let pk_s = <Kem as hpke::Kem>::PublicKey::from_bytes(sender_public_key).map_err(|_| Error::Malformed("sender public key"))?;
    let enc = <Kem as hpke::Kem>::EncappedKey::from_bytes(&hpke_part[..32]).map_err(|_| Error::Malformed("wrapped key"))?;
    let info = wrap_info_v2(workspace_id, epoch, head, sender_key_id, recipient_key_id)?;
    let mut opened = hpke::single_shot_open::<HpkeChaCha, HkdfSha256, Kem>(&OpModeR::Auth(pk_s), &device.secret, &enc, &info, &hpke_part[32..], b"")
        .map_err(|_| Error::Decrypt)?;
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&opened);
    opened.zeroize();
    if head < epoch {
        return Err(Error::Malformed("wrap head (older than its epoch)"));
    }
    Ok(OpenedWrap { key: WorkspaceKey(bytes), sender_key_id: sender_key_id.to_string(), head })
}

/// `(key id, wrapped key)` pairs, one per recipient device.
pub type Wraps = Vec<(String, Vec<u8>)>;

/// A new workspace (`epoch` 1) or a rotation (`epoch` = current + 1): a fresh random key and its
/// format 2 wraps, made by `sender`, for every recipient, as `(key id, wrapped key)`.
pub fn rotate(sender: &Device, workspace_id: &str, epoch: u32, recipients: &[Recipient]) -> Result<(WorkspaceKey, Wraps), Error> {
    let key = WorkspaceKey::generate();
    let wraps = recipients
        .iter()
        .map(|r| Ok((r.key_id.to_string(), seal_wrap(sender, r, workspace_id, epoch, epoch, &key)?)))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok((key, wraps))
}

/// A device public key's fingerprint, for people to compare out of band before trusting a key:
/// the first 16 bytes of SHA-256(`"hitop-sync/v1/fingerprint"` || public key) as eight groups of
/// four hex digits. Clients pin a key once it is confirmed (trust on first use) and refuse a
/// changed key for the same key id. Unchanged in format 2: the same key now also authenticates
/// the wraps its device makes.
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
