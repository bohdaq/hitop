use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::model::RequestResult;

static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();

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

    let method = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())
        .unwrap_or(reqwest::Method::GET);

    let mut req = client().request(method, url).headers(header_map);
    if let Some(t) = timeout {
        req = req.timeout(t);
    }

    if !body.is_empty() {
        req = req.body(body.to_string());
    }

    let start = Instant::now();
    let resp = req.send()?;
    let duration_ms = start.elapsed().as_millis() as u64;

    let status = resp.status().as_u16();

    let mut resp_headers: HashMap<String, String> = HashMap::new();
    for (k, v) in resp.headers() {
        resp_headers.insert(
            k.as_str().to_lowercase(),
            v.to_str().unwrap_or("").to_string(),
        );
    }

    // A body cut off by the timeout is an error, not an empty body.
    let body = match (timeout, resp.text()) {
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
