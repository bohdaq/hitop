//! Passwords (since 0.4): one password signs a person in to the sync server **and** unlocks their
//! key bundle on a new device, without the server learning anything that opens the bundle
//! (sync-server/THREAT_MODEL.md "Passwords").
//!
//! The password never leaves the device. From it, with the account's KDF parameters (a random
//! salt and Argon2id costs, kept by the server and handed to any client that asks):
//!
//! ```text
//! master    = Argon2id(NFC(password), salt, m, t, p, secret = none,
//!                      associated data = "hitop-sync/v1/password", 32 bytes)
//! authKey   = HKDF-SHA256(ikm = master, salt = none, info = "hitop-sync/v1/password/auth")
//! bundleKey = HKDF-SHA256(ikm = master, salt = none,
//!                         info = "hitop-sync/v1/password/bundle" || canonical(params))
//! ```
//!
//! The client sends only `authKey`; the server stores a slow hash of it (its own Argon2id with its
//! own salt and an optional pepper) and compares that at sign-in. `bundleKey` is the 32-byte unlock
//! key of [`crate::seal_key_bundle`] (server method `password`). HKDF with two labels makes the two
//! keys independent: knowing `authKey` (the server does, for a moment, at each sign-in) says
//! nothing about `bundleKey`; finding `bundleKey` from `authKey` means guessing the password and
//! paying Argon2id for every guess. The parameters are part of the bundle key's derivation, so a
//! server that reports other parameters than the bundle was sealed with gets a key that opens
//! nothing.
//!
//! Because the server sees `authKey`, a server that hands a client **weak parameters** would turn
//! it into an offline guessing oracle. [`PasswordKdf::check`] refuses parameters below
//! [`MIN_MEMORY_KIB`] / [`MIN_ITERATIONS`] (and above a ceiling, so a server can't make a client
//! spend minutes or gigabytes); every client calls it before deriving anything.

use argon2::{Algorithm, Argon2, AssociatedData, ParamsBuilder, Version};
use hkdf::Hkdf;
use sha2::Sha256;
use unicode_normalization::UnicodeNormalization;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{random, Error};

/// The only algorithm: Argon2id, version 0x13.
pub const PASSWORD_ALG: &str = "argon2id";
/// Default (and minimum) memory cost in KiB: 64 MiB.
pub const DEFAULT_MEMORY_KIB: u32 = 64 * 1024;
/// Default (and minimum) passes.
pub const DEFAULT_ITERATIONS: u32 = 3;
/// Default lanes.
pub const DEFAULT_PARALLELISM: u32 = 1;
/// Floors a client insists on, whatever the server says.
pub const MIN_MEMORY_KIB: u32 = 64 * 1024;
pub const MIN_ITERATIONS: u32 = 3;
/// Ceilings: 1 GiB, 10 passes, 4 lanes.
pub const MAX_MEMORY_KIB: u32 = 1024 * 1024;
pub const MAX_ITERATIONS: u32 = 10;
pub const MAX_PARALLELISM: u32 = 4;
/// Salt length a new KDF gets; 16 to 64 bytes are accepted.
pub const SALT_LEN: usize = 16;
/// Shortest password a client accepts (NIST SP 800-63B: at least 8; no composition rules).
pub const MIN_PASSWORD_CHARS: usize = 8;
/// Longest (in bytes, after NFC), so a pasted file isn't hashed.
pub const MAX_PASSWORD_BYTES: usize = 1024;

/// Argon2's associated data: binds the master key to this use.
pub const PASSWORD_AD: &[u8] = b"hitop-sync/v1/password";
/// HKDF info of the key the server sees.
pub const AUTH_KEY_INFO: &[u8] = b"hitop-sync/v1/password/auth";
/// HKDF info prefix of the key that seals the key bundle (the canonical parameters follow).
pub const BUNDLE_KEY_INFO: &[u8] = b"hitop-sync/v1/password/bundle";

/// An account's password KDF: Argon2id costs and salt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordKdf {
    /// Memory in KiB.
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub salt: Vec<u8>,
}

