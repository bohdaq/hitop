//! The host key (HITOP Cloud C5): a 32-byte key in a file on the server's host
//! (`HITOP_SYNC_HOST_KEY_FILE`, 64 hex characters, root only, kept out of database backups), under
//! which the server seals **its own** secrets at rest: today the per-organization OIDC client
//! secrets (orgsso.rs). A database dump or backup alone doesn't reveal them.
//!
//! This is not end-to-end encryption and has nothing to do with workspace data: the server holds
//! this key, so whoever runs the server (or takes over its host) can open what it sealed. Client
//! data stays sealed under keys only clients hold (THREAT_MODEL.md).
//!
//! Sealed form: `0x01 || key id (4 bytes: SHA-256(key)[..4]) || nonce (12) || AES-256-GCM
//! ciphertext and tag`, associated data `"hitop-sync/v1/host-sealed" || 0x00 || label`, so a sealed
//! value can't be moved to another purpose or org (the label names both).
//!
//! Without the file, everything that needs it is off: per-org SSO and domain claims answer 404
//! `org_sso_off` (the hosted test server keeps it unset, owner decision Q15).

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use sha2::{Digest, Sha256};

const VERSION: u8 = 1;
const AD_PREFIX: &[u8] = b"hitop-sync/v1/host-sealed\0";

#[derive(Clone)]
pub struct HostKey {
    key: [u8; 32],
}

impl std::fmt::Debug for HostKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never prints the key.
        write!(f, "HostKey({})", self.id_hex())
    }
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

impl HostKey {
    pub fn from_bytes(key: [u8; 32]) -> HostKey {
        HostKey { key }
    }

    /// 64 hex characters (whitespace around them ignored), e.g. from `openssl rand -hex 32`.
    pub fn parse(text: &str) -> Result<HostKey, String> {
        let bytes = hex_decode(text.trim())
            .filter(|b| b.len() == 32)
            .ok_or("the host key must be 64 hex characters (32 bytes): openssl rand -hex 32")?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        if key == [0u8; 32] {
            return Err("the host key is all zeros".into());
        }
        Ok(HostKey { key })
    }

    /// `HITOP_SYNC_HOST_KEY_FILE`; `None` when unset.
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<Option<HostKey>, String> {
        let Some(path) = var("HITOP_SYNC_HOST_KEY_FILE") else {
            return Ok(None);
        };
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("HITOP_SYNC_HOST_KEY_FILE {path}: {e}"))?;
        Self::parse(&text)
            .map(Some)
            .map_err(|e| format!("HITOP_SYNC_HOST_KEY_FILE {path}: {e}"))
    }

    fn id(&self) -> [u8; 4] {
        let d = Sha256::digest(self.key);
        [d[0], d[1], d[2], d[3]]
    }

    fn id_hex(&self) -> String {
        self.id().iter().map(|b| format!("{b:02x}")).collect()
    }

    fn aead(&self) -> LessSafeKey {
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &self.key).expect("a 32-byte AES key"))
    }

    fn ad(label: &str) -> Vec<u8> {
        [AD_PREFIX, label.as_bytes()].concat()
    }

    /// Seals `plaintext` for `label` (what it is and whose, e.g. `org-sso-secret:<org>`).
    pub fn seal(&self, label: &str, plaintext: &[u8]) -> Vec<u8> {
        let nonce_bytes = crate::api::random_bytes::<NONCE_LEN>();
        let mut buf = plaintext.to_vec();
        self.aead()
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(Self::ad(label)),
                &mut buf,
            )
            .expect("AES-GCM seal");
        let mut out = Vec::with_capacity(1 + 4 + NONCE_LEN + buf.len());
        out.push(VERSION);
        out.extend_from_slice(&self.id());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&buf);
        out
    }

    /// Opens what [`seal`](Self::seal) made for the same label.
    pub fn open(&self, label: &str, sealed: &[u8]) -> Result<Vec<u8>, String> {
        if sealed.len() < 1 + 4 + NONCE_LEN + 16 || sealed[0] != VERSION {
            return Err("not a sealed value of this server".into());
        }
        if sealed[1..5] != self.id() {
            return Err("sealed under another host key (HITOP_SYNC_HOST_KEY_FILE changed?)".into());
        }
        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&sealed[5..5 + NONCE_LEN]);
        let mut buf = sealed[5 + NONCE_LEN..].to_vec();
        let plain = self
            .aead()
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(Self::ad(label)),
                &mut buf,
            )
            .map_err(|_| {
                "the sealed value doesn't open (changed, or sealed for something else)".to_string()
            })?;
        Ok(plain.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seals_and_opens_only_for_the_same_label_and_key() {
        let k = HostKey::parse(&"ab".repeat(32)).unwrap();
        let s = k.seal("org-sso-secret:o1", b"client secret");
        assert_eq!(k.open("org-sso-secret:o1", &s).unwrap(), b"client secret");
        assert!(k.open("org-sso-secret:o2", &s).is_err());
        let other = HostKey::parse(&"cd".repeat(32)).unwrap();
        assert!(other
            .open("org-sso-secret:o1", &s)
            .unwrap_err()
            .contains("another host key"));
        let mut bad = s.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(k.open("org-sso-secret:o1", &bad).is_err());
        assert!(!format!("{k:?}").contains("abab"));
    }

    #[test]
    fn the_key_file_must_be_64_hex_characters() {
        assert!(HostKey::parse("abc").is_err());
        assert!(HostKey::parse(&"00".repeat(32)).is_err());
        assert!(HostKey::parse(&format!("{}\n", "0f".repeat(32))).is_ok());
    }
}
