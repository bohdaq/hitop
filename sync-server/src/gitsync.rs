//! Git sync of the personal library (HITOP Cloud C8, spec/CLOUD_SERVICE.md "Git sync"): a library
//! collection linked to a folder of a GitHub repository, kept there in the hitop folder format
//! (`hitop.yaml`, `requests/`; spec/collection-format.md), through the **HITOP Cloud GitHub App**.
//!
//! Owner decisions 2026-11-05: a GitHub App (no personal tokens), **every save is pushed** (saves
//! within `HITOP_SYNC_GIT_DEBOUNCE` seconds, 10, go into one commit), and a change on both sides is
//! **merged file by file** (requests, `hitop.yaml` key by key with hitop-core's `sync::merge_text`);
//! only a request changed on both sides **asks**: keep mine, take GitHub's, keep both. Nothing is
//! pushed while a conflict is open.
//!
//! - Connect: `POST /v1/git/github/connect {returnTo}` → the App's install URL and its OAuth
//!   authorize URL (both carry a one-time `state`). GitHub sends the browser to
//!   `GET /v1/git/github/callback?code&state[&installation_id]`; the server trades the code for a
//!   user token, lists `GET /user/installations` and records the installations **of this App the
//!   person can reach** (so nobody can link someone else's installation), then sends the browser
//!   back to `returnTo#git=connected`. The user token is dropped.
//! - Every GitHub call for a link uses a short-lived installation token (App JWT, RS256, minted
//!   with the App's private key), cached until a minute before it expires.
//! - `PUT /v1/library/collection/:id/git {installation, repo, branch?, path?}` links (and syncs at
//!   once: an existing `hitop.yaml` in that folder wins the first time, an empty folder gets the
//!   collection); `GET` shows the link, `DELETE` unlinks (the repository is never touched),
//!   `POST .../git/sync` syncs now, `POST .../git/resolve {choices: {path: mine|theirs|both}}`
//!   answers the conflicts and syncs.
//! - A library save of a linked collection sets the link due (library.rs calls [`touched`]); the
//!   worker ([`run_due`], every 2 s) syncs due links. GitHub's `push` webhook
//!   (`POST /v1/git/github/webhook`, `X-Hub-Signature-256` checked) sets links of that branch due.
//! - A sync: the branch head; the folder's collection files there ("theirs", only `hitop.yaml`
//!   and `requests/**.yaml`; other files in the repository are never read or changed); the library
//!   collection as files ("mine"); the files of the last sync ("base"). The merge goes into the
//!   library (when GitHub had changes) and into one commit on top of the head (when the library
//!   had changes), through the Git Data API. A push refused because the branch moved is retried
//!   from the new head. Secret values are never in these files (environments aren't synced).
//!
//! Settings (all or none): `HITOP_SYNC_GITHUB_APP_ID`, `HITOP_SYNC_GITHUB_APP_SLUG`,
//! `HITOP_SYNC_GITHUB_APP_KEY_FILE` (the App's private key, PEM; root only, out of backups),
//! `HITOP_SYNC_GITHUB_APP_CLIENT_ID`, `HITOP_SYNC_GITHUB_APP_CLIENT_SECRET[_FILE]`,
//! `HITOP_SYNC_GITHUB_APP_WEBHOOK_SECRET[_FILE]`. The App's callback URL is
//! `<public URL>/v1/git/github/callback`, its webhook URL `<public URL>/v1/git/github/webhook`,
//! "Request user authorization (OAuth) during installation" on, permissions Contents read and
//! write and Metadata read, event `push`. `HITOP_SYNC_GITHUB_URL` / `HITOP_SYNC_GITHUB_API_URL`
//! (shared with github.rs) point at GitHub Enterprise Server or a test double. Off: 404 `git_off`.
//!
//! The server reads and writes these collections in the clear: they are the personal library's,
//! which the server can read anyway (THREAT_MODEL.md "Outside this model"). Shared workspaces
//! never go through here.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hitop_core::format::files::{read_files, to_files, Files};
use hitop_core::format::Folder;
use hitop_core::model::Collection;
use rust_web_server::request::Request;
use rust_web_server::response::Response;
use rust_web_server::router::PathParams;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};
use url::Url;

use crate::api::{body, new_token, query, ApiError, ApiResult, State};
use crate::auth::{authenticate, User};
use crate::db::{DbError, Row, Tx};

const FLOW_TTL: i64 = 10 * 60;
const MANIFEST: &str = "hitop.yaml";
/// Files of one collection folder at most, and bytes per file.
const MAX_FILES: usize = 2000;
const MAX_FILE_BYTES: usize = 1 << 20;
/// A failed sync waits this long before the worker tries again.
const RETRY_SECS: i64 = 60;

// --- settings ---------------------------------------------------------------------------------

#[derive(Clone)]
pub struct GitConfig {
    pub app_id: String,
    pub slug: String,
    key: jsonwebtoken::EncodingKey,
    pub client_id: String,
    client_secret: String,
    webhook_secret: String,
    /// `https://github.com`.
    pub web_url: String,
    /// `https://api.github.com`.
    pub api_url: String,
    /// Saves within this many seconds go into one commit.
    pub debounce_secs: i64,
}

impl std::fmt::Debug for GitConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never prints the key or the secrets.
        f.debug_struct("GitConfig").field("app_id", &self.app_id).field("slug", &self.slug).field("api_url", &self.api_url).finish_non_exhaustive()
    }
}

fn secret(var: &dyn Fn(&str) -> Option<String>, name: &str) -> Result<Option<String>, String> {
    match (var(name), var(&format!("{name}_FILE"))) {
        (Some(v), None) => Ok(Some(v.trim().to_string())),
        (None, Some(path)) => Ok(Some(std::fs::read_to_string(&path).map_err(|e| format!("{name}_FILE {path}: {e}"))?.trim().to_string())),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(format!("set {name} or {name}_FILE, not both")),
    }
}

