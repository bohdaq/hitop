#![cfg(feature = "native")]
use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::cookies::{self, Jar};
use crate::redirects::{self, Follower, Hop};
use crate::model::{RequestResult, Timings};
use crate::timing::{Recorder, TimedConnectLayer, TimedResolver};

static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
/// The default client's timing hooks; unset when another client was installed.
static RECORDER: std::sync::OnceLock<Recorder> = std::sync::OnceLock::new();

/// A check every URL passes before it is sent.
pub type UrlCheck = fn(&str) -> std::result::Result<(), String>;

static URL_CHECK: std::sync::OnceLock<UrlCheck> = std::sync::OnceLock::new();

/// Every request's URL goes through `check` before it is sent (the sync server's monitors refuse
/// private addresses, which a client's DNS resolver never sees when the URL has an IP literal).
pub fn install_url_check(check: UrlCheck) -> bool {
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

fn builder() -> reqwest::blocking::ClientBuilder {
    let recorder = RECORDER.get_or_init(Recorder::default).clone();
    Client::builder()
        .use_rustls_tls()
        // Redirects are followed by `exchange` (src/redirects.rs), one hop at a time.
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(std::sync::Arc::new(TimedResolver::new(recorder.clone())))
        .connector_layer(TimedConnectLayer::new(recorder))
}

/// A host's network settings, for a person who turned them on (the Android app): certificate
/// authorities trusted beside the webpki roots (DER) and a proxy for HTTP requests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Network {
    pub roots: Vec<Vec<u8>>,
    /// `http://host:port`.
    pub proxy: Option<String>,
    /// Hosts that don't use the proxy, comma separated.
    pub no_proxy: String,
}

impl Network {
    /// `b` with these settings.
    pub fn apply(&self, mut b: reqwest::blocking::ClientBuilder) -> Result<reqwest::blocking::ClientBuilder> {
        for der in &self.roots {
            b = b.add_root_certificate(reqwest::Certificate::from_der(der)?);
        }
        if let Some(p) = &self.proxy {
            b = b.proxy(reqwest::Proxy::all(p)?.no_proxy(reqwest::NoProxy::from_string(&self.no_proxy)));
        }
        Ok(b)
    }
}

static NETWORK: std::sync::RwLock<Option<(Network, Client)>> = std::sync::RwLock::new(None);

/// Uses `n` from now on (None: the defaults, as when this is never called).
pub fn set_network(n: Option<Network>) -> Result<()> {
    if network() == n {
        return Ok(());
    }
    let next = match n {
        Some(n) => {
            let c = n.apply(builder())?.build()?;
            Some((n, c))
        }
        None => None,
    };
    *NETWORK.write().unwrap_or_else(|p| p.into_inner()) = next;
    Ok(())
}

/// The settings in use, if any.
pub fn network() -> Option<Network> {
    NETWORK.read().unwrap_or_else(|p| p.into_inner()).as_ref().map(|(n, _)| n.clone())
}

fn client() -> Client {
    if let Some((_, c)) = NETWORK.read().unwrap_or_else(|p| p.into_inner()).as_ref() {
        return c.clone();
    }
    CLIENT.get_or_init(|| builder().build().expect("failed to build HTTP client")).clone()
}

/// The phases of a request to `url` (None with an installed client, which has no hooks).
fn timings(url: &str, start: Instant, headers: Instant, done: Instant) -> Option<Timings> {
    RECORDER.get()?.phases_for(url, start, headers, done)
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
    execute_opts(method, url, headers, body.as_bytes().to_vec(), Options { timeout, jar: None, ..Default::default() })
}

/// How [`execute_opts`] and [`send_with`] send a request.
#[derive(Default)]
pub struct Options<'a> {
    /// Time limit for the whole request (connect, send and reading the body).
    pub timeout: Option<Duration>,
    /// Cookies: the jar's go out with the request (unless it has a `Cookie`
    /// header already) and the response's `Set-Cookie` lines go in.
    pub jar: Option<&'a mut Jar>,
    /// Answer with the first response, even a redirect (the request's "Follow redirects" off).
    pub no_redirects: bool,
}

