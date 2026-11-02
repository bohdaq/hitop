//! Which wrapped workspace keys a device may use: the one acceptance policy every client (the CLI,
//! and the web and desktop apps through WASM) runs, so they can't drift apart.

use std::collections::BTreeMap;

use crate::{fingerprint, open_wrap, push_len_prefixed, unwrap_key, wrap_header, Device, Error, WorkspaceKey, WrapHeader};

/// What a device remembers about one workspace's keys between runs. The caller keeps it (a file,
/// app storage) next to its pins, and passes it to every [`accept_wraps`]; it changes only when
/// that call succeeds. With the `serde` feature it (de)serializes as camelCase JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase", default))]
pub struct WorkspaceMemory {
    /// Only authenticated (format 2) wraps are accepted. Set by the caller when this device made
    /// the workspace or joined it through an invite with hitop 1.2 or newer, and by
    /// [`accept_wraps`] once an authenticated wrap was accepted.
    pub strict: bool,
    /// The newest key epoch this device knows the workspace reached: a server that later says
    /// the current epoch is older is hiding a rotation.
    pub max_epoch: u32,
    /// Epoch → [`key_commitment`] of the key accepted for it: the key of an epoch never changes.
    pub commitments: BTreeMap<u32, String>,
    /// Fingerprints (hex digits only, lower case) of devices expected to hand this device the
    /// key: the inviter's, from an invite code given out of band. A sender listed with one of them
    /// is pinned on first use.
    pub expected_senders: Vec<String>,
}

/// One wrapped key the server returned for this device.
#[derive(Debug, Clone, Copy)]
pub struct ReceivedWrap<'a> {
    pub epoch: u32,
    pub wrapped: &'a [u8],
}

/// Something [`accept_wraps`] did that the person should see (show every one; the first two are
/// warnings about an unauthenticated key).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase"))]
#[non_exhaustive]
pub enum Notice {
    /// A format 1 wrap (no sender authentication, from hitop before 1.2) was used for `epoch`.
    LegacyAccepted { epoch: u32 },
    /// A format 1 wrap for an older epoch was not used: this device accepts only authenticated
    /// wraps for this workspace. Files still under that epoch can't be read until it is re-wrapped.
    LegacyIgnored { epoch: u32 },
    /// A wrap for an older epoch from a device that isn't trusted was not used.
    UntrustedIgnored { epoch: u32, sender_key_id: String, fingerprint: Option<String> },
    /// A wrap from a device that isn't trusted opened to the key this device already had for that
    /// epoch, so it was used; trust the device before it hands out a *new* key.
    KnownKeyFromUntrusted { epoch: u32, sender_key_id: String, fingerprint: String },
    /// A sender was trusted because its fingerprint is the one an invite code named.
    SenderPinned { sender_key_id: String, fingerprint: String },
    /// The server lists another public key for a device whose key you trusted. The trusted key is
    /// still the one used, so wraps from the new key are refused.
    SenderKeyChanged { sender_key_id: String, fingerprint: String },
}

impl std::fmt::Display for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Notice::LegacyAccepted { epoch } => write!(
                f,
                "the workspace key (epoch {epoch}) was handed out without authentication by hitop before 1.2; using it, but a malicious server could have chosen it. The owner's next sync with hitop 1.2 replaces it with an authenticated one"
            ),
            Notice::LegacyIgnored { epoch } => write!(f, "ignored an unauthenticated workspace key for the older epoch {epoch}"),
            Notice::UntrustedIgnored { epoch, sender_key_id, fingerprint } => match fingerprint {
                Some(fp) => write!(f, "ignored the workspace key for the older epoch {epoch} from device {sender_key_id} (fingerprint {fp}), which you haven't trusted"),
                None => write!(f, "ignored the workspace key for the older epoch {epoch} from device {sender_key_id}, which you haven't trusted"),
            },
            Notice::KnownKeyFromUntrusted { epoch, sender_key_id, fingerprint } => write!(
                f,
                "device {sender_key_id} (fingerprint {fingerprint}) re-sent the workspace key of epoch {epoch} you already had; trust it after comparing its fingerprint, or its next key will be refused"
            ),
            Notice::SenderPinned { sender_key_id, fingerprint } => write!(f, "trusted device {sender_key_id} (fingerprint {fingerprint}): it is the one your invite code named"),
            Notice::SenderKeyChanged { sender_key_id, fingerprint } => write!(
                f,
                "WARNING: the server now lists a different key (fingerprint {fingerprint}) for device {sender_key_id}, whose key you trusted. Keys from it are refused; if its owner really replaced it, they register a new device"
            ),
        }
    }
}