impl GitConfig {
    pub fn from_env(var: &dyn Fn(&str) -> Option<String>) -> Result<Option<GitConfig>, String> {
        const P: &str = "HITOP_SYNC_GITHUB_APP";
        let id = var(&format!("{P}_ID"));
        let slug = var(&format!("{P}_SLUG"));
        let key_file = var(&format!("{P}_KEY_FILE"));
        let client = crate::config::client_pair(var, P)?;
        let webhook = secret(var, &format!("{P}_WEBHOOK_SECRET"))?;
        let given = [id.is_some(), slug.is_some(), key_file.is_some(), client.is_some(), webhook.is_some()];
        if given.iter().all(|g| !g) {
            return Ok(None);
        }
        let (Some(app_id), Some(slug), Some(key_file), Some((client_id, client_secret)), Some(webhook_secret)) = (id, slug, key_file, client, webhook) else {
            return Err(format!("{P}_ID, _SLUG, _KEY_FILE, _CLIENT_ID, _CLIENT_SECRET and _WEBHOOK_SECRET go together"));
        };
        let pem = std::fs::read(&key_file).map_err(|e| format!("{P}_KEY_FILE {key_file}: {e}"))?;
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(&pem).map_err(|e| format!("{P}_KEY_FILE {key_file}: not an RSA private key ({e})"))?;
        let web_url = var("HITOP_SYNC_GITHUB_URL").unwrap_or_else(|| "https://github.com".into()).trim_end_matches('/').to_string();
        let api_url = var("HITOP_SYNC_GITHUB_API_URL").unwrap_or_else(|| "https://api.github.com".into()).trim_end_matches('/').to_string();
        for (name, u) in [("HITOP_SYNC_GITHUB_URL", &web_url), ("HITOP_SYNC_GITHUB_API_URL", &api_url)] {
            if !crate::oidc::secure_url(u) {
                return Err(format!("{name} must be an https:// URL (http:// only on localhost): {u}"));
            }
        }
        let debounce_secs = match var("HITOP_SYNC_GIT_DEBOUNCE") {
            Some(v) => v.parse().map_err(|_| "HITOP_SYNC_GIT_DEBOUNCE is seconds".to_string())?,
            None => 10,
        };
        Ok(Some(GitConfig { app_id, slug, key, client_id, client_secret, webhook_secret, web_url, api_url, debounce_secs }))
    }

    /// For tests: an App with the given private key (PEM) at `api` (and `web`).
    #[allow(clippy::too_many_arguments)]
    pub fn for_tests(app_id: &str, slug: &str, pem: &[u8], client_id: &str, client_secret: &str, webhook_secret: &str, web: &str, api: &str) -> GitConfig {
        GitConfig {
            app_id: app_id.into(),
            slug: slug.into(),
            key: jsonwebtoken::EncodingKey::from_rsa_pem(pem).expect("RSA key"),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            webhook_secret: webhook_secret.into(),
            web_url: web.trim_end_matches('/').into(),
            api_url: api.trim_end_matches('/').into(),
            debounce_secs: 0,
        }
    }
}

/// What the server keeps in memory for Git sync.
pub struct Git {
    http: ureq::Agent,
    /// Connect flows: state → (expires, user id, returnTo).
    flows: Mutex<HashMap<String, (i64, String, String)>>,
    /// Installation tokens: installation id → (token, expires).
    tokens: Mutex<HashMap<i64, (String, i64)>>,
    /// Links being synced now (user id, collection id): one sync per link at a time.
    busy: Mutex<HashSet<(String, String)>>,
}

impl Default for Git {
    fn default() -> Git {
        Git { http: crate::oidc::http_agent(), flows: Mutex::default(), tokens: Mutex::default(), busy: Mutex::default() }
    }
}

fn config(state: &State) -> Result<&GitConfig, ApiError> {
    state.config.git.as_ref().ok_or_else(|| ApiError::new(404, "git_off", "Git sync isn't configured on this server"))
}

fn person(state: &State, tx: &mut Tx, request: &Request) -> Result<User, ApiError> {
    let user = authenticate(state, tx, request)?;
    user.person()?;
    Ok(user)
}

// --- GitHub HTTP -------------------------------------------------------------------------------

/// Why a sync stopped: `disconnected` (the App, repository or branch is gone: no retry) or
/// `error` (GitHub unreachable or refusing: retried).
#[derive(Debug)]
struct Fail {
    state: &'static str,
    message: String,
}

fn error(message: impl Into<String>) -> Fail {
    Fail { state: "error", message: message.into() }
}

fn gone(message: impl Into<String>) -> Fail {
    Fail { state: "disconnected", message: message.into() }
}

fn gh(state: &State, method: &str, url: &str, auth: &str, body: Option<&Json>) -> Result<(u16, Json), Fail> {
    let http = &state.git.http;
    let auth = format!("Bearer {auth}");
    let result = match method {
        "GET" => http.get(url).header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28").header("Authorization", &auth).call(),
        _ => {
            let text = body.map(Json::to_string).unwrap_or_default();
            let b = match method {
                "POST" => http.post(url),
                "PATCH" => http.patch(url),
                "PUT" => http.put(url),
                _ => return Err(error(format!("{method}: not a method used here"))),
            };
            b.header("Accept", "application/vnd.github+json").header("X-GitHub-Api-Version", "2022-11-28").header("Authorization", &auth).header("Content-Type", "application/json").send(text)
        }
    };
    let mut res = result.map_err(|e| error(format!("GitHub is unreachable ({e})")))?;
    let status = res.status().as_u16();
    let text = res.body_mut().with_config().limit(32 << 20).read_to_string().map_err(|e| error(format!("GitHub's answer: {e}")))?;
    Ok((status, serde_json::from_str(&text).unwrap_or(Json::Null)))
}

#[derive(Serialize)]
struct AppClaims<'a> {
    iat: i64,
    exp: i64,
    iss: &'a str,
}

/// An installation token for `installation`, from the cache or freshly minted.
fn installation_token(state: &State, c: &GitConfig, installation: i64) -> Result<String, Fail> {
    let now = state.now();
    if let Some((t, exp)) = state.git.tokens.lock().unwrap_or_else(|e| e.into_inner()).get(&installation) {
        if *exp > now + 60 {
            return Ok(t.clone());
        }
    }
    let jwt = jsonwebtoken::encode(&jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256), &AppClaims { iat: now - 60, exp: now + 540, iss: &c.app_id }, &c.key)
        .map_err(|e| error(format!("the App's key can't sign: {e}")))?;
    let (status, doc) = gh(state, "POST", &format!("{}/app/installations/{installation}/access_tokens", c.api_url), &jwt, Some(&json!({})))?;
    match status {
        201 | 200 => {}
        404 => return Err(gone("the HITOP Cloud App is no longer installed there: connect GitHub again")),
        _ => return Err(error(format!("GitHub refused an installation token ({status}): {}", doc["message"].as_str().unwrap_or("")))),
    }
    let token = doc["token"].as_str().unwrap_or_default().to_string();
    if token.is_empty() {
        return Err(error("GitHub sent no installation token"));
    }
    // GitHub's tokens last an hour; the cache keeps them 55 minutes.
    state.git.tokens.lock().unwrap_or_else(|e| e.into_inner()).insert(installation, (token.clone(), now + 55 * 60));
    Ok(token)
}

// --- the folder's files ------------------------------------------------------------------------

/// The files a link syncs: `hitop.yaml` and the request files.
fn in_scope(rel: &str) -> bool {
    rel == MANIFEST || (rel.starts_with("requests/") && (rel.ends_with(".yaml") || rel.ends_with(".yml")) && !rel.split('/').any(|s| s.starts_with('.') || s.is_empty()))
}

fn rel_of<'a>(path: &str, full: &'a str) -> Option<&'a str> {
    if path.is_empty() {
        Some(full)
    } else {
        full.strip_prefix(path).and_then(|r| r.strip_prefix('/'))
    }
}

