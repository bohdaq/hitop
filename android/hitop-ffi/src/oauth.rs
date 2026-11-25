//! OAuth 2.0's authorization code grant on a phone: a send asks the app (`oauthPending {sendId}`)
//! to open the sign-in page; hitop-core catches the answer on a loopback address (PKCE, state) and
//! trades it. Tokens stay in memory until they expire, the app ends or `oauthForget`; `oauthCancel
//! {sendId}` ends a sign-in. Runs use a token that is there and never open a browser.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use hitop_core::model::OAuth2;
use hitop_core::oauth2::{self, Token};
use serde_json::Value;

use crate::Args;

#[derive(Default)]
struct State {
    /// key → (token, until)
    tokens: HashMap<String, (Token, Option<Instant>)>,
    /// send id → the page to open (taken once)
    pages: HashMap<u64, Option<String>>,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner())
}

thread_local! {
    static SEND: Cell<Option<u64>> = const { Cell::new(None) };
}

fn sign_in(o: &OAuth2, fill: &dyn Fn(&str) -> Result<String>, timeout: Option<Duration>) -> Result<Token> {
    let mut key = String::new();
    for part in [&o.auth_url, &o.token_url, &o.client_id, &o.client_secret, &o.scope] {
        key.push_str(&fill(part)?);
        key.push('\n');
    }
    let have = state().tokens.get(&key).cloned();
    if let Some((token, until)) = have {
        let left = until.map(|u| u.saturating_duration_since(Instant::now()));
        if left.is_none_or(|l| !l.is_zero()) {
            // What is left of it (it is taken again 30 s early), so a long run comes back here.
            return Ok(Token { lifetime: left.map(|l| l.as_secs() + 30), refresh: None, ..token });
        }
        // Expired: the refresh token first, also in a run; a refusal signs in again.
        if let Some(t) = token.refresh.as_deref().and_then(|r| oauth2::refresh_with(o, fill, r, timeout).ok()) {
            return Ok(keep(key, t));
        }
    }
    let Some(id) = SEND.get() else {
        bail!("OAuth 2.0: sign in first by sending one request that uses it, then run the collection");
    };
    let mut o = o.clone();
    // Only a loopback address comes back here (the web app's callback page doesn't).
    let redirect = fill(&o.redirect_uri)?;
    if !(redirect.starts_with("http://127.0.0.1:") || redirect.starts_with("http://localhost:")) {
        o.redirect_uri = String::new();
    }
    let page = fill(&o.auth_url)?;
    if !(page.starts_with("https://") || page.starts_with("http://")) {
        bail!("OAuth 2.0: the authorization URL must start with https:// or http://");
    }
    let token = oauth2::sign_in_with(&o, fill, timeout, &mut |url| {
        if let Some(p) = state().pages.get_mut(&id) {
            *p = Some(url.to_string());
        }
        Ok(())
    }, &|| !state().pages.contains_key(&id))?;
    Ok(keep(key, token))
}

/// Remembers `t` here; its refresh token stays here.
fn keep(key: String, t: Token) -> Token {
    let until = t.lifetime.map(|l| Instant::now() + Duration::from_secs(l.saturating_sub(30)));
    state().tokens.insert(key, (t.clone(), until));
    Token { refresh: None, ..t }
}

/// This thread's send (None: nobody to ask), until it is dropped.
pub(crate) struct Scope(Option<u64>);

impl Scope {
    pub(crate) fn new(id: Option<u64>) -> Scope {
        oauth2::install_sign_in(sign_in);
        if let Some(id) = id {
            state().pages.insert(id, None);
        }
        SEND.set(id);
        Scope(id)
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        SEND.set(None);
        if let Some(id) = self.0 {
            state().pages.remove(&id);
        }
    }
}

/// The page to open for a send, once; null when there is none.
pub(crate) fn pending(a: &Args) -> Result<Value, String> {
    let id = a.u64("sendId")?;
    Ok(state().pages.get_mut(&id).and_then(Option::take).map_or(Value::Null, Value::String))
}

pub(crate) fn cancel(a: &Args) -> Result<Value, String> {
    state().pages.remove(&a.u64("sendId")?);
    Ok(Value::Bool(true))
}

pub(crate) fn forget() -> Result<Value, String> {
    state().tokens.clear();
    Ok(Value::Bool(true))
}

#[cfg(test)]
mod tests {
    use crate::call_json;
    use serde_json::{json, Value};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    fn call(op: &str, a: Value) -> Value {
        serde_json::from_str(&call_json(op, &a.to_string())).unwrap()
    }