/// The keys a device may use for one workspace, from [`accept_wraps`].
#[derive(Debug)]
pub struct Keyring {
    /// Epoch → key, for every accepted wrap.
    pub keys: BTreeMap<u32, WorkspaceKey>,
    /// The current epoch (the server's, checked against what this device has seen).
    pub current: u32,
    pub notices: Vec<Notice>,
    /// Devices trusted during this call (key id → public key), from [`WorkspaceMemory::expected_senders`]:
    /// add them to the caller's pins.
    pub new_pins: Vec<(String, Vec<u8>)>,
}

impl Keyring {
    /// The key of the current epoch (present whenever [`accept_wraps`] succeeded).
    pub fn current_key(&self) -> &WorkspaceKey {
        &self.keys[&self.current]
    }

    /// Whether a key in use came from an unauthenticated (format 1) wrap.
    pub fn uses_legacy(&self) -> bool {
        self.notices.iter().any(|n| matches!(n, Notice::LegacyAccepted { .. }))
    }
}

/// A commitment to the key of one epoch, which a device keeps to notice a swapped key later:
/// lower-case hex of SHA-256(`"hitop-sync/v2/key-commitment"` || u16 len || workspace id || u32
/// epoch || key). It reveals nothing about the 256-bit key.
pub fn key_commitment(workspace_id: &str, epoch: u32, key: &WorkspaceKey) -> String {
    use sha2::Digest;
    let mut bytes = b"hitop-sync/v2/key-commitment".to_vec();
    // Workspace ids are 32 characters; an id too long for the prefix still gets a distinct hash.
    if push_len_prefixed(&mut bytes, workspace_id.as_bytes()).is_err() {
        bytes.extend_from_slice(workspace_id.as_bytes());
    }
    bytes.extend_from_slice(&epoch.to_be_bytes());
    bytes.extend_from_slice(key.as_bytes());
    let digest = sha2::Sha256::digest(&bytes);
    bytes.iter_mut().for_each(|b| *b = 0);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fingerprint as compared: its hex digits only, lower case (`"62BD a406 …"` → `"62bda406…"`).
pub fn normalize_fingerprint(fp: &str) -> String {
    fp.chars().filter(char::is_ascii_hexdigit).collect::<String>().to_lowercase()
}

/// Decides which of the wraps the server returned for this device (`me`) may be used, for a
/// workspace whose current epoch the server says is `current_epoch`:
///
/// - a format 2 wrap is used only when it opens under the key `pinned` holds for the sender it
///   names (else [`Error::Forged`]); a sender that isn't pinned is refused
///   ([`Error::UntrustedSender`], with the fingerprint from `listed`, the server's member device
///   keys, when it is there), unless `listed` shows it with a fingerprint from
///   [`WorkspaceMemory::expected_senders`] (pinned now, see [`Keyring::new_pins`]);
/// - a format 1 wrap is used, with [`Notice::LegacyAccepted`], only while `memory` isn't strict;
/// - the key of an epoch `memory` already holds a commitment for must be that key
///   ([`Error::KeyChanged`]);
/// - `current_epoch` must not be older than any epoch seen before or named as a wrapper's head
///   ([`Error::Rollback`]), and there must be a key for it ([`Error::MissingKey`]).
///
/// Wraps for older epochs that can't be used are skipped with a [`Notice`]; forged or changed keys
/// always fail the call. On success `memory` is updated (save it); on failure it is unchanged.
pub fn accept_wraps(
    me: &Device,
    workspace_id: &str,
    current_epoch: u32,
    wraps: &[ReceivedWrap],
    pinned: &BTreeMap<String, Vec<u8>>,
    listed: &BTreeMap<String, Vec<u8>>,
    memory: &mut WorkspaceMemory,
) -> Result<Keyring, Error> {
    let mut mem = memory.clone();
    if current_epoch < mem.max_epoch {
        return Err(Error::Rollback { current: current_epoch, known: mem.max_epoch });
    }
    let mut pins = pinned.clone();
    let mut keys = BTreeMap::new();
    let mut notices = Vec::new();
    let mut new_pins = Vec::new();
    let mut head = 0;
    let expected: Vec<String> = mem.expected_senders.iter().map(|f| normalize_fingerprint(f)).collect();

    // Authenticated wraps first: once one is accepted, this workspace is strict.
    let mut parsed = wraps.iter().map(|w| Ok((w, wrap_header(w.wrapped)?))).collect::<Result<Vec<_>, Error>>()?;
    parsed.sort_by_key(|(w, h)| (matches!(h, WrapHeader::Legacy), w.epoch));

    let accept = |mem: &mut WorkspaceMemory, keys: &mut BTreeMap<u32, WorkspaceKey>, epoch: u32, key: WorkspaceKey| -> Result<(), Error> {
        let c = key_commitment(workspace_id, epoch, &key);
        match mem.commitments.get(&epoch) {
            Some(known) if *known != c => return Err(Error::KeyChanged { epoch }),
            _ => {}
        }
        mem.commitments.insert(epoch, c);
        mem.max_epoch = mem.max_epoch.max(epoch);
        keys.insert(epoch, key);
        Ok(())
    };

    let mut warned_changed: Vec<String> = Vec::new();
    for (w, header) in parsed {
        match header {
            WrapHeader::Authenticated { sender_key_id, .. } => {
                let listed_pk = listed.get(&sender_key_id);
                if let (Some(p), Some(l)) = (pins.get(&sender_key_id), listed_pk) {
                    if p != l && !warned_changed.contains(&sender_key_id) {
                        warned_changed.push(sender_key_id.clone());
                        notices.push(Notice::SenderKeyChanged { sender_key_id: sender_key_id.clone(), fingerprint: fingerprint(l) });
                    }
                }
                if !pins.contains_key(&sender_key_id) {
                    if let Some(l) = listed_pk {
                        let fp = fingerprint(l);
                        if expected.contains(&normalize_fingerprint(&fp)) {
                            pins.insert(sender_key_id.clone(), l.clone());
                            new_pins.push((sender_key_id.clone(), l.clone()));
                            notices.push(Notice::SenderPinned { sender_key_id: sender_key_id.clone(), fingerprint: fp });
                        }
                    }
                }
                match pins.get(&sender_key_id) {
                    Some(pk) => {
                        let opened = open_wrap(me.keys, me.key_id, pk, workspace_id, w.epoch, w.wrapped)
                            .map_err(|_| Error::Forged { epoch: w.epoch, sender_key_id: sender_key_id.clone() })?;
                        head = head.max(opened.head);
                        accept(&mut mem, &mut keys, w.epoch, opened.key)?;
                        mem.strict = true;
                    }
                    None => {
                        let fp = listed_pk.map(|l| fingerprint(l));
                        // Not trusted, but it may only re-send a key this device already holds
                        // (an owner replacing legacy wraps): nothing new is learned from it.
                        let known = listed_pk.and_then(|l| open_wrap(me.keys, me.key_id, l, workspace_id, w.epoch, w.wrapped).ok()).filter(|o| {
                            mem.commitments.get(&w.epoch).is_some_and(|c| *c == key_commitment(workspace_id, w.epoch, &o.key))
                        });
                        match (known, fp) {
                            (Some(o), Some(fp)) => {
                                accept(&mut mem, &mut keys, w.epoch, o.key)?;
                                mem.strict = true;
                                notices.push(Notice::KnownKeyFromUntrusted { epoch: w.epoch, sender_key_id, fingerprint: fp });
                            }
                            (_, fp) if w.epoch >= current_epoch => return Err(Error::UntrustedSender { epoch: w.epoch, sender_key_id, fingerprint: fp }),
                            (_, fp) => notices.push(Notice::UntrustedIgnored { epoch: w.epoch, sender_key_id, fingerprint: fp }),
                        }
                    }
                }
            }
            WrapHeader::Legacy => {
                if keys.contains_key(&w.epoch) {
                    continue;
                }
                if mem.strict {
                    if w.epoch >= current_epoch {
                        return Err(Error::LegacyRefused { epoch: w.epoch });
                    }
                    notices.push(Notice::LegacyIgnored { epoch: w.epoch });
                    continue;
                }
                let key = unwrap_key(me.keys, workspace_id, w.epoch, w.wrapped)?;
                accept(&mut mem, &mut keys, w.epoch, key)?;
                notices.push(Notice::LegacyAccepted { epoch: w.epoch });
            }
        }
    }
    mem.max_epoch = mem.max_epoch.max(head);
    if current_epoch < mem.max_epoch {
        return Err(Error::Rollback { current: current_epoch, known: mem.max_epoch });
    }
    if !keys.contains_key(&current_epoch) {
        return Err(Error::MissingKey { epoch: current_epoch });
    }
    *memory = mem;
    Ok(Keyring { keys, current: current_epoch, notices, new_pins })
}