fn full_of(path: &str, rel: &str) -> String {
    if path.is_empty() {
        rel.to_string()
    } else {
        format!("{path}/{rel}")
    }
}

/// The library collection as folder files.
fn collection_files(data: &Json) -> Result<Files, Fail> {
    let collection: Collection = serde_json::from_value(data.clone()).map_err(|e| error(format!("this collection can't be written as files: {e}")))?;
    let files = to_files(&Folder { collection, environments: vec![], globals: None }).map_err(|e| error(format!("this collection can't be written as files: {e:#}")))?;
    Ok(files.into_iter().filter(|(p, _)| in_scope(p)).collect())
}

/// Folder files back into the library's collection JSON, keeping the library's id.
fn collection_json(files: &Files, name: &str, id: &str) -> Result<Json, Fail> {
    let mut folder = read_files(files, name).map_err(|e| error(format!("the folder on GitHub isn't a valid hitop collection: {e:#}")))?;
    if let Ok(n) = id.parse() {
        folder.collection.id = n;
    }
    serde_json::to_value(&folder.collection).map_err(|e| error(e.to_string()))
}

/// Git's blob id of `text` (to skip downloading files that didn't change).
fn blob_sha(text: &str) -> String {
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
    ctx.update(format!("blob {}\0", text.len()).as_bytes());
    ctx.update(text.as_bytes());
    ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

struct Head {
    commit: String,
    tree: String,
}

/// A sync's result: the head, the files as synced, open conflicts and a message.
type Outcome = Option<(Head, Files, Vec<Conflict>, String)>;

fn repo_url(c: &GitConfig, repo: &str, rest: &str) -> String {
    format!("{}/repos/{repo}/{rest}", c.api_url)
}

fn branch_head(state: &State, c: &GitConfig, token: &str, repo: &str, branch: &str) -> Result<Head, Fail> {
    let (status, doc) = gh(state, "GET", &repo_url(c, repo, &format!("git/ref/heads/{branch}")), token, None)?;
    match status {
        200 => {}
        404 => return Err(gone(format!("the branch {branch} of {repo} is gone, or the App can't see {repo}"))),
        409 => return Err(gone(format!("{repo} is empty: add a first commit (a README) on GitHub, then sync"))),
        _ => return Err(error(format!("GitHub answered {status} for the branch {branch}"))),
    }
    let commit = doc["object"]["sha"].as_str().unwrap_or_default().to_string();
    let (status, c_doc) = gh(state, "GET", &repo_url(c, repo, &format!("git/commits/{commit}")), token, None)?;
    if status != 200 {
        return Err(error(format!("GitHub answered {status} for commit {commit}")));
    }
    Ok(Head { commit, tree: c_doc["tree"]["sha"].as_str().unwrap_or_default().to_string() })
}

/// The collection files under `path` in `tree`; `known` = files already held (by their blob id).
fn files_at(state: &State, c: &GitConfig, token: &str, repo: &str, tree: &str, path: &str, known: &Files) -> Result<Files, Fail> {
    let (status, doc) = gh(state, "GET", &repo_url(c, repo, &format!("git/trees/{tree}?recursive=1")), token, None)?;
    if status != 200 {
        return Err(error(format!("GitHub answered {status} for the file list")));
    }
    if doc["truncated"] == true {
        return Err(error(format!("{repo} has too many files for one listing: link a smaller folder")));
    }
    let by_sha: HashMap<String, &String> = known.values().map(|t| (blob_sha(t), t)).collect();
    let mut out = Files::new();
    for e in doc["tree"].as_array().into_iter().flatten() {
        let (Some(full), Some(sha)) = (e["path"].as_str(), e["sha"].as_str()) else { continue };
        if e["type"] != "blob" {
            continue;
        }
        let Some(rel) = rel_of(path, full).filter(|r| in_scope(r)) else { continue };
        if out.len() >= MAX_FILES {
            return Err(error(format!("the folder has more than {MAX_FILES} request files")));
        }
        if let Some(t) = by_sha.get(sha) {
            out.insert(rel.to_string(), (*t).clone());
            continue;
        }
        let (status, blob) = gh(state, "GET", &repo_url(c, repo, &format!("git/blobs/{sha}")), token, None)?;
        if status != 200 {
            return Err(error(format!("GitHub answered {status} for {full}")));
        }
        let raw = STANDARD.decode(blob["content"].as_str().unwrap_or_default().replace(['\n', '\r'], "")).map_err(|_| error(format!("{full}: bad base64 from GitHub")))?;
        if raw.len() > MAX_FILE_BYTES {
            return Err(error(format!("{full} is over 1 MB")));
        }
        let text = String::from_utf8(raw).map_err(|_| error(format!("{full} isn't UTF-8 text")))?;
        out.insert(rel.to_string(), text);
    }
    Ok(out)
}

/// What changed, for a commit message: "hitop: add get-order, update list-orders".
fn message_of(before: &Files, after: &Files) -> String {
    let stem = |p: &str| p.trim_start_matches("requests/").trim_end_matches(".yaml").trim_end_matches(".yml").to_string();
    let mut parts: Vec<String> = Vec::new();
    for (verb, list) in [
        ("add", after.keys().filter(|p| !before.contains_key(*p)).collect::<Vec<_>>()),
        ("update", after.iter().filter(|(p, t)| before.get(*p).is_some_and(|b| b != *t)).map(|(p, _)| p).collect()),
        ("remove", before.keys().filter(|p| !after.contains_key(*p)).collect()),
    ] {
        let names: Vec<String> = list.iter().map(|p| if *p == MANIFEST { "the collection".to_string() } else { stem(p) }).collect();
        if !names.is_empty() {
            let shown: Vec<&str> = names.iter().take(5).map(String::as_str).collect();
            let more = if names.len() > 5 { format!(" and {} more", names.len() - 5) } else { String::new() };
            parts.push(format!("{verb} {}{more}", shown.join(", ")));
        }
    }
    format!("hitop: {}", if parts.is_empty() { "sync".to_string() } else { parts.join("; ") })
}

enum Pushed {
    Done(Head),
    /// The branch moved after we read it: sync again from the new head.
    Moved,
}

/// One commit on top of `head` that makes the folder's collection files `after` (from `before`).
#[allow(clippy::too_many_arguments)]
fn commit(state: &State, c: &GitConfig, token: &str, link: &Link, head: &Head, before: &Files, after: &Files, author: &User) -> Result<Pushed, Fail> {
    let mut entries = Vec::new();
    for (rel, text) in after {
        if before.get(rel) == Some(text) {
            continue;
        }
        let (status, doc) = gh(state, "POST", &repo_url(c, &link.repo, "git/blobs"), token, Some(&json!({ "content": text, "encoding": "utf-8" })))?;
        if status != 201 {
            return Err(write_refused(status, &doc, &link.repo));
        }
        entries.push(json!({ "path": full_of(&link.path, rel), "mode": "100644", "type": "blob", "sha": doc["sha"] }));
    }
    for rel in before.keys().filter(|r| !after.contains_key(*r)) {
        entries.push(json!({ "path": full_of(&link.path, rel), "mode": "100644", "type": "blob", "sha": Json::Null }));
    }
    let (status, tree) = gh(state, "POST", &repo_url(c, &link.repo, "git/trees"), token, Some(&json!({ "base_tree": head.tree, "tree": entries })))?;
    if status != 201 {
        return Err(write_refused(status, &tree, &link.repo));
    }
    let tree_sha = tree["sha"].as_str().unwrap_or_default().to_string();
    let name = author.email.split('@').next().unwrap_or("hitop").to_string();
    let body = json!({ "message": message_of(before, after), "tree": tree_sha, "parents": [head.commit], "author": { "name": name, "email": author.email } });
    let (status, made) = gh(state, "POST", &repo_url(c, &link.repo, "git/commits"), token, Some(&body))?;
    if status != 201 {
        return Err(write_refused(status, &made, &link.repo));
    }
    let sha = made["sha"].as_str().unwrap_or_default().to_string();
    let (status, doc) = gh(state, "PATCH", &repo_url(c, &link.repo, &format!("git/refs/heads/{}", link.branch)), token, Some(&json!({ "sha": sha, "force": false })))?;
    match status {
        200 => Ok(Pushed::Done(Head { commit: sha, tree: tree_sha })),
        // "Update is not a fast forward": someone pushed meanwhile.
        422 | 409 => Ok(Pushed::Moved),
        _ => Err(write_refused(status, &doc, &link.repo)),
    }
}

fn write_refused(status: u16, doc: &Json, repo: &str) -> Fail {
    let why = doc["message"].as_str().unwrap_or("");
    if status == 403 || status == 404 {
        gone(format!("the HITOP Cloud App can't write to {repo} ({status} {why}): give it Contents read and write there"))
    } else {
        error(format!("GitHub refused the commit ({status} {why})"))
    }
}

// --- the merge -------------------------------------------------------------------------------

/// A request (or `hitop.yaml`) changed on both sides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub path: String,
    pub name: String,
    pub mine: Option<String>,
    pub theirs: Option<String>,
}

