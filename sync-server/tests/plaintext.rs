//! The server never sees plaintext: a full session (two members, edits, a removal with key
//! rotation) is run against a server with its database on disk, and then neither the files on
//! disk nor any answer the server gave contains the plaintext, a workspace key or a device secret.

mod common;

use common::*;
use hitop_sync_crypto as crypto;
use serde_json::json;

const WS: &str = "fedcba9876543210fedcba9876543210";

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn the_server_never_sees_plaintext_or_keys() {
    let s = Server::new("plaintext");
    let ada = Device::new(&s, "ada@example.com");
    let bob = Device::new(&s, "bob@example.com");
    let k1 = create_workspace(&s, &ada, WS);
    let bob_wraps = wraps_for(&[(bob.key_id.clone(), bob.keys.public_key().to_vec())], WS, 1, &k1);
    s.call("POST", &format!("/v1/workspaces/{WS}/members"), Some(&ada.session), Some(json!({ "email": "bob@example.com", "role": "editor", "wraps": bob_wraps })));

    // Distinctive plaintexts: a request file with a token in it.
    let secrets: Vec<Vec<u8>> = (0..4).map(|i| format!("name: Get user {i}\nurl: https://api.example.com/users\nheaders:\n  - name: Authorization\n    value: Bearer sk_live_PLAINTEXT_MARKER_{i}\n").into_bytes()).collect();
    let mut replies = Vec::new();
    let mut put = |d: &Device, blob: &str, version: u64, vv: serde_json::Value, epoch: u32, key: &crypto::WorkspaceKey, pt: &[u8]| {
        let r = s.call("PUT", &format!("/v1/workspaces/{WS}/blobs/{blob}"), Some(&d.session), Some(json!({ "version": version, "vv": vv, "epoch": epoch, "content": seal(key, WS, blob, version, epoch, pt) })));
        assert_eq!(r.status, 200, "{}", r.json);
        replies.push(r.raw);
    };
    put(&ada, "b1", 1, json!({ "a": 1 }), 1, &k1, &secrets[0]);
    put(&bob, "b1", 2, json!({ "a": 1, "b": 1 }), 1, &k1, &secrets[1]);
    put(&bob, "b2", 1, json!({ "b": 1 }), 1, &k1, &secrets[2]);

    // Bob is removed; Ada rotates and rewrites.
    let k2 = crypto::WorkspaceKey::generate();
    let remaining = [(ada.key_id.clone(), ada.keys.public_key().to_vec())];
    let r = s.call("DELETE", &format!("/v1/workspaces/{WS}/members/{}", bob.user_id), Some(&ada.session), Some(json!({ "epoch": 2, "wraps": wraps_for(&remaining, WS, 2, &k2) })));
    assert_eq!(r.status, 200);
    put(&ada, "b1", 3, json!({ "a": 2, "b": 1 }), 2, &k2, &secrets[3]);

    // Everything a client can read back.
    for path in [format!("/v1/workspaces/{WS}"), format!("/v1/workspaces/{WS}/changes?since=0"), format!("/v1/workspaces/{WS}/blobs/b1"), format!("/v1/workspaces/{WS}/blobs/b2"), format!("/v1/workspaces/{WS}/wraps?keyId={}", ada.key_id), "/v1/me".into()] {
        let r = s.call("GET", &path, Some(&ada.session), None);
        assert_eq!(r.status, 200, "{path}");
        replies.push(r.raw);
    }
    // The client still reads its data.
    let got = s.call("GET", &format!("/v1/workspaces/{WS}/blobs/b1"), Some(&ada.session), None);
    assert_eq!(open(&ada.workspace_key(&s, WS, 2), WS, "b1", 3, 2, got.json["content"].as_str().unwrap()), secrets[3]);

    let disk = s.disk_bytes();
    assert!(disk.len() > 4096, "the database is on disk");
    let forbidden: Vec<(String, Vec<u8>)> = secrets
        .iter()
        .enumerate()
        .flat_map(|(i, pt)| [(format!("plaintext {i}"), pt.clone()), (format!("marker {i}"), format!("PLAINTEXT_MARKER_{i}").into_bytes())])
        .chain([
            ("workspace key 1".to_string(), k1.as_bytes().to_vec()),
            ("workspace key 2".to_string(), k2.as_bytes().to_vec()),
            ("ada's device secret".to_string(), ada.keys.secret_bytes().to_vec()),
            ("bob's device secret".to_string(), bob.keys.secret_bytes().to_vec()),
        ])
        .collect();
    for (what, bytes) in &forbidden {
        assert!(!contains(&disk, bytes), "{what} is in the server's files");
        assert!(!contains(&disk, b64(bytes).as_bytes()), "{what} (base64url) is in the server's files");
        for reply in &replies {
            assert!(!contains(reply, bytes) && !contains(reply, b64(bytes).as_bytes()), "{what} is in a server answer");
        }
    }
    // And the ciphertext really is there: the server stores the envelopes as sent.
    let env = unb64(got.json["content"].as_str().unwrap());
    assert!(contains(&disk, &env[5..29]), "the stored envelope (its nonce) is on disk");
}