    /// A token endpoint and an API on one port; the heads and bodies it got.
    fn server() -> (u16, Arc<Mutex<Vec<String>>>) {
        server_with(r#"{"access_token":"tok-1","expires_in":3600}"#)
    }

    fn server_with(token: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
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
                let body = if all.starts_with("POST /token") { token } else { "{}" };
                s2.lock().unwrap().push(all);
                write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (port, seen)
    }

    fn args(port: u16, scope: &str, id: u64) -> Value {
        let auth = json!({ "type": "oauth2", "grant": "authorization_code", "authUrl": "https://id.test/authorize", "tokenUrl": format!("http://127.0.0.1:{port}/token"), "clientId": "app", "scope": scope, "redirectUri": "https://hitop.rws8.tech/app/oauth-callback.html" });
        let c = json!({ "id": 1, "name": "c", "auth": auth, "requests": [{ "id": 2, "name": "r", "method": "GET", "url": format!("http://127.0.0.1:{port}/api"), "headers": [], "body": "" }] });
        json!({ "collection": c.to_string(), "request": 0, "sendId": id })
    }

    fn wait_page(id: u64) -> String {
        for _ in 0..500 {
            if let Some(u) = call("oauthPending", json!({ "sendId": id }))["ok"].as_str() {
                return u.to_string();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no page");
    }

    fn param(url: &str, k: &str) -> String {
        let v = url.split(['?', '&']).find_map(|p| p.strip_prefix(&format!("{k}="))).unwrap();
        v.replace("%3A", ":").replace("%2F", "/")
    }

    #[test]
    fn a_person_signs_in_once_and_later_sends_and_runs_use_the_token() {
        let (port, seen) = server();
        let a = args(port, "one", 101);
        let t = std::thread::spawn(move || call("send", a));
        let page = wait_page(101);
        let redirect = param(&page, "redirect_uri");
        assert!(redirect.starts_with("http://127.0.0.1:") && page.contains("code_challenge_method=S256"), "{page}");
        let host = redirect.trim_start_matches("http://").split('/').next().unwrap().to_string();
        // Another app's guess without the state is not taken.
        for state in ["wrong".to_string(), param(&page, "state")] {
            let mut s = TcpStream::connect(&host).unwrap();
            write!(s, "GET /callback?code=c1&state={state} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
        }
        let out = t.join().unwrap();
        assert_eq!(out["ok"]["response"]["status"], 200, "{out}");
        let again = call("send", args(port, "one", 102));
        assert_eq!(again["ok"]["response"]["status"], 200);
        let all = seen.lock().unwrap().join("\n---\n");
        assert_eq!(all.matches("POST /token").count(), 1, "{all}");
        assert!(all.contains("code=c1") && all.contains("code_verifier="), "{all}");
        assert_eq!(all.to_lowercase().matches("authorization: bearer tok-1").count(), 2, "{all}");
    }

    #[test]
    fn a_sign_in_can_be_cancelled_and_a_run_never_asks() {
        let (port, _) = server();
        let a = args(port, "two", 201);
        let t = std::thread::spawn(move || call("send", a));
        wait_page(201);
        call("oauthCancel", json!({ "sendId": 201 }));
        assert!(t.join().unwrap()["ok"]["error"].as_str().unwrap().contains("cancelled"));
        let mut a = args(port, "three", 0);
        a.as_object_mut().unwrap().remove("sendId");
        assert!(call("send", a)["ok"]["error"].as_str().unwrap().contains("sign in first"));
    }

    #[test]
    fn an_expired_token_is_refreshed_also_in_a_run() {
        // Expired at once (taken again 30 s early).
        let (port, seen) = server_with(r#"{"access_token":"tok-1","expires_in":30,"refresh_token":"ref-1"}"#);
        let a = args(port, "four", 301);
        let t = std::thread::spawn(move || call("send", a));
        let page = wait_page(301);
        let host = param(&page, "redirect_uri").trim_start_matches("http://").split('/').next().unwrap().to_string();
        let mut s = TcpStream::connect(&host).unwrap();
        write!(s, "GET /callback?code=c1&state={} HTTP/1.1\r\nHost: x\r\n\r\n", param(&page, "state")).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        assert_eq!(t.join().unwrap()["ok"]["response"]["status"], 200);
        let mut a = args(port, "four", 0);
        a.as_object_mut().unwrap().remove("sendId");
        let out = call("send", a);
        assert_eq!(out["ok"]["response"]["status"], 200, "{out}");
        assert!(seen.lock().unwrap().join("\n").contains("grant_type=refresh_token&refresh_token=ref-1"));
    }
}
