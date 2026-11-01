#![allow(dead_code)]
//! A minimal OpenID Connect provider for tests that need tokens Keycloak won't mint (wrong
//! audience, expired, bad signatures): discovery, JWKS and a token endpoint on 127.0.0.1, signing
//! with P-256 keys generated when the test runs (no private key in the repository).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use hitop_sync_server::oidc::OidcConfig;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const CLIENT_ID: &str = "hitop-sync";
pub const CLIENT_SECRET: &str = "mock-client-secret";
pub const GOOD_CODE: &str = "good-code";

pub struct Key {
    pub kid: String,
    pub pkcs8: Vec<u8>,
    pub public: Vec<u8>,
}

impl Key {
    pub fn generate(kid: &str) -> Key {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap().as_ref().to_vec();
        let public = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, &rng).unwrap().public_key().as_ref().to_vec();
        Key { kid: kid.into(), pkcs8, public }
    }

    fn jwk(&self) -> Value {
        // Uncompressed point: 0x04 || x || y.
        json!({ "kty": "EC", "crv": "P-256", "alg": "ES256", "use": "sig", "kid": self.kid, "x": URL_SAFE_NO_PAD.encode(&self.public[1..33]), "y": URL_SAFE_NO_PAD.encode(&self.public[33..65]) })
    }
}

#[derive(Default)]
pub struct Inner {
    /// Published in the JWKS.
    pub keys: Vec<Key>,
    /// The issuer the discovery document claims (default: the real one).
    pub claimed_issuer: Option<String>,
    /// What the token endpoint answers with next.
    pub id_token: String,
    /// PKCE: the `code_challenge` the authorization URL carried.
    pub challenge: String,
    pub jwks_fetches: u32,
    pub token_requests: u32,
}

#[derive(Clone)]
pub struct MockIdp {
    pub issuer: String,
    /// Added to the clock for `claims` (move it with the server's, `Server::advance`).
    pub offset: Arc<std::sync::atomic::AtomicI64>,
    pub inner: Arc<Mutex<Inner>>,
}

impl MockIdp {
    pub fn start() -> MockIdp {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let issuer = format!("http://127.0.0.1:{}/realm", listener.local_addr().unwrap().port());
        let idp = MockIdp { issuer, offset: Arc::default(), inner: Arc::new(Mutex::new(Inner { keys: vec![Key::generate("k1")], ..Default::default() })) };
        let me = idp.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let mut headers = Vec::new();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    headers.push(h.trim().to_string());
                }
                let len = headers.iter().find_map(|h| h.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                let mut body = vec![0; len];
                let _ = reader.read_exact(&mut body);
                let (status, answer) = me.answer(&path, &headers, &String::from_utf8_lossy(&body));
                let text = answer.to_string();
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        idp
    }

    fn answer(&self, path: &str, headers: &[String], body: &str) -> (u16, Value) {
        let mut inner = self.inner.lock().unwrap();
        match path {
            "/realm/.well-known/openid-configuration" => (
                200,
                json!({
                    "issuer": inner.claimed_issuer.clone().unwrap_or_else(|| self.issuer.clone()),
                    "authorization_endpoint": format!("{}/auth", self.issuer),
                    "token_endpoint": format!("{}/token", self.issuer),
                    "jwks_uri": format!("{}/jwks", self.issuer),
                }),
            ),
            "/realm/jwks" => {
                inner.jwks_fetches += 1;
                (200, json!({ "keys": inner.keys.iter().map(Key::jwk).collect::<Vec<_>>() }))
            }
            "/realm/token" => {
                inner.token_requests += 1;
                let form: std::collections::HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes()).into_owned().collect();
                let basic = format!("Basic {}", STANDARD.encode(format!("{CLIENT_ID}:{CLIENT_SECRET}")));
                if !headers.iter().any(|h| h.eq_ignore_ascii_case(&format!("authorization: {basic}"))) {
                    return (401, json!({ "error": "invalid_client" }));
                }
                let pkce = URL_SAFE_NO_PAD.encode(Sha256::digest(form.get("code_verifier").map(String::as_bytes).unwrap_or_default()));
                if form.get("grant_type").map(String::as_str) != Some("authorization_code") || form.get("code").map(String::as_str) != Some(GOOD_CODE) || pkce != inner.challenge {
                    return (400, json!({ "error": "invalid_grant" }));
                }
                (200, json!({ "access_token": "opaque", "token_type": "Bearer", "id_token": inner.id_token }))
            }
            _ => (404, json!({ "error": "not_found" })),
        }
    }

    pub fn config(&self) -> OidcConfig {
        OidcConfig {
            issuer: self.issuer.clone(),
            client_id: CLIENT_ID.into(),
            client_secret: CLIENT_SECRET.into(),
            scopes: "openid email profile".into(),
            name: "Mock".into(),
            allowed_domains: vec![],
            groups_claim: "groups".into(),
            required_groups: vec![],
            sso_required: false,
            assume_verified: false,
        }
    }

    /// Standard claims for a sign-in with `nonce`.
    pub fn claims(&self, sub: &str, email: &str, nonce: &str) -> Value {
        let now = hitop_sync_server::api::now_secs() + self.offset.load(std::sync::atomic::Ordering::Relaxed);
        json!({ "iss": self.issuer, "aud": CLIENT_ID, "sub": sub, "email": email, "email_verified": true, "iat": now, "exp": now + 300, "nonce": nonce })
    }

    pub fn advance(&self, secs: i64) {
        self.offset.fetch_add(secs, std::sync::atomic::Ordering::Relaxed);
    }

    /// Signs with the newest published key (ES256, its kid).
    pub fn sign(&self, claims: &Value) -> String {
        let inner = self.inner.lock().unwrap();
        let key = inner.keys.last().unwrap();
        sign_with(&key.kid, &key.pkcs8, claims)
    }

    /// Adds a key and signs with it from now on (the old one stays published).
    pub fn rotate(&self, kid: &str) {
        self.inner.lock().unwrap().keys.push(Key::generate(kid));
    }

    pub fn jwks_fetches(&self) -> u32 {
        self.inner.lock().unwrap().jwks_fetches
    }

    /// What the next token request gets, and the PKCE challenge it must match.
    pub fn next(&self, id_token: String, challenge: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.id_token = id_token;
        inner.challenge = challenge.to_string();
    }
}

pub fn sign_with(kid: &str, pkcs8: &[u8], claims: &Value) -> String {
    let header = Header { kid: Some(kid.into()), ..Header::new(Algorithm::ES256) };
    jsonwebtoken::encode(&header, claims, &EncodingKey::from_ec_der(pkcs8)).unwrap()
}
