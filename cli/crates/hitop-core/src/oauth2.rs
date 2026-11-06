//! OAuth 2.0 auth (RFC 6749): the token request for the client credentials and password grants,
//! its answer, and (native) a cache that keeps each token in memory until shortly before it
//! expires. The token goes out as `Authorization: Bearer`; it is never written anywhere.
use std::collections::HashMap;

use anyhow::{bail, Result};
use base64::Engine;
use serde_json::Value;

use crate::model::{Auth, OAuth2};

/// The token request: POST `url` with a form body.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// What the token is cached by (URL, client, scope, user).
    pub key: String,
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The request for `o`, its values filled in by `interpolate`.
pub fn token_request(o: &OAuth2, interpolate: &dyn Fn(&str) -> Result<String>) -> Result<TokenRequest> {
    let url = interpolate(&o.token_url)?;
    if url.trim().is_empty() {
        bail!("OAuth 2.0: the token URL is empty");
    }
    let client_id = interpolate(&o.client_id)?;
    let secret = interpolate(&o.client_secret)?;
    let scope = interpolate(&o.scope)?;
    let grant = if o.grant.is_empty() { "client_credentials" } else { o.grant.as_str() };
    let mut pairs: Vec<(&str, String)> = vec![("grant_type", grant.to_string())];
    let (user, pass) = match grant {
        "client_credentials" => (String::new(), String::new()),
        "password" => (interpolate(&o.username)?, interpolate(&o.password)?),
        "authorization_code" => bail!("OAuth 2.0: the authorization code grant needs a person to sign in in a browser"),
        other => bail!("OAuth 2.0: the grant {other} isn't supported (client_credentials, password or authorization_code)"),
    };
    if grant == "password" {
        pairs.push(("username", user.clone()));
        pairs.push(("password", pass));
    }
    pairs.push(("scope", scope.clone()));
    let mut headers = vec![("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()), ("Accept".to_string(), "application/json".to_string())];
    if o.client_auth == "body" {
        pairs.push(("client_id", client_id.clone()));
        pairs.push(("client_secret", secret));
    } else {
        // RFC 6749 §2.3.1: each part form-encoded, then Basic.
        let creds = format!("{}:{}", enc(&client_id), enc(&secret));
        headers.push(("Authorization".into(), format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(creds))));
    }
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Ok(TokenRequest { key: format!("{url}\n{client_id}\n{scope}\n{user}"), url, headers, body: form(&refs) })
}

/// The sign-in page for the authorization code grant with PKCE (S256).
pub fn authorize_url(o: &OAuth2, interpolate: &dyn Fn(&str) -> Result<String>, redirect_uri: &str, challenge: &str, state: &str) -> Result<String> {
    let base = interpolate(&o.auth_url)?;
    if base.trim().is_empty() {
        bail!("OAuth 2.0: the authorization URL is empty");
    }
    let q = form(&[("response_type", "code"), ("client_id", &interpolate(&o.client_id)?), ("redirect_uri", redirect_uri), ("scope", &interpolate(&o.scope)?), ("state", state), ("code_challenge", challenge), ("code_challenge_method", "S256")]);
    Ok(format!("{base}{}{q}", if base.contains('?') { "&" } else { "?" }))
}

/// The token request that trades the code (with the PKCE verifier) for a token.
pub fn code_request(o: &OAuth2, interpolate: &dyn Fn(&str) -> Result<String>, code: &str, verifier: &str, redirect_uri: &str) -> Result<TokenRequest> {
    let url = interpolate(&o.token_url)?;
    if url.trim().is_empty() {
        bail!("OAuth 2.0: the token URL is empty");
    }
    let client_id = interpolate(&o.client_id)?;
    let secret = interpolate(&o.client_secret)?;
    let mut pairs = vec![("grant_type", "authorization_code".to_string()), ("code", code.to_string()), ("redirect_uri", redirect_uri.to_string()), ("code_verifier", verifier.to_string()), ("client_id", client_id.clone())];
    let mut headers = vec![("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()), ("Accept".to_string(), "application/json".to_string())];
    if !secret.is_empty() {
        if o.client_auth == "body" {
            pairs.push(("client_secret", secret));
        } else {
            let creds = format!("{}:{}", enc(&client_id), enc(&secret));
            headers.push(("Authorization".into(), format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(creds))));
        }
    }
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Ok(TokenRequest { key: format!("{url}\n{client_id}\n{}\ncode", interpolate(&o.scope)?), url, headers, body: form(&refs) })
}

#[cfg(feature = "native")]
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                Ok(v) => {
                    out.push(v);
                    i += 3;
                    continue;
                }
                Err(_) => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The access token and its lifetime in seconds from the token endpoint's answer.
pub fn parse_token(status: u16, body: &str) -> Result<(String, Option<u64>)> {
    let json: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let why = json["error_description"].as_str().or(json["error"].as_str()).unwrap_or("no reason given");
        bail!("OAuth 2.0: the token endpoint answered {status}: {why}");
    }
    let Some(token) = json["access_token"].as_str().filter(|t| !t.is_empty()) else {
        bail!("OAuth 2.0: the token endpoint's answer has no access_token");
    };
    Ok((token.to_string(), json["expires_in"].as_u64().or_else(|| json["expires_in"].as_str().and_then(|s| s.parse().ok()))))
}

/// A person signs in for the authorization code grant: (access token, lifetime in seconds).
pub type SignIn = Box<dyn FnMut(&OAuth2, &dyn Fn(&str) -> Result<String>) -> Result<(String, Option<u64>)>>;

/// Tokens by `TokenRequest::key`: (token, valid until, as seconds of `now`). `sign_in` handles the
/// authorization code grant ([`Tokens::interactive`]: in a browser, when a person is at the terminal).
#[derive(Default)]
pub struct Tokens(HashMap<String, (String, Option<u64>)>, Option<SignIn>);

impl Tokens {
    /// For `auth` when it is OAuth 2.0: a Bearer preset with a token from the cache or from
    /// `fetch(request) -> (status, body)`; None for any other auth. `now`: seconds.
    pub fn resolve(
        &mut self,
        auth: Option<&Auth>,
        interpolate: &dyn Fn(&str) -> Result<String>,
        now: u64,
        fetch: &mut dyn FnMut(&TokenRequest) -> Result<(u16, String)>,
    ) -> Result<Option<Auth>> {
        let Some(Auth::OAuth2(o)) = auth else { return Ok(None) };
        if o.grant == "authorization_code" {
            let key = format!("code\n{}\n{}\n{}", interpolate(&o.auth_url)?, interpolate(&o.client_id)?, interpolate(&o.scope)?);
            if let Some((token, until)) = self.0.get(&key) {
                if until.is_none_or(|u| now < u) {
                    return Ok(Some(Auth::Bearer { token: token.clone() }));
                }
            }
            let sign_in = self.1.as_mut().ok_or_else(|| anyhow::anyhow!("OAuth 2.0: the authorization code grant needs a person to sign in in a browser: run hitop send or hitop requests send in a terminal, or use client credentials in CI"))?;
            let (token, lifetime) = sign_in(o, interpolate)?;
            self.0.insert(key, (token.clone(), lifetime.map(|l| now + l.saturating_sub(30))));
            return Ok(Some(Auth::Bearer { token }));
        }
        let req = token_request(o, interpolate)?;
        if let Some((token, until)) = self.0.get(&req.key) {
            if until.is_none_or(|u| now < u) {
                return Ok(Some(Auth::Bearer { token: token.clone() }));
            }
        }
        let (status, body) = fetch(&req)?;
        let (token, lifetime) = parse_token(status, &body)?;
        // Taken again 30 s before it runs out.
        self.0.insert(req.key, (token.clone(), lifetime.map(|l| now + l.saturating_sub(30))));
        Ok(Some(Auth::Bearer { token }))
    }

    /// Tokens whose authorization code grant signs in in a browser (when a person is there).
    #[cfg(feature = "native")]
    pub fn interactive(timeout: Option<std::time::Duration>) -> Self {
        Tokens(HashMap::new(), Some(Box::new(move |o, fill| browser_sign_in(o, fill, timeout))))
    }
}

/// The authorization code grant with PKCE: the sign-in page in the browser (or
/// `HITOP_OAUTH_OPENER <url>`, tests), the code caught on a loopback address, traded for a token.
/// Only with a person at the terminal (stderr is one), unless an opener is set.
#[cfg(feature = "native")]
pub fn browser_sign_in(o: &OAuth2, interpolate: &dyn Fn(&str) -> Result<String>, timeout: Option<std::time::Duration>) -> Result<(String, Option<u64>)> {
    use std::io::{IsTerminal, Read, Write};
    use std::time::{Duration, Instant};
    let opener = std::env::var("HITOP_OAUTH_OPENER").ok().filter(|s| !s.is_empty());
    if opener.is_none() && !std::io::stderr().is_terminal() {
        bail!("OAuth 2.0: the authorization code grant needs a person to sign in in a browser: run it in a terminal, or use client credentials in CI");
    }
    let rand = ring::rand::SystemRandom::new();
    let mut bytes = [0u8; 32];
    ring::rand::SecureRandom::fill(&rand, &mut bytes).map_err(|_| anyhow::anyhow!("no randomness"))?;
    let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
    let verifier = b64(&bytes);
    let challenge = b64(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()).as_ref());
    ring::rand::SecureRandom::fill(&rand, &mut bytes[..16]).map_err(|_| anyhow::anyhow!("no randomness"))?;
    let state = b64(&bytes[..16]);
    let given = interpolate(&o.redirect_uri)?;
    let port = reqwest::Url::parse(&given).ok().filter(|u| matches!(u.host_str(), Some("127.0.0.1" | "localhost"))).and_then(|u| u.port()).unwrap_or(0);
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("OAuth 2.0: can't listen for the sign-in on 127.0.0.1:{port}: {e}"))?;
    let redirect = if given.is_empty() { format!("http://127.0.0.1:{}/callback", listener.local_addr()?.port()) } else { given };
    let url = authorize_url(o, interpolate, &redirect, &challenge, &state)?;
    match &opener {
        Some(cmd) => {
            std::process::Command::new("sh").arg("-c").arg(format!("{cmd} \"$0\"")).arg(&url).spawn()?;
        }
        None => {
            eprintln!("Sign in to continue: {url}");
            let open = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
            let _ = std::process::Command::new(open).arg(&url).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
        }
    }
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_secs(180);
    let code = loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(10)))?;
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]);
                let target = head.split_whitespace().nth(1).unwrap_or("/");
                let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
                let param = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(&format!("{k}="))).map(pct_decode);
                let (code, error) = (param("code"), param("error"));
                let ok = code.is_some() && param("state").as_deref() == Some(state.as_str());
                let page = if ok { "Signed in: you can close this tab and go back to hitop." } else { "The sign-in didn't finish: go back to hitop." };
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
                if let Some(e) = error {
                    bail!("OAuth 2.0: the sign-in was refused: {e}");
                }
                if ok {
                    break code.unwrap_or_default();
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => bail!("OAuth 2.0: no sign-in within 3 minutes"),
        }
    };
    let req = code_request(o, interpolate, &code, &verifier, &redirect)?;
    let res = crate::http::execute_opts("POST", &req.url, &req.headers, req.body.into_bytes(), crate::http::Options { timeout, jar: None })?;
    parse_token(res.status, &res.body)
}

