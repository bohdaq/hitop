//! Format 2 (authenticated) wraps and the acceptance policy: what a malicious server can try
//! with the wraps it stores, and what every client must refuse.

use std::collections::BTreeMap;

use super::{hex, unhex};
use crate::*;
use serde::{Deserialize, Serialize};

const WS: &str = "3f6c1a9e0b7d4c2a8e5f1b3d7a9c0e24";
const VECTORS_V2: &str = include_str!("../vectors/v2.json");

struct Dev {
    id: &'static str,
    keys: DeviceKeyPair,
}

impl Dev {
    fn new(id: &'static str) -> Dev {
        Dev { id, keys: DeviceKeyPair::generate() }
    }
    fn sender(&self) -> Device<'_> {
        Device { key_id: self.id, keys: &self.keys }
    }
    fn pk(&self) -> Vec<u8> {
        self.keys.public_key().to_vec()
    }
    /// A format 2 wrap of `key` from `self` for `to`.
    fn wrap(&self, to: &Dev, epoch: u32, head: u32, key: &WorkspaceKey) -> Vec<u8> {
        let pk = to.keys.public_key();
        seal_wrap(&self.sender(), &Recipient { key_id: to.id, public_key: &pk }, WS, epoch, head, key).unwrap()
    }
}

fn pins(devs: &[&Dev]) -> BTreeMap<String, Vec<u8>> {
    devs.iter().map(|d| (d.id.to_string(), d.pk())).collect()
}

fn accept(me: &Dev, current: u32, wraps: &[(u32, Vec<u8>)], pinned: &BTreeMap<String, Vec<u8>>, listed: &BTreeMap<String, Vec<u8>>, memory: &mut WorkspaceMemory) -> Result<Keyring, Error> {
    let received: Vec<ReceivedWrap> = wraps.iter().map(|(epoch, wrapped)| ReceivedWrap { epoch: *epoch, wrapped }).collect();
    accept_wraps(&Device { key_id: me.id, keys: &me.keys }, WS, current, &received, pinned, listed, memory)
}

#[test]
fn format_2_round_trip_binds_every_field() {
    let (owner, bob, mallory) = (Dev::new("owner-key"), Dev::new("bob-key"), Dev::new("mallory-key"));
    let key = WorkspaceKey::generate();
    let w = owner.wrap(&bob, 1, 3, &key);
    assert_eq!(w.len(), 7 + "owner-key".len() + WRAPPED_KEY_LEN);
    assert_eq!(wrap_header(&w).unwrap(), WrapHeader::Authenticated { sender_key_id: "owner-key".into(), head: 3 });
    let o = open_wrap(&bob.keys, "bob-key", &owner.pk(), WS, 1, &w).unwrap();
    assert_eq!((o.key, o.sender_key_id.as_str(), o.head), (key.clone(), "owner-key", 3));
    // The wrong sender key, epoch, workspace, recipient key id or recipient all fail.
    assert_eq!(open_wrap(&bob.keys, "bob-key", &mallory.pk(), WS, 1, &w), Err(Error::Decrypt));
    assert_eq!(open_wrap(&bob.keys, "bob-key", &owner.pk(), WS, 2, &w), Err(Error::Decrypt));
    assert_eq!(open_wrap(&bob.keys, "bob-key", &owner.pk(), "00000000000000000000000000000000", 1, &w), Err(Error::Decrypt));
    assert_eq!(open_wrap(&bob.keys, "other-key", &owner.pk(), WS, 1, &w), Err(Error::Decrypt));
    assert_eq!(open_wrap(&mallory.keys, "bob-key", &owner.pk(), WS, 1, &w), Err(Error::Decrypt));
    // The header is authenticated: a rewritten head or sender id fails.
    let mut head = w.clone();
    head[4] = 9;
    assert_eq!(open_wrap(&bob.keys, "bob-key", &owner.pk(), WS, 1, &head), Err(Error::Decrypt));
    let mut sender = w.clone();
    sender[7] = b'x';
    assert_eq!(open_wrap(&bob.keys, "bob-key", &owner.pk(), WS, 1, &sender), Err(Error::Decrypt));
    // A legacy (base mode) opener can't read a format 2 wrap, nor the other way round.
    assert!(unwrap_key(&bob.keys, WS, 1, &w).is_err());
    let legacy = wrap_key(&bob.keys.public_key(), WS, 1, &key).unwrap();
    assert_eq!(wrap_header(&legacy).unwrap(), WrapHeader::Legacy);
    assert!(open_wrap(&bob.keys, "bob-key", &owner.pk(), WS, 1, &legacy).is_err());
    // Malformed shapes.
    assert!(wrap_header(&w[..w.len() - 1]).is_err());
    assert!(wrap_header(&[2u8; 7]).is_err());
    let pk = bob.keys.public_key();
    let r = Recipient { key_id: "bob-key", public_key: &pk };
    assert!(seal_wrap(&owner.sender(), &r, WS, 2, 1, &key).is_err(), "head older than the epoch");
    assert!(seal_wrap(&Device { key_id: "", keys: &owner.keys }, &r, WS, 1, 1, &key).is_err());
}

