#![cfg(feature = "native")]
use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::cookies::{self, Jar};
use crate::model::RequestResult;

static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();

static URL_CHECK: std::sync::OnceLock<fn(&str) -> std::result::Result<(), String>> = std::sync::OnceLock::new();

/// Every request's URL goes through `check` before it is sent (the sync server's monitors refuse
/// private addresses, which a client's DNS resolver never sees when the URL has an IP literal).
pub fn install_url_check(check: fn(&str) -> std::result::Result<(), String>) -> bool {
    URL_CHECK.set(check).is_ok()
}

fn check_url(url: &str) -> Result<()> {
    match URL_CHECK.get() {
        Some(check) => check(url).map_err(|e| anyhow::anyhow!(e)),
        None => Ok(()),
    }
}

/// Uses `c` for every request from now on instead of the default client (the sync server's
/// monitors install one that refuses private addresses). False when a client is already in use.
pub fn install_client(c: Client) -> bool {
    CLIENT.set(c).is_ok()
}

fn client() -> &'static Client {
    CLIENT.get_or_init(|| {
        Client::builder()
            .use_rustls_tls()
            .build()
            .expect("failed to build HTTP client")
    })
}

pub fn execute(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &str,
) -> Result<RequestResult> {
    execute_with(method, url, headers, body, None)
}

/// [`execute`] with a time limit for the whole request (connect, send and
/// reading the body).
pub fn execute_with(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &str,
    timeout: Option<Duration>,
) -> Result<RequestResult> {
    execute_opts(method, url, headers, body.as_bytes().to_vec(), Options { timeout, jar: None })
}

/// How [`execute_opts`] and [`send_with`] send a request.
#[derive(Default)]
pub struct Options<'a> {
    /// Time limit for the whole request (connect, send and reading the body).
    pub timeout: Option<Duration>,
    /// Cookies: the jar's go out with the request (unless it has a `Cookie`
    /// header already) and the response's `Set-Cookie` lines go in.
    pub jar: Option<&'a mut Jar>,
}

/// The jar's `Cookie` header for `url`, when the request has none.
fn jar_cookie(jar: &Option<&mut Jar>, url: &str, headers: &[(String, String)]) -> Option<String> {
    let jar = jar.as_ref()?;
    if headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("cookie") && !v.is_empty()) {
        return None;
    }
    let value = jar.header(&reqwest::Url::parse(url).ok()?, cookies::now_ms());
    (!value.is_empty()).then_some(value)
}

fn jar_store(jar: &mut Option<&mut Jar>, resp: &reqwest::blocking::Response) {
    if let Some(jar) = jar {
        let lines: Vec<&str> = resp.headers().get_all(reqwest::header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()).collect();
        jar.merge(lines, resp.url(), cookies::now_ms());
    }
}

/// [`execute_with`] for a body of bytes (form and multipart bodies), with a
/// cookie jar.
pub fn execute_opts(method: &str, url: &str, headers: &[(String, String)], body: Vec<u8>, mut opts: Options) -> Result<RequestResult> {
    let mut header_map = HeaderMap::new();
    for (k, v) in headers {
        if k.is_empty() {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            header_map.insert(name, value);
        }
    }
    if let Some(cookie) = jar_cookie(&opts.jar, url, headers) {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            header_map.insert(reqwest::header::COOKIE, value);
        }
    }

    let method = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
        .unwrap_or(reqwest::Method::GET);

    check_url(url)?;
    let mut req = client().request(method, url).headers(header_map);
    if let Some(t) = opts.timeout {
        req = req.timeout(t);
    }

    if !body.is_empty() {
        req = req.body(body);
    }

    let start = Instant::now();
    let resp = req.send()?;
    let duration_ms = start.elapsed().as_millis() as u64;
    jar_store(&mut opts.jar, &resp);

    let status = resp.status().as_u16();

    let mut resp_headers: HashMap<String, String> = HashMap::new();
    for (k, v) in resp.headers() {
        resp_headers.insert(
            k.as_str().to_lowercase(),
            v.to_str().unwrap_or("").to_string(),
        );
    }

    // A body cut off by the timeout is an error, not an empty body.
    let body = match (opts.timeout, resp.text()) {
        (_, Ok(b)) => b,
        (Some(_), Err(e)) if e.is_timeout() => return Err(e.into()),
        (_, Err(_)) => String::new(),
    };

    Ok(RequestResult {
        status,
        headers: resp_headers,
        body,
        duration_ms,
    })
}

