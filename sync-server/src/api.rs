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
        403 => s.n403_forbidden,
        404 => s.n404_not_found,
        405 => s.n405_method_not_allowed,
        409 => s.n409_conflict,
        413 => s.n413_payload_too_large,
        429 => s.n429_too_many_requests,
        503 => s.n503_service_unavailable,
        _ => s.n500_internal_server_error,
    }
}

pub fn json_response(status: u16, body: &Json) -> Response {
    if status == 204 {
        return Response::get_response(phrase(204), None, None);
    }
    Response::json(phrase(status), body.to_string().into_bytes())
}

fn error_response(e: &ApiError) -> Response {
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
    /// Seconds added to the clock (tests move time forward with it).
    pub clock_offset: AtomicI64,
}

impl State {
    pub fn new(db: Db, mailer: Mailer, config: Config) -> Result<State, String> {
        let webauthn = config.webauthn()?;
        Ok(State { db, mailer, config, webauthn, ceremonies: Mutex::new(HashMap::new()), clock_offset: AtomicI64::new(0) })
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
            ("GET", "/v1/me", crate::auth::me),
            ("POST", "/v1/me/keys", crate::workspaces::add_device_key),
            ("GET", "/v1/users/keys", crate::workspaces::user_keys),
            ("GET", "/v1/workspaces", crate::workspaces::list),
            ("POST", "/v1/workspaces", crate::workspaces::create),
            ("GET", "/v1/workspaces/:ws", crate::workspaces::get),
            ("DELETE", "/v1/workspaces/:ws", crate::workspaces::delete),
            ("GET", "/v1/workspaces/:ws/wraps", crate::workspaces::get_wraps),
            ("POST", "/v1/workspaces/:ws/wraps", crate::workspaces::add_wraps),
            ("POST", "/v1/workspaces/:ws/members", crate::workspaces::add_member),
            ("DELETE", "/v1/workspaces/:ws/members/:user", crate::workspaces::remove_member),
            ("POST", "/v1/workspaces/:ws/rotate", crate::workspaces::rotate),
            ("GET", "/v1/workspaces/:ws/changes", crate::blobs::changes),
            ("GET", "/v1/workspaces/:ws/blobs/:id", crate::blobs::get),
            ("PUT", "/v1/workspaces/:ws/blobs/:id", crate::blobs::put),
        ];
        let mut router = Router::new();
        for &(method, pattern, handler) in routes {
            let st = Arc::clone(&state);
            let run = move |req: &Request, params: &PathParams, _: &ConnectionInfo| match handler(&st, req, params) {
                Ok((status, body)) => json_response(status, &body),
                Err(e) => error_response(&e),
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
        } else if request.body.len() > self.state.config.max_request_bytes() {
            error_response(&ApiError::new(413, "too_large", "request body too large"))
        } else {
            self.router.handle(request, connection).unwrap_or_else(|| error_response(&ApiError::not_found("no such endpoint")))
        };
        response.headers.push(Header { name: "Cache-Control".into(), value: "no-store".into() });
        self.cors(request, &mut response);
        Ok(response)
    }
}
