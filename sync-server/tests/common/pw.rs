#![allow(dead_code)]
//! Password sign-up and sign-in as a client does them (hitop-sync-crypto `derive_password_keys`).

use hitop_sync_crypto as crypto;
use serde_json::{json, Value};

use super::*;

pub fn kdf_of(v: &Value) -> crypto::PasswordKdf {
    crypto::PasswordKdf {
        memory_kib: v["m"].as_u64().unwrap() as u32,
        iterations: v["t"].as_u64().unwrap() as u32,
        parallelism: v["p"].as_u64().unwrap() as u32,
        salt: unb64(v["salt"].as_str().unwrap()),
    }
}

pub fn kdf_json(k: &crypto::PasswordKdf) -> Value {
    json!({ "alg": "argon2id", "m": k.memory_kib, "t": k.iterations, "p": k.parallelism, "salt": b64(&k.salt) })
}

/// A client's new password: its KDF (new salt) and keys.
pub fn new_password(password: &str) -> (crypto::PasswordKdf, crypto::PasswordKeys) {
    let kdf = crypto::PasswordKdf::generate();
    let keys = crypto::derive_password_keys(password, &kdf).unwrap();
    (kdf, keys)
}

/// What a client sends to sign in: the server's parameters for the address, then the auth key.
pub fn login(s: &Server, email: &str, password: &str) -> Reply {
    let p = s.call("POST", "/v1/auth/password/params", None, Some(json!({ "email": email })));
    assert_eq!(p.status, 200, "{}", p.json);
    let keys = crypto::derive_password_keys(password, &kdf_of(&p.json["kdf"])).unwrap();
    s.call("POST", "/v1/auth/password/login", None, Some(json!({ "email": email, "authKey": b64(&keys.auth_key) })))
}

/// Sign-up + the mailed code: the session and the bundle key.
pub fn sign_up(s: &Server, email: &str, password: &str) -> (String, [u8; 32]) {
    let (kdf, keys) = new_password(password);
    let r = s.call("POST", "/v1/auth/password/signup", None, Some(json!({ "email": email, "kdf": kdf_json(&kdf), "authKey": b64(&keys.auth_key) })));
    assert_eq!(r.status, 202, "{}", r.json);
    let v = s.call("POST", "/v1/auth/password/verify", None, Some(json!({ "token": s.mailed_token(email) })));
    assert_eq!(v.status, 200, "{}", v.json);
    (v.json["session"].as_str().unwrap().to_string(), keys.bundle_key)
}