/// One request with its redirects: each hop is sent with redirects off and read by a
/// [`Follower`], so cookies a hop sets are kept and sent on (src/redirects.rs).
struct Exchange {
    resp: reqwest::blocking::Response,
    hops: Vec<Hop>,
    start: Instant,
    headers_at: Instant,
}

fn exchange(method: &str, url: &str, headers: Vec<(String, String)>, body: Vec<u8>, opts: &mut Options, strict: bool) -> Result<Exchange> {
    let parsed = reqwest::Url::parse(url)?;
    let jar = opts.jar.as_deref().cloned().unwrap_or_default();
    let mut follower = Follower::new(method, parsed, headers, body, jar, !opts.no_redirects);
    let start = Instant::now();
    loop {
        let step = follower.request();
        let mut header_map = HeaderMap::new();
        for (k, v) in &step.headers {
            if k.is_empty() {
                continue;
            }
            match (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                // A saved request sends the last of two headers with one name (what its snippets
                // show); `hitop send -H` sends each one given.
                (Ok(name), Ok(value)) if strict => {
                    header_map.append(name, value);
                }
                (Ok(name), Ok(value)) => {
                    header_map.insert(name, value);
                }
                _ if strict => {
                    HeaderName::from_bytes(k.as_bytes()).map_err(|_| anyhow::anyhow!("invalid header name {k:?}"))?;
                    anyhow::bail!("invalid value for header {k}");
                }
                _ => {}
            }
        }
        let m = reqwest::Method::from_bytes(step.method.to_uppercase().as_bytes());
        let m = match m {
            Ok(m) => m,
            Err(_) if strict => anyhow::bail!("invalid method {:?}", step.method),
            Err(_) => reqwest::Method::GET,
        };
        check_url(step.url.as_str())?;
        let mut req = client().request(m, step.url.clone()).headers(header_map);
        if let Some(t) = opts.timeout {
            req = req.timeout(t.saturating_sub(start.elapsed()).max(Duration::from_millis(1)));
        }
        if !step.body.is_empty() {
            req = req.body(step.body);
        }
        let resp = req.send()?;
        let headers_at = Instant::now();
        let lines = redirects::set_cookie_lines(resp.headers());
        if let Some(jar) = opts.jar.as_deref_mut() {
            jar.merge(lines.iter().map(String::as_str), resp.url(), cookies::now_ms());
        }
        let again = follower.answer(resp.status().as_u16(), redirects::location(resp.headers()).as_deref(), lines).map_err(|e| anyhow::anyhow!(e))?;
        if !again {
            return Ok(Exchange { resp, hops: follower.hops, start, headers_at });
        }
    }
}

