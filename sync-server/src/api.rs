//! HTTP plumbing: the application (routing, CORS, errors), shared state and request helpers.
//! Every API answer is JSON. Requests that match no route get a JSON 404: this server serves no
//! files, so nothing on disk (the SQLite database included) is reachable over HTTP.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rust_web_server::application::Application;
use rust_web_server::header::Header;
use rust_web_server::request::Request;
use rust_web_server::response::{Response, StatusCodeReasonPhrase, STATUS_CODE_REASON_PHRASE};
use rust_web_server::router::{PathParams, Router};
use rust_web_server::server::ConnectionInfo;
use serde::de::DeserializeOwned;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};

use crate::audit::Client;
use crate::config::Config;
use crate::db::{Db, DbError};
use crate::mail::Mailer;

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
    pub extra: Option<Json>,
}

impl ApiError {
    pub fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        ApiError { status, code, message: message.into(), extra: None }
    }
    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(400, "bad_request", message)
    }
    pub fn unauthorized() -> Self {
        Self::new(401, "unauthorized", "sign in first (Authorization: Bearer <session>)")
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(403, "forbidden", message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(404, "not_found", message)
    }
    pub fn conflict(code: &'static str, message: impl Into<String>, extra: Json) -> Self {
        ApiError { status: 409, code, message: message.into(), extra: Some(extra) }
    }
}

impl From<DbError> for ApiError {
    fn from(e: DbError) -> Self {
        eprintln!("{e}");
        Self::new(500, "internal", "internal error")
    }
}

pub type ApiResult = Result<(u16, Json), ApiError>;

fn phrase(status: u16) -> &'static StatusCodeReasonPhrase {
    let s = STATUS_CODE_REASON_PHRASE;
    match status {
        200 => s.n200_ok,
        201 => s.n201_created,
        202 => s.n202_accepted,
        204 => s.n204_no_content,
        400 => s.n400_bad_request,
        401 => s.n401_unauthorized,
        402 => s.n402_payment_required,
        403 => s.n403_forbidden,
        404 => s.n404_not_found,
        405 => s.n405_method_not_allowed,
        409 => s.n409_conflict,
        413 => s.n413_payload_too_large,
        429 => s.n429_too_many_requests,
        501 => s.n501_not_implemented,
        503 => s.n503_service_unavailable,
        502 => s.n502_bad_gateway,
        _ => s.n500_internal_server_error,
    }
}

pub fn json_response(status: u16, body: &Json) -> Response {
    if status == 204 {
        return Response::get_response(phrase(204), None, None);
    }
    Response::json(phrase(status), body.to_string().into_bytes())
}

pub(crate) fn error_response(e: &ApiError) -> Response {
    let mut body = json!({ "error": e.code, "message": e.message });
    if let Some(Json::Object(extra)) = &e.extra {
        for (k, v) in extra {
            body[k] = v.clone();
        }
    }
    json_response(e.status, &body)
}

pub fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("the OS random number generator failed");
    bytes
}

pub fn random_hex<const N: usize>() -> String {
    random_bytes::<N>().iter().map(|b| format!("{b:02x}")).collect()
}

/// A secret token for a client (session, sign-in link): 32 random bytes, base64url.
pub fn new_token() -> String {
    URL_SAFE_NO_PAD.encode(random_bytes::<32>())
}

/// What the database keeps instead of a token.
pub fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(field: &str, s: &str) -> Result<Vec<u8>, ApiError> {
    URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).map_err(|_| ApiError::bad(format!("{field} must be base64url")))
}

pub fn body<T: DeserializeOwned>(request: &Request) -> Result<T, ApiError> {
    serde_json::from_slice(&request.body).map_err(|e| ApiError::bad(format!("invalid JSON body: {e}")))
}

pub fn query(request: &Request) -> HashMap<String, String> {
    let q = request.request_uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    url::form_urlencoded::parse(q.as_bytes()).into_owned().collect()
}

pub fn bearer(request: &Request) -> Option<String> {
    request.header("authorization").and_then(|h| h.value.strip_prefix("Bearer ").map(|t| t.trim().to_string()))
}