fn request_name(text: Option<&str>, path: &str) -> String {
    text.and_then(|t| yaml_serde::from_str::<yaml_serde::Value>(t).ok())
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
        .unwrap_or_else(|| path.trim_start_matches("requests/").to_string())
}

/// Mine kept next to theirs: the same request under a new file, named "… (mine)".
fn keep_both_copy(rel: &str, mine: &str, taken: &Files) -> Option<(String, String)> {
    let mut v: yaml_serde::Value = yaml_serde::from_str(mine).ok()?;
    let map = v.as_mapping_mut()?;
    let name = map.get("name").and_then(|n| n.as_str()).unwrap_or("request").to_string();
    map.insert("name".into(), format!("{name} (mine)").into());
    map.remove("id");
    let (stem, ext) = rel.rsplit_once('.')?;
    let mut n = 1;
    let mut path = format!("{stem}-mine.{ext}");
    while taken.contains_key(&path) {
        n += 1;
        path = format!("{stem}-mine-{n}.{ext}");
    }
    Some((path, yaml_serde::to_string(&v).ok()?))
}

/// Three-way merge of a folder's files. `choices` answers conflicts by path.
fn merge3(base: &Files, mine: &Files, theirs: &Files, choices: &BTreeMap<String, String>) -> (Files, Vec<Conflict>) {
    let paths: BTreeSet<&String> = base.keys().chain(mine.keys()).chain(theirs.keys()).collect();
    let mut out = Files::new();
    let mut conflicts = Vec::new();
    let mut copies = Vec::new();
    for p in paths {
        let (b, m, t) = (base.get(p), mine.get(p), theirs.get(p));
        let r: Option<Option<String>> = if m == t {
            Some(m.cloned())
        } else if m == b {
            Some(t.cloned())
        } else if t == b {
            Some(m.cloned())
        } else {
            hitop_core::sync::merge_text(p, b.map(String::as_str), m.map(String::as_str), t.map(String::as_str))
        };
        match r {
            Some(Some(text)) => {
                out.insert(p.clone(), text);
            }
            Some(None) => {}
            None => match choices.get(p).map(String::as_str) {
                Some("mine") => {
                    if let Some(x) = m {
                        out.insert(p.clone(), x.clone());
                    }
                }
                Some("theirs") => {
                    if let Some(x) = t {
                        out.insert(p.clone(), x.clone());
                    }
                }
                Some("both") if p != MANIFEST => {
                    if let Some(x) = t.or(m) {
                        out.insert(p.clone(), x.clone());
                    }
                    if let (Some(mine_text), Some(_)) = (m, t) {
                        copies.push((p.clone(), mine_text.clone()));
                    }
                }
                _ => conflicts.push(Conflict { path: p.clone(), name: request_name(m.or(t).map(String::as_str), p), mine: m.cloned(), theirs: t.cloned() }),
            },
        }
    }
    for (p, mine_text) in copies {
        if let Some((path, text)) = keep_both_copy(&p, &mine_text, &out) {
            out.insert(path, text);
        }
    }
    (out, conflicts)
}

// --- links -----------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Link {
    user_id: String,
    collection_id: String,
    installation: i64,
    repo: String,
    branch: String,
    path: String,
    head: String,
    tree: String,
    base: Files,
    state: String,
    conflicts: Vec<Conflict>,
    message: String,
    synced_at: i64,
}

const LINK_COLUMNS: &str = "user_id, collection_id, installation_id, repo, branch, path, head, tree, base, state, conflicts, message, synced_at";

fn link_of(r: &Row) -> Link {
    Link {
        user_id: r.text(0),
        collection_id: r.text(1),
        installation: r.int(2),
        repo: r.text(3),
        branch: r.text(4),
        path: r.text(5),
        head: r.text(6),
        tree: r.text(7),
        base: serde_json::from_str(&r.text(8)).unwrap_or_default(),
        state: r.text(9),
        conflicts: serde_json::from_str(&r.text(10)).unwrap_or_default(),
        message: r.text(11),
        synced_at: r.int(12),
    }
}

fn load_link(tx: &mut Tx, user: &str, collection: &str) -> Result<Option<Link>, DbError> {
    Ok(tx.query_one(&format!("SELECT {LINK_COLUMNS} FROM git_links WHERE user_id = $1 AND collection_id = $2"), &[user.into(), collection.into()])?.map(|r| link_of(&r)))
}