/// [`execute_with`] for a body of bytes (form and multipart bodies), with a
/// cookie jar.
pub fn execute_opts(method: &str, url: &str, headers: &[(String, String)], body: Vec<u8>, mut opts: Options) -> Result<RequestResult> {
    let Exchange { resp, hops, start, headers_at } = exchange(method, url, headers.to_vec(), body, &mut opts, false)?;
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
    let done = Instant::now();

    Ok(RequestResult {
        status,
        headers: resp_headers,
        body,
        duration_ms: done.duration_since(start).as_millis() as u64,
        timings: timings(url, start, headers_at, done),
        redirects: hops,
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
    /// Where the time went, when the client could tell (src/timing.rs).
    pub timings: Option<Timings>,
    /// The response's protocol version ("HTTP/1.1", "HTTP/2", "HTTP/3"); `None` when unknown.
    /// Not in any run report: runs go through [`execute_opts`].
    pub http_version: Option<&'static str>,
    /// The redirects on the way, first one first (empty when there were none).
    pub redirects: Vec<Hop>,
}

/// A protocol version as the apps show it; `None` for one reqwest doesn't name.
pub fn version_name(v: reqwest::Version) -> Option<&'static str> {
    match v {
        reqwest::Version::HTTP_09 => Some("HTTP/0.9"),
        reqwest::Version::HTTP_10 => Some("HTTP/1.0"),
        reqwest::Version::HTTP_11 => Some("HTTP/1.1"),
        reqwest::Version::HTTP_2 => Some("HTTP/2"),
        reqwest::Version::HTTP_3 => Some("HTTP/3"),
        _ => None,
    }
}

/// [`execute`] for any body (a multipart body with files is bytes), keeping
/// repeated response headers. Empty header names are skipped; an invalid
/// header is an error rather than silently dropped.
pub fn send(method: &str, url: &str, headers: &[(String, String)], body: Vec<u8>) -> Result<Response> {
    send_with(method, url, headers, body, Options::default())
}

/// [`send`] with a timeout and a cookie jar.
pub fn send_with(method: &str, url: &str, headers: &[(String, String)], body: Vec<u8>, mut opts: Options) -> Result<Response> {
    let Exchange { resp, hops, start, headers_at } = exchange(method, url, headers.to_vec(), body, &mut opts, true)?;
    let status = resp.status();
    let final_url = resp.url().to_string();
    let http_version = version_name(resp.version());
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_lowercase(), String::from_utf8_lossy(v.as_bytes()).into_owned()))
        .collect();
    let body = resp.bytes()?.to_vec();
    let done = Instant::now();
    Ok(Response {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        url: final_url,
        headers,
        body,
        duration_ms: done.duration_since(start).as_millis() as u64,
        timings: timings(url, start, headers_at, done),
        http_version,
        redirects: hops,
    })
}