thread_local! {
    /// The caller of the request this thread is handling (set by the router before a handler
    /// runs; handlers run synchronously on the request's thread). For the audit log.
    static CLIENT: std::cell::RefCell<Client> = std::cell::RefCell::new(Client::default());
}

/// The current request's client (IP as the audit policy keeps it, user agent).
pub fn client() -> Client {
    CLIENT.with(|c| c.borrow().clone())
}

fn set_client(c: Client) {
    CLIENT.with(|cell| *cell.borrow_mut() = c);
}

/// Challenge state for passkey ceremonies, kept in memory for a few minutes.
pub enum Ceremony {
    Register { user_id: String, state: webauthn_rs::prelude::PasskeyRegistration },
    Login { user_id: String, state: webauthn_rs::prelude::PasskeyAuthentication },
}

pub struct State {
    pub db: Db,
    pub mailer: Mailer,
    pub config: Config,
    pub webauthn: webauthn_rs::Webauthn,
    pub ceremonies: Mutex<HashMap<String, (i64, Ceremony)>>,
    /// OpenID Connect sign-in: provider metadata and keys (cached), flows in progress.
    pub sso: crate::oidc::Sso,
    /// Seconds added to the clock (tests move time forward with it).
    pub clock_offset: AtomicI64,
    /// Request rate limits (ratelimit.rs).
    pub limiter: crate::ratelimit::Limiter,
    /// "Continue with Google" (an OpenID Connect provider, oidc.rs).
    pub google: crate::oidc::Sso,
    /// "Continue with GitHub" (github.rs).
    pub github: crate::github::Github,
    /// Wrong passwords per address (password.rs).
    pub password_failures: crate::password::Backoff,
    /// Calls to the Stripe API (billing.rs), DNS over HTTPS (orgsso.rs).
    pub billing_http: ureq::Agent,
    /// Single sign-on per organization: verified domains and providers (orgsso.rs).
    pub org_sso: crate::orgsso::OrgSso,
    /// Git sync of the personal library (gitsync.rs): connect flows, installation tokens.
    pub git: crate::gitsync::Git,
}

impl State {
    pub fn new(db: Db, mailer: Mailer, config: Config) -> Result<State, String> {
        let webauthn = config.webauthn()?;
        let sso = crate::oidc::Sso::new(config.oidc.clone())?;
        let limiter = crate::ratelimit::Limiter::new(config.rate_limits.clone());
        let google = crate::oidc::Sso::provider_of("google", config.google.clone(), vec![crate::oidc::GOOGLE_ISSUER_ALT.into()])?;
        let github = crate::github::Github::new(config.github.clone());
        let domains = db.tx(crate::orgsso::load).map_err(|e| e.to_string())?;
        let org_sso = crate::orgsso::OrgSso::default();
        org_sso.set_domains(domains);
        Ok(State {
            db,
            mailer,
            config,
            webauthn,
            ceremonies: Mutex::new(HashMap::new()),
            sso,
            clock_offset: AtomicI64::new(0),
            limiter,
            google,
            github,
            password_failures: crate::password::Backoff::default(),
            billing_http: crate::oidc::http_agent(),
            org_sso,
            git: crate::gitsync::Git::default(),
        })
    }

    pub fn now(&self) -> i64 {
        now_secs() + self.clock_offset.load(Ordering::Relaxed)
    }
}

type Handler = fn(&State, &Request, &PathParams) -> ApiResult;

/// The sync server as an rws [`Application`].
#[derive(Clone)]
pub struct SyncApp {
    state: Arc<State>,
    router: Router,
}