impl PasswordKdf {
    /// The defaults with a new random salt (sign-up, a password change).
    pub fn generate() -> Self {
        PasswordKdf { memory_kib: DEFAULT_MEMORY_KIB, iterations: DEFAULT_ITERATIONS, parallelism: DEFAULT_PARALLELISM, salt: random::<SALT_LEN>().to_vec() }
    }

    /// Refuses parameters outside the floors and ceilings (see the module docs: a server must not
    /// be able to weaken them).
    pub fn check(&self) -> Result<(), Error> {
        let bad = |why: String| Err(Error::Password(why));
        if self.memory_kib < MIN_MEMORY_KIB || self.iterations < MIN_ITERATIONS || self.parallelism < 1 {
            return bad(format!(
                "the server asks for weaker password hashing (m={} KiB, t={}, p={}) than hitop accepts (m>={MIN_MEMORY_KIB}, t>={MIN_ITERATIONS}): refusing to derive keys with it",
                self.memory_kib, self.iterations, self.parallelism
            ));
        }
        if self.memory_kib > MAX_MEMORY_KIB || self.iterations > MAX_ITERATIONS || self.parallelism > MAX_PARALLELISM {
            return bad(format!("the server asks for password hashing costs above hitop's ceiling (m={} KiB, t={}, p={})", self.memory_kib, self.iterations, self.parallelism));
        }
        if self.salt.len() < 16 || self.salt.len() > 64 {
            return bad(format!("a password salt has 16 to 64 bytes, this one {}", self.salt.len()));
        }
        Ok(())
    }

    /// `argon2id$v=19$m=<m>,t=<t>,p=<p>$<salt hex>`: the bytes the bundle key's info ends with.
    pub fn canonical(&self) -> String {
        let salt: String = self.salt.iter().map(|b| format!("{b:02x}")).collect();
        format!("{PASSWORD_ALG}$v=19$m={},t={},p={}${salt}", self.memory_kib, self.iterations, self.parallelism)
    }
}

/// The two keys from one password. Zeroed on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct PasswordKeys {
    /// Sent to the server (which stores only a slow hash of it).
    pub auth_key: [u8; 32],
    /// Seals the key bundle; never leaves the device.
    pub bundle_key: [u8; 32],
}

impl std::fmt::Debug for PasswordKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PasswordKeys(..)")
    }
}

/// The password as hashed: NFC, so the same password typed on two systems gives the same keys.
pub fn normalize_password(password: &str) -> String {
    password.nfc().collect()
}

/// Refuses passwords a client shouldn't accept for a new password: shorter than
/// [`MIN_PASSWORD_CHARS`] characters or longer than [`MAX_PASSWORD_BYTES`].
pub fn check_new_password(password: &str) -> Result<(), Error> {
    let p = normalize_password(password);
    if p.chars().count() < MIN_PASSWORD_CHARS {
        return Err(Error::Password(format!("a password has at least {MIN_PASSWORD_CHARS} characters (a few words are easiest to remember)")));
    }
    if p.len() > MAX_PASSWORD_BYTES {
        return Err(Error::Password(format!("a password has at most {MAX_PASSWORD_BYTES} bytes")));
    }
    Ok(())
}

