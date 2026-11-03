//! "Continue with GitHub": OAuth 2 (GitHub has no OpenID Connect for people), the same browser
//! flow and one-time code as SSO and Google (oidc.rs):
//!
//! 1. `POST /v1/auth/github/start {challenge, returnTo?}` → GitHub's authorization URL (`state`,
//!    PKCE S256, scopes `read:user user:email`).
//! 2. `GET /v1/auth/github/callback?code&state`: the server exchanges the code (client secret +
//!    PKCE verifier) for an access token, reads `GET /user` (the numeric id is the subject) and
//!    `GET /user/emails`, and takes the **primary, verified** address; no such address, no
//!    sign-in. The token is used for those two calls and then dropped (never stored).
//! 3. The account: linked by (issuer `https://github.com`, user id) in `oidc_identities`; the
//!    first sign-in links to the account with the same address (GitHub verified it) or makes one.
//!    `POST /v1/auth/github/finish {code, verifier}` → a session.
//!
//! Settings: `HITOP_SYNC_GITHUB_CLIENT_ID`, `HITOP_SYNC_GITHUB_CLIENT_SECRET` (or `_FILE`); the
//! OAuth app's callback URL is `<HITOP_SYNC_PUBLIC_URL>/v1/auth/github/callback`. Off when unset.
//! `HITOP_SYNC_GITHUB_URL` and `HITOP_SYNC_GITHUB_API_URL` point at a GitHub Enterprise Server or
//! a test double (https, or http on localhost).

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rust_web_server::request::Request;
use rust_web_server::response::Response;
use rust_web_server::router::PathParams;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use url::Url;

use crate::api::{new_token, query, ApiError, ApiResult, State};
use crate::auth::normalize_email;
use crate::oidc::{self, Flow, Flows, Identity, Refused};

const FLOW_TTL: i64 = 10 * 60;
/// The issuer recorded for GitHub identities.
pub const GITHUB_ISSUER: &str = "https://github.com";

#[derive(Clone)]
pub struct GithubConfig {
    pub client_id: String,
    pub client_secret: String,
    /// `https://github.com` (authorize and token endpoints under `/login/oauth/`).
    pub web_url: String,
    /// `https://api.github.com`.
    pub api_url: String,
}

impl std::fmt::Debug for GithubConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubConfig").field("client_id", &self.client_id).field("web_url", &self.web_url).finish_non_exhaustive()
    }
}

impl GithubConfig {
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<Option<GithubConfig>, String> {
        let Some((client_id, client_secret)) = crate::config::client_pair(var, "HITOP_SYNC_GITHUB")? else {
            return Ok(None);
        };
        let c = GithubConfig {
            client_id,
            client_secret,
            web_url: var("HITOP_SYNC_GITHUB_URL").unwrap_or_else(|| "https://github.com".into()).trim_end_matches('/').to_string(),
            api_url: var("HITOP_SYNC_GITHUB_API_URL").unwrap_or_else(|| "https://api.github.com".into()).trim_end_matches('/').to_string(),
        };
        for (name, u) in [("HITOP_SYNC_GITHUB_URL", &c.web_url), ("HITOP_SYNC_GITHUB_API_URL", &c.api_url)] {
            if !oidc::secure_url(u) {
                return Err(format!("{name} must be an https:// URL (http:// only on localhost): {u}"));
            }
        }
        Ok(Some(c))
    }
}

pub struct Github {
    pub config: Option<GithubConfig>,
    http: ureq::Agent,
    flows: Flows,
}

impl Github {
    pub fn new(config: Option<GithubConfig>) -> Github {
        Github { config, http: oidc::http_agent(), flows: Flows::default() }
    }
}

fn config(state: &State) -> Result<&GithubConfig, ApiError> {
    state.github.config.as_ref().ok_or_else(|| ApiError::new(404, "provider_off", "sign-in with GitHub isn't configured on this server"))
}

/// `POST /v1/auth/github/start {challenge, returnTo?}` → `{url, expiresAt}`.
pub fn start(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let c = config(state)?;
    let (client_challenge, return_to, _hint) = oidc::start_input(state, request)?;
    let now = state.now();
    let state_token = new_token();
    let pkce_verifier = new_token();
    let pkce_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce_verifier.as_bytes()));
    let mut url = Url::parse(&format!("{}/login/oauth/authorize", c.web_url)).map_err(|_| ApiError::new(500, "internal", "bad HITOP_SYNC_GITHUB_URL"))?;
    url.query_pairs_mut()
        .append_pair("client_id", &c.client_id)
        .append_pair("redirect_uri", &oidc::redirect_uri_of(state, "github"))
        .append_pair("scope", "read:user user:email")
        .append_pair("state", &state_token)
        .append_pair("code_challenge", &pkce_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("allow_signup", "true");
    state.github.flows.remember_flow(&state_token, Flow { expires: now + FLOW_TTL, nonce: String::new(), pkce_verifier, client_challenge, return_to }, now);
    Ok((200, json!({ "url": url.to_string(), "expiresAt": now + FLOW_TTL })))
}