#[test]
fn a_wrap_from_a_pinned_sender_is_accepted_and_remembered() {
    let (owner, bob) = (Dev::new("o"), Dev::new("b"));
    let (k1, k2) = (WorkspaceKey::generate(), WorkspaceKey::generate());
    let mut mem = WorkspaceMemory::default();
    let wraps = vec![(1, owner.wrap(&bob, 1, 2, &k1)), (2, owner.wrap(&bob, 2, 2, &k2))];
    let ring = accept(&bob, 2, &wraps, &pins(&[&owner]), &BTreeMap::new(), &mut mem).unwrap();
    assert_eq!(ring.keys.len(), 2);
    assert_eq!(*ring.current_key(), k2);
    assert!(ring.notices.is_empty() && !ring.uses_legacy());
    assert!(mem.strict);
    assert_eq!(mem.max_epoch, 2);
    assert_eq!(mem.commitments[&1], key_commitment(WS, 1, &k1));
    // Same answer again: fine, nothing changes.
    let before = mem.clone();
    accept(&bob, 2, &wraps, &pins(&[&owner]), &BTreeMap::new(), &mut mem).unwrap();
    assert_eq!(mem, before);
}

#[test]
fn a_forged_wrap_naming_a_pinned_sender_is_refused() {
    // The server makes its own key pair and a wrap that claims to come from the owner's device.
    let (owner, bob, server) = (Dev::new("o"), Dev::new("b"), Dev::new("o"));
    let theirs = WorkspaceKey::generate();
    let forged = server.wrap(&bob, 1, 1, &theirs);
    let mut mem = WorkspaceMemory::default();
    let e = accept(&bob, 1, &[(1, forged)], &pins(&[&owner]), &BTreeMap::new(), &mut mem).unwrap_err();
    assert_eq!(e, Error::Forged { epoch: 1, sender_key_id: "o".into() });
    assert_eq!(mem, WorkspaceMemory::default(), "nothing remembered from a failed call");
}

#[test]
fn a_wrap_from_a_device_nobody_trusted_is_refused_with_its_fingerprint() {
    let (owner, bob, server) = (Dev::new("o"), Dev::new("b"), Dev::new("s"));
    let theirs = WorkspaceKey::generate();
    let w = server.wrap(&bob, 1, 1, &theirs);
    let listed = pins(&[&owner, &server]);
    let mut mem = WorkspaceMemory::default();
    match accept(&bob, 1, &[(1, w.clone())], &pins(&[&owner]), &listed, &mut mem) {
        Err(Error::UntrustedSender { epoch: 1, sender_key_id, fingerprint: Some(fp) }) => {
            assert_eq!(sender_key_id, "s");
            assert_eq!(fp, fingerprint(&server.pk()));
        }
        other => panic!("{other:?}"),
    }
    // Not listed by the server either: refused, no fingerprint.
    assert!(matches!(accept(&bob, 1, &[(1, w)], &pins(&[&owner]), &BTreeMap::new(), &mut mem), Err(Error::UntrustedSender { fingerprint: None, .. })));
}

