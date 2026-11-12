//! "Continue with Apple": Sign in with Apple, an OpenID Connect provider with three differences
//! from Google (oidc.rs does the rest: flows, ID token checks, linking, the one-time code):
//!
//! 1. The client secret is a short-lived ES256 JWT the server signs with its Sign in with Apple
//!    key (`iss` = team id, `sub` = Services ID, `aud` = Apple), sent in the token request body
//!    (`client_secret_post`, the only method Apple supports). No PKCE: Apple doesn't support it;
//!    `state`, `nonce` and the client secret protect the exchange.
//! 2. Asking for the address makes Apple POST the callback (`response_mode=form_post`), so
//!    `POST /v1/auth/apple/callback` takes a form body.
//! 3. Users may hide their address: Apple then gives a verified `@privaterelay.appleid.com`
//!    address, which becomes a new account (it matches no existing one). Mail to it is relayed
//!    only from senders registered with Apple.
//!
//! Apple's server-to-server notifications (`POST /v1/auth/apple/notifications {payload}`, a JWT
//! signed with Apple's keys): when someone stops using Sign in with Apple with HITOP
//! (`consent-revoked`) or deletes their Apple account (`account-delete`), the Apple identity is
//! unlinked from the HITOP account; the account itself stays (it may have other sign-ins).
//!
//! Settings (environment), all or none: `HITOP_SYNC_APPLE_CLIENT_ID` (the Services ID),
//! `HITOP_SYNC_APPLE_TEAM_ID`, `HITOP_SYNC_APPLE_KEY_ID`, and `HITOP_SYNC_APPLE_PRIVATE_KEY_FILE`
//! (the `.p8` file; or the PEM itself in `HITOP_SYNC_APPLE_PRIVATE_KEY`). Optional:
//! `HITOP_SYNC_APPLE_APP_IDS` (comma separated App IDs, accepted as the audience of notifications
//! besides the Services ID), `HITOP_SYNC_APPLE_ISSUER` (a test double). The Services ID's return
//! URL is `<HITOP_SYNC_PUBLIC_URL>/v1/auth/apple/callback`.

use std::sync::Mutex;

use jsonwebtoken::{Algorithm, EncodingKey, Header, Validation};
use rust_web_server::request::Request;
use rust_web_server::response::Response;
use rust_web_server::router::PathParams;
use serde_json::{json, Value as Json};

use crate::api::{body, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::oidc::OidcConfig;

pub const APPLE_ISSUER: &str = "https://appleid.apple.com";
/// A client secret lives this long; a new one is signed when less than a quarter is left.
const SECRET_TTL: i64 = 3600;
/// The largest callback form accepted (Apple's is a few kilobytes).
const MAX_CALLBACK_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct AppleConfig {
    /// The OpenID Connect side: issuer, the Services ID as client id, scopes. Its
    /// `client_secret` is unused (see [`ClientSecret`]).
    pub oidc: OidcConfig,
    pub team_id: String,
    pub key_id: String,
    /// The `.p8` key (PKCS#8 PEM, P-256).
    pub private_key_pem: String,
    /// App IDs whose notifications are accepted besides the Services ID's.
    pub app_ids: Vec<String>,
}

impl std::fmt::Debug for AppleConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never prints the key.
        f.debug_struct("AppleConfig").field("client_id", &self.oidc.client_id).field("team_id", &self.team_id).field("key_id", &self.key_id).finish_non_exhaustive()
    }
}