/// One request's OAuth 2.0 token, fetched now (`hitop send`, `hitop requests send`): a Bearer
/// preset, or None when `auth` isn't OAuth 2.0.
#[cfg(feature = "native")]
pub fn fetch_once(auth: Option<&Auth>, variables: &HashMap<String, String>, timeout: Option<std::time::Duration>) -> Result<Option<Auth>> {
    Tokens::interactive(timeout).resolve(auth, &|s| crate::interpolation::interpolate(s, variables), 0, &mut |r| {
        let res = crate::http::execute_opts("POST", &r.url, &r.headers, r.body.clone().into_bytes(), crate::http::Options { timeout, jar: None })?;
        Ok((res.status, res.body))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(s: &str) -> Result<String> {
        Ok(s.replace("${secret}", "s3 cret"))
    }

    fn cc() -> OAuth2 {
        OAuth2 { grant: "client_credentials".into(), token_url: "https://id.test/token".into(), client_id: "app".into(), client_secret: "${secret}".into(), scope: "read write".into(), ..Default::default() }
    }

    #[test]
    fn client_credentials_with_basic_client_auth() {
        let r = token_request(&cc(), &same).unwrap();
        assert_eq!(r.body, "grant_type=client_credentials&scope=read%20write");
        let basic = base64::engine::general_purpose::STANDARD.encode("app:s3%20cret");
        assert!(r.headers.contains(&("Authorization".into(), format!("Basic {basic}"))));
    }

    #[test]
    fn password_grant_with_the_client_in_the_body() {
        let o = OAuth2 { grant: "password".into(), username: "ada".into(), password: "p&w".into(), client_auth: "body".into(), scope: String::new(), ..cc() };
        let r = token_request(&o, &same).unwrap();
        assert_eq!(r.body, "grant_type=password&username=ada&password=p%26w&client_id=app&client_secret=s3%20cret");
        assert!(!r.headers.iter().any(|(k, _)| k == "Authorization"));
        assert!(token_request(&OAuth2 { grant: "implicit".into(), ..cc() }, &same).is_err());
    }

    #[test]
    fn answers_and_the_cache() {
        assert_eq!(parse_token(200, r#"{"access_token":"t1","expires_in":3600}"#).unwrap(), ("t1".into(), Some(3600)));
        assert!(parse_token(401, r#"{"error":"invalid_client"}"#).unwrap_err().to_string().contains("invalid_client"));
        assert!(parse_token(200, "{}").is_err());
        let auth = Auth::OAuth2(Box::new(cc()));
        let mut tokens = Tokens::default();
        let mut calls = 0;
        let mut fetch = |_: &TokenRequest| -> Result<(u16, String)> {
            calls += 1;
            Ok((200, format!(r#"{{"access_token":"tok{calls}","expires_in":100}}"#)))
        };
        let first = tokens.resolve(Some(&auth), &same, 1000, &mut fetch).unwrap();
        let again = tokens.resolve(Some(&auth), &same, 1050, &mut fetch).unwrap();
        let later = tokens.resolve(Some(&auth), &same, 1080, &mut fetch).unwrap();
        assert_eq!(first, Some(Auth::Bearer { token: "tok1".into() }));
        assert_eq!(again, first, "cached while valid");
        assert_eq!(later, Some(Auth::Bearer { token: "tok2".into() }), "taken again 30 s before it runs out");
        assert_eq!(tokens.resolve(Some(&Auth::None), &same, 0, &mut fetch).unwrap(), None);
    }
}