#[test]
fn an_invite_fingerprint_pins_the_inviter_and_nobody_else() {
    let (owner, bob, server) = (Dev::new("o"), Dev::new("b"), Dev::new("s"));
    let k = WorkspaceKey::generate();
    let listed = pins(&[&owner, &server]);
    let fp = fingerprint(&owner.pk());
    let mut mem = WorkspaceMemory { strict: true, expected_senders: vec![fp.to_uppercase()], ..Default::default() };
    let ring = accept(&bob, 1, &[(1, owner.wrap(&bob, 1, 1, &k))], &BTreeMap::new(), &listed, &mut mem).unwrap();
    assert_eq!(ring.new_pins, vec![("o".to_string(), owner.pk())]);
    assert!(ring.notices.contains(&Notice::SenderPinned { sender_key_id: "o".into(), fingerprint: fp }));
    // The server swapping in its own key under the owner's key id doesn't match the fingerprint.
    let swapped = BTreeMap::from([("o".to_string(), server.pk())]);
    let mut fresh = WorkspaceMemory { strict: true, expected_senders: vec![fingerprint(&owner.pk())], ..Default::default() };
    let fake = Dev { id: "o", keys: server.keys.clone() };
    let w = fake.wrap(&bob, 1, 1, &k);
    assert!(matches!(accept(&bob, 1, &[(1, w)], &BTreeMap::new(), &swapped, &mut fresh), Err(Error::UntrustedSender { .. })));
}

#[test]
fn a_swapped_key_for_a_known_epoch_is_refused() {
    // A member (or the server, with a pinned member's help) hands out a different key for an
    // epoch this device already holds.
    let (owner, editor, bob) = (Dev::new("o"), Dev::new("e"), Dev::new("b"));
    let (k1, other) = (WorkspaceKey::generate(), WorkspaceKey::generate());
    let mut mem = WorkspaceMemory::default();
    accept(&bob, 1, &[(1, owner.wrap(&bob, 1, 1, &k1))], &pins(&[&owner, &editor]), &BTreeMap::new(), &mut mem).unwrap();
    let e = accept(&bob, 1, &[(1, editor.wrap(&bob, 1, 1, &other))], &pins(&[&owner, &editor]), &BTreeMap::new(), &mut mem).unwrap_err();
    assert_eq!(e, Error::KeyChanged { epoch: 1 });
    // A legacy wrap with another key for a known epoch too.
    let mut legacy_mem = WorkspaceMemory::default();
    let l1 = wrap_key(&bob.keys.public_key(), WS, 1, &k1).unwrap();
    accept(&bob, 1, &[(1, l1)], &BTreeMap::new(), &BTreeMap::new(), &mut legacy_mem).unwrap();
    let l2 = wrap_key(&bob.keys.public_key(), WS, 1, &other).unwrap();
    assert_eq!(accept(&bob, 1, &[(1, l2)], &BTreeMap::new(), &BTreeMap::new(), &mut legacy_mem).unwrap_err(), Error::KeyChanged { epoch: 1 });
}

#[test]
fn a_hidden_rotation_or_an_old_wrap_replayed_after_one_is_refused() {
    let (owner, bob) = (Dev::new("o"), Dev::new("b"));
    let (k1, k2) = (WorkspaceKey::generate(), WorkspaceKey::generate());
    let p = pins(&[&owner]);
    let w1 = owner.wrap(&bob, 1, 1, &k1);
    let mut mem = WorkspaceMemory::default();
    accept(&bob, 1, &[(1, w1.clone())], &p, &BTreeMap::new(), &mut mem).unwrap();
    accept(&bob, 2, &[(1, w1.clone()), (2, owner.wrap(&bob, 2, 2, &k2))], &p, &BTreeMap::new(), &mut mem).unwrap();
    // Later the server says the workspace is back at epoch 1 (to make Bob write under a key a
    // removed member knows).
    assert_eq!(accept(&bob, 1, &[(1, w1.clone())], &p, &BTreeMap::new(), &mut mem).unwrap_err(), Error::Rollback { current: 1, known: 2 });
    // The old epoch's wrap presented as the new epoch doesn't open (the epoch is in `info`).
    assert_eq!(accept(&bob, 2, &[(2, w1.clone())], &p, &BTreeMap::new(), &mut mem).unwrap_err(), Error::Forged { epoch: 2, sender_key_id: "o".into() });
    // A device with no memory yet (a new member) learns the head from the wraps it gets: the
    // owner wrapped epoch 1 for it while the workspace was at epoch 2.
    let carol = Dev::new("c");
    let mut fresh = WorkspaceMemory::default();
    let e = accept(&carol, 1, &[(1, owner.wrap(&carol, 1, 2, &k1))], &p, &BTreeMap::new(), &mut fresh).unwrap_err();
    assert_eq!(e, Error::Rollback { current: 1, known: 2 });
    // And a current epoch nobody wrapped for this device: no key.
    let mut m = WorkspaceMemory::default();
    assert_eq!(accept(&carol, 3, &[(1, owner.wrap(&carol, 1, 1, &k1))], &p, &BTreeMap::new(), &mut m).unwrap_err(), Error::MissingKey { epoch: 3 });
}

