//! Single sign-on with an OpenID Connect provider (Keycloak, Google Workspace, Entra ID, Okta, ...).
//!
//! The sync server is the OIDC client (a confidential client: it holds the client secret, the
//! hitop clients never see it) and uses the authorization code flow with PKCE:
//!
//! 1. A hitop client makes a random `verifier` and calls `POST /v1/auth/sso/start {challenge =
//!    SHA-256(verifier), returnTo?}` → the provider's authorization URL, which it opens in a
//!    browser. The server keeps `state`, `nonce` and its own PKCE verifier for 10 minutes.
//! 2. The provider sends the browser back to `<public URL>/v1/auth/sso/callback?code&state`. The
//!    server exchanges the code (client secret + PKCE), checks the ID token (signature from the
//!    provider's JWKS, `iss`, `aud`/`azp`, `exp`, `iat`, `nonce`), checks the address is verified,
//!    in an allowed domain and (optionally) in an allowed group, and finds or links the account.
//! 3. It hands the browser a one-time code: by redirecting to `returnTo` (a loopback address of
//!    the CLI, or an allowed web origin), or on a page to paste into hitop. The client trades
//!    `{code, verifier}` for a session at `POST /v1/auth/sso/finish`.
//!
//! The code only ever travels through the browser that signed in, and finishing also needs the
//! verifier that only the client that started holds: someone who starts a flow and sends the
//! provider link to a victim gets nothing back.
//!
//! Accounts: an identity (issuer, subject) is linked to one account. The first SSO sign-in links
//! to the existing account with the same address only when the provider says the address is
//! verified (`email_verified`); unverified addresses are refused, never linked. Providers that
//! don't send `email_verified` (Entra ID) need `HITOP_SYNC_OIDC_ASSUME_VERIFIED=1` together with
//! `HITOP_SYNC_OIDC_ALLOWED_DOMAINS` (domains the tenant owns).
//!
//! Settings (environment):
//!
//! - `HITOP_SYNC_OIDC_ISSUER`: the issuer URL (discovery at `<issuer>/.well-known/openid-configuration`)
//! - `HITOP_SYNC_OIDC_CLIENT_ID`, and `HITOP_SYNC_OIDC_CLIENT_SECRET` or
//!   `HITOP_SYNC_OIDC_CLIENT_SECRET_FILE` (a file holding it; keeps it out of the unit's env)
//! - `HITOP_SYNC_OIDC_ALLOWED_DOMAINS`: comma separated email domains that may sign in by SSO
//!   (default: any)
//! - `HITOP_SYNC_OIDC_REQUIRED_GROUPS`: comma separated; the user must be in one of them, read
//!   from the claim `HITOP_SYNC_OIDC_GROUPS_CLAIM` (default `groups`)
//! - `HITOP_SYNC_SSO_REQUIRED=1`: addresses in the allowed domains (all addresses when none are
//!   set) can sign in only by SSO: no sign-in mails, no passkey sign-in
//! - `HITOP_SYNC_OIDC_SCOPES` (default `openid email profile`), `HITOP_SYNC_OIDC_NAME` (the
//!   button label, default `SSO`), `HITOP_SYNC_OIDC_ASSUME_VERIFIED` (see above)

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rust_web_server::header::Header;
use rust_web_server::mime_type::MimeType;
use rust_web_server::range::Range;
use rust_web_server::request::Request;
use rust_web_server::response::{Response, STATUS_CODE_REASON_PHRASE};
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use url::Url;
use webauthn_rs::prelude::Uuid;

use crate::api::{body, new_token, query, token_hash, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::{new_session, normalize_email, User};
use crate::db::Tx;

const FLOW_TTL: i64 = 10 * 60;
const CODE_TTL: i64 = 5 * 60;
const MAX_FLOWS: usize = 10_000;
/// Clock skew allowed for `exp` and `iat`.
const LEEWAY: i64 = 60;
/// Provider metadata and keys are fetched again after this long.
const METADATA_TTL: i64 = 3600;
/// An unknown key id makes the server fetch the JWKS again (key rotation), at most this often.
const JWKS_REFETCH: i64 = 60;
const ALGORITHMS: &[Algorithm] = &[Algorithm::RS256, Algorithm::RS384, Algorithm::RS512, Algorithm::PS256, Algorithm::PS384, Algorithm::PS512, Algorithm::ES256, Algorithm::ES384, Algorithm::EdDSA];

#[derive(Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: String,
    pub name: String,
    pub allowed_domains: Vec<String>,
    pub groups_claim: String,
    pub required_groups: Vec<String>,
    pub sso_required: bool,
    pub assume_verified: bool,
}

impl std::fmt::Debug for OidcConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never prints the client secret.
        f.debug_struct("OidcConfig").field("issuer", &self.issuer).field("client_id", &self.client_id).field("allowed_domains", &self.allowed_domains).finish_non_exhaustive()
    }
}

fn list(value: &str) -> Vec<String> {
    value.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect()
}

fn flag(value: Option<String>) -> bool {
    matches!(value.as_deref(), Some("1" | "true" | "yes" | "on"))
}

