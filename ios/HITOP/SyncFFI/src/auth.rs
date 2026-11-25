//! Auth presets HitopKit doesn't have (OAuth 2.0, AWS Signature, Digest), by hitop-core. `auth` is the
//! preset's JSON with its variables filled in.
//!
//! - `authSign {auth, method, url, headers: [[name, value]], body}` → the headers with AWS's added;
//! - `authDigest {auth, challenge, method, url, headers}` → the headers to send again with, or null;
//! - `oauthToken {auth}` → `{token}`, or `{needsSignIn: true}` (authorization code, nobody signed in):
//!   cached until shortly before it expires, an expired one refreshed; blocking;
//! - `oauthSignIn {auth}` → `{id}`: PKCE and a loopback listener on its own thread; `oauthSignInNext
//!   {id}` → `{page}` (once), `{error}` or `{done}`; `oauthSignInCancel {id}`; `oauthForget`.
//!
//! Tokens are in memory only.
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use hitop_core::model::{Auth, OAuth2};
use hitop_core::oauth2::{self, Token};
use hitop_core::{aws_sigv4, digest_auth};
use serde_json::{json, Value};

use crate::{Args, R};

const LIMIT: Option<Duration> = Some(Duration::from_secs(30));

#[derive(Default)]
struct State {
    /// key → (token, when to take it again)
    tokens: HashMap<String, (Token, Option<Instant>)>,
    sign_ins: HashMap<u64, SignIn>,
    next: u64,
}

#[derive(Default)]
struct SignIn {
    page: Option<String>,
    error: Option<String>,
    done: bool,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner())
}

/// Variables are filled in already.
fn same(s: &str) -> anyhow::Result<String> {
    Ok(s.to_string())
}