#[test]
fn legacy_wraps_only_with_a_notice_and_never_once_strict() {
    let (owner, bob) = (Dev::new("o"), Dev::new("b"));
    let k1 = WorkspaceKey::generate();
    let legacy = wrap_key(&bob.keys.public_key(), WS, 1, &k1).unwrap();
    // A workspace this device knew before 1.2: used, with a warning.
    let mut mem = WorkspaceMemory::default();
    let ring = accept(&bob, 1, &[(1, legacy.clone())], &BTreeMap::new(), &BTreeMap::new(), &mut mem).unwrap();
    assert!(ring.uses_legacy());
    assert_eq!(ring.notices, vec![Notice::LegacyAccepted { epoch: 1 }]);
    assert!(!mem.strict);
    assert!(ring.notices[0].to_string().contains("without authentication"));
    // A device that joined with 1.2 (strict) refuses it: the server could have made it.
    let mut strict = WorkspaceMemory { strict: true, ..Default::default() };
    assert_eq!(accept(&bob, 1, &[(1, legacy.clone())], &BTreeMap::new(), &BTreeMap::new(), &mut strict).unwrap_err(), Error::LegacyRefused { epoch: 1 });
    // Once an authenticated wrap was accepted, legacy ones are no longer used, even next to it.
    let k2 = WorkspaceKey::generate();
    let ring = accept(&bob, 2, &[(1, legacy.clone()), (2, owner.wrap(&bob, 2, 2, &k2))], &pins(&[&owner]), &BTreeMap::new(), &mut mem).unwrap();
    assert!(mem.strict);
    assert_eq!(ring.notices, vec![Notice::LegacyIgnored { epoch: 1 }]);
    assert!(!ring.keys.contains_key(&1));
    // The owner re-wraps epoch 1 (migration): the same key, now authenticated.
    let ring = accept(&bob, 2, &[(1, owner.wrap(&bob, 1, 2, &k1)), (2, owner.wrap(&bob, 2, 2, &k2))], &pins(&[&owner]), &BTreeMap::new(), &mut mem).unwrap();
    assert_eq!(ring.keys[&1], k1);
    assert!(ring.notices.is_empty());
}

#[test]
fn a_re_wrap_from_an_untrusted_device_is_used_only_for_a_key_already_held() {
    // Bob accepted epoch 1 from a legacy wrap; the owner (whom Bob hasn't pinned) replaces it with
    // an authenticated wrap of the same key: usable, with a notice, but a new key from that device
    // isn't.
    let (owner, bob) = (Dev::new("o"), Dev::new("b"));
    let (k1, k2) = (WorkspaceKey::generate(), WorkspaceKey::generate());
    let mut mem = WorkspaceMemory::default();
    accept(&bob, 1, &[(1, wrap_key(&bob.keys.public_key(), WS, 1, &k1).unwrap())], &BTreeMap::new(), &BTreeMap::new(), &mut mem).unwrap();
    let listed = pins(&[&owner]);
    let ring = accept(&bob, 1, &[(1, owner.wrap(&bob, 1, 1, &k1))], &BTreeMap::new(), &listed, &mut mem).unwrap();
    assert!(matches!(&ring.notices[..], [Notice::KnownKeyFromUntrusted { epoch: 1, .. }]));
    assert!(mem.strict);
    let e = accept(&bob, 2, &[(1, owner.wrap(&bob, 1, 2, &k1)), (2, owner.wrap(&bob, 2, 2, &k2))], &BTreeMap::new(), &listed, &mut mem).unwrap_err();
    assert!(matches!(e, Error::UntrustedSender { epoch: 2, .. }), "{e:?}");
    // An untrusted device's wrap of a *different* key for epoch 1 is refused even then.
    let mut m2 = WorkspaceMemory::default();
    accept(&bob, 1, &[(1, wrap_key(&bob.keys.public_key(), WS, 1, &k1).unwrap())], &BTreeMap::new(), &BTreeMap::new(), &mut m2).unwrap();
    assert!(matches!(accept(&bob, 1, &[(1, owner.wrap(&bob, 1, 1, &k2))], &BTreeMap::new(), &listed, &mut m2), Err(Error::UntrustedSender { .. })));
}