/// http:// is accepted only for a provider on this machine (tests, a local Keycloak).
pub(crate) fn secure_url(u: &str) -> bool {
    match Url::parse(u) {
        Ok(url) => url.scheme() == "https" || (url.scheme() == "http" && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
        Err(_) => false,
    }
}

impl OidcConfig {
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<Option<OidcConfig>, String> {
        let issuer = var("HITOP_SYNC_OIDC_ISSUER");
        let client_id = var("HITOP_SYNC_OIDC_CLIENT_ID");
        let secret = match (var("HITOP_SYNC_OIDC_CLIENT_SECRET"), var("HITOP_SYNC_OIDC_CLIENT_SECRET_FILE")) {
            (Some(s), _) => Some(s),
            (None, Some(path)) => Some(std::fs::read_to_string(&path).map_err(|e| format!("HITOP_SYNC_OIDC_CLIENT_SECRET_FILE {path}: {e}"))?.trim().to_string()),
            (None, None) => None,
        };
        let (issuer, client_id, client_secret) = match (issuer, client_id, secret) {
            (None, None, None) => return Ok(None),
            (Some(i), Some(c), Some(s)) if !s.is_empty() => (i, c, s),
            _ => return Err("SSO needs HITOP_SYNC_OIDC_ISSUER, HITOP_SYNC_OIDC_CLIENT_ID and HITOP_SYNC_OIDC_CLIENT_SECRET (or _FILE) together".into()),
        };
        let c = OidcConfig {
            issuer: issuer.trim_end_matches('/').to_string(),
            client_id,
            client_secret,
            scopes: var("HITOP_SYNC_OIDC_SCOPES").unwrap_or_else(|| "openid email profile".into()),
            name: var("HITOP_SYNC_OIDC_NAME").unwrap_or_else(|| "SSO".into()),
            allowed_domains: var("HITOP_SYNC_OIDC_ALLOWED_DOMAINS").map(|v| list(&v)).unwrap_or_default(),
            groups_claim: var("HITOP_SYNC_OIDC_GROUPS_CLAIM").unwrap_or_else(|| "groups".into()),
            required_groups: var("HITOP_SYNC_OIDC_REQUIRED_GROUPS").map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()).unwrap_or_default(),
            sso_required: flag(var("HITOP_SYNC_SSO_REQUIRED")),
            assume_verified: flag(var("HITOP_SYNC_OIDC_ASSUME_VERIFIED")),
        };
        c.check()?;
        Ok(Some(c))
    }

    pub fn check(&self) -> Result<(), String> {
        if !secure_url(&self.issuer) {
            return Err(format!("HITOP_SYNC_OIDC_ISSUER must be an https:// URL (http:// only on localhost): {}", self.issuer));
        }
        if !self.scopes.split_whitespace().any(|s| s == "openid") {
            return Err("HITOP_SYNC_OIDC_SCOPES must include openid".into());
        }
        if self.assume_verified && self.allowed_domains.is_empty() {
            return Err("HITOP_SYNC_OIDC_ASSUME_VERIFIED needs HITOP_SYNC_OIDC_ALLOWED_DOMAINS (domains your provider owns)".into());
        }
        Ok(())
    }

    fn domain_allowed(&self, email: &str) -> bool {
        let domain = email.rsplit('@').next().unwrap_or("");
        self.allowed_domains.is_empty() || self.allowed_domains.iter().any(|d| d == domain)
    }
}

/// Google's issuer, and the other spelling its ID tokens may carry in `iss`.
pub const GOOGLE_ISSUER: &str = "https://accounts.google.com";
pub const GOOGLE_ISSUER_ALT: &str = "accounts.google.com";

/// "Continue with Google": `HITOP_SYNC_GOOGLE_CLIENT_ID` and `HITOP_SYNC_GOOGLE_CLIENT_SECRET`
/// (or `_FILE`); `HITOP_SYNC_GOOGLE_ISSUER` points at a test double. Any Google account with a
/// verified address may sign in (or sign up); linking to an existing account needs Google to
/// say the address is verified, as for SSO.
pub fn google_from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<Option<OidcConfig>, String> {
    let Some((client_id, client_secret)) = crate::config::client_pair(var, "HITOP_SYNC_GOOGLE")? else {
        return Ok(None);
    };
    let c = OidcConfig {
        issuer: var("HITOP_SYNC_GOOGLE_ISSUER").unwrap_or_else(|| GOOGLE_ISSUER.into()).trim_end_matches('/').to_string(),
        client_id,
        client_secret,
        scopes: "openid email".into(),
        name: "Google".into(),
        allowed_domains: vec![],
        groups_claim: "groups".into(),
        required_groups: vec![],
        sso_required: false,
        assume_verified: false,
    };
    c.check()?;
    Ok(Some(c))
}

/// Is `email` limited to SSO sign-in: the server's (`HITOP_SYNC_SSO_REQUIRED`), or its domain is
/// verified by an org with "SSO required" (orgsso.rs)?
pub fn sso_required_for(state: &State, email: &str) -> bool {
    state.config.oidc.as_ref().is_some_and(|c| c.sso_required && c.domain_allowed(email)) || org_required_for(state, email).is_some()
}

/// The org whose "SSO required" covers `email`'s domain, if any.
pub fn org_required_for(state: &State, email: &str) -> Option<String> {
    state.org_sso.for_email(email).filter(|p| p.required).map(|p| p.org)
}

#[derive(Clone, Debug)]
struct Provider {
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    fetched_at: i64,
}

struct Keys {
    set: JwkSet,
    fetched_at: i64,
}

/// A flow between start and callback.
pub(crate) struct Flow {
    pub(crate) expires: i64,
    pub(crate) nonce: String,
    pub(crate) pkce_verifier: String,
    pub(crate) client_challenge: String,
    pub(crate) return_to: Option<Url>,
    /// The org whose provider this sign-in uses (orgsso.rs); `None` = the server's own.
    pub(crate) org: Option<String>,
}

/// A signed-in browser's one-time code, between callback and finish.
pub(crate) struct Handoff {
    expires: i64,
    user_id: String,
    email: String,
    client_challenge: String,
}

/// Flows between start and callback, and one-time codes between callback and finish, of one
/// sign-in provider (SSO, Google, GitHub).
#[derive(Default)]
pub(crate) struct Flows {
    flows: Mutex<HashMap<String, Flow>>,
    handoffs: Mutex<HashMap<String, Handoff>>,
}

impl Flows {
    pub(crate) fn remember_flow(&self, state_token: &str, flow: Flow, now: i64) {
        let mut flows = self.flows.lock().unwrap_or_else(|e| e.into_inner());
        flows.retain(|_, f| f.expires > now);
        if flows.len() >= MAX_FLOWS {
            if let Some(oldest) = flows.iter().min_by_key(|(_, f)| f.expires).map(|(k, _)| k.clone()) {
                flows.remove(&oldest);
            }
        }
        flows.insert(token_hash(state_token), flow);
    }