/// Derives the auth key and the bundle key (checks `kdf` first).
pub fn derive_password_keys(password: &str, kdf: &PasswordKdf) -> Result<PasswordKeys, Error> {
    kdf.check()?;
    let mut p = normalize_password(password);
    if p.is_empty() || p.len() > MAX_PASSWORD_BYTES {
        p.zeroize();
        return Err(Error::Password("the password is empty or too long".into()));
    }
    let params = ParamsBuilder::new()
        .m_cost(kdf.memory_kib)
        .t_cost(kdf.iterations)
        .p_cost(kdf.parallelism)
        .output_len(32)
        .data(AssociatedData::new(PASSWORD_AD).map_err(|_| Error::Malformed("password associated data"))?)
        .build()
        .map_err(|e| Error::Password(format!("password hashing parameters: {e}")))?;
    let mut master = [0u8; 32];
    let r = Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(p.as_bytes(), &kdf.salt, &mut master);
    p.zeroize();
    r.map_err(|e| Error::Password(format!("password hashing: {e}")))?;
    let hk = Hkdf::<Sha256>::new(None, &master);
    master.zeroize();
    let mut keys = PasswordKeys { auth_key: [0u8; 32], bundle_key: [0u8; 32] };
    hk.expand(AUTH_KEY_INFO, &mut keys.auth_key).map_err(|_| Error::Malformed("HKDF output"))?;
    let mut info = BUNDLE_KEY_INFO.to_vec();
    info.extend_from_slice(kdf.canonical().as_bytes());
    hk.expand(&info, &mut keys.bundle_key).map_err(|_| Error::Malformed("HKDF output"))?;
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kdf() -> PasswordKdf {
        PasswordKdf { memory_kib: MIN_MEMORY_KIB, iterations: MIN_ITERATIONS, parallelism: 1, salt: (0u8..16).collect() }
    }

    #[test]
    fn keys_are_independent_deterministic_and_bound_to_everything() {
        let k = derive_password_keys("correct horse battery staple", &kdf()).unwrap();
        assert_ne!(k.auth_key, k.bundle_key);
        assert_eq!(derive_password_keys("correct horse battery staple", &kdf()).unwrap().auth_key, k.auth_key);
        assert_ne!(derive_password_keys("correct horse battery stapl3", &kdf()).unwrap().auth_key, k.auth_key, "password");
        let mut other_salt = kdf();
        other_salt.salt[0] ^= 1;
        assert_ne!(derive_password_keys("correct horse battery staple", &other_salt).unwrap().bundle_key, k.bundle_key, "salt");
        let mut more = kdf();
        more.iterations += 1;
        let m = derive_password_keys("correct horse battery staple", &more).unwrap();
        assert_ne!(m.bundle_key, k.bundle_key, "params");
    }

    #[test]
    fn unicode_forms_give_the_same_keys() {
        // "é" precomposed vs e + combining acute accent.
        let a = derive_password_keys("caf\u{e9} au lait", &kdf()).unwrap();
        let b = derive_password_keys("cafe\u{301} au lait", &kdf()).unwrap();
        assert_eq!(a.auth_key, b.auth_key);
    }

    #[test]
    fn weak_or_huge_parameters_are_refused() {
        for (m, t, p, salt) in [(MIN_MEMORY_KIB - 1, 3, 1, 16), (MIN_MEMORY_KIB, 2, 1, 16), (MIN_MEMORY_KIB, 3, 0, 16), (MAX_MEMORY_KIB + 1, 3, 1, 16), (MIN_MEMORY_KIB, 11, 1, 16), (MIN_MEMORY_KIB, 3, 5, 16), (MIN_MEMORY_KIB, 3, 1, 15), (MIN_MEMORY_KIB, 3, 1, 65)] {
            let k = PasswordKdf { memory_kib: m, iterations: t, parallelism: p, salt: vec![7; salt] };
            assert!(matches!(derive_password_keys("long enough password", &k), Err(Error::Password(_))), "{m} {t} {p} {salt}");
        }
        assert!(PasswordKdf::generate().check().is_ok());
        assert_eq!(PasswordKdf::generate().salt.len(), SALT_LEN);
    }

    #[test]
    fn new_passwords_have_a_minimum_length() {
        assert!(check_new_password("short").is_err());
        assert!(check_new_password("eight ch").is_ok());
        assert!(check_new_password(&"x".repeat(MAX_PASSWORD_BYTES + 1)).is_err());
        assert!(derive_password_keys("", &kdf()).is_err());
    }

    #[test]
    fn the_canonical_parameters_are_pinned() {
        assert_eq!(kdf().canonical(), "argon2id$v=19$m=65536,t=3,p=1$000102030405060708090a0b0c0d0e0f");
    }

    // ---- published vectors (vectors/password.json) -------------------------------------------

    const VECTORS: &str = include_str!("../vectors/password.json");

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    fn cases() -> Vec<(&'static str, &'static str, PasswordKdf)> {
        let k = |m, t, p, salt: Vec<u8>| PasswordKdf { memory_kib: m, iterations: t, parallelism: p, salt };
        vec![
            ("ascii, minimum costs", "correct horse battery staple", k(65536, 3, 1, (0u8..16).collect())),
            ("NFD input (hashes as NFC)", "cafe\u{301} cr\u{e8}me br\u{fb}le\u{301}e", k(65536, 3, 1, vec![0xa5; 16])),
            ("more costs, 32-byte salt", "pässwörd with spaces 🔑", k(65536 * 2, 4, 2, (100u8..132).collect())),
        ]
    }

    fn compute() -> serde_json::Value {
        let mut out = Vec::new();
        for (name, password, kdf) in cases() {
            let keys = derive_password_keys(password, &kdf).unwrap();
            let bundle = crate::seal_key_bundle_with_nonce(&keys.bundle_key, "account-1", br#"{"v":1}"#, &[7u8; 24]).unwrap();
            out.push(serde_json::json!({
                "name": name,
                "password": password,
                "memoryKib": kdf.memory_kib,
                "iterations": kdf.iterations,
                "parallelism": kdf.parallelism,
                "salt": hex(&kdf.salt),
                "canonical": kdf.canonical(),
                "authKey": hex(&keys.auth_key),
                "bundleKey": hex(&keys.bundle_key),
                "bundleAccount": "account-1",
                "bundlePlaintext": "{\"v\":1}",
                "bundleNonce": hex(&[7u8; 24]),
                "bundle": hex(&bundle),
            }));
        }
        serde_json::json!({
            "description": [
                "hitop sync password test vectors (hitop-sync-crypto 0.4). Byte strings are lower-case hex; the password is UTF-8.",
                "master = Argon2id v0x13 (RFC 9106) over NFC(password), salt, m = memoryKib, t = iterations, p = parallelism, no secret, associated data \"hitop-sync/v1/password\", 32 bytes",
                "authKey = HKDF-SHA256 (RFC 5869) expand of master (no salt), info \"hitop-sync/v1/password/auth\", 32 bytes; sent to the server",
                "bundleKey = HKDF-SHA256 expand of master, info \"hitop-sync/v1/password/bundle\" || canonical (UTF-8), 32 bytes; never sent",
                "canonical = \"argon2id$v=19$m=<memoryKib>,t=<iterations>,p=<parallelism>$<salt hex>\"",
                "bundle = seal_key_bundle(bundleKey, bundleAccount, bundlePlaintext) with bundleNonce: 0x01 || nonce || XChaCha20-Poly1305(aad = \"hitop-sync/v1/recovery\" || u16 len || account id)"
            ],
            "cases": out,
        })
    }

    #[test]
    fn the_published_vectors_pass() {
        let v: serde_json::Value = serde_json::from_str(VECTORS).expect("vectors/password.json");
        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 3);
        for c in cases {
            let kdf = PasswordKdf {
                memory_kib: c["memoryKib"].as_u64().unwrap() as u32,
                iterations: c["iterations"].as_u64().unwrap() as u32,
                parallelism: c["parallelism"].as_u64().unwrap() as u32,
                salt: unhex(c["salt"].as_str().unwrap()),
            };
            assert_eq!(kdf.canonical(), c["canonical"].as_str().unwrap());
            let keys = derive_password_keys(c["password"].as_str().unwrap(), &kdf).unwrap();
            assert_eq!(hex(&keys.auth_key), c["authKey"].as_str().unwrap(), "{}", c["name"]);
            assert_eq!(hex(&keys.bundle_key), c["bundleKey"].as_str().unwrap(), "{}", c["name"]);
            let opened = crate::open_key_bundle(&keys.bundle_key, c["bundleAccount"].as_str().unwrap(), &unhex(c["bundle"].as_str().unwrap())).unwrap();
            assert_eq!(opened, c["bundlePlaintext"].as_str().unwrap().as_bytes());
        }
        assert_eq!(v, compute(), "vectors/password.json is what this version computes");
    }

    /// Writes vectors/password.json: `cargo test -p hitop-sync-crypto -- --ignored write_password_vectors`.
    #[test]
    #[ignore]
    fn write_password_vectors() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/password.json");
        std::fs::write(path, serde_json::to_string_pretty(&compute()).unwrap() + "\n").unwrap();
    }
}
