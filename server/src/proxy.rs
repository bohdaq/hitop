//! `POST /api/proxy`: the browser sends one request as JSON, the server sends
//! it and answers with the response as JSON.
//!
//! Request:  `{"method","url","headers":[[name,value]...],"body","bodyEncoding"?}`
//! Response: `{"status","statusText","headers":[[name,value]...],"body",
//!             "bodyEncoding":"utf8"|"base64","truncated","timeMs","url"}`
//! Failures: `{"error": "..."}` with a 4xx/5xx status. Nothing is logged.

use std::io::Read;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{redirect, Url};
use rust_web_server::prelude::*;
use rust_web_server::response::StatusCodeReasonPhrase;
use serde::{Deserialize, Serialize};

use crate::guard::{ip_allowed, port_allowed, PublicResolver};

pub const MAX_REQUEST_BODY: usize = 10 * 1024 * 1024;
pub const MAX_RESPONSE_BODY: u64 = 5 * 1024 * 1024;
const MAX_IN_FLIGHT: usize = 16;
const MAX_REDIRECTS: usize = 10;
/// Sent on every outbound request; an inbound request carrying it came from
/// this proxy (directly or through a redirect), so it is a loop.
const LOOP_HEADER: &str = "x-hitop-proxy";

/// Connection management and framing are the proxy's business, not the user's.
const DROPPED_HEADERS: &[&str] = &[
    "connection", "keep-alive", "transfer-encoding", "te", "trailer", "upgrade", "proxy-connection",
    "proxy-authorization", "content-length", LOOP_HEADER,
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProxyRequest {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    body_encoding: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProxyResponse {
    status: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    body: String,
    body_encoding: &'static str,
    truncated: bool,
    time_ms: u64,
    url: String,
}

#[derive(Clone)]
pub struct Proxy {
    client: Arc<Client>,
    origins: Arc<Vec<String>>,
    deny: Arc<Vec<IpAddr>>,
    in_flight: Arc<AtomicUsize>,
}

/// Scheme, port and (for IP literals) address checks for one URL. Hostnames
/// are checked when `PublicResolver` resolves them.
fn check_url(url: &Url, deny: &[IpAddr]) -> Result<(), String> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("only http and https URLs can be sent".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("put credentials in an Authorization header, not the URL".into());
    }
    let port = url.port_or_known_default().unwrap_or(0);
    if !port_allowed(port) {
        return Err(format!("port {port} is blocked"));
    }
    let literal = match url.host() {
        Some(url::Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
        Some(url::Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
        Some(url::Host::Domain(_)) => None,
        None => return Err("the URL has no host".into()),
    };
    if let Some(ip) = literal {
        if !ip_allowed(ip, deny) {
            return Err(format!("{ip} is a private or reserved address"));
        }
    }
    Ok(())
}

fn error(status: &'static StatusCodeReasonPhrase, message: &str) -> Response {
    Response::json(status, serde_json::json!({ "error": message }).to_string().into_bytes())
}

impl Proxy {
    pub fn from_env() -> Self {
        let list = |name: &str, default: &str| -> Vec<String> {
            std::env::var(name)
                .unwrap_or_else(|_| default.into())
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };
        let deny: Vec<IpAddr> = list("HITOP_DENY_IPS", "").iter().filter_map(|s| s.parse().ok()).collect();
        Self::new(list("HITOP_ALLOWED_ORIGINS", "https://hitop.rws8.tech"), deny)
    }

    pub fn new(origins: Vec<String>, deny: Vec<IpAddr>) -> Self {
        let redirect_deny = deny.clone();
        let policy = redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if let Err(e) = check_url(attempt.url(), &redirect_deny) {
                attempt.error(format!("redirect refused: {e}"))
            } else {
                attempt.follow()
            }
        });
        let client = Client::builder()
            .use_rustls_tls()
            .no_proxy()
            .redirect(policy)
            .dns_resolver(Arc::new(PublicResolver { deny: deny.clone() }))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP client");
        Proxy {
            client: Arc::new(client),
            origins: Arc::new(origins),
            deny: Arc::new(deny),
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn handle(&self, request: &Request) -> Response {
        let s = STATUS_CODE_REASON_PHRASE;
        if request.header(LOOP_HEADER).is_some() {
            return error(s.n508_loop_detected, "the proxy can't call itself");
        }
        // Browsers always send Origin on POST: this keeps other sites' pages
        // from using the proxy. (curl can fake it; the rate limit covers that.)
        let origin = request.header("origin").map(|h| h.value.to_string()).unwrap_or_default();
        if !self.origins.iter().any(|o| *o == origin) {
            return error(s.n403_forbidden, "requests must come from the HITOP web app");
        }
        if request.body.len() > MAX_REQUEST_BODY + 1024 {
            return error(s.n413_payload_too_large, "the request is over 10 MB");
        }
        let req: ProxyRequest = match serde_json::from_slice(&request.body) {
            Ok(r) => r,
            Err(e) => return error(s.n400_bad_request, &format!("invalid request JSON: {e}")),
        };
        if self.in_flight.fetch_add(1, Ordering::SeqCst) >= MAX_IN_FLIGHT {
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            return error(s.n503_service_unavailable, "the proxy is busy. Try again in a few seconds.");
        }
        let response = self.send(req);
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        match response {
            Ok(body) => Response::json(s.n200_ok, serde_json::to_vec(&body).unwrap_or_default()),
            Err((status, message)) => error(status, &message),
        }
    }

    fn send(&self, req: ProxyRequest) -> Result<ProxyResponse, (&'static StatusCodeReasonPhrase, String)> {
        let s = STATUS_CODE_REASON_PHRASE;
        let bad = |m: String| (s.n400_bad_request, m);

        let url = Url::parse(req.url.trim()).map_err(|e| bad(format!("invalid URL: {e}")))?;
        check_url(&url, &self.deny).map_err(|e| (s.n403_forbidden, e))?;
        let method = reqwest::Method::from_bytes(req.method.trim().to_uppercase().as_bytes())
            .map_err(|_| bad(format!("invalid method {}", req.method)))?;
        if method == reqwest::Method::CONNECT || method == reqwest::Method::TRACE {
            return Err(bad(format!("{method} is not supported")));
        }

        let mut headers = HeaderMap::new();
        for (name, value) in &req.headers {
            if name.trim().is_empty() || DROPPED_HEADERS.contains(&name.trim().to_ascii_lowercase().as_str()) {
                continue;
            }
            let name = HeaderName::from_bytes(name.trim().as_bytes()).map_err(|_| bad(format!("invalid header name {name}")))?;
            let value = HeaderValue::from_str(value).map_err(|_| bad(format!("invalid value for header {name}")))?;
            headers.append(name, value);
        }
        headers.insert(LOOP_HEADER, HeaderValue::from_static("1"));

        let body = match (req.body, req.body_encoding.as_deref()) {
            (None, _) => None,
            (Some(b), Some("base64")) => Some(
                base64::engine::general_purpose::STANDARD.decode(b).map_err(|_| bad("body is not valid base64".into()))?,
            ),
            (Some(b), _) => Some(b.into_bytes()),
        };
        if body.as_ref().is_some_and(|b| b.len() > MAX_REQUEST_BODY) {
            return Err((s.n413_payload_too_large, "the request body is over 10 MB".into()));
        }

        let mut builder = self.client.request(method, url).headers(headers);
        if let Some(b) = body {
            builder = builder.body(b);
        }
        let started = Instant::now();
        let response = builder.send().map_err(|e| {
            let status = if e.is_timeout() { s.n504_gateway_timeout } else { s.n502_bad_gateway };
            (status, describe(&e))
        })?;

        let status = response.status();
        let final_url = response.url().to_string();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter(|(n, _)| n.as_str() != LOOP_HEADER)
            .map(|(n, v)| (n.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
            .collect();
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BODY + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| (s.n502_bad_gateway, format!("reading the response failed: {e}")))?;
        let truncated = bytes.len() as u64 > MAX_RESPONSE_BODY;
        bytes.truncate(MAX_RESPONSE_BODY as usize);
        let (body, body_encoding) = match String::from_utf8(bytes) {
            Ok(text) => (text, "utf8"),
            Err(e) => (base64::engine::general_purpose::STANDARD.encode(e.into_bytes()), "base64"),
        };
        Ok(ProxyResponse {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("").to_string(),
            headers,
            body,
            body_encoding,
            truncated,
            time_ms: started.elapsed().as_millis() as u64,
            url: final_url,
        })
    }
}

/// reqwest's Display hides the cause ("error sending request for url ..."):
/// show the innermost one, which says what actually failed.
fn describe(e: &reqwest::Error) -> String {
    let mut source: &dyn std::error::Error = e;
    while let Some(next) = source.source() {
        source = next;
    }
    if e.is_timeout() {
        "the server didn't answer within 20 seconds".into()
    } else if e.is_redirect() {
        format!("{source}")
    } else {
        format!("couldn't reach the server: {source}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Result<(), String> {
        check_url(&Url::parse(s).unwrap(), &["168.119.17.57".parse().unwrap()])
    }

    #[test]
    fn url_checks() {
        assert!(url("https://api.github.com/").is_ok());
        assert!(url("http://example.com:8080/x").is_ok());
        assert!(url("http://127.0.0.1:9901/").is_err());
        assert!(url("http://[::1]/").is_err());
        assert!(url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(url("http://168.119.17.57/").is_err());
        assert!(url("http://example.com:25/").is_err());
        assert!(url("ftp://example.com/").is_err());
        assert!(url("file:///etc/passwd").is_err());
        assert!(url("http://user:pw@example.com/").is_err());
    }
}
