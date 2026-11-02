//! Recovery phrases (since 0.3): 24 words that bring an account's device back after the device is
//! lost (`hitop sync recovery create|use`, the Sync page).
//!
//! The phrase is 32 random bytes written as a BIP-39 mnemonic (English list, 24 words, the last
//! word carries an 8-bit SHA-256 checksum, so a mistyped word is caught). The bytes never leave
//! the device; from them:
//!
//! - the **recovery key** = SHA-256(`"hitop-sync/v1/recovery-key"` || bytes) (32 bytes), which
//!   seals the **recovery blob** with XChaCha20-Poly1305: `0x01 || nonce (24) || ciphertext ||
//!   tag (16)`, associated data `"hitop-sync/v1/recovery" || u16 len || account id`, so the server
//!   can't hand one account's blob to another. The server stores only the blob.
//!
//! What a client puts in the blob is its business (hitop: the device key, its key id, its pins
//! and its [`crate::WorkspaceMemory`] per workspace, as JSON; THREAT_MODEL.md "Recovery"). The
//! phrase has 256 bits of entropy: nobody guesses it, the server included.

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{push_len_prefixed, random, Error};

/// The BIP-39 English word list, from
/// <https://github.com/bitcoin/bips/blob/master/bip-0039/english.txt> (SHA-256
/// 2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda, as BIP-39 states).
const WORDLIST: &str = include_str!("../wordlist/bip39-english.txt");

/// Words in a phrase.
pub const PHRASE_WORDS: usize = 24;
/// First byte of a recovery blob.
pub const RECOVERY_FORMAT: u8 = 1;
/// Domain separation of the recovery key.
pub const RECOVERY_KEY_LABEL: &[u8] = b"hitop-sync/v1/recovery-key";
/// Domain separation prefix of the recovery blob's associated data.
pub const RECOVERY_AAD_LABEL: &[u8] = b"hitop-sync/v1/recovery";

fn words() -> Vec<&'static str> {
    WORDLIST.lines().map(str::trim).filter(|w| !w.is_empty()).collect()
}

/// A recovery phrase: 32 bytes of entropy. Zeroed on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct RecoveryPhrase([u8; 32]);

impl RecoveryPhrase {
    /// A new phrase from the OS RNG.
    pub fn generate() -> Self {
        RecoveryPhrase(random::<32>())
    }

    /// The phrase for these bytes (test vectors).
    pub fn from_entropy(bytes: [u8; 32]) -> Self {
        RecoveryPhrase(bytes)
    }

    pub fn entropy(&self) -> &[u8; 32] {
        &self.0
    }

    /// Reads a phrase as people type it: words separated by any whitespace (or commas), any
    /// case. Refused unless there are 24 words, each in the list, with the right checksum.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let list = words();
        let given: Vec<String> = text.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect();
        if given.len() != PHRASE_WORDS {
            return Err(Error::Phrase(format!("a recovery phrase has {PHRASE_WORDS} words, this has {}", given.len())));
        }
        // 24 words × 11 bits = 256 bits of entropy + 8 bits of checksum.
        let mut bits: Vec<bool> = Vec::with_capacity(PHRASE_WORDS * 11);
        for (i, w) in given.iter().enumerate() {
            let Ok(index) = list.binary_search(&w.as_str()) else {
                return Err(Error::Phrase(format!("word {} ({w}) is not a recovery phrase word", i + 1)));
            };
            bits.extend((0..11).rev().map(|b| (index >> b) & 1 == 1));
        }
        let mut entropy = [0u8; 32];
        for (i, byte) in entropy.iter_mut().enumerate() {
            *byte = (0..8).fold(0u8, |acc, b| (acc << 1) | bits[i * 8 + b] as u8);
        }
        let checksum = (0..8).fold(0u8, |acc, b| (acc << 1) | bits[256 + b] as u8);
        if Sha256::digest(entropy)[0] != checksum {
            entropy.zeroize();
            return Err(Error::Phrase("the words don't check out (a word is wrong or two are swapped)".into()));
        }
        Ok(RecoveryPhrase(entropy))
    }

    /// The 24 words, separated by single spaces.
    pub fn to_words(&self) -> String {
        let list = words();
        let checksum = Sha256::digest(self.0)[0];
        let mut bits: Vec<bool> = self.0.iter().flat_map(|byte| (0..8).rev().map(move |b| (byte >> b) & 1 == 1)).collect();
        bits.extend((0..8).rev().map(|b| (checksum >> b) & 1 == 1));
        bits.chunks(11).map(|c| list[c.iter().fold(0usize, |acc, &bit| (acc << 1) | bit as usize)]).collect::<Vec<_>>().join(" ")
    }

    /// The key that seals the recovery blob.
    pub fn recovery_key(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(RECOVERY_KEY_LABEL);
        h.update(self.0);
        h.finalize().into()
    }
}

impl std::fmt::Debug for RecoveryPhrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryPhrase(..)")
    }
}

/// `label || u16 len || account id`.
pub fn recovery_associated_data(account_id: &str) -> Result<Vec<u8>, Error> {
    let mut aad = RECOVERY_AAD_LABEL.to_vec();
    push_len_prefixed(&mut aad, account_id.as_bytes())?;
    Ok(aad)
}

/// Seals `plaintext` for the account `account_id` under the phrase's recovery key.
pub fn seal_recovery(phrase: &RecoveryPhrase, account_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
    seal_recovery_with_nonce(phrase, account_id, plaintext, &random::<24>())
}