#[test]
fn a_changed_key_for_a_known_device_is_announced() {
    let (owner, bob, server) = (Dev::new("o"), Dev::new("b"), Dev::new("x"));
    let k = WorkspaceKey::generate();
    // The server lists its own key under the owner's key id; the pinned key still verifies.
    let listed = BTreeMap::from([("o".to_string(), server.pk())]);
    let mut mem = WorkspaceMemory::default();
    let ring = accept(&bob, 1, &[(1, owner.wrap(&bob, 1, 1, &k))], &pins(&[&owner]), &listed, &mut mem).unwrap();
    assert_eq!(ring.notices, vec![Notice::SenderKeyChanged { sender_key_id: "o".into(), fingerprint: fingerprint(&server.pk()) }]);
    assert!(ring.notices[0].to_string().starts_with("WARNING"));
}

#[test]
fn old_epochs_from_untrusted_devices_are_skipped_not_fatal() {
    let (owner, stranger, bob) = (Dev::new("o"), Dev::new("s"), Dev::new("b"));
    let (k1, k2) = (WorkspaceKey::generate(), WorkspaceKey::generate());
    let mut mem = WorkspaceMemory::default();
    let ring = accept(&bob, 2, &[(1, stranger.wrap(&bob, 1, 2, &k1)), (2, owner.wrap(&bob, 2, 2, &k2))], &pins(&[&owner]), &BTreeMap::new(), &mut mem).unwrap();
    assert_eq!(ring.notices, vec![Notice::UntrustedIgnored { epoch: 1, sender_key_id: "s".into(), fingerprint: None }]);
    assert_eq!(ring.keys.keys().copied().collect::<Vec<_>>(), [2]);
}

