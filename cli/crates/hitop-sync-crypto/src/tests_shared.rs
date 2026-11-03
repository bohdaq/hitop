//! Run reports and shares: round trips, what a server can try with what it stores, and the
//! published vectors (vectors/shared.json).

use crate::*;
use serde::{Deserialize, Serialize};

const WS: &str = "3f6c1a9e0b7d4c2a8e5f1b3d7a9c0e24";
const VECTORS: &str = include_str!("../vectors/shared.json");

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn arr<const N: usize>(s: &str) -> [u8; N] {
    unhex(s).try_into().expect("length")
}

#[test]
fn a_run_report_opens_only_for_its_workspace_and_run() {
    let key = WorkspaceKey::generate();
    let sealed = seal_run(&key, WS, "r1", 3, b"{\"reportVersion\":1}").unwrap();
    assert_eq!(run_epoch(&sealed).unwrap(), 3);
    assert_eq!(blob_epoch(&sealed).unwrap(), 3, "the blob layout: clients pick the key the same way");
    assert_eq!(open_run(&key, WS, "r1", &sealed).unwrap(), b"{\"reportVersion\":1}");
    assert_eq!(open_run(&key, WS, "r2", &sealed), Err(Error::Decrypt), "moved to another run");
    assert_eq!(open_run(&key, "00000000000000000000000000000000", "r1", &sealed), Err(Error::Decrypt));
    assert_eq!(open_run(&WorkspaceKey::generate(), WS, "r1", &sealed), Err(Error::Decrypt));
    let mut epoch = sealed.clone();
    epoch[4] = 4;
    assert_eq!(open_run(&key, WS, "r1", &epoch), Err(Error::Decrypt), "epoch rewritten");
    // Never a file: the same bytes don't open as any blob of the workspace.
    for id in ["r1", "runs/r1"] {
        assert!(decrypt_blob(&key, &BlobContext { workspace_id: WS, blob_id: id, version: 1, epoch: 3 }, &sealed).is_err());
    }
    let file = encrypt_blob(&key, &BlobContext { workspace_id: WS, blob_id: "r1", version: 1, epoch: 3 }, b"x").unwrap();
    assert_eq!(open_run(&key, WS, "r1", &file), Err(Error::Decrypt), "a file doesn't open as a run");
}

#[test]
fn a_share_opens_at_its_version_only() {
    let key = WorkspaceKey::generate();
    let v2 = seal_share(&key, WS, "s1", 2, b"values").unwrap();
    assert_eq!(open_share(&key, WS, "s1", 2, &v2).unwrap(), b"values");
    assert_eq!(open_share(&key, WS, "s1", 1, &v2), Err(Error::EpochMismatch { expected: 1, found: 2 }));
    let mut relabeled = v2.clone();
    relabeled[4] = 3;
    assert_eq!(open_share(&key, WS, "s1", 3, &relabeled), Err(Error::Decrypt), "a version can't be rewritten");
    assert_eq!(open_share(&key, WS, "s2", 2, &v2), Err(Error::Decrypt));
    assert!(seal_share(&key, WS, "s1", 0, b"x").is_err());
    assert_eq!(open_run(&key, WS, "s1", &v2), Err(Error::Decrypt), "a share isn't a run report");
}

struct Dev {
    id: &'static str,
    keys: DeviceKeyPair,
}

impl Dev {
    fn new(id: &'static str) -> Dev {
        Dev { id, keys: DeviceKeyPair::generate() }
    }
    fn me(&self) -> Device<'_> {
        Device { key_id: self.id, keys: &self.keys }
    }
}