    pub(crate) fn take_flow(&self, state_token: &str, now: i64) -> Option<Flow> {
        self.flows.lock().unwrap_or_else(|e| e.into_inner()).remove(&token_hash(state_token)).filter(|f| f.expires > now)
    }

    fn remember_code(&self, code: &str, h: Handoff, now: i64) {
        let mut map = self.handoffs.lock().unwrap_or_else(|e| e.into_inner());
        map.retain(|_, x| x.expires > now);
        if map.len() >= MAX_FLOWS {
            if let Some(oldest) = map.iter().min_by_key(|(_, x)| x.expires).map(|(k, _)| k.clone()) {
                map.remove(&oldest);
            }
        }
        map.insert(token_hash(code), h);
    }

    fn take_code(&self, code: &str, now: i64) -> Option<Handoff> {
        self.handoffs.lock().unwrap_or_else(|e| e.into_inner()).remove(&token_hash(code)).filter(|h| h.expires > now)
    }
}

/// The HTTP client for identity providers: 10 s, no redirects, statuses as answers.
pub(crate) fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .max_redirects(0)
        .user_agent(concat!("hitop-sync-server/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// One OpenID Connect provider: the server's SSO (`kind` "sso") or "Continue with Google"
/// ("google"). The kind names its routes (`/v1/auth/<kind>/...`) and the sign-in method in the
/// audit log.
pub struct Sso {
    pub kind: &'static str,
    pub config: Option<OidcConfig>,
    /// Other spellings of the issuer the provider puts in `iss` (Google: `accounts.google.com`).
    pub extra_issuers: Vec<String>,
    http: ureq::Agent,
    provider: Mutex<Option<Provider>>,
    keys: Mutex<Option<Keys>>,
    last_jwks_fetch: Mutex<i64>,
    pub(crate) flows: Flows,
    /// Sign in with Apple (apple.rs): the client secret is a JWT signed here, sent in the token
    /// request body; no PKCE; the callback is a form POST.
    pub(crate) minted_secret: Option<crate::apple::ClientSecret>,
    /// How many times the JWKS was fetched (tests check the cache).
    pub jwks_fetches: std::sync::atomic::AtomicU64,
}

impl Sso {
    pub fn new(config: Option<OidcConfig>) -> Result<Sso, String> {
        Self::provider_of("sso", config, vec![])
    }

    pub fn provider_of(kind: &'static str, config: Option<OidcConfig>, extra_issuers: Vec<String>) -> Result<Sso, String> {
        if let Some(c) = &config {
            c.check()?;
        }
        Ok(Sso {
            kind,
            config,
            extra_issuers,
            http: http_agent(),
            provider: Mutex::new(None),
            keys: Mutex::new(None),
            last_jwks_fetch: Mutex::new(i64::MIN / 2),
            flows: Flows::default(),
            minted_secret: None,
            jwks_fetches: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// "Continue with Apple" (`kind` "apple"), or an unconfigured one.
    pub fn apple(config: Option<&crate::apple::AppleConfig>) -> Result<Sso, String> {
        let mut sso = Self::provider_of("apple", config.map(|c| c.oidc.clone()), vec![])?;
        sso.minted_secret = config.map(crate::apple::ClientSecret::new).transpose()?;
        Ok(sso)
    }

    /// A key of the provider's JWKS (Apple's notifications are signed with them too).
    pub(crate) fn decoding_key(&self, config: &OidcConfig, kid: Option<&str>, now: i64) -> Result<DecodingKey, String> {
        let p = self.provider(config, now)?;
        self.key(&p, kid, now)
    }

    fn get_json(&self, url: &str) -> Result<Json, String> {
        let mut res = self.http.get(url).header("Accept", "application/json").call().map_err(|e| format!("{url}: {e}"))?;
        let status = res.status().as_u16();
        let text = res.body_mut().with_config().limit(1 << 20).read_to_string().map_err(|e| format!("{url}: {e}"))?;
        if status != 200 {
            return Err(format!("{url}: HTTP {status}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("{url}: not JSON ({e})"))
    }

    /// The provider's endpoints, from discovery (cached for an hour). The discovery document's
    /// `issuer` must be exactly the configured issuer (OpenID Connect Discovery 1.0, section 4.3).
    fn provider(&self, config: &OidcConfig, now: i64) -> Result<Provider, String> {
        if let Some(p) = self.provider.lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|p| now - p.fetched_at < METADATA_TTL) {
            return Ok(p.clone());
        }
        let doc = self.get_json(&format!("{}/.well-known/openid-configuration", config.issuer))?;
        let field = |name: &str| -> Result<String, String> {
            let v = doc[name].as_str().ok_or_else(|| format!("discovery has no {name}"))?.to_string();
            if name != "issuer" && !secure_url(&v) {
                return Err(format!("discovery {name} is not https: {v}"));
            }
            Ok(v)
        };
        let issuer = field("issuer")?;
        if issuer.trim_end_matches('/') != config.issuer {
            return Err(format!("the provider says its issuer is {issuer}, but HITOP_SYNC_OIDC_ISSUER is {}", config.issuer));
        }
        let p = Provider { authorization_endpoint: field("authorization_endpoint")?, token_endpoint: field("token_endpoint")?, jwks_uri: field("jwks_uri")?, fetched_at: now };
        *self.provider.lock().unwrap_or_else(|e| e.into_inner()) = Some(p.clone());
        Ok(p)
    }

    fn fetch_keys(&self, provider: &Provider, now: i64) -> Result<(), String> {
        *self.last_jwks_fetch.lock().unwrap_or_else(|e| e.into_inner()) = now;
        self.jwks_fetches.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let doc = self.get_json(&provider.jwks_uri)?;
        let set: JwkSet = serde_json::from_value(doc).map_err(|e| format!("JWKS: {e}"))?;
        *self.keys.lock().unwrap_or_else(|e| e.into_inner()) = Some(Keys { set, fetched_at: now });
        Ok(())
    }

    /// The key for `kid`: from the cache, or fetched again when the cache is old or doesn't know
    /// the key id (the provider rotated its keys), at most once a minute.
    fn key(&self, provider: &Provider, kid: Option<&str>, now: i64) -> Result<DecodingKey, String> {
        let lookup = |keys: &Keys| -> Option<Result<DecodingKey, String>> {
            let jwk = match kid {
                Some(k) => keys.set.find(k),
                None if keys.set.keys.len() == 1 => keys.set.keys.first(),
                None => None,
            }?;
            Some(DecodingKey::from_jwk(jwk).map_err(|e| format!("JWKS key: {e}")))
        };
        {
            let keys = self.keys.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(k) = keys.as_ref().filter(|k| now - k.fetched_at < METADATA_TTL).and_then(lookup) {
                return k;
            }
        }
        let last = *self.last_jwks_fetch.lock().unwrap_or_else(|e| e.into_inner());
        let stale = self.keys.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_none_or(|k| now - k.fetched_at >= METADATA_TTL);
        if stale || now - last >= JWKS_REFETCH {
            self.fetch_keys(provider, now)?;
        }
        let keys = self.keys.lock().unwrap_or_else(|e| e.into_inner());
        keys.as_ref().and_then(lookup).unwrap_or_else(|| Err(format!("no key {} in the provider's JWKS", kid.unwrap_or("(none)"))))
    }

    /// Fetches the discovery document and the keys (a configuration check).
    pub fn check_provider(&self, config: &OidcConfig, now: i64) -> Result<(), String> {
        let p = self.provider(config, now)?;
        self.fetch_keys(&p, now)
    }
}

fn config(sso: &Sso) -> Result<&OidcConfig, ApiError> {
    sso.config.as_ref().ok_or_else(|| match sso.kind {
        "sso" => ApiError::new(404, "sso_off", "single sign-on isn't configured on this server"),
        k => ApiError::new(404, "provider_off", format!("sign-in with {} isn't configured on this server", provider_name(k))),
    })
}

pub(crate) fn provider_name(kind: &str) -> &'static str {
    match kind {
        "google" => "Google",
        "github" => "GitHub",
        "apple" => "Apple",
        _ => "single sign-on",
    }
}

fn config_error(state: &State, message: String) -> ApiError {
    eprintln!("hitop-sync-server: SSO: {message}");
    audit::record_alone(state, Event::new(audit::ACCOUNT, "sso_config_error", None).detail(json!({ "error": message.chars().take(300).collect::<String>() })));
    ApiError::new(503, "sso_unavailable", "single sign-on is misconfigured or the identity provider is unreachable; the server log has details")
}

/// `GET /v1/auth/sso[?email=]` → `{enabled, name, required, domains}`: whether to show "Sign in
/// with SSO". With `email` whose domain an org with SSO verified: that org's (`org`, its name and
/// "required"). `orgSso: true` when orgs sign in by SSO here (clients then ask for the address).
pub fn info(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let q = query(request);
    if let Some(p) = q.get("email").map(|e| e.trim().to_lowercase()).and_then(|e| state.org_sso.for_email(&e)).filter(|p| p.sso) {
        let domain = q.get("email").map(|e| crate::orgsso::domain_of(e)).unwrap_or_default();
        return Ok((200, json!({ "enabled": true, "name": p.name, "required": p.required, "domains": [domain], "org": p.org })));
    }
    let orgs = state.org_sso.any();
    let mut out = match &state.config.oidc {
        None if orgs => json!({ "enabled": true, "name": "SSO", "required": false, "domains": [] }),
        None => json!({ "enabled": false }),
        Some(c) => json!({ "enabled": true, "name": c.name, "required": c.sso_required, "domains": c.allowed_domains }),
    };
    if orgs {
        out["orgSso"] = json!(true);
    }
    Ok((200, out))
}

/// Where the browser may be sent with the one-time code: the CLI's loopback listener
/// (`http://127.0.0.1:<port>/...`, `http://localhost:<port>/...`) or a page of an allowed web
/// origin (`HITOP_SYNC_ALLOWED_ORIGINS`, the web app).
pub(crate) fn return_to(state: &State, value: &str) -> Result<Url, ApiError> {
    let bad = || ApiError::bad("returnTo must be http://127.0.0.1:<port>/... or a page of an allowed web origin");
    let url = Url::parse(value).map_err(|_| bad())?;
    if url.fragment().is_some() || !url.username().is_empty() || url.password().is_some() {
        return Err(bad());
    }
    let loopback = url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")) && url.port().is_some();
    let origin = url.origin().ascii_serialization();
    if loopback || state.config.cors_origins.contains(&origin) {
        Ok(url)
    } else {
        Err(bad())
    }
}

fn is_loopback(url: &Url) -> bool {
    url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Start {
    /// base64url(SHA-256(verifier)), the verifier staying with the client.
    challenge: String,
    #[serde(default)]
    return_to: Option<String>,
    /// A hint for the provider's sign-in page (and checked against the allowed domains early).
    #[serde(default)]
    email: Option<String>,
}

/// `POST /v1/auth/sso/start {challenge, returnTo?, email?, org?}` → `{url, expiresAt}`: open `url`
/// in a browser. The provider: `org`'s, else the org that verified the `email` hint's domain, else
/// the server's own (orgsso.rs).
pub fn start(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let raw: Json = serde_json::from_slice(&request.body).unwrap_or(Json::Null);
    let named = raw["org"].as_str().map(str::to_string);
    let hinted = raw["email"].as_str().map(|e| e.trim().to_lowercase()).and_then(|e| state.org_sso.for_email(&e)).filter(|p| p.sso).map(|p| p.org);
    let org = match (named, hinted) {
        (Some(n), _) => Some(n),
        (None, Some(h)) => Some(h),
        (None, None) if state.config.oidc.is_none() && state.org_sso.any() => {
            return Err(ApiError::bad("enter your work address (hitop sync login --sso --email <address>) so the server finds your organization's single sign-on"));
        }
        (None, None) => None,
    };
    match org {
        None => start_with(state, &state.sso, None, request),
        Some(org) => {
            if state.config.host_key.is_none() {
                return Err(ApiError::new(404, "org_sso_off", "single sign-on per organization isn't available on this server"));
            }
            let sso = crate::orgsso::provider(state, &org).map_err(|e| ApiError::new(404, "sso_off", format!("no single sign-on for this organization: {e}")))?;
            start_with(state, &sso, Some(org), request)
        }
    }
}

/// `POST /v1/auth/google/start`: as [`start`], with Google.
pub fn google_start(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    start_with(state, &state.google, None, request)
}

/// `POST /v1/auth/apple/start`: as [`start`], with Apple.
pub fn apple_start(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    start_with(state, &state.apple, None, request)
}

/// The start of a browser sign-in: the client's challenge (32 bytes, base64url), where to send the
/// code, and an address hint.
pub(crate) fn start_input(state: &State, request: &Request) -> Result<(String, Option<Url>, Option<String>), ApiError> {
    let mut raw: Json = body(request)?;
    // `org` is read by `start` (SSO only); the rest is the same for every provider.
    if let Some(o) = raw.as_object_mut() {
        o.remove("org");
    }
    let input: Start = serde_json::from_value(raw).map_err(|e| ApiError::bad(format!("invalid JSON body: {e}")))?;
    let challenge = URL_SAFE_NO_PAD.decode(input.challenge.trim_end_matches('=')).ok().filter(|b| b.len() == 32).ok_or_else(|| ApiError::bad("challenge must be base64url(SHA-256(verifier)), 32 bytes"))?;
    let return_to = input.return_to.as_deref().filter(|s| !s.is_empty()).map(|r| return_to(state, r)).transpose()?;
    let hint = input.email.as_deref().map(str::trim).filter(|e| !e.is_empty()).map(normalize_email).transpose()?;
    Ok((URL_SAFE_NO_PAD.encode(challenge), return_to, hint))
}

fn start_with(state: &State, sso: &Sso, org: Option<String>, request: &Request) -> ApiResult {
    let c = config(sso)?;
    let (client_challenge, return_to, hint) = start_input(state, request)?;
    if let Some(e) = &hint {
        if !c.domain_allowed(e) {
            return Err(ApiError::forbidden("this address's domain can't sign in with SSO on this server"));
        }
    }
    let now = state.now();
    let provider = sso.provider(c, now).map_err(|e| config_error(state, e))?;
    let state_token = new_token();
    let nonce = new_token();
    let pkce_verifier = new_token();
    let pkce_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce_verifier.as_bytes()));
    let mut url = Url::parse(&provider.authorization_endpoint).map_err(|e| config_error(state, format!("authorization_endpoint: {e}")))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &c.client_id)
            .append_pair("redirect_uri", &redirect_uri_of(state, sso.kind))
            .append_pair("scope", &c.scopes)
            .append_pair("state", &state_token)
            .append_pair("nonce", &nonce);
        if sso.minted_secret.is_some() {
            // Apple: no PKCE; asking for the address needs a form POST back.
            q.append_pair("response_mode", "form_post");
        } else {
            q.append_pair("code_challenge", &pkce_challenge).append_pair("code_challenge_method", "S256");
        }
        if let Some(h) = &hint {
            q.append_pair("login_hint", h);
        }
    }
    // Every SSO flow, the server's own and each org's, waits in the server's SSO flows: the
    // callback finds which provider it belongs to from it.
    let flows = if sso.kind == "sso" { &state.sso.flows } else { &sso.flows };
    flows.remember_flow(&state_token, Flow { expires: now + FLOW_TTL, nonce, pkce_verifier, client_challenge, return_to, org }, now);
    Ok((200, json!({ "url": url.to_string(), "expiresAt": now + FLOW_TTL })))
}

pub fn redirect_uri(state: &State) -> String {
    redirect_uri_of(state, "sso")
}

/// `<public URL>/v1/auth/<kind>/callback`: what each provider must have registered.
pub fn redirect_uri_of(state: &State, kind: &str) -> String {
    state.config.public_url.join(&format!("v1/auth/{kind}/callback")).map(|u| u.to_string()).unwrap_or_default()
}

/// Why an ID token or its user was refused (also the `signin_failed` reason).
#[derive(Debug)]
pub struct Refused {
    pub reason: &'static str,
    pub message: String,
    pub email: String,
}

fn refused(reason: &'static str, message: impl Into<String>) -> Refused {
    Refused { reason, message: message.into(), email: String::new() }
}

/// The checked identity from an ID token.
#[derive(Debug, Clone)]
pub struct Identity {
    pub subject: String,
    pub email: String,
}

fn claim_bool(v: &Json) -> Option<bool> {
    match v {
        Json::Bool(b) => Some(*b),
        Json::String(s) => Some(s == "true"),
        _ => None,
    }
}

/// Checks an ID token: signature (an asymmetric algorithm, a key from the provider's JWKS),
/// issuer, audience and `azp`, expiry, issue time and nonce; then the address and the groups.
fn check_id_token(state: &State, sso: &Sso, c: &OidcConfig, provider: &Provider, token: &str, nonce: &str) -> Result<Identity, Refused> {
    let now = state.now();
    let header = jsonwebtoken::decode_header(token).map_err(|e| refused("bad_token", format!("the ID token is malformed: {e}")))?;
    if !ALGORITHMS.contains(&header.alg) {
        return Err(refused("bad_token", format!("the ID token uses {:?}; only asymmetric signatures are accepted", header.alg)));
    }
    let key = sso.key(provider, header.kid.as_deref(), now).map_err(|e| refused("bad_token", e))?;
    let mut v = Validation::new(header.alg);
    let mut issuers = vec![c.issuer.as_str()];
    issuers.extend(sso.extra_issuers.iter().map(String::as_str));
    v.set_issuer(&issuers);
    v.set_audience(&[&c.client_id]);
    v.set_required_spec_claims(&["iss", "aud", "sub", "exp"]);
    // exp and iat are checked below against the server's clock (which tests can move).
    v.validate_exp = false;
    v.validate_nbf = false;
    let data = jsonwebtoken::decode::<Json>(token, &key, &v).map_err(|e| refused("bad_token", format!("the ID token was refused: {e}")))?;
    let claims = data.claims;
    let exp = claims["exp"].as_i64().ok_or_else(|| refused("bad_token", "the ID token has no exp"))?;
    if exp + LEEWAY < now {
        return Err(refused("expired_token", "the ID token has expired"));
    }
    // iat and nbf are optional, but when present they must be numbers: a string must not
    // switch the check off (GHSA-h395-gr6q-cpjc, which is why jsonwebtoken's own nbf check is unused).
    let time_claim = |name: &str| match &claims[name] {
        Json::Null => Ok(None),
        v => v.as_i64().map(Some).ok_or_else(|| refused("bad_token", format!("the ID token's {name} isn't a number"))),
    };
    if time_claim("iat")?.is_some_and(|iat| iat > now + LEEWAY) {
        return Err(refused("bad_token", "the ID token was issued in the future"));
    }
    if time_claim("nbf")?.is_some_and(|nbf| nbf > now + LEEWAY) {
        return Err(refused("bad_token", "the ID token isn't valid yet (nbf)"));
    }
    // Several audiences: the token must have been issued to us (OIDC Core 3.1.3.7).
    let audiences = match &claims["aud"] {
        Json::Array(a) => a.len(),
        _ => 1,
    };
    let azp = claims["azp"].as_str();
    if (audiences > 1 && azp.is_none()) || azp.is_some_and(|a| a != c.client_id) {
        return Err(refused("bad_token", "the ID token was issued to another client (azp)"));
    }
    if claims["nonce"].as_str() != Some(nonce) {
        return Err(refused("bad_nonce", "the ID token doesn't belong to this sign-in (nonce)"));
    }
    let subject = claims["sub"].as_str().unwrap_or("").to_string();
    if subject.is_empty() || subject.len() > 255 {
        return Err(refused("bad_token", "the ID token has no usable subject"));
    }
    let raw_email = claims["email"].as_str().unwrap_or("");
    let email = normalize_email(raw_email).map_err(|_| refused("no_email", "the identity provider sent no usable email address (add the email scope)"))?;
    let fail = |reason: &'static str, message: String| Refused { reason, message, email: email.clone() };
    match claim_bool(&claims["email_verified"]) {
        Some(true) => {}
        Some(false) => return Err(fail("unverified_email", format!("the identity provider hasn't verified {email}"))),
        None if c.assume_verified => {}
        None => return Err(fail("unverified_email", format!("the identity provider doesn't say whether {email} is verified"))),
    }
    if !c.domain_allowed(&email) {
        return Err(fail("domain_not_allowed", format!("{email} isn't in a domain allowed to sign in here")));
    }
    if !c.required_groups.is_empty() {
        let groups: Vec<String> = match &claims[c.groups_claim.as_str()] {
            Json::Array(a) => a.iter().filter_map(|g| g.as_str().map(|s| s.trim_start_matches('/').to_string())).collect(),
            Json::String(s) => vec![s.trim_start_matches('/').to_string()],
            _ => vec![],
        };
        if !c.required_groups.iter().any(|g| groups.contains(g)) {
            return Err(fail("group_not_allowed", format!("{email} isn't in a group allowed to sign in here")));
        }
    }
    Ok(Identity { subject, email })
}

/// Checks an ID token as the callback does (fetching provider metadata and keys as needed).
/// Public for tests that bring their own provider tokens.
pub fn validate_id_token(state: &State, token: &str, nonce: &str) -> Result<Identity, Refused> {
    let c = state.config.oidc.as_ref().ok_or_else(|| refused("sso_off", "single sign-on isn't configured"))?;
    let provider = state.sso.provider(c, state.now()).map_err(|e| refused("provider_unreachable", e))?;
    check_id_token(state, &state.sso, c, &provider, token, nonce)
}

/// The account for a checked identity: the one linked to (issuer, subject), else the account
/// with the same (verified) address, which gets linked, else a new account.
fn account(tx: &mut Tx, state: &State, c: &OidcConfig, id: &Identity) -> Result<User, ApiError> {
    linked_account(tx, state, &c.issuer, id)
}

/// The account for an identity checked by a provider with this `issuer` (SSO, Google, GitHub):
/// the one linked to (issuer, subject); else the account with the same address, which gets
/// linked (callers make sure the provider verified the address); else a new account.
pub(crate) fn linked_account(tx: &mut Tx, state: &State, issuer: &str, id: &Identity) -> Result<User, ApiError> {
    let now = state.now();
    let issuer = issuer.to_string();
    if let Some(r) = tx.query_one("SELECT u.id, u.email FROM oidc_identities i JOIN users u ON u.id = i.user_id WHERE i.issuer = $1 AND i.subject = $2", &[(&issuer).into(), (&id.subject).into()])? {
        tx.execute("UPDATE oidc_identities SET last_used_at = $1, email = $2 WHERE issuer = $3 AND subject = $4", &[now.into(), (&id.email).into(), (&issuer).into(), (&id.subject).into()])?;
        return Ok(User { id: r.text(0), email: r.text(1), runner_of: String::new() });
    }
    let user = match tx.query_one("SELECT id, email FROM users WHERE email = $1 AND runner_of = ''", &[(&id.email).into()])? {
        Some(r) => User { id: r.text(0), email: r.text(1), runner_of: String::new() },
        None => {
            let user = User { id: Uuid::new_v4().to_string(), email: id.email.clone(), runner_of: String::new() };
            tx.execute("INSERT INTO users (id, email, created_at) VALUES ($1, $2, $3)", &[(&user.id).into(), (&user.email).into(), now.into()])?;
            user
        }
    };
    tx.execute(
        "INSERT INTO oidc_identities (issuer, subject, user_id, email, created_at, last_used_at) VALUES ($1, $2, $3, $4, $5, $5)",
        &[(&issuer).into(), (&id.subject).into(), (&user.id).into(), (&id.email).into(), now.into()],
    )?;
    audit::record(tx, state, Event::new(audit::ACCOUNT, "sso_linked", Some(&user)).target(&user.id).detail(json!({ "issuer": issuer, "subject": id.subject })))?;
    Ok(user)
}

const PAGE_STYLE: &str = "body{font:16px/1.5 system-ui,sans-serif;max-width:36rem;margin:3rem auto;padding:0 1rem;color:#1b1b1f;background:#fff}code{display:block;padding:.75rem;background:#f2f0f4;border-radius:8px;word-break:break-all;font-size:15px}@media (prefers-color-scheme:dark){body{color:#e4e1e6;background:#1b1b1f}code{background:#303034}}";

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn page(status: u16, title: &str, body: &str) -> Response {
    let html = format!("<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{t}</title><style>{PAGE_STYLE}</style></head><body><h1>{t}</h1>{body}</body></html>\n", t = escape(title));
    let range = Range::get_content_range(html.into_bytes(), MimeType::TEXT_HTML.to_string());
    let phrase = if status == 200 { STATUS_CODE_REASON_PHRASE.n200_ok } else { STATUS_CODE_REASON_PHRASE.n400_bad_request };
    let mut r = Response::get_response(phrase, None, Some(vec![range]));
    r.headers.push(Header { name: "Content-Security-Policy".into(), value: "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'".into() });
    r.headers.push(Header { name: "Referrer-Policy".into(), value: "no-referrer".into() });
    r
}

pub(crate) fn failed_page(message: &str) -> Response {
    page(400, "SSO sign-in failed", &format!("<p>{}</p><p>Start again in hitop.</p>", escape(message)))
}

pub(crate) fn redirect(to: &str) -> Response {
    let mut r = Response::get_response(STATUS_CODE_REASON_PHRASE.n302_found, None, None);
    r.headers.push(Header { name: "Location".into(), value: to.to_string().into() });
    r.headers.push(Header { name: "Referrer-Policy".into(), value: "no-referrer".into() });
    r
}

pub(crate) fn failed(state: &State, method: &str, why: Refused) -> Response {
    audit::record_alone(state, Event::new(audit::ACCOUNT, "signin_failed", None).email(&why.email).detail(json!({ "method": method, "reason": why.reason })));
    failed_page(&why.message)
}

/// `GET /v1/auth/sso/callback?code&state` (the browser, from the provider). Answers a page or
/// a redirect, never JSON. The flow says whose provider: the server's or an org's.
pub fn callback(state: &State, request: &Request) -> Response {
    let q = query(request);
    let now = state.now();
    let Some(flow) = q.get("state").and_then(|s| state.sso.flows.take_flow(s, now)) else {
        return failed(state, "sso", refused("unknown_state", "this sign-in expired or was already used"));
    };
    match flow.org.clone() {
        None => match state.sso.config.as_ref() {
            Some(c) => complete(state, &state.sso, c, flow, &q),
            None => failed_page("single sign-on isn't configured on this server"),
        },
        Some(org) => match crate::orgsso::provider(state, &org) {
            Ok(sso) => match sso.config.as_ref() {
                Some(c) => complete(state, &sso, c, flow, &q),
                None => failed_page("this organization has no single sign-on"),
            },
            Err(e) => failed(state, "sso", refused("sso_off", e)),
        },
    }
}

/// `GET /v1/auth/google/callback`.
pub fn google_callback(state: &State, request: &Request) -> Response {
    callback_with(state, &state.google, request)
}

fn callback_with(state: &State, sso: &Sso, request: &Request) -> Response {
    callback_query(state, sso, query(request))
}

/// A callback's parameters, from the query or (Apple) a form body.
pub(crate) fn callback_query(state: &State, sso: &Sso, q: HashMap<String, String>) -> Response {
    let kind = sso.kind;
    let Some(c) = sso.config.as_ref() else {
        return failed_page(&format!("sign-in with {} isn't configured on this server", provider_name(kind)));
    };
    let now = state.now();
    let Some(flow) = q.get("state").and_then(|s| sso.flows.take_flow(s, now)) else {
        return failed(state, kind, refused("unknown_state", "this sign-in expired or was already used"));
    };
    complete(state, sso, c, flow, &q)
}

/// The rest of a callback once its flow is known.
fn complete(state: &State, sso: &Sso, c: &OidcConfig, flow: Flow, q: &HashMap<String, String>) -> Response {
    let kind = sso.kind;
    let now = state.now();
    if let Some(error) = q.get("error") {
        let detail = q.get("error_description").map(|d| format!(": {d}")).unwrap_or_default();
        return failed(state, kind, refused("provider_error", format!("the identity provider said {error}{detail}")));
    }
    let Some(code) = q.get("code").filter(|c| !c.is_empty()) else {
        return failed(state, kind, refused("no_code", "the identity provider sent no code"));
    };
    let provider = match sso.provider(c, now) {
        Ok(p) => p,
        Err(e) => {
            config_error(state, e);
            return failed_page("the identity provider is unreachable or misconfigured");
        }
    };
    let id_token = match exchange(state, sso, c, &provider, code, &flow.pkce_verifier) {
        Ok(t) => t,
        Err(why) => return failed(state, kind, why),
    };
    let identity = match check_id_token(state, sso, c, &provider, &id_token, &flow.nonce) {
        Ok(i) => i,
        Err(why) => return failed(state, kind, why),
    };
    // Google (and GitHub) never get around a "sign in with SSO only" domain; nor does the
    // server's own SSO get around an org's "SSO required" (only that org's provider does).
    if kind != "sso" && sso_required_for(state, &identity.email) {
        let email = identity.email.clone();
        return failed(state, kind, Refused { reason: "sso_required", message: format!("{email} signs in with single sign-on on this server"), email });
    }
    if kind == "sso" && org_required_for(state, &identity.email).is_some_and(|o| flow.org.as_deref() != Some(o.as_str())) {
        let email = identity.email.clone();
        return failed(state, kind, Refused { reason: "sso_required", message: format!("{email} signs in with its organization's single sign-on"), email });
    }
    let user = match state.db.tx(|tx| account(tx, state, c, &identity)) {
        Ok(u) => u,
        Err(e) => return failed_page(&e.message),
    };
    let flows = if kind == "sso" { &state.sso.flows } else { &sso.flows };
    hand_off(state, flows, kind, &user, flow)
}

/// After a provider signed the browser in: a one-time code for the client that started, sent
/// back to it (its loopback listener or an allowed web page) or shown to paste.
pub(crate) fn hand_off(state: &State, flows: &Flows, kind: &str, user: &User, flow: Flow) -> Response {
    let now = state.now();
    let handoff = new_token();
    flows.remember_code(&handoff, Handoff { expires: now + CODE_TTL, user_id: user.id.clone(), email: user.email.clone(), client_challenge: flow.client_challenge }, now);
    let title = match kind {
        "sso" => "Signed in with SSO".to_string(),
        k => format!("Signed in with {}", provider_name(k)),
    };
    match flow.return_to {
        Some(mut to) if is_loopback(&to) => {
            to.query_pairs_mut().append_pair("sso", &handoff);
            redirect(to.as_str())
        }
        Some(mut to) => {
            to.set_fragment(Some(&format!("sso={handoff}")));
            redirect(to.as_str())
        }
        None => page(
            200,
            &title,
            &format!(
                "<p>Signed in as {}. Paste this code into hitop on the device you started from. It works once, for 5 minutes. Never give it to anyone.</p><code>{}</code>",
                escape(&user.email),
                escape(&handoff)
            ),
        ),
    }
}

/// The token request: client secret (HTTP Basic, RFC 6749 2.3.1) and the PKCE verifier.
fn exchange(state: &State, sso: &Sso, c: &OidcConfig, provider: &Provider, code: &str, pkce_verifier: &str) -> Result<String, Refused> {
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    let basic = STANDARD.encode(format!("{}:{}", enc(&c.client_id), enc(&c.client_secret)));
    let redirect = redirect_uri_of(state, sso.kind);
    let mut form = vec![("grant_type", "authorization_code"), ("code", code), ("redirect_uri", redirect.as_str()), ("client_id", c.client_id.as_str())];
    let minted;
    let mut req = sso.http.post(&provider.token_endpoint).header("Accept", "application/json");
    match &sso.minted_secret {
        // Apple: client_secret_post with the signed secret, and no PKCE.
        Some(m) => {
            minted = m.get(c, state.now()).map_err(|e| {
                let _ = config_error(state, e);
                refused("provider_unreachable", "the server can't sign its Sign in with Apple request")
            })?;
            form.push(("client_secret", minted.as_str()));
        }
        None => {
            req = req.header("Authorization", &format!("Basic {basic}"));
            form.push(("code_verifier", pkce_verifier));
        }
    }
    let result = req.send_form(form);
    let mut res = result.map_err(|e| {
        let _ = config_error(state, format!("token endpoint: {e}"));
        refused("provider_unreachable", "the identity provider is unreachable")
    })?;
    let status = res.status().as_u16();
    let text = res.body_mut().with_config().limit(1 << 20).read_to_string().unwrap_or_default();
    let doc: Json = serde_json::from_str(&text).unwrap_or(Json::Null);
    if status != 200 {
        let error = doc["error"].as_str().unwrap_or("error");
        if error == "invalid_client" || error == "unauthorized_client" {
            let _ = config_error(state, format!("token endpoint refused the client: {error}"));
        }
        return Err(refused("code_refused", format!("the identity provider refused the sign-in ({error})")));
    }
    doc["id_token"].as_str().map(str::to_string).ok_or_else(|| refused("bad_token", "the identity provider sent no ID token"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Finish {
    code: String,
    verifier: String,
}

/// `POST /v1/auth/sso/finish {code, verifier}` → a session, like the other sign-ins.
pub fn finish(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    // Org providers hand their codes over here too, so a server without its own SSO still finishes.
    if state.config.oidc.is_none() && state.config.host_key.is_none() {
        config(&state.sso)?;
    }
    finish_with(state, &state.sso.flows, "sso", request)
}

/// `POST /v1/auth/google/finish {code, verifier}`.
pub fn google_finish(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    config(&state.google)?;
    finish_with(state, &state.google.flows, "google", request)
}

/// `POST /v1/auth/apple/finish {code, verifier}`.
pub fn apple_finish(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    config(&state.apple)?;
    finish_with(state, &state.apple.flows, "apple", request)
}

pub(crate) fn finish_with(state: &State, flows: &Flows, method: &str, request: &Request) -> ApiResult {
    let input: Finish = body(request)?;
    let now = state.now();
    let handoff = flows.take_code(input.code.trim(), now);
    let refuse = |reason: &'static str, email: &str| {
        audit::record_alone(state, Event::new(audit::ACCOUNT, "signin_failed", None).email(email).detail(json!({ "method": method, "reason": reason })));
        ApiError::new(401, "invalid_code", "this sign-in code is invalid, used or expired; sign in again")
    };
    let Some(h) = handoff else {
        return Err(refuse("unknown_code", ""));
    };
    if URL_SAFE_NO_PAD.encode(Sha256::digest(input.verifier.as_bytes())) != h.client_challenge {
        return Err(refuse("bad_verifier", &h.email));
    }
    state.db.tx(|tx| {
        let Some(r) = tx.query_one("SELECT email FROM users WHERE id = $1", &[(&h.user_id).into()])? else {
            return Err(ApiError::new(401, "invalid_code", "this account no longer exists"));
        };
        let user = User { id: h.user_id.clone(), email: r.text(0), runner_of: String::new() };
        new_session(state, tx, &user, method)
    })
}