/// [`seal_recovery`] with a caller-chosen nonce. Only for test vectors.
#[doc(hidden)]
pub fn seal_recovery_with_nonce(phrase: &RecoveryPhrase, account_id: &str, plaintext: &[u8], nonce: &[u8; 24]) -> Result<Vec<u8>, Error> {
    let aad = recovery_associated_data(account_id)?;
    let mut key = phrase.recovery_key();
    let cipher = XChaCha20Poly1305::new(&Key::from(key));
    key.zeroize();
    let ciphertext = cipher.encrypt(&XNonce::from(*nonce), Payload { msg: plaintext, aad: &aad }).map_err(|_| Error::Malformed("plaintext (too long)"))?;
    let mut out = Vec::with_capacity(1 + 24 + ciphertext.len());
    out.push(RECOVERY_FORMAT);
    out.extend_from_slice(nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Opens a recovery blob of the account `account_id`: [`Error::Decrypt`] for a wrong phrase,
/// another account's blob, or tampered bytes.
pub fn open_recovery(phrase: &RecoveryPhrase, account_id: &str, blob: &[u8]) -> Result<Vec<u8>, Error> {
    if blob.len() < 1 + 24 + crate::TAG_LEN {
        return Err(Error::Malformed("recovery blob (too short)"));
    }
    if blob[0] != RECOVERY_FORMAT {
        return Err(Error::Malformed("recovery blob (unknown format)"));
    }
    let aad = recovery_associated_data(account_id)?;
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&blob[1..25]);
    let mut key = phrase.recovery_key();
    let cipher = XChaCha20Poly1305::new(&Key::from(key));
    key.zeroize();
    cipher.decrypt(&XNonce::from(nonce), Payload { msg: &blob[25..], aad: &aad }).map_err(|_| Error::Decrypt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_list_is_bip39_english() {
        let digest: String = Sha256::digest(WORDLIST.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(digest, "2f5eed53a4727b4bf8880d8f3f199efc90e58503646d9ff8eff3a2ed3b24dbda");
        let list = words();
        assert_eq!(list.len(), 2048);
        assert!(list.windows(2).all(|w| w[0] < w[1]), "sorted, so binary search works");
    }

    #[test]
    fn bip39_test_vectors() {
        // From the BIP-39 reference vectors (256-bit entropy).
        let cases: [([u8; 32], &str); 3] = [
            ([0u8; 32], "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art"),
            ([0x7f; 32], "legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth title"),
            ([0xff; 32], "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo vote"),
        ];
        for (entropy, words) in cases {
            assert_eq!(RecoveryPhrase::from_entropy(entropy).to_words(), words);
            assert_eq!(RecoveryPhrase::parse(words).unwrap().entropy(), &entropy);
        }
    }

    #[test]
    fn phrases_round_trip_and_typos_are_caught() {
        let p = RecoveryPhrase::generate();
        let words = p.to_words();
        assert_eq!(words.split(' ').count(), 24);
        assert_eq!(RecoveryPhrase::parse(&format!("  {}\n", words.to_uppercase().replace(' ', ",\n "))).unwrap().entropy(), p.entropy());
        let mut w: Vec<&str> = words.split(' ').collect();
        assert!(matches!(RecoveryPhrase::parse(&w[..23].join(" ")), Err(Error::Phrase(_))));
        w[3] = "notaword";
        assert!(RecoveryPhrase::parse(&w.join(" ")).unwrap_err().to_string().contains("word 4"));
        // A wrong (but listed) word: the checksum catches it (1 in 256 slips by, so try two).
        let original: Vec<&str> = words.split(' ').collect();
        let wrong = ["abandon", "ability"].iter().map(|alt| {
            let mut v = original.clone();
            v[0] = if v[0] == *alt { "zoo" } else { alt };
            RecoveryPhrase::parse(&v.join(" ")).is_err()
        });
        assert!(wrong.into_iter().any(|refused| refused));
    }

    #[test]
    fn blobs_open_only_with_the_phrase_for_the_account() {
        let p = RecoveryPhrase::generate();
        let blob = seal_recovery(&p, "u1", b"device secret").unwrap();
        assert_eq!(open_recovery(&p, "u1", &blob).unwrap(), b"device secret");
        assert_eq!(open_recovery(&RecoveryPhrase::generate(), "u1", &blob), Err(Error::Decrypt), "another phrase");
        assert_eq!(open_recovery(&p, "u2", &blob), Err(Error::Decrypt), "another account's blob");
        let mut tampered = blob.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert_eq!(open_recovery(&p, "u1", &tampered), Err(Error::Decrypt));
        assert!(matches!(open_recovery(&p, "u1", &blob[..10]), Err(Error::Malformed(_))));
    }

    #[test]
    fn the_recovery_blob_layout_is_pinned() {
        let p = RecoveryPhrase::from_entropy([0u8; 32]);
        let blob = seal_recovery_with_nonce(&p, "u1", b"hello", &[9u8; 24]).unwrap();
        assert_eq!(blob[0], RECOVERY_FORMAT);
        assert_eq!(&blob[1..25], &[9u8; 24]);
        assert_eq!(blob.len(), 1 + 24 + 5 + 16);
        let mut key_input = RECOVERY_KEY_LABEL.to_vec();
        key_input.extend([0u8; 32]);
        assert_eq!(p.recovery_key(), <[u8; 32]>::from(Sha256::digest(&key_input)));
        // Pinned so other implementations (and later versions) can check theirs.
        let hex: String = blob.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, PINNED_BLOB);
    }

    const PINNED_BLOB: &str = "010909090909090909090909090909090909090909090909092d66bbd3da67f3e04ae5c641f630d30b4c87331c84";
}