impl AppleConfig {
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<Option<AppleConfig>, String> {
        let key = match (var("HITOP_SYNC_APPLE_PRIVATE_KEY"), var("HITOP_SYNC_APPLE_PRIVATE_KEY_FILE")) {
            (Some(k), _) => Some(k),
            (None, Some(path)) => Some(std::fs::read_to_string(&path).map_err(|e| format!("HITOP_SYNC_APPLE_PRIVATE_KEY_FILE {path}: {e}"))?),
            (None, None) => None,
        };
        let parts = (var("HITOP_SYNC_APPLE_CLIENT_ID"), var("HITOP_SYNC_APPLE_TEAM_ID"), var("HITOP_SYNC_APPLE_KEY_ID"), key);
        let (client_id, team_id, key_id, private_key_pem) = match parts {
            (None, None, None, None) => return Ok(None),
            (Some(c), Some(t), Some(k), Some(p)) if ![&c, &t, &k, &p].iter().any(|s| s.trim().is_empty()) => (c.trim().to_string(), t.trim().to_string(), k.trim().to_string(), p),
            _ => return Err("Sign in with Apple needs HITOP_SYNC_APPLE_CLIENT_ID, HITOP_SYNC_APPLE_TEAM_ID, HITOP_SYNC_APPLE_KEY_ID and HITOP_SYNC_APPLE_PRIVATE_KEY_FILE (or _PRIVATE_KEY) together".into()),
        };
        let issuer = var("HITOP_SYNC_APPLE_ISSUER").unwrap_or_else(|| APPLE_ISSUER.into());
        let app_ids = var("HITOP_SYNC_APPLE_APP_IDS").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
        let c = AppleConfig::new(&issuer, &client_id, &team_id, &key_id, &private_key_pem, app_ids);
        c.check()?;
        Ok(Some(c))
    }

    pub fn new(issuer: &str, client_id: &str, team_id: &str, key_id: &str, private_key_pem: &str, app_ids: Vec<String>) -> AppleConfig {
        AppleConfig {
            oidc: OidcConfig {
                issuer: issuer.trim_end_matches('/').to_string(),
                client_id: client_id.into(),
                client_secret: String::new(),
                scopes: "openid email".into(),
                name: "Apple".into(),
                allowed_domains: vec![],
                groups_claim: "groups".into(),
                required_groups: vec![],
                sso_required: false,
                assume_verified: false,
            },
            team_id: team_id.into(),
            key_id: key_id.into(),
            private_key_pem: private_key_pem.into(),
            app_ids,
        }
    }

    /// The issuer is https (or a local test double) and the key signs.
    pub fn check(&self) -> Result<(), String> {
        self.oidc.check()?;
        ClientSecret::new(self)?.get(&self.oidc, 0).map(|_| ())
    }
}

/// Signs and caches the client secret JWT.
pub struct ClientSecret {
    team_id: String,
    key_id: String,
    key: EncodingKey,
    cached: Mutex<Option<(String, i64)>>,
}

impl ClientSecret {
    pub fn new(c: &AppleConfig) -> Result<ClientSecret, String> {
        let key = EncodingKey::from_ec_pem(c.private_key_pem.trim().as_bytes()).map_err(|e| format!("HITOP_SYNC_APPLE_PRIVATE_KEY: not a P-256 PKCS#8 PEM key ({e})"))?;
        Ok(ClientSecret { team_id: c.team_id.clone(), key_id: c.key_id.clone(), key, cached: Mutex::new(None) })
    }

    /// A client secret valid for at least another quarter of its life at `now`.
    pub fn get(&self, c: &OidcConfig, now: i64) -> Result<String, String> {
        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((secret, _)) = cached.as_ref().filter(|(_, exp)| exp - now > SECRET_TTL / 4) {
            return Ok(secret.clone());
        }
        let exp = now + SECRET_TTL;
        let header = Header { kid: Some(self.key_id.clone()), ..Header::new(Algorithm::ES256) };
        let claims = json!({ "iss": self.team_id, "iat": now, "exp": exp, "aud": c.issuer, "sub": c.client_id });
        let secret = jsonwebtoken::encode(&header, &claims, &self.key).map_err(|e| format!("signing the Sign in with Apple client secret: {e}"))?;
        *cached = Some((secret.clone(), exp));
        Ok(secret)
    }
}

/// `POST /v1/auth/apple/start`: as Google's.
pub fn start(state: &State, request: &Request, p: &PathParams) -> ApiResult {
    crate::oidc::apple_start(state, request, p)
}

/// `POST /v1/auth/apple/finish {code, verifier}`.
pub fn finish(state: &State, request: &Request, p: &PathParams) -> ApiResult {
    crate::oidc::apple_finish(state, request, p)
}

