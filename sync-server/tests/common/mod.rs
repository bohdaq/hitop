#![allow(dead_code)]
//! Drives the server in-process through rws's `Application::execute`, as a client would.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hitop_sync_crypto as crypto;
use hitop_sync_server::mail::Mail;
use hitop_sync_server::{Config, Db, Mailer, State, SyncApp};
use rust_web_server::application::Application;
use rust_web_server::header::Header;
use rust_web_server::request::Request;
use rust_web_server::server::{Address, ConnectionInfo};
use serde_json::{json, Value};

pub mod fill;
pub mod idp;
pub mod t25;

pub const ORIGIN: &str = "https://sync.test.example";

pub struct Server {
    pub app: SyncApp,
    pub outbox: Arc<Mutex<Vec<Mail>>>,
    pub dir: PathBuf,
}

pub struct Reply {
    pub status: i16,
    pub json: Value,
    pub headers: Vec<(String, String)>,
    pub raw: Vec<u8>,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(s: &str) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(s).expect("base64url")
}

impl Server {
    /// A server on a fresh SQLite file in a temp directory.
    pub fn new(name: &str) -> Server {
        Self::with(name, |_| {})
    }

    pub fn with(name: &str, tweak: impl FnOnce(&mut Config)) -> Server {
        let dir = std::env::temp_dir().join(format!("hitop-sync-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut config = Config::local(ORIGIN);
        config.database = format!("sqlite:{}", dir.join("sync.db").display());
        config.cors_origins = vec!["https://app.test.example".into()];
        tweak(&mut config);
        let outbox = Arc::new(Mutex::new(Vec::new()));
        let db = Db::open(&config.database).unwrap();
        let app = SyncApp::new(State::new(db, Mailer::Memory(outbox.clone()), config).unwrap());
        Server { app, outbox, dir }
    }

    pub fn advance(&self, secs: i64) {
        self.app.state().clock_offset.fetch_add(secs, Ordering::Relaxed);
    }

    pub fn call(&self, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> Reply {
        self.raw(method, path, token, body.map(|b| b.to_string().into_bytes()).unwrap_or_default(), &[])
    }

    pub fn raw(&self, method: &str, path: &str, token: Option<&str>, body: Vec<u8>, headers: &[(&str, &str)]) -> Reply {
        let mut request = Request { method: method.into(), request_uri: path.into(), http_version: "HTTP/1.1".into(), headers: vec![], body: body.into() };
        if let Some(t) = token {
            request.headers.push(Header { name: "Authorization".into(), value: format!("Bearer {t}").into() });
        }
        for (n, v) in headers {
            request.headers.push(Header { name: n.to_string().into(), value: v.to_string().into() });
        }
        let conn = ConnectionInfo::new(Address { ip: "127.0.0.1".into(), port: 50000 }, Address { ip: "127.0.0.1".into(), port: 7881 });
        let response = self.app.execute(&request, &conn).unwrap();
        let raw: Vec<u8> = response.content_range_list.iter().flat_map(|r| r.body.clone()).collect();
        let json = serde_json::from_slice(&raw).unwrap_or(Value::Null);
        let headers = response.headers.iter().map(|h| (h.name.to_string(), h.value.to_string())).collect();
        Reply { status: response.status_code, json, headers, raw }
    }

    /// The token from the last sign-in mail to `email`.
    pub fn mailed_token(&self, email: &str) -> String {
        let outbox = self.outbox.lock().unwrap();
        let mail = outbox.iter().rev().find(|m| m.to == email).expect("a mail");
        let link = mail.body.lines().find(|l| l.contains("/signin#")).expect("a link");
        link.split('#').nth(1).unwrap().trim().to_string()
    }

    /// Signs `email` in by magic link; returns the session token.
    pub fn sign_in(&self, email: &str) -> String {
        assert_eq!(self.call("POST", "/v1/auth/email", None, Some(json!({ "email": email }))).status, 202);
        let token = self.mailed_token(email);
        let r = self.call("POST", "/v1/auth/email/verify", None, Some(json!({ "token": token })));
        assert_eq!(r.status, 200, "{}", r.json);
        r.json["session"].as_str().unwrap().to_string()
    }

    /// Every byte the server has on disk: the database, its WAL and anything else in its directory.
    pub fn disk_bytes(&self) -> Vec<u8> {
        let mut all = Vec::new();
        for entry in std::fs::read_dir(&self.dir).unwrap() {
            all.extend(std::fs::read(entry.unwrap().path()).unwrap());
        }
        all
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A client device: a session plus an HPKE key pair registered with the server.
pub struct Device {
    pub session: String,
    pub key_id: String,
    pub keys: crypto::DeviceKeyPair,
    pub user_id: String,
}

impl Device {
    pub fn new(server: &Server, email: &str) -> Device {
        let session = server.sign_in(email);
        Self::another(server, email, &session)
    }

    /// One more device of an account that is already signed in.
    pub fn another(server: &Server, _email: &str, session: &str) -> Device {
        let keys = crypto::DeviceKeyPair::generate();
        let r = server.call("POST", "/v1/me/keys", Some(session), Some(json!({ "name": "laptop", "publicKey": b64(&keys.public_key()) })));
        assert_eq!(r.status, 201, "{}", r.json);
        let me = server.call("GET", "/v1/me", Some(session), None);
        Device { session: session.to_string(), key_id: r.json["id"].as_str().unwrap().into(), keys, user_id: me.json["id"].as_str().unwrap().into() }
    }

    /// This device's workspace key for `epoch`, unwrapped from what the server returns.
    pub fn workspace_key(&self, server: &Server, ws: &str, epoch: u32) -> crypto::WorkspaceKey {
        let r = server.call("GET", &format!("/v1/workspaces/{ws}/wraps?keyId={}", self.key_id), Some(&self.session), None);
        assert_eq!(r.status, 200, "{}", r.json);
        let wrap = r.json["wraps"].as_array().unwrap().iter().find(|w| w["epoch"] == epoch).expect("a wrap for the epoch");
        crypto::unwrap_key(&self.keys, ws, epoch, &unb64(wrap["wrapped"].as_str().unwrap())).unwrap()
    }
}

/// Wraps for every device key of every member, from `GET /v1/workspaces/:ws` or a key list.
pub fn wraps_for(keys: &[(String, Vec<u8>)], ws: &str, epoch: u32, key: &crypto::WorkspaceKey) -> Value {
    Value::Array(keys.iter().map(|(id, pk)| json!({ "keyId": id, "wrapped": b64(&crypto::wrap_key(pk, ws, epoch, key).unwrap()) })).collect())
}

pub fn member_keys(server: &Server, ws: &str, session: &str) -> Vec<(String, Vec<u8>)> {
    let r = server.call("GET", &format!("/v1/workspaces/{ws}"), Some(session), None);
    assert_eq!(r.status, 200, "{}", r.json);
    r.json["members"].as_array().unwrap().iter().flat_map(|m| m["keys"].as_array().unwrap().iter().map(|k| (k["id"].as_str().unwrap().to_string(), unb64(k["publicKey"].as_str().unwrap())))).collect()
}

/// Creates a workspace owned by `owner` (all of the owner's devices get epoch 1's key).
pub fn create_workspace(server: &Server, owner: &Device, ws: &str) -> crypto::WorkspaceKey {
    let key = crypto::WorkspaceKey::generate();
    let wraps = wraps_for(&[(owner.key_id.clone(), owner.keys.public_key().to_vec())], ws, 1, &key);
    let r = server.call("POST", "/v1/workspaces", Some(&owner.session), Some(json!({ "id": ws, "wraps": wraps })));
    assert_eq!(r.status, 201, "{}", r.json);
    key
}

pub fn seal(key: &crypto::WorkspaceKey, ws: &str, blob: &str, version: u64, epoch: u32, plaintext: &[u8]) -> String {
    b64(&crypto::encrypt_blob(key, &crypto::BlobContext { workspace_id: ws, blob_id: blob, version, epoch }, plaintext).unwrap())
}

pub fn open(key: &crypto::WorkspaceKey, ws: &str, blob: &str, version: u64, epoch: u32, content: &str) -> Vec<u8> {
    crypto::decrypt_blob(key, &crypto::BlobContext { workspace_id: ws, blob_id: blob, version, epoch }, &unb64(content)).unwrap()
}

/// One SSO sign-in in progress, as a client holds it.
pub struct SsoFlow {
    pub verifier: String,
    pub url: url::Url,
    pub state: String,
    pub nonce: String,
    /// The PKCE challenge the server sent the provider.
    pub pkce: String,
}

/// `POST /v1/auth/sso/start` with a fresh verifier.
pub fn sso_start(server: &Server, return_to: Option<&str>) -> SsoFlow {
    use sha2::Digest;
    let verifier = b64(&rand_bytes());
    let challenge = b64(&sha2::Sha256::digest(verifier.as_bytes()));
    let mut body = json!({ "challenge": challenge });
    if let Some(r) = return_to {
        body["returnTo"] = json!(r);
    }
    let r = server.call("POST", "/v1/auth/sso/start", None, Some(body));
    assert_eq!(r.status, 200, "{}", r.json);
    let url = url::Url::parse(r.json["url"].as_str().unwrap()).unwrap();
    let q: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["redirect_uri"], format!("{ORIGIN}/v1/auth/sso/callback"));
    SsoFlow { verifier, state: q["state"].clone(), nonce: q["nonce"].clone(), pkce: q["code_challenge"].clone(), url }
}

fn rand_bytes() -> Vec<u8> {
    [uuid::Uuid::new_v4().into_bytes(), uuid::Uuid::new_v4().into_bytes()].concat()
}

/// The provider's redirect back to the server, `GET /v1/auth/sso/callback?...`.
pub fn sso_callback(server: &Server, query: &str) -> Reply {
    server.raw("GET", &format!("/v1/auth/sso/callback?{query}"), None, vec![], &[("User-Agent", "test-browser/1.0")])
}

/// The one-time code from the callback's answer: a loopback redirect, a web redirect, or the
/// paste page.
pub fn sso_code(reply: &Reply) -> Option<String> {
    if let Some(location) = reply.header("location") {
        let url = url::Url::parse(location).unwrap();
        if let Some(f) = url.fragment() {
            return f.strip_prefix("sso=").map(str::to_string);
        }
        return url.query_pairs().find(|(k, _)| k == "sso").map(|(_, v)| v.into_owned());
    }
    if reply.status != 200 {
        return None;
    }
    let page = String::from_utf8_lossy(&reply.raw);
    let start = page.find("<code>")? + 6;
    Some(page[start..start + page[start..].find("</code>")?].to_string())
}

pub fn sso_finish(server: &Server, code: &str, verifier: &str) -> Reply {
    server.call("POST", "/v1/auth/sso/finish", None, Some(json!({ "code": code, "verifier": verifier })))
}

/// Every audit row of a chain, oldest first, straight from the database.
pub fn audit_rows(server: &Server, ws: &str) -> Vec<Value> {
    let rows = server.app.state().db.tx(|tx| tx.query("SELECT seq, event, actor_email, target, detail, ip, user_agent FROM audit_log WHERE workspace_id = $1 ORDER BY seq", &[ws.into()])).unwrap();
    rows.iter()
        .map(|r| json!({ "seq": r.int(0), "event": r.text(1), "actor": r.text(2), "target": r.text(3), "detail": serde_json::from_str::<Value>(&r.text(4)).unwrap(), "ip": r.text(5), "userAgent": r.text(6) }))
        .collect()
}