/// [`send_with`] for an answer read as it arrives (Server-Sent Events): `on_head` gets the response
/// without its body, then `on_chunk` each piece; either one ends the stream by answering false.
/// Without a time limit in `opts` there is none (the client's own would cut the stream).
pub fn stream_with(method: &str, url: &str, headers: &[(String, String)], body: Vec<u8>, mut opts: Options, on_head: &mut dyn FnMut(&Response) -> bool, on_chunk: &mut dyn FnMut(&[u8]) -> bool) -> Result<()> {
    use std::io::Read;
    opts.timeout.get_or_insert(Duration::from_secs(365 * 24 * 3600));
    let Exchange { mut resp, hops, start, headers_at } = exchange(method, url, headers.to_vec(), body, &mut opts, true)?;
    let head = Response {
        status: resp.status().as_u16(),
        status_text: resp.status().canonical_reason().unwrap_or("").to_string(),
        url: resp.url().to_string(),
        headers: resp.headers().iter().map(|(k, v)| (k.as_str().to_lowercase(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect(),
        body: vec![],
        duration_ms: headers_at.duration_since(start).as_millis() as u64,
        timings: None,
        http_version: version_name(resp.version()),
        redirects: hops,
    };
    if !on_head(&head) {
        return Ok(());
    }
    let mut buf = [0u8; 8192];
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 || !on_chunk(&buf[..n]) {
            return Ok(());
        }
    }
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

    /// A local server answering `/login` with a 302 that sets a cookie and `/home` with the
    /// Cookie header it got; each connection is closed after one answer.
    fn login_server() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().take(4) {
                let mut stream = stream.unwrap();
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap();
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                let answer = if text.starts_with("GET /login") || text.starts_with("POST /login") {
                    "HTTP/1.1 302 Found\r\nLocation: /home\r\nSet-Cookie: sid=abc; Path=/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                } else {
                    let cookie = text.lines().find_map(|l| l.strip_prefix("cookie: ").or_else(|| l.strip_prefix("Cookie: "))).unwrap_or("").to_string();
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{cookie}", cookie.len())
                };
                stream.write_all(answer.as_bytes()).unwrap();
            }
        });
        url
    }

    #[test]
    fn a_cookie_set_on_a_redirect_is_sent_on_and_kept() {
        let url = login_server();
        let mut jar = Jar::default();
        let r = send_with("POST", &format!("{url}/login"), &[], b"x".to_vec(), Options { jar: Some(&mut jar), ..Default::default() }).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(String::from_utf8_lossy(&r.body), "sid=abc", "the next hop got the cookie");
        assert_eq!(r.redirects.len(), 1);
        assert_eq!((r.redirects[0].status, r.redirects[0].location.as_str()), (302, format!("{url}/home").as_str()));
        assert_eq!(r.redirects[0].set_cookies, ["sid=abc; Path=/"]);
        assert_eq!(jar.cookies.len(), 1, "the jar kept the hop's cookie");
        let r = send_with("GET", &format!("{url}/login"), &[], vec![], Options { no_redirects: true, ..Default::default() }).unwrap();
        assert_eq!(r.status, 302, "not followed when asked");
        assert!(r.redirects.is_empty());
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
        assert_eq!(r.http_version, Some("HTTP/1.1"), "the protocol version comes out");
    }

    /// Serves `n` requests on one kept-alive connection.
    fn serve_keep_alive(n: usize) -> (u16, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            for _ in 0..n {
                let mut got = vec![];
                while !String::from_utf8_lossy(&got).contains("\r\n\r\n") {
                    let k = stream.read(&mut buf).unwrap();
                    got.extend_from_slice(&buf[..k]);
                }
                std::thread::sleep(Duration::from_millis(15));
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").unwrap();
            }
        });
        (port, handle)
    }

    #[test]
    fn timings_split_a_new_connection_and_see_a_reused_one() {
        let (port, server) = serve_keep_alive(2);
        // "localhost" goes through the timed resolver; the server waits 15 ms before answering.
        let url = format!("http://localhost:{port}/t");
        let first = send("GET", &url, &[], vec![]).unwrap();
        let t = first.timings.expect("the default client times requests");
        assert!(!t.reused, "{t:?}");
        assert_eq!(t.dns_ms + t.connect_ms + t.wait_ms + t.download_ms, t.total_ms, "{t:?}");
        assert!(t.wait_ms >= 10, "the server's think time is waiting: {t:?}");
        let second = execute("GET", &url, &[], "").unwrap();
        let t2 = second.timings.unwrap();
        assert_eq!(t2.dns_ms, 0, "no lookup on a pooled connection: {t2:?}");
        assert_eq!(t2.dns_ms + t2.connect_ms + t2.wait_ms + t2.download_ms, t2.total_ms);
        assert_eq!(t2.total_ms, second.duration_ms);
        server.join().unwrap();
    }

    #[test]
    fn send_refuses_invalid_headers() {
        assert!(send("GET", "http://127.0.0.1:9/", &[("Bad Name".into(), "x".into())], vec![]).unwrap_err().to_string().contains("invalid header name"));
    }

    #[test]
    fn a_stream_is_handed_over_as_it_arrives_and_can_be_ended() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (go, wait) = std::sync::mpsc::channel::<()>();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let _ = s.read(&mut [0u8; 4096]);
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: 1\n\n").unwrap();
            // The second event only once the first was read.
            wait.recv().unwrap();
            let _ = s.write_all(b"data: 2\n\n");
            let _ = wait.recv();
        });
        let mut text = String::new();
        let mut status = 0;
        stream_with("GET", &url, &[], vec![], Options::default(), &mut |h| { status = h.status; true }, &mut |b| {
            text.push_str(&String::from_utf8_lossy(b));
            let _ = go.send(());
            !text.contains("data: 2")
        }).unwrap();
        assert_eq!((status, text.as_str()), (200, "data: 1\n\ndata: 2\n\n"));
    }
}
