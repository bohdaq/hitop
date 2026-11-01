use super::*;
use serde::{Deserialize, Serialize};

const VECTORS: &str = include_str!("../vectors/v1.json");

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2), "odd hex length");
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn arr<const N: usize>(s: &str) -> [u8; N] {
    unhex(s).try_into().expect("length")
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vectors {
    description: Vec<String>,
    blobs: Vec<BlobVector>,
    blob_failures: Vec<BlobFailure>,
    wraps: Vec<WrapVector>,
    wrap_failures: Vec<WrapFailure>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobVector {
    name: String,
    key: String,
    workspace_id: String,
    blob_id: String,
    version: u64,
    epoch: u32,
    nonce: String,
    plaintext: String,
    aad: String,
    envelope: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlobFailure {
    name: String,
    key: String,
    workspace_id: String,
    blob_id: String,
    version: u64,
    epoch: u32,
    envelope: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapVector {
    name: String,
    recipient_ikm: String,
    recipient_secret: String,
    recipient_public: String,
    workspace_id: String,
    epoch: u32,
    info: String,
    wrapped: String,
    workspace_key: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapFailure {
    name: String,
    recipient_secret: String,
    workspace_id: String,
    epoch: u32,
    wrapped: String,
}

fn ctx<'a>(workspace_id: &'a str, blob_id: &'a str, version: u64, epoch: u32) -> BlobContext<'a> {
    BlobContext { workspace_id, blob_id, version, epoch }
}

#[test]
fn blob_round_trip_and_binding() {
    let key = WorkspaceKey::generate();
    let c = ctx("0123456789abcdef0123456789abcdef", "req-1", 3, 2);
    let env = encrypt_blob(&key, &c, b"name: Get user\n").unwrap();
    assert_eq!(env.len(), BLOB_HEADER_LEN + 15 + TAG_LEN);
    assert_eq!(blob_epoch(&env).unwrap(), 2);
    assert_eq!(decrypt_blob(&key, &c, &env).unwrap(), b"name: Get user\n");
    // Every field of the context is bound.
    for other in [
        ctx("0123456789abcdef0123456789abcdee", "req-1", 3, 2),
        ctx("0123456789abcdef0123456789abcdef", "req-2", 3, 2),
        ctx("0123456789abcdef0123456789abcdef", "req-1", 4, 2),
    ] {
        assert_eq!(decrypt_blob(&key, &other, &env), Err(Error::Decrypt));
    }
    assert_eq!(decrypt_blob(&key, &ctx("0123456789abcdef0123456789abcdef", "req-1", 3, 1), &env), Err(Error::EpochMismatch { expected: 1, found: 2 }));
    assert_eq!(decrypt_blob(&WorkspaceKey::generate(), &c, &env), Err(Error::Decrypt));
    // Rewriting the epoch byte in the header doesn't help: the epoch is in the AAD too.
    let mut forged = env.clone();
    forged[4] = 1;
    assert_eq!(decrypt_blob(&key, &ctx("0123456789abcdef0123456789abcdef", "req-1", 3, 1), &forged), Err(Error::Decrypt));
    // Any flipped ciphertext bit fails.
    let mut flipped = env.clone();
    *flipped.last_mut().unwrap() ^= 1;
    assert_eq!(decrypt_blob(&key, &c, &flipped), Err(Error::Decrypt));
    assert!(matches!(decrypt_blob(&key, &c, &env[..20]), Err(Error::Malformed(_))));
    // Two encryptions of the same plaintext differ (random nonce).
    assert_ne!(encrypt_blob(&key, &c, b"x").unwrap(), encrypt_blob(&key, &c, b"x").unwrap());
}

#[test]
fn length_prefixes_keep_fields_apart() {
    // ("ab", "c") and ("a", "bc") must not give the same associated data.
    let a = ctx("ab", "c", 1, 1).associated_data().unwrap();
    let b = ctx("a", "bc", 1, 1).associated_data().unwrap();
    assert_ne!(a, b);
    let long = "x".repeat(70_000);
    assert_eq!(ctx(&long, "b", 1, 1).associated_data(), Err(Error::TooLong));
}

#[test]
fn wrap_round_trip_and_binding() {
    let device = DeviceKeyPair::generate();
    let key = WorkspaceKey::generate();
    let wrapped = wrap_key(&device.public_key(), "ws", 1, &key).unwrap();
    assert_eq!(wrapped.len(), WRAPPED_KEY_LEN);
    assert_eq!(unwrap_key(&device, "ws", 1, &wrapped).unwrap(), key);
    assert_eq!(unwrap_key(&device, "ws", 2, &wrapped), Err(Error::Decrypt));
    assert_eq!(unwrap_key(&device, "other", 1, &wrapped), Err(Error::Decrypt));
    assert_eq!(unwrap_key(&DeviceKeyPair::generate(), "ws", 1, &wrapped), Err(Error::Decrypt));
    let restored = DeviceKeyPair::from_secret_bytes(&device.secret_bytes()).unwrap();
    assert_eq!(restored.public_key(), device.public_key());
    assert_eq!(unwrap_key(&restored, "ws", 1, &wrapped).unwrap(), key);
    assert!(wrap_key(&[0u8; 5], "ws", 1, &key).is_err());
}

#[test]
fn rotation_gives_a_new_key_only_to_the_remaining_devices() {
    let (alice, bob, carol) = (DeviceKeyPair::generate(), DeviceKeyPair::generate(), DeviceKeyPair::generate());
    let (pa, pb, pc) = (alice.public_key(), bob.public_key(), carol.public_key());
    let all = [Recipient { key_id: "a", public_key: &pa }, Recipient { key_id: "b", public_key: &pb }, Recipient { key_id: "c", public_key: &pc }];
    let (k1, wraps1) = rotate("ws", 1, &all).unwrap();
    assert_eq!(wraps1.len(), 3);
    assert_eq!(unwrap_key(&carol, "ws", 1, &wraps1[2].1).unwrap(), k1);
    // Carol is removed: epoch 2 goes to Alice and Bob only.
    let (k2, wraps2) = rotate("ws", 2, &all[..2]).unwrap();
    assert_ne!(k1, k2);
    assert_eq!(wraps2.iter().map(|w| w.0.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(unwrap_key(&bob, "ws", 2, &wraps2[1].1).unwrap(), k2);
    // Carol's epoch 1 key doesn't open epoch 2 blobs, nor does her key open the new wraps.
    let c2 = ctx("ws", "req", 5, 2);
    let blob = encrypt_blob(&k2, &c2, b"after removal").unwrap();
    assert_eq!(decrypt_blob(&k1, &c2, &blob), Err(Error::Decrypt));
    for (_, w) in &wraps2 {
        assert_eq!(unwrap_key(&carol, "ws", 2, w), Err(Error::Decrypt));
    }
}

#[test]
fn debug_output_hides_keys() {
    let key = WorkspaceKey::from_bytes([7; 32]);
    assert_eq!(format!("{key:?}"), "WorkspaceKey(..)");
    let device = DeviceKeyPair::derive(&[1; 32]);
    let shown = format!("{device:?}");
    assert!(!shown.contains(&format!("{:?}", device.secret_bytes())), "{shown}");
}

/// Checks every published vector: blobs both ways (deterministic with the given nonce), wrapped
/// keys in the open direction (HPKE encapsulation is randomized), and the cases that must fail.
#[test]
fn published_test_vectors() {
    let v: Vectors = serde_json::from_str(VECTORS).expect("vectors/v1.json");
    assert!(v.blobs.len() >= 3 && v.wraps.len() >= 2 && !v.blob_failures.is_empty() && !v.wrap_failures.is_empty());
    for b in &v.blobs {
        let key = WorkspaceKey::from_bytes(arr(&b.key));
        let c = ctx(&b.workspace_id, &b.blob_id, b.version, b.epoch);
        assert_eq!(hex(&c.associated_data().unwrap()), b.aad, "{}: aad", b.name);
        let env = encrypt_blob_with_nonce(&key, &c, &unhex(&b.plaintext), &arr(&b.nonce)).unwrap();
        assert_eq!(hex(&env), b.envelope, "{}: envelope", b.name);
        assert_eq!(hex(&decrypt_blob(&key, &c, &unhex(&b.envelope)).unwrap()), b.plaintext, "{}: plaintext", b.name);
    }
    for f in &v.blob_failures {
        let key = WorkspaceKey::from_bytes(arr(&f.key));
        let c = ctx(&f.workspace_id, &f.blob_id, f.version, f.epoch);
        assert!(decrypt_blob(&key, &c, &unhex(&f.envelope)).is_err(), "{} must fail", f.name);
    }
    for w in &v.wraps {
        let device = DeviceKeyPair::derive(&unhex(&w.recipient_ikm));
        assert_eq!(hex(&device.secret_bytes()), w.recipient_secret, "{}: DeriveKeyPair secret", w.name);
        assert_eq!(hex(&device.public_key()), w.recipient_public, "{}: DeriveKeyPair public", w.name);
        assert_eq!(hex(&wrap_info(&w.workspace_id, w.epoch).unwrap()), w.info, "{}: info", w.name);
        let key = unwrap_key(&device, &w.workspace_id, w.epoch, &unhex(&w.wrapped)).unwrap();
        assert_eq!(hex(key.as_bytes()), w.workspace_key, "{}: workspace key", w.name);
    }
    for f in &v.wrap_failures {
        let device = DeviceKeyPair::from_secret_bytes(&unhex(&f.recipient_secret)).unwrap();
        assert!(unwrap_key(&device, &f.workspace_id, f.epoch, &unhex(&f.wrapped)).is_err(), "{} must fail", f.name);
    }
}

/// Writes vectors/v1.json. Run once with `cargo test -p hitop-sync-crypto -- --ignored
/// generate_vectors`; the file is then frozen, and `published_test_vectors` checks it.
#[test]
#[ignore]
fn generate_vectors() {
    let ws = "3f6c1a9e0b7d4c2a8e5f1b3d7a9c0e24";
    let key1: [u8; 32] = std::array::from_fn(|i| i as u8);
    let key2: [u8; 32] = std::array::from_fn(|i| 0xa0 ^ (i as u8));
    let nonce: [u8; 24] = std::array::from_fn(|i| 0x40 + i as u8);
    let blob = |name: &str, key: [u8; 32], blob_id: &str, version: u64, epoch: u32, plaintext: &[u8]| {
        let c = ctx(ws, blob_id, version, epoch);
        let env = encrypt_blob_with_nonce(&WorkspaceKey::from_bytes(key), &c, plaintext, &nonce).unwrap();
        BlobVector {
            name: name.into(),
            key: hex(&key),
            workspace_id: ws.into(),
            blob_id: blob_id.into(),
            version,
            epoch,
            nonce: hex(&nonce),
            plaintext: hex(plaintext),
            aad: hex(&c.associated_data().unwrap()),
            envelope: hex(&env),
        }
    };
    let request = b"name: Get user\nmethod: GET\nurl: https://api.example.com/users/1\n";
    let blobs = vec![
        blob("request file, epoch 1", key1, "b1f0c3e2a4d6", 1, 1, request),
        blob("empty plaintext", key1, "b1f0c3e2a4d6", 2, 1, b""),
        blob("after rotation, epoch 2", key2, "9d8c7b6a5f4e", 7, 2, "unicode: \u{1F512} ключ".as_bytes()),
    ];
    let first = &blobs[0];
    let mut tampered = unhex(&first.envelope);
    tampered[BLOB_HEADER_LEN] ^= 0x01;
    let fail = |name: &str, key: &str, blob_id: &str, version: u64, epoch: u32, envelope: &[u8]| BlobFailure {
        name: name.into(),
        key: key.into(),
        workspace_id: ws.into(),
        blob_id: blob_id.into(),
        version,
        epoch,
        envelope: hex(envelope),
    };
    let env0 = unhex(&first.envelope);
    let blob_failures = vec![
        fail("flipped ciphertext bit", &first.key, "b1f0c3e2a4d6", 1, 1, &tampered),
        fail("replayed as version 2", &first.key, "b1f0c3e2a4d6", 2, 1, &env0),
        fail("moved to another blob id", &first.key, "another-blob", 1, 1, &env0),
        fail("wrong key", &hex(&key2), "b1f0c3e2a4d6", 1, 1, &env0),
    ];
    let wrap = |name: &str, ikm: &[u8], epoch: u32, key: [u8; 32]| {
        let device = DeviceKeyPair::derive(ikm);
        let wrapped = wrap_key(&device.public_key(), ws, epoch, &WorkspaceKey::from_bytes(key)).unwrap();
        WrapVector {
            name: name.into(),
            recipient_ikm: hex(ikm),
            recipient_secret: hex(&device.secret_bytes()),
            recipient_public: hex(&device.public_key()),
            workspace_id: ws.into(),
            epoch,
            info: hex(&wrap_info(ws, epoch).unwrap()),
            wrapped: hex(&wrapped),
            workspace_key: hex(&key),
        }
    };
    let ikm_a: [u8; 32] = std::array::from_fn(|i| 0x10 + i as u8);
    let ikm_b: [u8; 32] = std::array::from_fn(|i| 0xf0 - i as u8);
    let wraps = vec![wrap("member A, epoch 1", &ikm_a, 1, key1), wrap("member B, epoch 2", &ikm_b, 2, key2)];
    let wrap_failures = vec![
        WrapFailure { name: "wrong epoch".into(), recipient_secret: wraps[0].recipient_secret.clone(), workspace_id: ws.into(), epoch: 2, wrapped: wraps[0].wrapped.clone() },
        WrapFailure { name: "wrong recipient".into(), recipient_secret: wraps[1].recipient_secret.clone(), workspace_id: ws.into(), epoch: 1, wrapped: wraps[0].wrapped.clone() },
        WrapFailure { name: "wrong workspace".into(), recipient_secret: wraps[0].recipient_secret.clone(), workspace_id: "00000000000000000000000000000000".into(), epoch: 1, wrapped: wraps[0].wrapped.clone() },
    ];
    let description = [
        "hitop sync encryption test vectors, format 1. All byte strings are lower-case hex. Integers are big endian.",
        "blob aad = \"hitop-sync/v1/blob\" || u16 len(workspaceId) || workspaceId (UTF-8) || u16 len(blobId) || blobId (UTF-8) || u64 version || u32 epoch",
        "blob envelope = 0x01 || u32 epoch || nonce (24) || XChaCha20-Poly1305(key, nonce, aad, plaintext) (ciphertext || 16-byte tag)",
        "wrap: HPKE RFC 9180 base mode, KEM 0x0020 DHKEM(X25519, HKDF-SHA256), KDF 0x0001 HKDF-SHA256, AEAD 0x0003 ChaCha20-Poly1305",
        "wrap info = \"hitop-sync/v1/wrap\" || u16 len(workspaceId) || workspaceId || u32 epoch; aad empty; plaintext = the 32-byte workspace key",
        "wrapped = enc (32) || sealed key (32) || tag (16); recipient key pair = DeriveKeyPair(recipientIkm)",
        "wraps are checked in the open direction only (encapsulation is randomized); blobs both ways",
        "every entry in blobFailures and wrapFailures must fail to decrypt",
    ];
    let v = Vectors { description: description.map(String::from).to_vec(), blobs, blob_failures, wraps, wrap_failures };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/v1.json");
    std::fs::write(path, serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
}
