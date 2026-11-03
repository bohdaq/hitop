#![allow(dead_code)]
//! A stand-in for GitHub's OAuth app endpoints and the two API calls the server makes
//! (`POST /login/oauth/access_token`, `GET /user`, `GET /user/emails`), on 127.0.0.1.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hitop_sync_server::github::GithubConfig;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const CLIENT_ID: &str = "gh-client";
pub const CLIENT_SECRET: &str = "gh-secret";
pub const GOOD_CODE: &str = "gh-good-code";
const TOKEN: &str = "gho_mock_token";

#[derive(Default)]
pub struct Inner {
    /// `GET /user` answers this.
    pub user: Value,
    /// `GET /user/emails` answers this.
    pub emails: Value,
    /// The PKCE challenge the authorization URL carried.
    pub challenge: String,
    pub token_requests: u32,
}

#[derive(Clone)]
pub struct MockGithub {
    pub url: String,
    pub inner: Arc<Mutex<Inner>>,
}

impl MockGithub {
    pub fn start() -> MockGithub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let gh = MockGithub { url, inner: Arc::default() };
        let me = gh.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let mut headers = Vec::new();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    headers.push(h.trim().to_string());
                }
                let len = headers.iter().find_map(|h| h.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                let mut body = vec![0; len];
                let _ = reader.read_exact(&mut body);
                let (status, answer) = me.answer(&path, &headers, &String::from_utf8_lossy(&body));
                let text = answer.to_string();
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        gh
    }

    fn answer(&self, path: &str, headers: &[String], body: &str) -> (u16, Value) {
        let mut inner = self.inner.lock().unwrap();
        let authorized = headers.iter().any(|h| h.eq_ignore_ascii_case(&format!("authorization: Bearer {TOKEN}")));
        let has_agent = headers.iter().any(|h| h.to_lowercase().starts_with("user-agent:"));
        match path {
            "/login/oauth/access_token" => {
                inner.token_requests += 1;
                let form: std::collections::HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes()).into_owned().collect();
                let pkce = URL_SAFE_NO_PAD.encode(Sha256::digest(form.get("code_verifier").map(String::as_bytes).unwrap_or_default()));
                if form.get("client_id").map(String::as_str) != Some(CLIENT_ID) || form.get("client_secret").map(String::as_str) != Some(CLIENT_SECRET) {
                    return (200, json!({ "error": "incorrect_client_credentials" }));
                }
                // GitHub answers 200 with an error for a bad code.
                if form.get("code").map(String::as_str) != Some(GOOD_CODE) || pkce != inner.challenge {
                    return (200, json!({ "error": "bad_verification_code" }));
                }
                (200, json!({ "access_token": TOKEN, "token_type": "bearer", "scope": "read:user,user:email" }))
            }
            "/user" if authorized && has_agent => (200, inner.user.clone()),
            "/user/emails" if authorized && has_agent => (200, inner.emails.clone()),
            "/user" | "/user/emails" => (401, json!({ "message": "Requires authentication" })),
            _ => (404, json!({ "message": "Not Found" })),
        }
    }

    pub fn config(&self) -> GithubConfig {
        GithubConfig { client_id: CLIENT_ID.into(), client_secret: CLIENT_SECRET.into(), web_url: self.url.clone(), api_url: self.url.clone() }
    }

    /// The account GitHub reports next.
    pub fn next(&self, id: u64, emails: Value, challenge: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.user = json!({ "id": id, "login": format!("user{id}") });
        inner.emails = emails;
        inner.challenge = challenge.to_string();
    }
}