/// `POST /v1/auth/apple/callback` (the browser, from Apple, a form: `code`, `state`, `id_token`,
/// `user` the first time, or `error`). The ID token is taken from the token request, not the
/// form; the name in `user` isn't kept.
pub fn callback(state: &State, request: &Request) -> Response {
    if request.body.len() > MAX_CALLBACK_BYTES {
        return crate::oidc::failed_page("the sign-in answer is too large");
    }
    let form = url::form_urlencoded::parse(&request.body).into_owned().collect();
    crate::oidc::callback_query(state, &state.apple, form)
}

/// `POST /v1/auth/apple/notifications {payload}`: Apple's server-to-server notifications.
pub fn notifications(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let Some(c) = state.config.apple.as_ref() else {
        return Err(ApiError::new(404, "provider_off", "sign-in with Apple isn't configured on this server"));
    };
    let input: Json = body(request)?;
    let token = input["payload"].as_str().ok_or_else(|| ApiError::bad("payload (a JWT) is required"))?;
    let claims = verify(state, c, token).map_err(|e| {
        eprintln!("hitop-sync-server: Apple notification refused: {e}");
        ApiError::new(401, "bad_payload", "the notification isn't signed by Apple for this app")
    })?;
    // `events` is a JSON object, as a string.
    let events: Json = match &claims["events"] {
        Json::String(s) => serde_json::from_str(s).map_err(|_| ApiError::bad("events isn't JSON"))?,
        v @ Json::Object(_) => v.clone(),
        _ => return Err(ApiError::bad("the notification has no events")),
    };
    let kind = events["type"].as_str().unwrap_or("");
    let subject = events["sub"].as_str().unwrap_or("");
    if subject.is_empty() {
        return Err(ApiError::bad("the notification has no subject"));
    }
    let issuer = c.oidc.issuer.clone();
    state.db.tx(|tx| -> Result<(), ApiError> {
        let Some(r) = tx.query_one("SELECT user_id, email FROM oidc_identities WHERE issuer = $1 AND subject = $2", &[(&issuer).into(), subject.into()])? else {
            return Ok(());
        };
        let (user_id, email) = (r.text(0), r.text(1));
        match kind {
            "consent-revoked" | "account-delete" => {
                tx.execute("DELETE FROM oidc_identities WHERE issuer = $1 AND subject = $2", &[(&issuer).into(), subject.into()])?;
                audit::record(tx, state, Event::new(audit::ACCOUNT, "sso_unlinked", None).email(&email).target(&user_id).detail(json!({ "issuer": issuer, "subject": subject, "reason": kind })))?;
            }
            "email-disabled" | "email-enabled" => {
                audit::record(tx, state, Event::new(audit::ACCOUNT, "email_forwarding_changed", None).email(&email).target(&user_id).detail(json!({ "issuer": issuer, "forwarding": kind == "email-enabled" })))?;
            }
            _ => {}
        }
        Ok(())
    })?;
    Ok((200, json!({})))
}

/// A notification JWT: signed with a key from Apple's JWKS, issued by Apple, for the Services ID
/// or one of the App IDs, not from the future.
fn verify(state: &State, c: &AppleConfig, token: &str) -> Result<Json, String> {
    let now = state.now();
    let header = jsonwebtoken::decode_header(token).map_err(|e| format!("malformed: {e}"))?;
    if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
        return Err(format!("alg {:?}", header.alg));
    }
    let key = state.apple.decoding_key(&c.oidc, header.kid.as_deref(), now)?;
    let mut v = Validation::new(header.alg);
    v.set_issuer(&[&c.oidc.issuer]);
    let mut audiences = vec![c.oidc.client_id.as_str()];
    audiences.extend(c.app_ids.iter().map(String::as_str));
    v.set_audience(&audiences);
    v.set_required_spec_claims(&["iss", "aud"]);
    v.validate_exp = false;
    v.validate_nbf = false;
    let claims = jsonwebtoken::decode::<Json>(token, &key, &v).map_err(|e| e.to_string())?.claims;
    match &claims["iat"] {
        Json::Null => {}
        t => {
            let iat = t.as_i64().ok_or("iat isn't a number")?;
            if iat > now + 60 {
                return Err("issued in the future".into());
            }
        }
    }
    if claims["exp"].as_i64().is_some_and(|exp| exp + 60 < now) {
        return Err("expired".into());
    }
    Ok(claims)
}