#[test]
fn a_share_wrap_is_authenticated_and_never_a_workspace_key_wrap() {
    let (a, b, c) = (Dev::new("ka"), Dev::new("kb"), Dev::new("kc"));
    let share_key = WorkspaceKey::generate();
    let pk_b = b.keys.public_key();
    let w = seal_share_wrap(&a.me(), &Recipient { key_id: b.id, public_key: &pk_b }, WS, "s1", 4, &share_key).unwrap();
    assert_eq!(share_wrap_sender(&w).unwrap(), "ka");
    let pk_a = a.keys.public_key();
    let pk_c = c.keys.public_key();
    assert_eq!(open_share_wrap(&b.keys, "kb", &pk_a, WS, "s1", 4, &w).unwrap(), share_key);
    assert_eq!(open_share_wrap(&b.keys, "kb", &pk_c, WS, "s1", 4, &w), Err(Error::Decrypt), "verified against another sender");
    assert_eq!(open_share_wrap(&c.keys, "kb", &pk_a, WS, "s1", 4, &w), Err(Error::Decrypt), "another device");
    assert_eq!(open_share_wrap(&b.keys, "kc", &pk_a, WS, "s1", 4, &w), Err(Error::Decrypt), "another recipient key id");
    assert_eq!(open_share_wrap(&b.keys, "kb", &pk_a, WS, "s2", 4, &w), Err(Error::Decrypt), "another share");
    assert!(open_share_wrap(&b.keys, "kb", &pk_a, WS, "s1", 3, &w).is_err(), "another version");
    // The shapes match (the server checks it like a workspace key wrap), the meaning never does.
    assert!(open_wrap(&b.keys, "kb", &pk_a, WS, 4, &w).is_err(), "a share wrap doesn't pass for a workspace key wrap");
    let ws_wrap = seal_wrap(&a.me(), &Recipient { key_id: b.id, public_key: &pk_b }, WS, 4, 4, &share_key).unwrap();
    assert!(open_share_wrap(&b.keys, "kb", &pk_a, WS, "s1", 4, &ws_wrap).is_err(), "nor the other way round");
    // A server-made (unauthenticated) wrap is refused.
    let legacy = wrap_key(&pk_b, WS, 4, &share_key).unwrap();
    assert!(share_wrap_sender(&legacy).is_err());
    assert!(open_share_wrap(&b.keys, "kb", &pk_a, WS, "s1", 4, &legacy).is_err());
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vectors {
    description: Vec<String>,
    runs: Vec<Sealed>,
    shares: Vec<Sealed>,
    share_wraps: Vec<ShareWrap>,
    failures: Vec<Failure>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sealed {
    name: String,
    key: String,
    workspace_id: String,
    /// The run id, or the share id.
    id: String,
    /// The epoch (runs) or the version (shares).
    number: u32,
    nonce: String,
    plaintext: String,
    associated_data: String,
    envelope: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareWrap {
    name: String,
    sender_ikm: String,
    sender_public: String,
    sender_key_id: String,
    recipient_ikm: String,
    recipient_public: String,
    recipient_key_id: String,
    workspace_id: String,
    share_id: String,
    version: u32,
    info: String,
    wrapped: String,
    share_key: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Failure {
    name: String,
    /// `run`, `share` or `shareWrap`.
    kind: String,
    key: String,
    workspace_id: String,
    id: String,
    number: u32,
    bytes: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    sender_public: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    recipient_key_id: String,
}

#[test]
fn published_shared_vectors() {
    let v: Vectors = serde_json::from_str(VECTORS).expect("vectors/shared.json");
    assert!(!v.runs.is_empty() && !v.shares.is_empty() && !v.share_wraps.is_empty() && v.failures.len() >= 6);
    for r in &v.runs {
        let key = WorkspaceKey::from_bytes(arr(&r.key));
        assert_eq!(hex(&run_associated_data(&r.workspace_id, &r.id, r.number).unwrap()), r.associated_data, "{}", r.name);
        assert_eq!(hex(&seal_run_with_nonce(&key, &r.workspace_id, &r.id, r.number, &unhex(&r.plaintext), &arr(&r.nonce)).unwrap()), r.envelope, "{}", r.name);
        assert_eq!(hex(&open_run(&key, &r.workspace_id, &r.id, &unhex(&r.envelope)).unwrap()), r.plaintext, "{}", r.name);
    }
    for s in &v.shares {
        let key = WorkspaceKey::from_bytes(arr(&s.key));
        assert_eq!(hex(&share_associated_data(&s.workspace_id, &s.id, s.number).unwrap()), s.associated_data, "{}", s.name);
        assert_eq!(hex(&seal_share_with_nonce(&key, &s.workspace_id, &s.id, s.number, &unhex(&s.plaintext), &arr(&s.nonce)).unwrap()), s.envelope, "{}", s.name);
        assert_eq!(hex(&open_share(&key, &s.workspace_id, &s.id, s.number, &unhex(&s.envelope)).unwrap()), s.plaintext, "{}", s.name);
    }
    for w in &v.share_wraps {
        let sender = DeviceKeyPair::derive(&unhex(&w.sender_ikm));
        assert_eq!(hex(&sender.public_key()), w.sender_public, "{}: sender", w.name);
        let device = DeviceKeyPair::derive(&unhex(&w.recipient_ikm));
        assert_eq!(hex(&device.public_key()), w.recipient_public, "{}: recipient", w.name);
        assert_eq!(hex(&share_wrap_info(&w.workspace_id, &w.share_id, w.version, &w.sender_key_id, &w.recipient_key_id).unwrap()), w.info, "{}: info", w.name);
        let key = open_share_wrap(&device, &w.recipient_key_id, &unhex(&w.sender_public), &w.workspace_id, &w.share_id, w.version, &unhex(&w.wrapped)).unwrap();
        assert_eq!(hex(key.as_bytes()), w.share_key, "{}", w.name);
    }
    let wrap = &v.share_wraps[0];
    let device = DeviceKeyPair::derive(&unhex(&wrap.recipient_ikm));
    for f in &v.failures {
        let bytes = unhex(&f.bytes);
        let failed = match f.kind.as_str() {
            "run" => open_run(&WorkspaceKey::from_bytes(arr(&f.key)), &f.workspace_id, &f.id, &bytes).is_err(),
            "share" => open_share(&WorkspaceKey::from_bytes(arr(&f.key)), &f.workspace_id, &f.id, f.number, &bytes).is_err(),
            "shareWrap" => open_share_wrap(&device, &f.recipient_key_id, &unhex(&f.sender_public), &f.workspace_id, &f.id, f.number, &bytes).is_err(),
            other => panic!("unknown failure kind {other}"),
        };
        assert!(failed, "{} must fail", f.name);
    }
}

/// Writes vectors/shared.json. Run once with `cargo test -p hitop-sync-crypto -- --ignored
/// generate_shared_vectors`; the file is then frozen, and `published_shared_vectors` checks it.
#[test]
#[ignore]
fn generate_shared_vectors() {
    let key: [u8; 32] = std::array::from_fn(|i| 0x40 + i as u8);
    let share_key: [u8; 32] = std::array::from_fn(|i| 0xc0 ^ (i as u8));
    let nonce: [u8; 24] = std::array::from_fn(|i| 0x90 + i as u8);
    let report = br#"{"reportVersion":1,"collection":"Shop API","stats":{"requests":2,"failed":1}}"#;
    let content = br#"{"v":1,"environment":"prod","values":{"API_TOKEN":"s3cr3t"},"viewers":false,"runners":[]}"#;
    let run = |name: &str, id: &str, epoch: u32| {
        let k = WorkspaceKey::from_bytes(key);
        Sealed {
            name: name.into(),
            key: hex(&key),
            workspace_id: WS.into(),
            id: id.into(),
            number: epoch,
            nonce: hex(&nonce),
            plaintext: hex(report),
            associated_data: hex(&run_associated_data(WS, id, epoch).unwrap()),
            envelope: hex(&seal_run_with_nonce(&k, WS, id, epoch, report, &nonce).unwrap()),
        }
    };
    let share = |name: &str, id: &str, version: u32| {
        let k = WorkspaceKey::from_bytes(share_key);
        Sealed {
            name: name.into(),
            key: hex(&share_key),
            workspace_id: WS.into(),
            id: id.into(),
            number: version,
            nonce: hex(&nonce),
            plaintext: hex(content),
            associated_data: hex(&share_associated_data(WS, id, version).unwrap()),
            envelope: hex(&seal_share_with_nonce(&k, WS, id, version, content, &nonce).unwrap()),
        }
    };
    let runs = vec![run("run report, epoch 1", "0f8e2c1d9b7a65544332211000ffeedd", 1), run("run report, epoch 7", "a1", 7)];
    let shares = vec![share("share version 1", "5a5b5c5d5e5f60616263646566676869", 1), share("share version 3", "5a5b5c5d5e5f60616263646566676869", 3)];
    let sender_ikm: [u8; 32] = std::array::from_fn(|i| 0x30 + i as u8);
    let recipient_ikm: [u8; 32] = std::array::from_fn(|i| 0x10 + i as u8);
    let sender = DeviceKeyPair::derive(&sender_ikm);
    let recipient = DeviceKeyPair::derive(&recipient_ikm);
    let (sender_id, recipient_id, share_id) = ("0f1e2d3c4b5a69788796a5b4c3d2e1f0", "a1b2c3d4e5f60718293a4b5c6d7e8f90", "5a5b5c5d5e5f60616263646566676869");
    let rpk = recipient.public_key();
    let wrapped = seal_share_wrap(&Device { key_id: sender_id, keys: &sender }, &Recipient { key_id: recipient_id, public_key: &rpk }, WS, share_id, 3, &WorkspaceKey::from_bytes(share_key)).unwrap();
    let share_wraps = vec![ShareWrap {
        name: "share key, version 3".into(),
        sender_ikm: hex(&sender_ikm),
        sender_public: hex(&sender.public_key()),
        sender_key_id: sender_id.into(),
        recipient_ikm: hex(&recipient_ikm),
        recipient_public: hex(&rpk),
        recipient_key_id: recipient_id.into(),
        workspace_id: WS.into(),
        share_id: share_id.into(),
        version: 3,
        info: hex(&share_wrap_info(WS, share_id, 3, sender_id, recipient_id).unwrap()),
        wrapped: hex(&wrapped),
        share_key: hex(&share_key),
    }];
    let r = &runs[0];
    let s = &shares[1];
    let mut run_bit = unhex(&r.envelope);
    *run_bit.last_mut().unwrap() ^= 1;
    let mut share_version = unhex(&s.envelope);
    share_version[4] = 4;
    let other = DeviceKeyPair::derive(&[0x55; 32]);
    let forged = seal_share_wrap(&Device { key_id: sender_id, keys: &other }, &Recipient { key_id: recipient_id, public_key: &rpk }, WS, share_id, 3, &WorkspaceKey::from_bytes(share_key)).unwrap();
    let ws_wrap = seal_wrap(&Device { key_id: sender_id, keys: &sender }, &Recipient { key_id: recipient_id, public_key: &rpk }, WS, 3, 3, &WorkspaceKey::from_bytes(share_key)).unwrap();
    let f = |name: &str, kind: &str, k: &[u8; 32], id: &str, number: u32, bytes: &[u8], wrap: bool| Failure {
        name: name.into(),
        kind: kind.into(),
        key: hex(k),
        workspace_id: WS.into(),
        id: id.into(),
        number,
        bytes: hex(bytes),
        sender_public: if wrap { hex(&sender.public_key()) } else { String::new() },
        recipient_key_id: if wrap { recipient_id.into() } else { String::new() },
    };
    let blob = encrypt_blob_with_nonce(&WorkspaceKey::from_bytes(key), &BlobContext { workspace_id: WS, blob_id: &r.id, version: 1, epoch: 1 }, report, &nonce).unwrap();
    let failures = vec![
        f("run report, flipped tag bit", "run", &key, &r.id, 1, &run_bit, false),
        f("run report under another run id", "run", &key, "0f8e2c1d9b7a65544332211000ffeede", 1, &unhex(&r.envelope), false),
        f("a file blob with the run's id", "run", &key, &r.id, 1, &blob, false),
        f("share version rewritten", "share", &share_key, &s.id, 4, &share_version, false),
        f("share served as an older version", "share", &share_key, &s.id, 1, &unhex(&s.envelope), false),
        f("share key wrap made by another sender key", "shareWrap", &share_key, share_id, 3, &forged, true),
        f("workspace key wrap offered as a share wrap", "shareWrap", &share_key, share_id, 3, &ws_wrap, true),
        f("share key wrap for another version", "shareWrap", &share_key, share_id, 2, &wrapped, true),
        f("share key wrap for another share", "shareWrap", &share_key, "5a5b5c5d5e5f6061626364656667686a", 3, &wrapped, true),
    ];
    let v = Vectors {
        description: vec![
            "hitop sync test vectors for run reports and shared environment values (hitop-sync-crypto 0.5). Byte strings are lower-case hex; integers big endian; u16 lengths prefix every string (UTF-8).".into(),
            "run report: XChaCha20-Poly1305 under the workspace key of `number` (the epoch); envelope = 0x01 || u32 epoch || nonce (24) || ciphertext || tag (16); aad = \"hitop-sync/v1/run\" || u16 len || workspaceId || u16 len || runId || u32 epoch".into(),
            "share: XChaCha20-Poly1305 under the share's own 32-byte key; envelope = 0x01 || u32 version || nonce || ciphertext || tag; aad = \"hitop-sync/v1/env-share\" || u16 len || workspaceId || u16 len || shareId || u32 version; versions start at 1".into(),
            "share wrap: HPKE RFC 9180 Auth mode, DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305 (as format 2 workspace key wraps); info = \"hitop-sync/v1/env-share-wrap\" || u16 len || workspaceId || u16 len || shareId || u32 version || u16 len || senderKeyId || u16 len || recipientKeyId; aad empty; plaintext = the share key; wrapped = 0x02 || u32 version || u16 len || senderKeyId || enc (32) || sealed key (32) || tag (16)".into(),
            "key pairs = DeriveKeyPair(ikm); wraps are checked in the open direction only (encapsulation is randomized); every entry in failures must fail (shareWrap failures open with shareWraps[0]'s recipient)".into(),
        ],
        runs,
        shares,
        share_wraps,
        failures,
    };
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/shared.json"), serde_json::to_string_pretty(&v).unwrap() + "\n").unwrap();
}
