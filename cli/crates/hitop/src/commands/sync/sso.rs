//! `hitop sync login --sso`: single sign-on through the sync server's identity provider; and
//! `--google` / `--github` (since C1b), the same flow with the server's Google or GitHub app.
//!
//! The server is the OpenID Connect / OAuth client (it holds the client secret); this command only opens
//! the provider's page and collects the one-time code: on a loopback listener the browser is sent
//! back to (`http://127.0.0.1:<port>/`), or pasted from the server's page (`--no-browser`, or
//! when no browser opens). Finishing needs a verifier that never leaves this process, so a code
//! seen by anyone else is useless to them.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::client::{b64, Api};

/// How long to wait for the browser.
const WAIT: Duration = Duration::from_secs(10 * 60);

fn open_browser(url: &str) -> bool {
    if std::env::var_os("HITOP_NO_BROWSER").is_some() {
        return false;
    }
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    cmd.arg(url).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
}

const DONE_PAGE: &str = "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>hitop</title><style>body{font:16px/1.5 system-ui,sans-serif;max-width:36rem;margin:3rem auto;padding:0 1rem}</style></head><body><h1>Signed in</h1><p>You can close this tab and go back to the terminal.</p></body></html>";

/// Waits for the browser to come back to the listener with `?sso=<code>`.
fn wait_for_code(listener: &TcpListener) -> Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).ok();
                let path = line.split_whitespace().nth(1).unwrap_or("");
                let code = url::Url::parse(&format!("http://127.0.0.1{path}")).ok().and_then(|u| u.query_pairs().find(|(k, _)| k == "sso").map(|(_, v)| v.into_owned()));
                let mut stream = stream;
                match code {
                    Some(code) => {
                        let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{DONE_PAGE}", DONE_PAGE.len());
                        return Ok(code);
                    }
                    None => {
                        let _ = write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.into()),
        }
    }
    bail!("no answer from the browser in 10 minutes; run `hitop sync login` again")
}

/// Signs in by `kind` (`sso`, `google`, `github`); returns the server's session answer
/// (`{session, expiresAt, user}`).
pub fn sign_in(api: &Api, kind: &str, email: Option<&str>, no_browser: bool, interactive: bool, ask: impl Fn(&str) -> Result<String>) -> Result<Value> {
    let name = match kind {
        "sso" => {
            let info = api.call("GET", "/v1/auth/sso", None)?;
            if info["enabled"] != true {
                bail!("this sync server has no single sign-on; sign in with --email");
            }
            // Orgs that bring their own provider (HITOP Cloud Business) are found by the address:
            // without --email the server answers that it needs one.
            if info["orgSso"] == true && email.is_none() {
                eprintln!("{} organizations here sign in with their own provider: if yours does, add --email <you@company>", "note:".yellow());
            }
            info["name"].as_str().unwrap_or("SSO").to_string()
        }
        _ => {
            let label = if kind == "google" { "Google" } else { "GitHub" };
            let methods = api.raw("GET", "/v1/auth/methods", None)?;
            if methods.json[kind] != true {
                bail!("this sync server doesn't offer sign-in with {label}; sign in with --email");
            }
            label.to_string()
        }
    };
    let verifier = hitop_sync_crypto::random_hex::<32>();
    let challenge = b64(&Sha256::digest(verifier.as_bytes()));
    let listener = if no_browser { None } else { TcpListener::bind("127.0.0.1:0").ok() };
    if listener.is_none() && !interactive {
        bail!("--sso needs a browser on this computer, or a terminal to paste the code into");
    }
    let mut body = json!({ "challenge": challenge });
    if let Some(l) = &listener {
        body["returnTo"] = json!(format!("http://127.0.0.1:{}/", l.local_addr()?.port()));
    }
    if let Some(e) = email {
        body["email"] = json!(e);
    }
    if kind == "github" {
        body.as_object_mut().map(|o| o.remove("email"));
    }
    let start = api.call("POST", &format!("/v1/auth/{kind}/start"), Some(body))?;
    let url = start["url"].as_str().context("no sign-in URL in the answer")?;
    let code = match &listener {
        Some(l) => {
            if open_browser(url) {
                eprintln!("Signing in with {name} in your browser. Waiting for it to finish...");
            } else {
                eprintln!("Open this link in a browser on this computer to sign in with {name}:\n\n  {url}\n");
            }
            wait_for_code(l)?
        }
        None => {
            eprintln!("Open this link in any browser to sign in with {name}:\n\n  {url}\n");
            ask("Code shown after signing in: ")?
        }
    };
    let r = api.raw("POST", &format!("/v1/auth/{kind}/finish"), Some(json!({ "code": code.trim(), "verifier": verifier })))?;
    if r.status != 200 {
        return Err(super::client::api_error(&r));
    }
    Ok(r.json)
}
