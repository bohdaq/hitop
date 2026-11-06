//! `POST /api/proxy`: the browser sends one request as JSON, the server sends
//! it and answers with the response as JSON.
//!
//! Request:  `{"method","url","headers":[[name,value]...],"body","bodyEncoding"?}`
//! Response: `{"status","statusText","headers":[[name,value]...],"body",
//!             "bodyEncoding":"utf8"|"base64","truncated","timeMs","url"}`
//! Failures: `{"error": "..."}` with a 4xx/5xx status. Nothing is logged.
//!
//! `POST /api/proxy/stream` (Server-Sent Events, chunked streams): the same request, answered
//! with the upstream body as it arrives (the upstream `Content-Type`), its status and headers in
//! `X-Hitop-Status` and `X-Hitop-Headers` (JSON). At most `MAX_STREAMS` at once, each for up to
//! `STREAM_FOR` and `MAX_STREAM_BODY`.

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
use rust_web_server::header::Header;
use rust_web_server::response::StatusCodeReasonPhrase;
use serde::{Deserialize, Serialize};

use crate::guard::{ip_allowed, port_allowed, PublicResolver};

pub const MAX_REQUEST_BODY: usize = 10 * 1024 * 1024;
pub const MAX_RESPONSE_BODY: u64 = 5 * 1024 * 1024;
const MAX_IN_FLIGHT: usize = 16;
pub const MAX_STREAMS: usize = 8;
pub const MAX_STREAM_BODY: u64 = 50 * 1024 * 1024;
pub const STREAM_FOR: Duration = Duration::from_secs(10 * 60);
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
    // No total time limit (streams end on their own, or after STREAM_FOR).
    stream_client: Arc<Client>,
    streams: Arc<AtomicUsize>,
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
        let builder = |total: Option<Duration>| {
            let deny_redirect = deny.clone();
            let policy = redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error("too many redirects")
                } else if let Err(e) = check_url(attempt.url(), &deny_redirect) {
                    attempt.error(format!("redirect refused: {e}"))
                } else {
                    attempt.follow()
                }
            });
            let mut b = Client::builder()
                .use_rustls_tls()
                .no_proxy()
                .redirect(policy)
                .dns_resolver(Arc::new(PublicResolver { deny: deny.clone() }))
                .connect_timeout(Duration::from_secs(10));
            b = match total {
                Some(t) => b.timeout(t),
                None => b.timeout(None),
            };
            b.build().expect("HTTP client")
        };
        Proxy {
            client: Arc::new(builder(Some(Duration::from_secs(20)))),
            stream_client: Arc::new(builder(None)),
            streams: Arc::new(AtomicUsize::new(0)),
            origins: Arc::new(origins),
            deny: Arc::new(deny),
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// The checks both endpoints make: no loop, an allowed Origin, a request that parses.
    fn accept(&self, request: &Request) -> Result<ProxyRequest, Box<Response>> {
        let s = STATUS_CODE_REASON_PHRASE;
        if request.header(LOOP_HEADER).is_some() {
            return Err(Box::new(error(s.n508_loop_detected, "the proxy can't call itself")));
        }
        // Browsers always send Origin on POST: this keeps other sites' pages
        // from using the proxy. (curl can fake it; the rate limit covers that.)
        let origin = request.header("origin").map(|h| h.value.to_string()).unwrap_or_default();
        if !self.origins.contains(&origin) {
            return Err(Box::new(error(s.n403_forbidden, "requests must come from the HITOP web app")));
        }
        if request.body.len() > MAX_REQUEST_BODY + 1024 {
            return Err(Box::new(error(s.n413_payload_too_large, "the request is over 10 MB")));
        }
        serde_json::from_slice(&request.body).map_err(|e| Box::new(error(s.n400_bad_request, &format!("invalid request JSON: {e}"))))
    }

    pub fn handle(&self, request: &Request) -> Response {
        let s = STATUS_CODE_REASON_PHRASE;
        let req = match self.accept(request) {
            Ok(r) => r,
            Err(response) => return *response,
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

    /// `POST /api/proxy/stream`: the upstream body piped as it arrives.
    pub fn stream(&self, request: &Request) -> Response {
        let s = STATUS_CODE_REASON_PHRASE;
        let req = match self.accept(request) {
            Ok(r) => r,
            Err(response) => return *response,
        };
        if self.streams.fetch_add(1, Ordering::SeqCst) >= MAX_STREAMS {
            self.streams.fetch_sub(1, Ordering::SeqCst);
            return error(s.n503_service_unavailable, "too many streams are open. Try again in a minute.");
        }
        let slot = Slot(self.streams.clone());
        let upstream = match self.start(&self.stream_client, req) {
            Ok(r) => r,
            Err((status, message)) => return error(status, &message),
        };
        let status = upstream.status();
        let headers = upstream_headers(&upstream);
        let content_type = upstream.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("application/octet-stream").to_string();
        let mut response = Response::get_response(s.n200_ok, None, None);
        response.headers.push(Header::new(Header::_CONTENT_TYPE, content_type));
        response.headers.push(Header::new(Header::_CACHE_CONTROL, "no-store"));
        response.headers.push(Header::new("X-Hitop-Status", status.as_u16().to_string()));
        response.headers.push(Header::new("X-Hitop-Headers", serde_json::to_string(&headers).unwrap_or_default()));
        response.stream_pipe = Some(Box::new(Capped { inner: upstream.take(MAX_STREAM_BODY), until: Instant::now() + STREAM_FOR, _slot: slot }));
        response
    }

    /// Checks the request and sends it with `client`: the upstream response, body unread.
    fn start(&self, client: &Client, req: ProxyRequest) -> Result<reqwest::blocking::Response, (&'static StatusCodeReasonPhrase, String)> {
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
        // Some APIs refuse requests without one (Wikimedia, GitHub): ours, unless the request has its own.
        if !headers.contains_key(reqwest::header::USER_AGENT) {
            headers.insert(reqwest::header::USER_AGENT, HeaderValue::from_static("hitop (+https://hitop.rws8.tech)"));
        }

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

        let mut builder = client.request(method, url).headers(headers);
        if let Some(b) = body {
            builder = builder.body(b);
        }
        builder.send().map_err(|e| {
            let status = if e.is_timeout() { s.n504_gateway_timeout } else { s.n502_bad_gateway };
            (status, describe(&e))
        })
    }

    fn send(&self, req: ProxyRequest) -> Result<ProxyResponse, (&'static StatusCodeReasonPhrase, String)> {
        let s = STATUS_CODE_REASON_PHRASE;
        let started = Instant::now();
        let response = self.start(&self.client, req)?;
        let status = response.status();
        let final_url = response.url().to_string();
        let headers = upstream_headers(&response);
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

fn upstream_headers(response: &reqwest::blocking::Response) -> Vec<(String, String)> {
    response
        .headers()
        .iter()
        .filter(|(n, _)| n.as_str() != LOOP_HEADER)
        .map(|(n, v)| (n.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect()
}

/// A stream's place among `MAX_STREAMS`, given back when the stream ends.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The upstream body until `until`; then the stream ends.
struct Capped<R> {
    inner: R,
    until: Instant,
    _slot: Slot,
}

impl<R: Read> Read for Capped<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if Instant::now() >= self.until {
            return Ok(0);
        }
        self.inner.read(buf)
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
    fn a_stream_ends_at_its_deadline_and_gives_its_slot_back() {
        let streams = Arc::new(AtomicUsize::new(2));
        let mut open = Capped { inner: &b"data: 1\n\n"[..], until: Instant::now() + Duration::from_secs(60), _slot: Slot(streams.clone()) };
        let mut out = String::new();
        open.read_to_string(&mut out).unwrap();
        assert_eq!(out, "data: 1\n\n");
        let mut late = Capped { inner: &b"data: 2\n\n"[..], until: Instant::now(), _slot: Slot(streams.clone()) };
        assert_eq!(late.read(&mut [0; 16]).unwrap(), 0, "past the deadline: the end");
        drop(open);
        drop(late);
        assert_eq!(streams.load(Ordering::SeqCst), 0, "each stream gave its slot back");
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