/// A response with every header as received, repeats included (each
/// `Set-Cookie` apart), names in lower case.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub duration_ms: u64,
}

/// [`execute`] for any body (a multipart body with files is bytes), keeping
/// repeated response headers. Empty header names are skipped; an invalid
/// header is an error rather than silently dropped.
pub fn send(method: &str, url: &str, headers: &[(String, String)], body: Vec<u8>) -> Result<Response> {
    send_with(method, url, headers, body, Options::default())
}

/// [`send`] with a timeout and a cookie jar.
pub fn send_with(method: &str, url: &str, headers: &[(String, String)], body: Vec<u8>, mut opts: Options) -> Result<Response> {
    let mut header_map = HeaderMap::new();
    for (k, v) in headers {
        if k.is_empty() {
            continue;
        }
        let name = HeaderName::from_bytes(k.as_bytes()).map_err(|_| anyhow::anyhow!("invalid header name {k:?}"))?;
        let value = HeaderValue::from_str(v).map_err(|_| anyhow::anyhow!("invalid value for header {k}"))?;
        header_map.append(name, value);
    }
    if let Some(cookie) = jar_cookie(&opts.jar, url, headers) {
        header_map.insert(reqwest::header::COOKIE, HeaderValue::from_str(&cookie).map_err(|_| anyhow::anyhow!("invalid cookie in the jar"))?);
    }
    let method = reqwest::Method::from_bytes(method.to_uppercase().as_bytes()).map_err(|_| anyhow::anyhow!("invalid method {method:?}"))?;
    check_url(url)?;
    let mut req = client().request(method, url).headers(header_map);
    if let Some(t) = opts.timeout {
        req = req.timeout(t);
    }
    if !body.is_empty() {
        req = req.body(body);
    }
    let start = Instant::now();
    let resp = req.send()?;
    jar_store(&mut opts.jar, &resp);
    let status = resp.status();
    let final_url = resp.url().to_string();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_lowercase(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();
    let body = resp.bytes()?.to_vec();
    Ok(Response {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        url: final_url,
        headers,
        body,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod send_tests {
    use super::*;
    use std::io::{Read, Write};

    /// One-shot HTTP server on localhost: returns the raw request it got.
    fn serve_once(response: &'static [u8]) -> (String, std::thread::JoinHandle<Vec<u8>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut got = vec![];
            let mut buf = [0u8; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&got);
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length: ").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                    if got.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            stream.write_all(response).unwrap();
            got
        });
        (url, handle)
    }

    #[test]
    fn send_keeps_repeated_headers_and_binary_bodies() {
        let (url, server) = serve_once(b"HTTP/1.1 201 Created\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Length: 3\r\nConnection: close\r\n\r\n\xff\x00A");
        let r = send("POST", &format!("{url}/up"), &[("X-A".into(), "1".into()), ("".into(), "skipped".into())], vec![0xff, 0x00, 0x41]).unwrap();
        let got = server.join().unwrap();
        let text = String::from_utf8_lossy(&got).to_ascii_lowercase();
        assert!(text.starts_with("post /up http/1.1\r\n") && text.contains("\r\nx-a: 1\r\n") && !text.contains("skipped"), "{text}");
        assert!(got.ends_with(&[0xff, 0x00, 0x41]), "the body goes out as bytes");
        assert_eq!((r.status, r.status_text.as_str()), (201, "Created"));
        let cookies: Vec<_> = r.headers.iter().filter(|(k, _)| k == "set-cookie").map(|(_, v)| v.as_str()).collect();
        assert_eq!(cookies, ["a=1", "b=2"]);
        assert_eq!(r.body, [0xff, 0x00, 0x41]);
        assert!(r.url.ends_with("/up"));
    }

    #[test]
    fn send_refuses_invalid_headers() {
        assert!(send("GET", "http://127.0.0.1:9/", &[("Bad Name".into(), "x".into())], vec![]).unwrap_err().to_string().contains("invalid header name"));
    }
}