fn view(l: &Link, c: &GitConfig) -> Json {
    json!({
        "collectionId": l.collection_id,
        "installation": l.installation,
        "repo": l.repo,
        "branch": l.branch,
        "path": l.path,
        "url": format!("{}/{}/tree/{}/{}", c.web_url, l.repo, l.branch, l.path).trim_end_matches('/'),
        "commit": if l.head.is_empty() { Json::Null } else { l.head.clone().into() },
        "state": l.state,
        "message": l.message,
        "conflicts": l.conflicts,
        "syncedAt": if l.synced_at == 0 { Json::Null } else { l.synced_at.into() },
    })
}

/// The state of every link of `user`, by collection id (for `GET /v1/library`).
pub(crate) fn states(state: &State, tx: &mut Tx, user: &str) -> Result<Json, DbError> {
    let Some(c) = state.config.git.as_ref() else { return Ok(Json::Null) };
    let rows = tx.query(&format!("SELECT {LINK_COLUMNS} FROM git_links WHERE user_id = $1"), &[user.into()])?;
    let mut out = serde_json::Map::new();
    for r in rows {
        let l = link_of(&r);
        let mut v = view(&l, c);
        if let Some(o) = v.as_object_mut() {
            o.remove("conflicts");
            o.insert("conflicts".into(), l.conflicts.len().into());
        }
        out.insert(l.collection_id.clone(), v);
    }
    Ok(Json::Object(out))
}

/// A library save of `(kind, id)`: a linked collection is due for a push after the debounce.
pub(crate) fn touched(state: &State, tx: &mut Tx, user: &str, kind: &str, id: &str) -> Result<(), DbError> {
    let Some(c) = state.config.git.as_ref() else { return Ok(()) };
    if kind != "collection" {
        return Ok(());
    }
    tx.execute(
        "UPDATE git_links SET due_at = $1 WHERE user_id = $2 AND collection_id = $3 AND state NOT IN ('conflict', 'disconnected')",
        &[(state.now() + c.debounce_secs.max(0)).into(), user.into(), id.into()],
    )?;
    Ok(())
}

/// A deleted library collection is unlinked (the repository keeps its files).
pub(crate) fn deleted(tx: &mut Tx, user: &str, kind: &str, id: &str) -> Result<(), DbError> {
    if kind == "collection" {
        tx.execute("DELETE FROM git_links WHERE user_id = $1 AND collection_id = $2", &[user.into(), id.into()])?;
    }
    Ok(())
}

fn save_fail(state: &State, link: &Link, f: &Fail) {
    let due = if f.state == "error" { state.now() + RETRY_SECS } else { 0 };
    let r = state.db.tx(|tx| {
        tx.execute(
            "UPDATE git_links SET state = $1, message = $2, due_at = $3 WHERE user_id = $4 AND collection_id = $5",
            &[f.state.into(), (&f.message).into(), due.into(), (&link.user_id).into(), (&link.collection_id).into()],
        )
    });
    if let Err(e) = r {
        eprintln!("hitop-sync-server: git sync: {e}");
    }
}

/// Syncs one link now. `choices` answers open conflicts by path (mine, theirs, both).
fn sync_link(state: &State, c: &GitConfig, user_id: &str, collection_id: &str, choices: &BTreeMap<String, String>) -> Result<(), ApiError> {
    let key = (user_id.to_string(), collection_id.to_string());
    if !state.git.busy.lock().unwrap_or_else(|e| e.into_inner()).insert(key.clone()) {
        // A sync of this link is running: it picks up this change, or the worker does next.
        state.db.tx(|tx| tx.execute("UPDATE git_links SET due_at = $1 WHERE user_id = $2 AND collection_id = $3", &[(state.now() + 2).into(), user_id.into(), collection_id.into()]))?;
        return Ok(());
    }
    let r = sync_inner(state, c, user_id, collection_id, choices);
    state.git.busy.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
    r
}

fn sync_inner(state: &State, c: &GitConfig, user_id: &str, collection_id: &str, choices: &BTreeMap<String, String>) -> Result<(), ApiError> {
    let started = state.now();
    let (link, item, author) = state.db.tx(|tx| {
        let Some(link) = load_link(tx, user_id, collection_id)? else { return Err(ApiError::not_found("this collection isn't linked to GitHub")) };
        let item = crate::library::collection_item(tx, user_id, collection_id)?;
        let author = tx.query_one("SELECT email FROM users WHERE id = $1", &[user_id.into()])?.map_or(String::new(), |r| r.text(0));
        Ok((link, item, User { id: user_id.to_string(), email: author, runner_of: String::new() }))
    })?;
    let Some((data, version)) = item else {
        save_fail(state, &link, &gone("the collection is gone from your library"));
        return Ok(());
    };
    let attempt = |state: &State| -> Result<Outcome, Fail> {
        let token = installation_token(state, c, link.installation)?;
        for _ in 0..3 {
            let head = branch_head(state, c, &token, &link.repo, &link.branch)?;
            let theirs = if head.commit == link.head { link.base.clone() } else { files_at(state, c, &token, &link.repo, &head.tree, &link.path, &link.base)? };
            let mine = collection_files(&data)?;
            let (merged, conflicts) = if link.head.is_empty() {
                // The first sync: a collection already in the folder wins; an empty folder gets ours.
                (if theirs.contains_key(MANIFEST) { theirs.clone() } else { mine.clone() }, vec![])
            } else {
                merge3(&link.base, &mine, &theirs, choices)
            };
            if !conflicts.is_empty() {
                let n = conflicts.len();
                return Ok(Some((head, link.base.clone(), conflicts, format!("{n} request{} changed both here and on GitHub: choose which to keep", if n == 1 { "" } else { "s" }))));
            }
            // GitHub's changes into the library (only if nobody saved meanwhile).
            let canonical = if merged != mine {
                let name = data["name"].as_str().unwrap_or("Collection").to_string();
                let json = collection_json(&merged, &name, collection_id)?;
                let stored = state.db.tx(|tx| crate::library::store_from_server(state, tx, user_id, collection_id, &json, version)).map_err(|e| error(e.message))?;
                if !stored {
                    // A save arrived while we merged: start again with it.
                    return Err(Fail { state: "pending", message: "saving…".into() });
                }
                collection_files(&json)?
            } else {
                mine
            };
            if canonical == theirs {
                return Ok(Some((head, canonical, vec![], String::new())));
            }
            match commit(state, c, &token, &link, &head, &theirs, &canonical, &author)? {
                Pushed::Done(new) => return Ok(Some((new, canonical, vec![], String::new()))),
                Pushed::Moved => continue,
            }
        }
        Err(error("the branch keeps moving: trying again in a minute"))
    };
    match attempt(state) {
        Ok(Some((head, base, conflicts, message))) => {
            let st = if conflicts.is_empty() { "ok" } else { "conflict" };
            let conflicts_json = serde_json::to_string(&conflicts).unwrap_or_else(|_| "[]".into());
            let base_json = serde_json::to_string(&base).unwrap_or_else(|_| "{}".into());
            // With a conflict the head and base stay where the last good sync left them.
            let (head_sha, tree_sha, base_json) = if conflicts.is_empty() { (head.commit, head.tree, base_json) } else { (link.head.clone(), link.tree.clone(), serde_json::to_string(&link.base).unwrap_or_else(|_| "{}".into())) };
            state.db.tx(|tx| {
                tx.execute(
                    "UPDATE git_links SET head = $1, tree = $2, base = $3, state = $4, conflicts = $5, message = $6, synced_at = $7, due_at = CASE WHEN due_at > $8 THEN due_at ELSE 0 END WHERE user_id = $9 AND collection_id = $10",
                    &[(&head_sha).into(), (&tree_sha).into(), (&base_json).into(), st.into(), (&conflicts_json).into(), (&message).into(), state.now().into(), started.into(), user_id.into(), collection_id.into()],
                )
            })?;
        }
        Ok(None) => {}
        Err(f) if f.state == "pending" => {
            state.db.tx(|tx| tx.execute("UPDATE git_links SET due_at = $1 WHERE user_id = $2 AND collection_id = $3", &[(state.now() + 1).into(), user_id.into(), collection_id.into()]))?;
        }
        Err(f) => save_fail(state, &link, &f),
    }
    Ok(())
}