fn err(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn oauth(a: &Args) -> Result<OAuth2, String> {
    match a.json("auth")? {
        Auth::OAuth2(o) => Ok(*o),
        _ => Err("argument auth: OAuth 2.0".into()),
    }
}

/// Everything the token depends on: one collection's never goes to another token URL or client.
fn key(o: &OAuth2) -> String {
    [&o.grant, &o.auth_url, &o.token_url, &o.client_id, &o.client_secret, &o.scope, &o.username, &o.password].map(String::as_str).join("\n")
}

fn keep(key: String, t: Token) -> String {
    let until = t.lifetime.map(|l| Instant::now() + Duration::from_secs(l.saturating_sub(30)));
    let access = t.access.clone();
    state().tokens.insert(key, (t, until));
    access
}

fn token(a: &Args) -> R {
    let o = oauth(a)?;
    let k = key(&o);
    let have = state().tokens.get(&k).cloned();
    let code = o.grant == "authorization_code";
    if let Some((t, until)) = have {
        if until.is_none_or(|u| Instant::now() < u) {
            return Ok(json!({ "token": t.access }));
        }
        if code {
            if let Some(new) = t.refresh.as_deref().and_then(|r| oauth2::refresh_with(&o, &same, r, LIMIT).ok()) {
                return Ok(json!({ "token": keep(k, new) }));
            }
        }
    }
    if code {
        return Ok(json!({ "needsSignIn": true }));
    }
    let req = oauth2::token_request(&o, &same).map_err(err)?;
    let opts = hitop_core::http::Options { timeout: LIMIT, ..Default::default() };
    let res = hitop_core::http::execute_opts("POST", &req.url, &req.headers, req.body.into_bytes(), opts).map_err(err)?;
    Ok(json!({ "token": keep(k, Token::parse(res.status, &res.body).map_err(err)?) }))
}

fn sign_in(a: &Args) -> R {
    let mut o = oauth(a)?;
    if !(o.auth_url.starts_with("https://") || o.auth_url.starts_with("http://")) {
        return Err("OAuth 2.0: the authorization URL must start with https:// or http://".into());
    }
    let k = key(&o);
    // Only a loopback address comes back here (the web app's callback page doesn't).
    if !(o.redirect_uri.starts_with("http://127.0.0.1:") || o.redirect_uri.starts_with("http://localhost:")) {
        o.redirect_uri = String::new();
    }
    let id = {
        let mut s = state();
        s.next += 1;
        let id = s.next;
        s.sign_ins.insert(id, SignIn::default());
        id
    };
    std::thread::spawn(move || {
        let got = oauth2::sign_in_with(&o, &same, LIMIT, &mut |url| {
            if let Some(s) = state().sign_ins.get_mut(&id) {
                s.page = Some(url.to_string());
            }
            Ok(())
        }, &|| !state().sign_ins.contains_key(&id));
        match got {
            Ok(t) => {
                // Cancelled meanwhile: not kept.
                if state().sign_ins.contains_key(&id) {
                    keep(k, t);
                    if let Some(s) = state().sign_ins.get_mut(&id) {
                        s.done = true;
                    }
                }
            }
            Err(e) => {
                if let Some(s) = state().sign_ins.get_mut(&id) {
                    s.error = Some(err(e));
                }
            }
        }
    });
    Ok(json!({ "id": id }))
}

fn sign_in_next(a: &Args) -> R {
    let id = a.u64("id")?;
    let mut st = state();
    let Some(s) = st.sign_ins.get_mut(&id) else { return Ok(json!({ "error": "OAuth 2.0: the sign-in was cancelled" })) };
    if let Some(page) = s.page.take() {
        return Ok(json!({ "page": page }));
    }
    let out = match (&s.error, s.done) {
        (Some(e), _) => json!({ "error": e }),
        (None, true) => json!({ "done": true }),
        (None, false) => return Ok(json!({})),
    };
    st.sign_ins.remove(&id);
    Ok(out)
}

pub fn call(op: &str, a: &Args) -> R {
    match op {
        "authSign" => {
            let auth: Auth = a.json("auth")?;
            let mut headers: Vec<(String, String)> = a.json("headers")?;
            aws_sigv4::sign_if_aws(Some(&auth), &same, a.str("method")?, a.str("url")?, &mut headers, &a.bytes("body")?).map_err(err)?;
            Ok(json!(headers))
        }
        "authDigest" => {
            let auth: Auth = a.json("auth")?;
            let headers: Vec<(String, String)> = a.json("headers")?;
            let answered = HashMap::from([("www-authenticate".to_string(), a.str("challenge")?.to_string())]);
            let out = digest_auth::retry_headers(Some(&auth), &same, 401, &answered, a.str("method")?, a.str("url")?, &headers).map_err(err)?;
            Ok(json!(out))
        }
        "oauthToken" => token(a),
        "oauthSignIn" => sign_in(a),
        "oauthSignInNext" => sign_in_next(a),
        "oauthSignInCancel" => {
            state().sign_ins.remove(&a.u64("id")?);
            Ok(Value::Null)
        }
        "oauthForget" => {
            state().tokens.clear();
            Ok(Value::Null)
        }
        other => Err(format!("unknown operation {other}")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    fn call(op: &str, a: Value) -> Value {
        serde_json::from_str(&crate::call_json(op, &a.to_string())).unwrap()
    }

    fn server(answer: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}/token", l.local_addr().unwrap().port());
        let seen = Arc::new(Mutex::new(vec![]));
        let s2 = seen.clone();
        std::thread::spawn(move || {
            for s in l.incoming() {
                let mut s = s.unwrap();
                let mut all = String::new();
                let mut buf = [0u8; 8192];
                loop {
                    let n = s.read(&mut buf).unwrap();
                    all.push_str(&String::from_utf8_lossy(&buf[..n]));
                    let Some((head, body)) = all.split_once("\r\n\r\n") else { continue };
                    let len = head.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                    if body.len() >= len || n == 0 {
                        break;
                    }
                }
                s2.lock().unwrap().push(all);
                write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len()).unwrap();
            }
        });
        (url, seen)
    }

    #[test]
    fn client_credentials_are_fetched_once() {
        let (url, seen) = server(r#"{"access_token":"cc","expires_in":3600}"#);
        let auth = json!({ "type": "oauth2", "grant": "client_credentials", "tokenUrl": url, "clientId": "a", "clientSecret": "s" });
        for _ in 0..2 {
            assert_eq!(call("oauthToken", json!({ "auth": auth }))["ok"]["token"], "cc");
        }
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_sign_in_then_its_refresh_and_cancel() {
        let (url, seen) = server(r#"{"access_token":"u","expires_in":30,"refresh_token":"r1"}"#);
        let auth = json!({ "type": "oauth2", "grant": "authorization_code", "authUrl": "https://id.test/a", "tokenUrl": url, "clientId": "a", "redirectUri": "https://hitop.rws8.tech/app/oauth-callback.html" });
        assert_eq!(call("oauthToken", json!({ "auth": auth }))["ok"]["needsSignIn"], true);
        let id = call("oauthSignIn", json!({ "auth": auth }))["ok"]["id"].clone();
        let next = |id: &Value, k: &str| {
            for _ in 0..500 {
                let n = call("oauthSignInNext", json!({ "id": id }))["ok"].clone();
                if !n[k].is_null() {
                    return n[k].clone();
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("no {k}");
        };
        let page = next(&id, "page").as_str().unwrap().to_string();
        let param = |k: &str| page.split(['?', '&']).find_map(|p| p.strip_prefix(&format!("{k}="))).unwrap().replace("%3A", ":").replace("%2F", "/");
        let host = param("redirect_uri").trim_start_matches("http://").split('/').next().unwrap().to_string();
        let mut s = TcpStream::connect(host).unwrap();
        write!(s, "GET /callback?code=c&state={} HTTP/1.1\r\nHost: x\r\n\r\n", param("state")).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        assert_eq!(next(&id, "done"), true);
        // Expired at once (30 s early): refreshed, no second sign-in.
        assert_eq!(call("oauthToken", json!({ "auth": auth }))["ok"]["token"], "u");
        assert!(seen.lock().unwrap()[1].contains("grant_type=refresh_token&refresh_token=r1"));
        let id = call("oauthSignIn", json!({ "auth": auth }))["ok"]["id"].clone();
        call("oauthSignInCancel", json!({ "id": id }));
        assert!(call("oauthSignInNext", json!({ "id": id }))["ok"]["error"].as_str().unwrap().contains("cancelled"));
    }

    #[test]
    fn aws_and_digest() {
        let aws = json!({ "type": "awsv4", "accessKey": "AK", "secretKey": "SK", "region": "us-east-1", "service": "s3" });
        let h = call("authSign", json!({ "auth": aws, "method": "GET", "url": "https://b.s3.amazonaws.com/k", "headers": [["X-A", "1"]], "body": "" }));
        assert!(h["ok"].to_string().contains("AWS4-HMAC-SHA256 Credential=AK/"), "{h}");
        let digest = json!({ "type": "digest", "username": "u", "password": "p" });
        let h = call("authDigest", json!({ "auth": digest, "challenge": "Digest realm=\"r\", nonce=\"n\", qop=\"auth\"", "method": "GET", "url": "https://h.test/a?b=1", "headers": [] }));
        assert!(h["ok"].to_string().contains("uri=\\\"/a?b=1\\\""), "{h}");
        assert!(call("authDigest", json!({ "auth": digest, "challenge": "Basic realm=x", "method": "GET", "url": "https://h.test/", "headers": [] }))["ok"].is_null());
    }
}