impl SyncApp {
    pub fn new(state: State) -> Self {
        let state = Arc::new(state);
        let routes: &[(&str, &str, Handler)] = &[
            ("GET", "/", crate::routes::index),
            ("GET", "/healthz", crate::routes::health),
            ("POST", "/v1/auth/email", crate::auth::email_start),
            ("POST", "/v1/auth/email/verify", crate::auth::email_verify),
            ("POST", "/v1/auth/logout", crate::auth::logout),
            ("POST", "/v1/auth/passkey/register/start", crate::auth::passkey_register_start),
            ("POST", "/v1/auth/passkey/register/finish", crate::auth::passkey_register_finish),
            ("POST", "/v1/auth/passkey/login/start", crate::auth::passkey_login_start),
            ("POST", "/v1/auth/passkey/login/finish", crate::auth::passkey_login_finish),
            ("GET", "/v1/auth/sso", crate::oidc::info),
            ("POST", "/v1/auth/sso/start", crate::oidc::start),
            ("POST", "/v1/auth/sso/finish", crate::oidc::finish),
            // HITOP Cloud C1b: what a sign-in page offers; passwords; Google; GitHub.
            ("GET", "/v1/auth/methods", crate::auth::methods),
            ("POST", "/v1/auth/password/params", crate::password::params),
            ("POST", "/v1/auth/password/signup", crate::password::signup),
            ("POST", "/v1/auth/password/verify", crate::password::verify_signup),
            ("POST", "/v1/auth/password/login", crate::password::login),
            ("POST", "/v1/auth/password/forgot", crate::password::forgot),
            ("POST", "/v1/auth/password/reset", crate::password::reset),
            ("POST", "/v1/me/password", crate::password::set),
            ("POST", "/v1/auth/google/start", crate::oidc::google_start),
            ("POST", "/v1/auth/google/finish", crate::oidc::google_finish),
            ("POST", "/v1/auth/github/start", crate::github::start),
            ("POST", "/v1/auth/github/finish", crate::github::finish),
            ("GET", "/v1/me", crate::auth::me),
            ("GET", "/v1/me/audit", crate::audit::mine),
            ("POST", "/v1/me/keys", crate::workspaces::add_device_key),
            ("DELETE", "/v1/me/keys/:id", crate::recovery::delete_key),
            ("GET", "/v1/me/recovery", crate::recovery::get),
            ("PUT", "/v1/me/recovery", crate::recovery::put),
            ("DELETE", "/v1/me/recovery", crate::recovery::delete),
            ("GET", "/v1/users/keys", crate::workspaces::user_keys),
            ("GET", "/v1/workspaces", crate::workspaces::list),
            ("POST", "/v1/workspaces", crate::workspaces::create),
            ("GET", "/v1/workspaces/:ws", crate::workspaces::get),
            ("DELETE", "/v1/workspaces/:ws", crate::workspaces::delete),
            ("GET", "/v1/workspaces/:ws/wraps", crate::workspaces::get_wraps),
            ("POST", "/v1/workspaces/:ws/wraps", crate::workspaces::add_wraps),
            ("POST", "/v1/workspaces/:ws/members", crate::workspaces::add_member),
            ("DELETE", "/v1/workspaces/:ws/members/:user", crate::workspaces::remove_member),
            ("PUT", "/v1/workspaces/:ws/members/:user", crate::teams::set_role),
            ("POST", "/v1/workspaces/:ws/rotate", crate::workspaces::rotate),
            ("GET", "/v1/workspaces/:ws/invites", crate::teams::list),
            ("POST", "/v1/workspaces/:ws/invites", crate::teams::create),
            ("DELETE", "/v1/workspaces/:ws/invites/:id", crate::teams::delete),
            ("POST", "/v1/invites/accept", crate::teams::accept),
            ("POST", "/v1/workspaces/:ws/runners", crate::teams::create_runner),
            ("GET", "/v1/workspaces/:ws/audit", crate::audit::list),
            ("GET", "/v1/workspaces/:ws/audit/verify", crate::audit::verify_endpoint),
            ("GET", "/v1/workspaces/:ws/changes", crate::blobs::changes),
            // HITOP Cloud C4: run history, alerts, shared environment values.
            ("GET", "/v1/workspaces/:ws/run-policy", crate::runs::get_policy),
            ("GET", "/v1/workspaces/:ws/runs", crate::runs::list),
            ("POST", "/v1/workspaces/:ws/runs", crate::runs::upload),
            ("GET", "/v1/workspaces/:ws/runs/:id", crate::runs::get),
            ("GET", "/v1/workspaces/:ws/alerts", crate::alerts::get),
            ("PUT", "/v1/workspaces/:ws/alerts", crate::alerts::put),
            ("GET", "/v1/workspaces/:ws/alert-deliveries", crate::alerts::deliveries),
            ("GET", "/v1/workspaces/:ws/env-shares", crate::shares::list),
            ("PUT", "/v1/workspaces/:ws/env-shares/:id", crate::shares::put),
            ("DELETE", "/v1/workspaces/:ws/env-shares/:id", crate::shares::delete),
            ("POST", "/v1/workspaces/:ws/env-shares/:id/wraps", crate::shares::add_wraps),
            // HITOP Cloud C1: organizations, plans and limits, access tokens.
            ("GET", "/v1/orgs", crate::orgs::list),
            ("POST", "/v1/orgs", crate::orgs::create),
            ("GET", "/v1/orgs/:org", crate::orgs::get),
            ("PUT", "/v1/orgs/:org", crate::orgs::rename),
            ("DELETE", "/v1/orgs/:org", crate::orgs::delete),
            ("GET", "/v1/orgs/:org/members", crate::orgs::members),
            ("POST", "/v1/orgs/:org/members", crate::orgs::add_member),
            ("PUT", "/v1/orgs/:org/members/:user", crate::orgs::set_role),
            ("DELETE", "/v1/orgs/:org/members/:user", crate::orgs::remove_member),
            ("DELETE", "/v1/orgs/:org/invites/:email", crate::orgs::delete_invite),
            ("POST", "/v1/orgs/:org/workspaces", crate::orgs::workspaces),
            ("GET", "/v1/orgs/:org/usage", crate::orgs::get_usage),
            ("GET", "/v1/orgs/:org/limits", crate::orgs::get_limits),
            // HITOP Cloud C3: billing (Stripe Managed Payments), trials.
            ("POST", "/v1/billing/stripe", crate::billing::webhook),
            ("GET", "/v1/orgs/:org/billing", crate::billing::get),
            ("POST", "/v1/orgs/:org/billing/checkout", crate::billing::checkout),
            ("GET", "/v1/orgs/:org/billing/portal", crate::billing::portal),
            ("POST", "/v1/orgs/:org/billing/seats", crate::billing::seats),
            ("POST", "/v1/orgs/:org/billing/trial", crate::billing::start_trial),
            // HITOP Cloud C5: SSO per org, domain claims, audit export and anchors, admin console.
            ("GET", "/v1/orgs/:org/sso", crate::orgsso::get),
            ("PUT", "/v1/orgs/:org/sso", crate::orgsso::put),
            ("DELETE", "/v1/orgs/:org/sso", crate::orgsso::delete),
            ("POST", "/v1/orgs/:org/sso/test", crate::orgsso::test),
            ("GET", "/v1/orgs/:org/domains", crate::orgsso::list_domains),
            ("POST", "/v1/orgs/:org/domains", crate::orgsso::add_domain),
            ("POST", "/v1/orgs/:org/domains/:domain/verify", crate::orgsso::verify_domain),
            ("DELETE", "/v1/orgs/:org/domains/:domain", crate::orgsso::delete_domain),
            ("GET", "/v1/audit/key", crate::anchors::key),
            ("GET", "/v1/orgs/:org/audit/anchors", crate::anchors::list),
            ("POST", "/v1/orgs/:org/audit/anchors", crate::anchors::now),
            ("PUT", "/v1/orgs/:org/audit/settings", crate::anchors::put_settings),
            ("GET", "/v1/audit-anchors/:org/:day", crate::anchors::public),
            ("GET", "/v1/orgs/:org/admin", crate::admin::get),
            ("PUT", "/v1/orgs/:org/admin/workspaces/:ws/owner", crate::admin::transfer_owner),
            ("POST", "/v1/orgs/:org/admin/workspaces/:ws/rotation", crate::admin::force_rotation),
            ("DELETE", "/v1/orgs/:org/admin/devices/:key", crate::admin::remove_device),
            // HITOP Cloud C6: SCIM tokens and group roles (the SCIM API itself is /scim/v2, scim.rs).
            ("GET", "/v1/orgs/:org/scim", crate::scim::get),
            ("POST", "/v1/orgs/:org/scim/tokens", crate::scim::create_token),
            ("DELETE", "/v1/orgs/:org/scim/tokens/:id", crate::scim::delete_token),
            ("PUT", "/v1/orgs/:org/scim/groups/:group", crate::scim::put_group),
            ("GET", "/v1/me/tokens", crate::tokens::list),
            ("POST", "/v1/me/tokens", crate::tokens::create),
            ("DELETE", "/v1/me/tokens/:id", crate::tokens::delete),
            ("GET", "/v1/library", crate::library::list),
            ("PUT", "/v1/library/:kind/:id", crate::library::put),
            ("DELETE", "/v1/library/:kind/:id", crate::library::delete),
            ("GET", "/v1/git", crate::gitsync::info),
            ("POST", "/v1/git/github/connect", crate::gitsync::connect),
            ("POST", "/v1/git/github/webhook", crate::gitsync::webhook),
            ("GET", "/v1/git/installations/:id/repos", crate::gitsync::repos),
            ("GET", "/v1/library/collection/:id/git", crate::gitsync::get),
            ("PUT", "/v1/library/collection/:id/git", crate::gitsync::link),
            ("DELETE", "/v1/library/collection/:id/git", crate::gitsync::unlink),
            ("POST", "/v1/library/collection/:id/git/sync", crate::gitsync::sync_now),
            ("POST", "/v1/library/collection/:id/git/resolve", crate::gitsync::resolve),
            ("GET", "/v1/workspaces/:ws/blobs/:id", crate::blobs::get),
            ("PUT", "/v1/workspaces/:ws/blobs/:id", crate::blobs::put),
        ];
        let mut router = Router::new();
        for &(method, pattern, handler) in routes {
            let st = Arc::clone(&state);
            let run = move |req: &Request, params: &PathParams, conn: &ConnectionInfo| {
                set_client(crate::audit::client_of(req, &conn.client.ip, &st.config.trusted_proxies, st.config.audit_ip));
                match handler(&st, req, params) {
                    Ok((status, body)) => json_response(status, &body),
                    Err(e) => error_response(&e),
                }
            };
            router = match method {
                "GET" => router.get(pattern, run),
                "POST" => router.post(pattern, run),
                "PUT" => router.put(pattern, run),
                _ => router.delete(pattern, run),
            };
        }
        SyncApp { state, router }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    fn client_of(&self, request: &Request, connection: &ConnectionInfo) -> Client {
        crate::audit::client_of(request, &connection.client.ip, &self.state.config.trusted_proxies, self.state.config.audit_ip)
    }

    /// The body cap of a request: blob uploads may carry a blob, everything else is small JSON.
    fn max_body(&self, method: &str, path: &str) -> usize {
        let c = &self.state.config;
        let blob = method == "PUT" && path.starts_with("/v1/workspaces/") && path.contains("/blobs/");
        // A run upload carries a sealed report (runs.rs).
        let run = method == "POST" && path.starts_with("/v1/workspaces/") && path.ends_with("/runs");
        // A personal library item (library.rs): up to 5 MB of JSON.
        let library = method == "PUT" && path.starts_with("/v1/library/");
        if blob {
            c.max_request_bytes()
        } else if library {
            (crate::library::MAX_ITEM_BYTES + 64 * 1024).min(c.max_request_bytes()).max(c.max_json_bytes.min(c.max_request_bytes()))
        } else if run {
            (c.runs.max_run_bytes / 3 * 4 + 64 * 1024).max(c.max_json_bytes.min(c.max_request_bytes()).max(64 * 1024))
        } else {
            c.max_json_bytes.min(c.max_request_bytes()).max(64 * 1024)
        }
    }

    fn rate_check(&self, request: &Request, connection: &ConnectionInfo, path: &str) -> Result<(), crate::ratelimit::Refused> {
        let class = crate::ratelimit::classify(&request.method, path);
        if class == crate::ratelimit::Class::Read {
            return Ok(());
        }
        let ip = crate::audit::real_ip(request, &connection.client.ip, &self.state.config.trusted_proxies);
        let session = bearer(request).map(|t| token_hash(&t));
        self.state.limiter.check(&class, &ip, session.as_deref(), self.state.now())
    }

    /// CORS for the allowed web origins (the web app). Sessions are bearer tokens, never cookies,
    /// so credentials mode stays off.
    fn cors(&self, request: &Request, response: &mut Response) {
        let Some(origin) = request.header("origin").map(|h| h.value.to_string()) else { return };
        if !self.state.config.cors_origins.contains(&origin) {
            return;
        }
        let mut add = |name: &'static str, value: String| response.headers.push(Header { name: name.into(), value: value.into() });
        add("Access-Control-Allow-Origin", origin);
        add("Vary", "Origin".into());
        if request.method == "OPTIONS" {
            add("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE".into());
            add("Access-Control-Allow-Headers", "Authorization, Content-Type".into());
            add("Access-Control-Max-Age", "600".into());
        }
    }
}

impl Application for SyncApp {
    fn execute(&self, request: &Request, connection: &ConnectionInfo) -> Result<Response, String> {
        let path = request.request_uri.split(['?', '#']).next().unwrap_or("");
        let mut response = if request.method == "OPTIONS" {
            json_response(204, &Json::Null)
        } else if request.method == "GET" && path == "/signin" {
            crate::routes::signin_page()
        } else if request.method == "GET" && path == "/invite" {
            crate::routes::invite_page()
        } else if request.method == "GET" && path == "/v1/auth/sso/callback" {
            set_client(self.client_of(request, connection));
            crate::oidc::callback(&self.state, request)
        } else if request.method == "GET" && path == "/v1/auth/google/callback" {
            set_client(self.client_of(request, connection));
            crate::oidc::google_callback(&self.state, request)
        } else if request.method == "GET" && path == "/v1/git/github/callback" {
            crate::gitsync::callback(&self.state, request)
        } else if request.method == "GET" && path == "/v1/auth/github/callback" {
            set_client(self.client_of(request, connection));
            crate::github::callback(&self.state, request)
        } else if let Some(org) = (request.method == "GET").then(|| path.strip_prefix("/v1/orgs/").and_then(|r| r.strip_suffix("/audit/export"))).flatten().filter(|o| !o.is_empty() && !o.contains('/')) {
            // JSON lines, not one JSON value (anchors.rs).
            set_client(self.client_of(request, connection));
            let org = percent_encoding::percent_decode_str(org).decode_utf8_lossy().into_owned();
            crate::anchors::export(&self.state, request, &org)
        } else if request.body.len() > self.max_body(&request.method, path) {
            error_response(&ApiError::new(413, "too_large", "request body too large"))
        } else if let Err(r) = self.rate_check(request, connection, path) {
            let mut response = error_response(&ApiError {
                status: 429,
                code: "rate_limited",
                message: format!("too many requests ({} limit); try again in {} s", r.limit, r.retry_after),
                extra: Some(json!({ "limit": r.limit, "retryAfter": r.retry_after })),
            });
            response.headers.push(Header { name: "Retry-After".into(), value: r.retry_after.to_string().into() });
            response
        } else if path.starts_with("/scim/v2/") || path == "/scim/v2" {
            // SCIM 2.0 (scim.rs): its own errors, content type and PATCH, which the router lacks.
            set_client(self.client_of(request, connection));
            crate::scim::handle(&self.state, request, path)
        } else {
            self.router.handle(request, connection).unwrap_or_else(|| error_response(&ApiError::not_found("no such endpoint")))
        };
        response.headers.push(Header { name: "Cache-Control".into(), value: "no-store".into() });
        self.cors(request, &mut response);
        Ok(response)
    }
}