/// The worker's step: syncs every link that is due. Returns how many ran.
pub fn run_due(state: &State) -> usize {
    let Some(c) = state.config.git.as_ref() else { return 0 };
    let due = match state.db.tx(|tx| tx.query("SELECT user_id, collection_id FROM git_links WHERE due_at > 0 AND due_at <= $1 ORDER BY due_at LIMIT 50", &[state.now().into()])) {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("hitop-sync-server: git sync: {e}");
            return 0;
        }
    };
    for r in &due {
        if let Err(e) = sync_link(state, c, &r.text(0), &r.text(1), &BTreeMap::new()) {
            eprintln!("hitop-sync-server: git sync of {}: {}", r.text(1), e.message);
        }
    }
    due.len()
}

// --- the API ---------------------------------------------------------------------------------

/// `GET /v1/git`: whether Git sync is on, the App, and the installations this account connected.
pub fn info(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let Some(c) = state.config.git.as_ref() else {
        return Ok((200, json!({ "enabled": false })));
    };
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        let rows = tx.query("SELECT installation_id, account FROM git_installations WHERE user_id = $1 ORDER BY account", &[(&user.id).into()])?;
        let list: Vec<Json> = rows.iter().map(|r| json!({ "id": r.int(0), "account": r.text(1) })).collect();
        Ok((200, json!({ "enabled": true, "app": c.slug, "appUrl": format!("{}/apps/{}", c.web_url, c.slug), "installations": list })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Connect {
    return_to: String,
}

fn callback_url(state: &State) -> String {
    state.config.public_url.join("v1/git/github/callback").map(|u| u.to_string()).unwrap_or_default()
}

/// `POST /v1/git/github/connect {returnTo}` → `{installUrl, authorizeUrl, expiresAt}`.
pub fn connect(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let c = config(state)?;
    let input: Connect = body(request)?;
    let back = crate::oidc::return_to(state, &input.return_to)?;
    let user = state.db.tx(|tx| person(state, tx, request))?;
    let now = state.now();
    let st = new_token();
    {
        let mut flows = state.git.flows.lock().unwrap_or_else(|e| e.into_inner());
        flows.retain(|_, (exp, _, _)| *exp > now);
        if flows.len() > 10_000 {
            return Err(ApiError::new(429, "rate_limited", "too many GitHub connections in progress; try again shortly"));
        }
        flows.insert(st.clone(), (now + FLOW_TTL, user.id, back.to_string()));
    }
    let mut install = Url::parse(&format!("{}/apps/{}/installations/new", c.web_url, c.slug)).map_err(|_| ApiError::new(500, "internal", "bad HITOP_SYNC_GITHUB_URL"))?;
    install.query_pairs_mut().append_pair("state", &st);
    let mut authorize = Url::parse(&format!("{}/login/oauth/authorize", c.web_url)).map_err(|_| ApiError::new(500, "internal", "bad HITOP_SYNC_GITHUB_URL"))?;
    authorize.query_pairs_mut().append_pair("client_id", &c.client_id).append_pair("redirect_uri", &callback_url(state)).append_pair("state", &st);
    Ok((200, json!({ "installUrl": install.to_string(), "authorizeUrl": authorize.to_string(), "expiresAt": now + FLOW_TTL })))
}

fn back_with(to: &str, fragment: &str) -> Response {
    crate::oidc::redirect(&format!("{to}#{fragment}"))
}

/// `GET /v1/git/github/callback?code&state[&installation_id&setup_action]` (the browser, from GitHub).
pub fn callback(state: &State, request: &Request) -> Response {
    let Some(c) = state.config.git.as_ref() else {
        return crate::oidc::failed_page("Git sync isn't configured on this server");
    };
    let q = query(request);
    let now = state.now();
    let st = q.get("state").cloned().unwrap_or_default();
    let flow = state.git.flows.lock().unwrap_or_else(|e| e.into_inner()).get(&st).cloned().filter(|(exp, _, _)| *exp > now);
    let Some((_, user_id, back)) = flow else {
        return crate::oidc::failed_page("this GitHub connection expired or was already used: start again in hitop");
    };
    let Some(code) = q.get("code").filter(|c| !c.is_empty()) else {
        // Installed without the OAuth step (an update of an existing installation): ask for it.
        if q.contains_key("installation_id") {
            let mut authorize = Url::parse(&format!("{}/login/oauth/authorize", c.web_url)).expect("checked at start");
            authorize.query_pairs_mut().append_pair("client_id", &c.client_id).append_pair("redirect_uri", &callback_url(state)).append_pair("state", &st);
            return crate::oidc::redirect(authorize.as_str());
        }
        state.git.flows.lock().unwrap_or_else(|e| e.into_inner()).remove(&st);
        return back_with(&back, "git=error&reason=no_code");
    };
    state.git.flows.lock().unwrap_or_else(|e| e.into_inner()).remove(&st);
    match record_installations(state, c, &user_id, code) {
        Ok(0) => back_with(&back, "git=error&reason=no_installation"),
        Ok(_) => back_with(&back, "git=connected"),
        Err(f) => {
            eprintln!("hitop-sync-server: GitHub connect: {}", f.message);
            back_with(&back, "git=error&reason=github")
        }
    }
}

/// Code → user token → the installations of this App that person can reach, recorded for them.
fn record_installations(state: &State, c: &GitConfig, user_id: &str, code: &str) -> Result<usize, Fail> {
    let form = [("client_id", c.client_id.as_str()), ("client_secret", c.client_secret.as_str()), ("code", code), ("redirect_uri", &callback_url(state))];
    let mut res = state.git.http.post(&format!("{}/login/oauth/access_token", c.web_url)).header("Accept", "application/json").send_form(form.iter().map(|(k, v)| (*k, *v))).map_err(|e| error(format!("GitHub is unreachable ({e})")))?;
    let doc: Json = serde_json::from_str(&res.body_mut().read_to_string().unwrap_or_default()).unwrap_or(Json::Null);
    let Some(token) = doc["access_token"].as_str().filter(|t| !t.is_empty()) else {
        return Err(error(format!("GitHub refused the code ({})", doc["error"].as_str().unwrap_or("error"))));
    };
    let (status, list) = gh(state, "GET", &format!("{}/user/installations?per_page=100", c.api_url), token, None)?;
    if status != 200 {
        return Err(error(format!("GitHub answered {status} for the installations")));
    }
    let ours: Vec<(i64, String)> = list["installations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["app_id"].to_string().trim_matches('"') == c.app_id)
        .filter_map(|i| Some((i["id"].as_i64()?, i["account"]["login"].as_str().unwrap_or("").to_string())))
        .collect();
    let now = state.now();
    state
        .db
        .tx(|tx| {
            for (id, account) in &ours {
                tx.execute(
                    "INSERT INTO git_installations (user_id, installation_id, account, created_at) VALUES ($1, $2, $3, $4) ON CONFLICT (user_id, installation_id) DO UPDATE SET account = $3",
                    &[user_id.into(), (*id).into(), account.into(), now.into()],
                )?;
            }
            Ok::<_, DbError>(())
        })
        .map_err(|e| error(e.to_string()))?;
    Ok(ours.len())
}

fn own_installation(tx: &mut Tx, user: &str, installation: i64) -> Result<(), ApiError> {
    if tx.query_one("SELECT 1 FROM git_installations WHERE user_id = $1 AND installation_id = $2", &[user.into(), installation.into()])?.is_none() {
        return Err(ApiError::not_found("connect GitHub first: that installation isn't one you connected"));
    }
    Ok(())
}

fn api_err(f: Fail) -> ApiError {
    if f.state == "disconnected" {
        ApiError::new(409, "git_disconnected", f.message)
    } else {
        ApiError::new(502, "github", f.message)
    }
}

/// `GET /v1/git/installations/:id/repos` → the repositories the App can reach there.
pub fn repos(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let installation: i64 = params.get("id").unwrap_or_default().parse().map_err(|_| ApiError::bad("an installation id is a number"))?;
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        own_installation(tx, &user.id, installation)
    })?;
    let token = installation_token(state, c, installation).map_err(api_err)?;
    let (status, doc) = gh(state, "GET", &format!("{}/installation/repositories?per_page=100", c.api_url), &token, None).map_err(api_err)?;
    if status != 200 {
        return Err(ApiError::new(502, "github", format!("GitHub answered {status} for the repositories")));
    }
    let list: Vec<Json> = doc["repositories"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| json!({ "fullName": r["full_name"], "defaultBranch": r["default_branch"], "private": r["private"] }))
        .collect();
    Ok((200, json!({ "repositories": list })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LinkInput {
    installation: i64,
    repo: String,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

fn clean_path(p: &str) -> Result<String, ApiError> {
    let p = p.trim().trim_matches('/');
    if p.split('/').any(|s| s == ".." || s.starts_with('.')) || p.contains("//") || p.len() > 200 || !p.chars().all(|ch| ch.is_ascii_alphanumeric() || "-_./ ".contains(ch)) {
        return Err(ApiError::bad("the folder is a path inside the repository: letters, digits, - _ . and /, no .. or hidden folders"));
    }
    Ok(p.to_string())
}

fn collection_param(params: &PathParams) -> String {
    params.get("id").unwrap_or_default().to_string()
}

/// `PUT /v1/library/collection/:id/git {installation, repo, branch?, path?}` → the link, synced.
pub fn link(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let id = collection_param(params);
    let input: LinkInput = body(request)?;
    let path = clean_path(input.path.as_deref().unwrap_or(""))?;
    let repo = input.repo.trim().to_string();
    if repo.split('/').count() != 2 || repo.split('/').any(|s| s.is_empty()) {
        return Err(ApiError::bad("repo is owner/name"));
    }
    let user = state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        own_installation(tx, &user.id, input.installation)?;
        if crate::library::collection_item(tx, &user.id, &id)?.is_none() {
            return Err(ApiError::not_found("that collection isn't in your library"));
        }
        Ok(user)
    })?;
    let token = installation_token(state, c, input.installation).map_err(api_err)?;
    let (status, doc) = gh(state, "GET", &format!("{}/repos/{repo}", c.api_url), &token, None).map_err(api_err)?;
    if status != 200 {
        return Err(ApiError::new(404, "repo_not_found", format!("the HITOP Cloud App can't see {repo}: add it to the App's repositories on GitHub")));
    }
    let full = doc["full_name"].as_str().unwrap_or(&repo).to_string();
    let branch = input.branch.filter(|b| !b.trim().is_empty()).map(|b| b.trim().to_string()).unwrap_or_else(|| doc["default_branch"].as_str().unwrap_or("main").to_string());
    if branch.starts_with('-') || branch.contains("..") || !branch.chars().all(|ch| ch.is_ascii_alphanumeric() || "-_./".contains(ch)) {
        return Err(ApiError::bad("not a branch name"));
    }
    let now = state.now();
    state.db.tx(|tx| {
        tx.execute("DELETE FROM git_links WHERE user_id = $1 AND collection_id = $2", &[(&user.id).into(), (&id).into()])?;
        tx.execute(
            &format!("INSERT INTO git_links ({LINK_COLUMNS}, due_at, created_at) VALUES ($1, $2, $3, $4, $5, $6, '', '', '{{}}', 'pending', '[]', '', 0, 0, $7)"),
            &[(&user.id).into(), (&id).into(), input.installation.into(), (&full).into(), (&branch).into(), (&path).into(), now.into()],
        )?;
        Ok::<_, DbError>(())
    })?;
    sync_link(state, c, &user.id, &id, &BTreeMap::new())?;
    show(state, c, &user.id, &id).map(|v| (200, v))
}

fn show(state: &State, c: &GitConfig, user: &str, id: &str) -> Result<Json, ApiError> {
    let l = state.db.tx(|tx| load_link(tx, user, id))?.ok_or_else(|| ApiError::not_found("this collection isn't linked to GitHub"))?;
    Ok(view(&l, c))
}

/// `GET /v1/library/collection/:id/git` → the link.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let user = state.db.tx(|tx| person(state, tx, request))?;
    show(state, c, &user.id, &collection_param(params)).map(|v| (200, v))
}

/// `DELETE /v1/library/collection/:id/git`: unlinks; the repository keeps its files.
pub fn unlink(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    config(state)?;
    let id = collection_param(params);
    state.db.tx(|tx| {
        let user = person(state, tx, request)?;
        tx.execute("DELETE FROM git_links WHERE user_id = $1 AND collection_id = $2", &[(&user.id).into(), (&id).into()])?;
        Ok((200, json!({ "collectionId": id, "linked": false })))
    })
}

/// `POST /v1/library/collection/:id/git/sync` → the link after a sync now.
pub fn sync_now(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let id = collection_param(params);
    let user = state.db.tx(|tx| person(state, tx, request))?;
    sync_link(state, c, &user.id, &id, &BTreeMap::new())?;
    show(state, c, &user.id, &id).map(|v| (200, v))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resolve {
    choices: BTreeMap<String, String>,
}

/// `POST /v1/library/collection/:id/git/resolve {choices: {path: mine|theirs|both}}` → the link.
pub fn resolve(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let c = config(state)?;
    let id = collection_param(params);
    let input: Resolve = body(request)?;
    if input.choices.values().any(|v| !matches!(v.as_str(), "mine" | "theirs" | "both")) {
        return Err(ApiError::bad("a choice is mine, theirs or both"));
    }
    if input.choices.get(MANIFEST).is_some_and(|v| v == "both") {
        return Err(ApiError::bad("hitop.yaml can't be kept twice: choose mine or theirs"));
    }
    let user = state.db.tx(|tx| person(state, tx, request))?;
    sync_link(state, c, &user.id, &id, &input.choices)?;
    show(state, c, &user.id, &id).map(|v| (200, v))
}

/// `POST /v1/git/github/webhook`: GitHub's `push` (links of that branch due now) and
/// `installation` events (a removed installation disconnects its links).
pub fn webhook(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let c = config(state)?;
    let sig = request.header("x-hub-signature-256").map(|h| h.value.to_string()).unwrap_or_default();
    let Some(hex) = sig.strip_prefix("sha256=") else {
        return Err(ApiError::new(401, "bad_signature", "X-Hub-Signature-256 is missing"));
    };
    let bytes: Option<Vec<u8>> = (0..hex.len()).step_by(2).map(|i| hex.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect();
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, c.webhook_secret.as_bytes());
    if bytes.as_deref().is_none_or(|b| ring::hmac::verify(&key, &request.body, b).is_err()) {
        return Err(ApiError::new(401, "bad_signature", "X-Hub-Signature-256 doesn't match the body"));
    }
    let event = request.header("x-github-event").map(|h| h.value.to_string()).unwrap_or_default();
    let doc: Json = serde_json::from_slice(&request.body).map_err(|_| ApiError::bad("the body isn't JSON"))?;
    let installation = doc["installation"]["id"].as_i64().unwrap_or(0);
    let now = state.now();
    state.db.tx(|tx| {
        let n = match event.as_str() {
            "push" => {
                let repo = doc["repository"]["full_name"].as_str().unwrap_or_default();
                let branch = doc["ref"].as_str().unwrap_or_default().strip_prefix("refs/heads/").unwrap_or_default();
                let after = doc["after"].as_str().unwrap_or_default();
                tx.execute(
                    "UPDATE git_links SET due_at = $1 WHERE installation_id = $2 AND LOWER(repo) = LOWER($3) AND branch = $4 AND head <> $5 AND state <> 'disconnected'",
                    &[now.into(), installation.into(), repo.into(), branch.into(), after.into()],
                )?
            }
            "installation" if matches!(doc["action"].as_str(), Some("deleted" | "suspend")) => {
                tx.execute("DELETE FROM git_installations WHERE installation_id = $1", &[installation.into()])?;
                tx.execute(
                    "UPDATE git_links SET state = 'disconnected', message = 'the HITOP Cloud App was removed from GitHub: connect GitHub again', due_at = 0 WHERE installation_id = $1",
                    &[installation.into()],
                )?
            }
            _ => 0,
        };
        Ok((200, json!({ "event": event, "links": n })))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> Files {
        pairs.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect()
    }

    #[test]
    fn merge_takes_each_side_and_asks_only_on_a_clash() {
        let base = files(&[("hitop.yaml", "format: 1\nname: Shop\n"), ("requests/a.yaml", "name: A\nurl: /a\n"), ("requests/b.yaml", "name: B\nurl: /b\n")]);
        let mine = files(&[("hitop.yaml", "format: 1\nname: Shop\n"), ("requests/a.yaml", "name: A\nurl: /a2\n"), ("requests/b.yaml", "name: B\nurl: /b\n"), ("requests/c.yaml", "name: C\n")]);
        let theirs = files(&[("hitop.yaml", "format: 1\nname: Shop\n"), ("requests/a.yaml", "name: A\nurl: /a\n"), ("requests/b.yaml", "name: B\nurl: /b3\n")]);
        let (out, conflicts) = merge3(&base, &mine, &theirs, &BTreeMap::new());
        assert!(conflicts.is_empty());
        assert_eq!(out["requests/a.yaml"], "name: A\nurl: /a2\n");
        assert_eq!(out["requests/b.yaml"], "name: B\nurl: /b3\n");
        assert!(out.contains_key("requests/c.yaml"));

        let theirs = files(&[("hitop.yaml", "format: 1\nname: Shop\n"), ("requests/a.yaml", "name: A\nurl: /a9\n")]);
        let (_, conflicts) = merge3(&base, &mine, &theirs, &BTreeMap::new());
        assert_eq!(conflicts.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), vec!["requests/a.yaml"], "b deleted there and unchanged here is no clash");
        assert_eq!(conflicts[0].name, "A");

        let both: BTreeMap<String, String> = [("requests/a.yaml".to_string(), "both".to_string())].into();
        let (out, conflicts) = merge3(&base, &mine, &theirs, &both);
        assert!(conflicts.is_empty());
        assert_eq!(out["requests/a.yaml"], "name: A\nurl: /a9\n");
        assert!(out["requests/a-mine.yaml"].contains("A (mine)") && out["requests/a-mine.yaml"].contains("/a2"));
        let mine_wins: BTreeMap<String, String> = [("requests/a.yaml".to_string(), "mine".to_string())].into();
        assert_eq!(merge3(&base, &mine, &theirs, &mine_wins).0["requests/a.yaml"], "name: A\nurl: /a2\n");
    }

    #[test]
    fn scope_paths_and_messages() {
        assert!(in_scope("hitop.yaml") && in_scope("requests/x/y.yml") && !in_scope("README.md") && !in_scope("environments/dev.yaml") && !in_scope("requests/.hidden/a.yaml"));
        assert_eq!(rel_of("apis/shop", "apis/shop/hitop.yaml"), Some("hitop.yaml"));
        assert_eq!(rel_of("apis/shop", "apis/shopping/hitop.yaml"), None);
        assert!(clean_path("../x").is_err() && clean_path("a/.git").is_err());
        assert_eq!(clean_path("/apis/shop/").unwrap(), "apis/shop");
        let before = files(&[("requests/a.yaml", "1"), ("requests/b.yaml", "1")]);
        let after = files(&[("requests/a.yaml", "2"), ("requests/c.yaml", "1")]);
        assert_eq!(message_of(&before, &after), "hitop: add c; update a; remove b");
        assert_eq!(blob_sha("hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
    }
}