#[test]
#[cfg(feature = "serde")]
fn normalized_fingerprints_and_memory_json() {
    assert_eq!(normalize_fingerprint("62BD a406-8c54"), "62bda4068c54");
    let mem = WorkspaceMemory { strict: true, max_epoch: 3, commitments: BTreeMap::from([(1, "ab".to_string())]), expected_senders: vec!["00".into()] };
    let json = serde_json::to_string(&mem).unwrap();
    assert_eq!(json, r#"{"strict":true,"maxEpoch":3,"commitments":{"1":"ab"},"expectedSenders":["00"]}"#);
    assert_eq!(serde_json::from_str::<WorkspaceMemory>(&json).unwrap(), mem);
    assert_eq!(serde_json::from_str::<WorkspaceMemory>("{}").unwrap(), WorkspaceMemory::default());
    let n = serde_json::to_value(Notice::LegacyAccepted { epoch: 2 }).unwrap();
    assert_eq!(n, serde_json::json!({"kind": "legacyAccepted", "epoch": 2}));
}

// ---- published vectors (vectors/v2.json) ----------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorsV2 {
    description: Vec<String>,
    wraps: Vec<WrapV2>,
    wrap_failures: Vec<WrapV2Failure>,
    commitments: Vec<Commitment>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapV2 {
    name: String,
    sender_ikm: String,
    sender_public: String,
    sender_key_id: String,
    recipient_ikm: String,
    recipient_secret: String,
    recipient_public: String,
    recipient_key_id: String,
    workspace_id: String,
    epoch: u32,
    head: u32,
    info: String,
    wrapped: String,
    workspace_key: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapV2Failure {
    name: String,
    sender_public: String,
    recipient_secret: String,
    recipient_key_id: String,
    workspace_id: String,
    epoch: u32,
    wrapped: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commitment {
    workspace_id: String,
    epoch: u32,
    workspace_key: String,
    commitment: String,
}

#[test]
fn published_v2_test_vectors() {
    let v: VectorsV2 = serde_json::from_str(VECTORS_V2).expect("vectors/v2.json");
    assert!(v.wraps.len() >= 2 && v.wrap_failures.len() >= 5 && !v.commitments.is_empty());
    for w in &v.wraps {
        let sender = DeviceKeyPair::derive(&unhex(&w.sender_ikm));
        assert_eq!(hex(&sender.public_key()), w.sender_public, "{}: sender DeriveKeyPair", w.name);
        let device = DeviceKeyPair::derive(&unhex(&w.recipient_ikm));
        assert_eq!(hex(&device.secret_bytes()), w.recipient_secret, "{}: DeriveKeyPair secret", w.name);
        assert_eq!(hex(&device.public_key()), w.recipient_public, "{}: DeriveKeyPair public", w.name);
        assert_eq!(hex(&wrap_info_v2(&w.workspace_id, w.epoch, w.head, &w.sender_key_id, &w.recipient_key_id).unwrap()), w.info, "{}: info", w.name);
        let wrapped = unhex(&w.wrapped);
        assert_eq!(wrap_header(&wrapped).unwrap(), WrapHeader::Authenticated { sender_key_id: w.sender_key_id.clone(), head: w.head });
        let o = open_wrap(&device, &w.recipient_key_id, &unhex(&w.sender_public), &w.workspace_id, w.epoch, &wrapped).unwrap();
        assert_eq!(hex(o.key.as_bytes()), w.workspace_key, "{}: workspace key", w.name);
    }
    for f in &v.wrap_failures {
        let device = DeviceKeyPair::from_secret_bytes(&unhex(&f.recipient_secret)).unwrap();
        assert!(open_wrap(&device, &f.recipient_key_id, &unhex(&f.sender_public), &f.workspace_id, f.epoch, &unhex(&f.wrapped)).is_err(), "{} must fail", f.name);
    }
    for c in &v.commitments {
        let key = WorkspaceKey::from_bytes(unhex(&c.workspace_key).try_into().unwrap());
        assert_eq!(key_commitment(&c.workspace_id, c.epoch, &key), c.commitment);
    }
}

/// Writes vectors/v2.json. Run once with `cargo test -p hitop-sync-crypto -- --ignored
/// generate_v2_vectors`; the file is then frozen, and `published_v2_test_vectors` checks it.
#[test]
#[ignore]
fn generate_v2_vectors() {
    let key1: [u8; 32] = std::array::from_fn(|i| i as u8);
    let key2: [u8; 32] = std::array::from_fn(|i| 0xa0 ^ (i as u8));
    let owner_ikm: [u8; 32] = std::array::from_fn(|i| 0x30 + i as u8);
    let ikm_a: [u8; 32] = std::array::from_fn(|i| 0x10 + i as u8);
    let ikm_b: [u8; 32] = std::array::from_fn(|i| 0xf0 - i as u8);
    let owner = DeviceKeyPair::derive(&owner_ikm);
    let owner_id = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";
    let wrap = |name: &str, ikm: &[u8], recipient_id: &str, epoch: u32, head: u32, key: [u8; 32]| {
        let device = DeviceKeyPair::derive(ikm);
        let pk = device.public_key();
        let wrapped = seal_wrap(&Device { key_id: owner_id, keys: &owner }, &Recipient { key_id: recipient_id, public_key: &pk }, WS, epoch, head, &WorkspaceKey::from_bytes(key)).unwrap();
        WrapV2 {
            name: name.into(),
            sender_ikm: hex(&owner_ikm),
            sender_public: hex(&owner.public_key()),
            sender_key_id: owner_id.into(),
            recipient_ikm: hex(ikm),
            recipient_secret: hex(&device.secret_bytes()),
            recipient_public: hex(&pk),
            recipient_key_id: recipient_id.into(),
            workspace_id: WS.into(),
            epoch,
            head,
            info: hex(&wrap_info_v2(WS, epoch, head, owner_id, recipient_id).unwrap()),
            wrapped: hex(&wrapped),
            workspace_key: hex(&key),
        }
    };
    let wraps = vec![
        wrap("member A, epoch 1, workspace at epoch 1", &ikm_a, "a1b2c3d4e5f60718293a4b5c6d7e8f90", 1, 1, key1),
        wrap("member B, epoch 1 handed out while the workspace is at epoch 2", &ikm_b, "b1b2c3d4e5f60718293a4b5c6d7e8f90", 1, 2, key1),
        wrap("member B, epoch 2", &ikm_b, "b1b2c3d4e5f60718293a4b5c6d7e8f90", 2, 2, key2),
    ];
    let a = &wraps[0];
    let other = DeviceKeyPair::derive(&[0x55; 32]);
    let fail = |name: &str, sender_public: &str, recipient_secret: &str, recipient_key_id: &str, workspace_id: &str, epoch: u32, wrapped: &[u8]| WrapV2Failure {
        name: name.into(),
        sender_public: sender_public.into(),
        recipient_secret: recipient_secret.into(),
        recipient_key_id: recipient_key_id.into(),
        workspace_id: workspace_id.into(),
        epoch,
        wrapped: hex(wrapped),
    };
    let wa = unhex(&a.wrapped);
    let mut head = wa.clone();
    head[4] = 2;
    let mut sender = wa.clone();
    sender[7] ^= 1;
    let mut bit = wa.clone();
    *bit.last_mut().unwrap() ^= 1;
    let forged = {
        let pk = unhex(&a.recipient_public);
        seal_wrap(&Device { key_id: owner_id, keys: &other }, &Recipient { key_id: &a.recipient_key_id, public_key: &pk }, WS, 1, 1, &WorkspaceKey::from_bytes(key2)).unwrap()
    };
    let wrap_failures = vec![
        fail("made by another sender key (forged)", &a.sender_public, &a.recipient_secret, &a.recipient_key_id, WS, 1, &forged),
        fail("verified against another sender key", &hex(&other.public_key()), &a.recipient_secret, &a.recipient_key_id, WS, 1, &wa),
        fail("head rewritten", &a.sender_public, &a.recipient_secret, &a.recipient_key_id, WS, 1, &head),
        fail("sender key id rewritten", &a.sender_public, &a.recipient_secret, &a.recipient_key_id, WS, 1, &sender),
        fail("flipped tag bit", &a.sender_public, &a.recipient_secret, &a.recipient_key_id, WS, 1, &bit),
        fail("wrong epoch", &a.sender_public, &a.recipient_secret, &a.recipient_key_id, WS, 2, &wa),
        fail("wrong recipient key id", &a.sender_public, &a.recipient_secret, "b1b2c3d4e5f60718293a4b5c6d7e8f90", WS, 1, &wa),
        fail("wrong recipient", &a.sender_public, &wraps[1].recipient_secret, &a.recipient_key_id, WS, 1, &wa),
        fail("wrong workspace", &a.sender_public, &a.recipient_secret, &a.recipient_key_id, "00000000000000000000000000000000", 1, &wa),
        fail("format 1 bytes", &a.sender_public, &a.recipient_secret, &a.recipient_key_id, WS, 1, &wa[wa.len() - WRAPPED_KEY_LEN..]),
    ];
    let commitments = [(1, key1), (2, key2)]
        .iter()
        .map(|(epoch, k)| Commitment { workspace_id: WS.into(), epoch: *epoch, workspace_key: hex(k), commitment: key_commitment(WS, *epoch, &WorkspaceKey::from_bytes(*k)) })
        .collect();
    let description = [
        "hitop sync encryption test vectors, format 2 (authenticated key wraps, hitop-sync-crypto 0.2). All byte strings are lower-case hex. Integers are big endian.",
        "wrap: HPKE RFC 9180 Auth mode (0x02), KEM 0x0020 DHKEM(X25519, HKDF-SHA256), KDF 0x0001 HKDF-SHA256, AEAD 0x0003 ChaCha20-Poly1305; skS = the sending device's X25519 key",
        "info = \"hitop-sync/v2/wrap\" || u16 len(workspaceId) || workspaceId || u32 epoch || u32 head || u16 len(senderKeyId) || senderKeyId || u16 len(recipientKeyId) || recipientKeyId (UTF-8); aad empty; plaintext = the 32-byte workspace key",
        "wrapped = 0x02 || u32 head || u16 len(senderKeyId) || senderKeyId || enc (32) || sealed key (32) || tag (16); head = the newest epoch the sender knew, >= epoch",
        "sender and recipient key pairs = DeriveKeyPair(ikm); wraps are checked in the open direction only (encapsulation is randomized)",
        "every entry in wrapFailures must fail to open (with senderPublic as the pinned sender key)",
        "commitment = SHA-256(\"hitop-sync/v2/key-commitment\" || u16 len(workspaceId) || workspaceId || u32 epoch || workspace key)",
    ];
    let v = VectorsV2 { description: description.map(String::from).to_vec(), wraps, wrap_failures, commitments };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/v2.json");
    std::fs::write(path, serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
}