fn refused(reason: &'static str, message: impl Into<String>) -> Refused {
    Refused { reason, message: message.into(), email: String::new() }
}

/// GET or POST to GitHub, JSON back. `None` body = GET.
fn call(gh: &Github, url: &str, token: Option<&str>, form: Option<&[(&str, &str)]>) -> Result<(u16, Json), String> {
    let result = match form {
        Some(f) => gh.http.post(url).header("Accept", "application/json").send_form(f.iter().copied()),
        None => {
            let mut r = gh.http.get(url).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(t) = token {
                r = r.header("Authorization", &format!("Bearer {t}"));
            }
            r.call()
        }
    };
    let mut res = result.map_err(|e| format!("{url}: {e}"))?;
    let status = res.status().as_u16();
    let text = res.body_mut().with_config().limit(1 << 20).read_to_string().map_err(|e| format!("{url}: {e}"))?;
    Ok((status, serde_json::from_str(&text).unwrap_or(Json::Null)))
}

/// Code → access token → the user's id and primary verified address.
fn identity(state: &State, c: &GithubConfig, code: &str, pkce_verifier: &str) -> Result<Identity, Refused> {
    let gh = &state.github;
    let unreachable = |e: String| {
        eprintln!("hitop-sync-server: GitHub: {e}");
        refused("provider_unreachable", "GitHub is unreachable; try again")
    };
    let redirect = oidc::redirect_uri_of(state, "github");
    let form = [("client_id", c.client_id.as_str()), ("client_secret", c.client_secret.as_str()), ("code", code), ("redirect_uri", redirect.as_str()), ("code_verifier", pkce_verifier)];
    let (status, doc) = call(gh, &format!("{}/login/oauth/access_token", c.web_url), None, Some(&form)).map_err(unreachable)?;
    // GitHub answers 200 with {"error": ...} for a bad code.
    let Some(token) = doc["access_token"].as_str().filter(|t| status == 200 && !t.is_empty()) else {
        let error = doc["error"].as_str().unwrap_or("error");
        if error == "incorrect_client_credentials" || error == "redirect_uri_mismatch" {
            eprintln!("hitop-sync-server: GitHub refused the OAuth app: {error}");
        }
        return Err(refused("code_refused", format!("GitHub refused the sign-in ({error})")));
    };
    let (status, user) = call(gh, &format!("{}/user", c.api_url), Some(token), None).map_err(unreachable)?;
    let subject = match &user["id"] {
        Json::Number(n) if status == 200 => n.to_string(),
        _ => return Err(refused("bad_token", "GitHub didn't say who signed in")),
    };
    let (status, emails) = call(gh, &format!("{}/user/emails", c.api_url), Some(token), None).map_err(unreachable)?;
    let list = if status == 200 { emails.as_array().cloned().unwrap_or_default() } else { vec![] };
    let primary = list.iter().find(|e| e["primary"] == true);
    let Some(primary) = primary else {
        return Err(refused("no_email", "GitHub sent no primary email address (the app needs the user:email scope)"));
    };
    let email = normalize_email(primary["email"].as_str().unwrap_or("")).map_err(|_| refused("no_email", "GitHub's primary address isn't usable"))?;
    if primary["verified"] != true {
        return Err(Refused { reason: "unverified_email", message: format!("GitHub hasn't verified {email}: verify it on GitHub, then try again"), email });
    }
    Ok(Identity { subject, email })
}

/// `GET /v1/auth/github/callback?code&state` (the browser, from GitHub).
pub fn callback(state: &State, request: &Request) -> Response {
    let Some(c) = state.github.config.as_ref() else {
        return oidc::failed_page("sign-in with GitHub isn't configured on this server");
    };
    let q = query(request);
    let now = state.now();
    let Some(flow) = q.get("state").and_then(|s| state.github.flows.take_flow(s, now)) else {
        return oidc::failed(state, "github", refused("unknown_state", "this sign-in expired or was already used"));
    };
    if let Some(error) = q.get("error") {
        let detail = q.get("error_description").map(|d| format!(": {d}")).unwrap_or_default();
        return oidc::failed(state, "github", refused("provider_error", format!("GitHub said {error}{detail}")));
    }
    let Some(code) = q.get("code").filter(|c| !c.is_empty()) else {
        return oidc::failed(state, "github", refused("no_code", "GitHub sent no code"));
    };
    let id = match identity(state, c, code, &flow.pkce_verifier) {
        Ok(i) => i,
        Err(why) => return oidc::failed(state, "github", why),
    };
    if oidc::sso_required_for(state, &id.email) {
        let email = id.email.clone();
        return oidc::failed(state, "github", Refused { reason: "sso_required", message: format!("{email} signs in with single sign-on on this server"), email });
    }
    let user = match state.db.tx(|tx| oidc::linked_account(tx, state, GITHUB_ISSUER, &id)) {
        Ok(u) => u,
        Err(e) => return oidc::failed_page(&e.message),
    };
    oidc::hand_off(state, &state.github.flows, "github", &user, flow)
}

/// `POST /v1/auth/github/finish {code, verifier}` → a session.
pub fn finish(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    config(state)?;
    oidc::finish_with(state, &state.github.flows, "github", request)
}
